// SPDX-License-Identifier: Apache-2.0

//! The replica side: pull the vault into a folder, and prove it restores.
//!
//! # Two commands, one honest division of labour
//!
//! [`replicate`] makes the mirror match what the vault lists — downloads for
//! anything missing or changed, deletions only behind `--prune`, and a write
//! that lands under its real name only after it has been flushed and renamed.
//! [`drill`] is the part ADR-0004 §2 exists for: every listed file is
//! downloaded **afresh** and compared against the mirror byte for byte. A
//! drill that trusted the mirror's own bytes would be the archive grading its
//! own homework; the comparison only means something when both sides come
//! from independent reads — the wire and the disk.
//!
//! Neither command updates any receipt unless it finished completely. A
//! cycle that dies halfway leaves the previous receipt standing, where it
//! ages out and starts reading as *nobody measuring* — which, while the
//! companion is down, is exactly true.

use crate::cell::{Cell, CellError};
use crate::plan::{self, Skipped};
use std::path::Path;

/// What a completed cycle did, ready to become a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycle {
    /// How many files ended up mirrored (or compared).
    pub files: u64,
    /// Their combined size in bytes.
    pub bytes: u64,
    /// The newest file mtime the cycle confirmed seeing.
    pub covered_mtime: u64,
}

/// Why a cycle stopped, carrying whatever the operator needs to find it.
#[derive(Debug)]
pub enum MirrorError {
    /// The cell refused or could not be reached.
    Cell(CellError),
    /// The mirror directory could not be walked.
    Walk(String),
    /// A specific file failed, named, mid-cycle.
    File {
        /// Which file.
        name: String,
        /// What happened to it.
        why: String,
    },
}

impl core::fmt::Display for MirrorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Cell(e) => write!(f, "{e}"),
            Self::Walk(why) => write!(f, "{why}"),
            Self::File { name, why } => write!(f, "{name}: {why}"),
        }
    }
}

impl From<CellError> for MirrorError {
    fn from(e: CellError) -> Self {
        Self::Cell(e)
    }
}

/// Pulls the vault into `dir` so the folder matches the listing.
///
/// Files already present at the right size and mtime are left untouched;
/// everything else is downloaded to a temporary name beside its destination
/// and renamed into place after a flush, so an interrupted cycle cannot
/// leave a half-written file wearing a real name.
///
/// With `prune`, local copies of files the vault no longer has are removed —
/// the mirror's job is to be the vault's double, and keeping ghosts of
/// deleted files would make it quietly worse than useless for restoring.
///
/// # Errors
///
/// Listing failures, walk failures, and the first file failure all stop the
/// cycle; nothing is reported as done when it is not.
pub fn replicate(
    cell: &Cell,
    token: &str,
    dir: &str,
    prune: bool,
) -> Result<(Cycle, Vec<Skipped>), MirrorError> {
    let remote = cell.listing(token)?;
    let (locals, skipped) = plan::walk(dir).map_err(|e| MirrorError::Walk(e.to_string()))?;

    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut covered_mtime = 0u64;
    for entry in &remote {
        let wanted = locals.iter().find(|l| l.name == entry.name);
        let up_to_date =
            matches!(wanted, Some(l) if l.bytes == entry.bytes && l.modified == entry.modified);
        if !up_to_date {
            let body = cell.get(&entry.name, token)?;
            if body.len() as u64 != entry.bytes {
                return Err(MirrorError::File {
                    name: entry.name.clone(),
                    why: format!(
                        "the vault says {} bytes but sent {}",
                        entry.bytes,
                        body.len()
                    ),
                });
            }
            write_durably(dir, &entry.name, &body)?;
        }
        files += 1;
        bytes += entry.bytes;
        covered_mtime = covered_mtime.max(entry.modified);
    }

    if prune {
        for local in &locals {
            if !remote.iter().any(|r| r.name == local.name) {
                std::fs::remove_file(Path::new(dir).join(&local.name)).map_err(|e| {
                    MirrorError::File {
                        name: local.name.clone(),
                        why: format!("could not prune the stale copy: {e}"),
                    }
                })?;
            }
        }
    }

    Ok((
        Cycle {
            files,
            bytes,
            covered_mtime,
        },
        skipped,
    ))
}

/// Restores every listed file afresh and compares it against the mirror.
///
/// This is the verification the whole receipt system stands on. Both reads
/// are independent — one over the wire, one from this disk — so agreement
/// means the mirror really does restore, and disagreement names the file
/// rather than rounding it away.
///
/// # Errors
///
/// Any mismatched byte, missing mirror copy, size disagreement, count
/// disagreement, or refusal stops the drill unnamed-nothing.
pub fn drill(cell: &Cell, token: &str, dir: &str) -> Result<Cycle, MirrorError> {
    let remote = cell.listing(token)?;
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut covered_mtime = 0u64;
    for entry in &remote {
        let fresh = cell.get(&entry.name, token)?;
        let mirrored =
            std::fs::read(Path::new(dir).join(&entry.name)).map_err(|e| MirrorError::File {
                name: entry.name.clone(),
                why: format!("the mirror has no readable copy to compare: {e}"),
            })?;
        if fresh.len() != mirrored.len() || fresh != mirrored {
            return Err(MirrorError::File {
                name: entry.name.clone(),
                why: "the fresh download does not match the mirror byte for byte".to_owned(),
            });
        }
        files += 1;
        bytes += entry.bytes;
        covered_mtime = covered_mtime.max(entry.modified);
    }
    Ok(Cycle {
        files,
        bytes,
        covered_mtime,
    })
}

/// Writes `bytes` under `dir/name` via a flushed temporary and a rename.
fn write_durably(dir: &str, name: &str, bytes: &[u8]) -> Result<(), MirrorError> {
    use std::io::Write as _;
    let dest = Path::new(dir).join(name);
    let tmp = dest.with_extension("vayutmp");
    let mut f = std::fs::File::create(&tmp).map_err(|e| MirrorError::File {
        name: name.to_owned(),
        why: format!("the mirror could not be written: {e}"),
    })?;
    f.write_all(bytes).map_err(|e| MirrorError::File {
        name: name.to_owned(),
        why: format!("the write fell short: {e}"),
    })?;
    f.sync_all().map_err(|e| MirrorError::File {
        name: name.to_owned(),
        why: format!("the flush failed: {e}"),
    })?;
    drop(f);
    // Same rule as the vault itself: nothing wears its real name until it is
    // complete on the disk beneath it.
    std::fs::rename(&tmp, &dest).map_err(|e| MirrorError::File {
        name: name.to_owned(),
        why: format!("could not move it into place: {e}"),
    })?;
    Ok(())
}
