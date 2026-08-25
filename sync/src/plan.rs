// SPDX-License-Identifier: Apache-2.0
//! The folder on this machine, the vault's answer, and the difference
//! between them.

use crate::cell::StoredListing;
use vayucell_core::vault::Name;

/// One thing the plan wants done.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Send the local file; the vault does not have it, or does not have
    /// this version of it.
    Upload {
        /// The name it is stored under.
        name: String,
        /// Why: `new`, or what differed.
        why: Difference,
    },
    /// Remove it from the vault. Only ever acted on behind `--prune`.
    Prune {
        /// The remote name with no local counterpart.
        name: String,
    },
}

/// What differed about a file that is being uploaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difference {
    /// Nothing is stored under this name.
    New,
    /// Same name, different size.
    Size,
    /// Same size, written at a different time.
    Mtime,
}

impl core::fmt::Display for Difference {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::New => f.write_str("new"),
            Self::Size => f.write_str("size differs"),
            Self::Mtime => f.write_str("written at a different time"),
        }
    }
}

/// A local file the walk considered.
#[derive(Debug, PartialEq, Eq)]
pub struct Local {
    /// The name it would be stored under.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// Its last write, in seconds since the Unix epoch.
    pub modified: u64,
}

/// Names a local file was skipped for, alongside the file itself.
#[derive(Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The name as found on disk.
    pub name: String,
    /// Why it will not be synced.
    pub why: SkipReason,
}

/// Why a local file is out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Inside a nested directory; the vault is flat and so is this sync.
    Nested,
    /// Begins with a dot, which the vault refuses as a class.
    Hidden,
    /// The vault could not store this name even if it wanted to.
    Unaddressable,
}

impl core::fmt::Display for Skipped {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let why = match self.why {
            SkipReason::Nested => "not a plain file at the top level, and the vault is flat",
            SkipReason::Hidden => "a hidden name, which the vault refuses as a class",
            SkipReason::Unaddressable => "not a name the vault can be asked to store over HTTP",
        };
        write!(f, "{} — {}", self.name, why)
    }
}

/// Walks the top level of `dir`, skipping everything the vault could not
/// take, and reports what it skipped rather than doing it quietly.
///
/// # Errors
///
/// Returns the io error when the directory itself cannot be read: a folder
/// that cannot be opened must not read as an empty one, for the same reason
/// the vault refuses to answer a listing it could not produce.
pub fn walk(dir: &str) -> std::io::Result<(Vec<Local>, Vec<Skipped>)> {
    let mut locals = Vec::new();
    let mut skipped = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let raw = entry.file_name().into_string().unwrap_or_default();
        let meta = entry.metadata()?;
        if !meta.is_file() {
            // A nested folder is the common case; a link and anything else
            // that is not an ordinary file lands here too.
            skipped.push(Skipped {
                name: raw,
                why: SkipReason::Nested,
            });
            continue;
        }
        if raw.starts_with('.') {
            skipped.push(Skipped {
                name: raw,
                why: SkipReason::Hidden,
            });
            continue;
        }
        if Name::new(&raw).is_err() {
            skipped.push(Skipped {
                name: raw,
                why: SkipReason::Unaddressable,
            });
            continue;
        }
        let modified = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        locals.push(Local {
            name: raw,
            bytes: meta.len(),
            modified,
        });
    }
    locals.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((locals, skipped))
}

/// Computes the actions that bring the vault in line with the folder:
/// uploads for new and changed files, prunes for remote names with no local
/// counterpart. Up to date files produce nothing.
#[must_use]
pub fn diff(locals: &[Local], remote: &[StoredListing]) -> Vec<Action> {
    let mut actions = Vec::new();
    for local in locals {
        let same = remote.iter().find(|r| r.name == local.name);
        match same {
            None => actions.push(Action::Upload {
                name: local.name.clone(),
                why: Difference::New,
            }),
            Some(r) if r.bytes != local.bytes => actions.push(Action::Upload {
                name: local.name.clone(),
                why: Difference::Size,
            }),
            Some(r) if r.modified != local.modified => actions.push(Action::Upload {
                name: local.name.clone(),
                why: Difference::Mtime,
            }),
            Some(_) => {}
        }
    }
    for r in remote {
        if !locals.iter().any(|l| l.name == r.name) {
            actions.push(Action::Prune {
                name: r.name.clone(),
            });
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(name: &str, bytes: u64, modified: u64) -> Local {
        Local {
            name: name.to_owned(),
            bytes,
            modified,
        }
    }

    fn remote(name: &str, bytes: u64, modified: u64) -> StoredListing {
        StoredListing {
            name: name.to_owned(),
            bytes,
            modified,
        }
    }

    #[test]
    fn a_file_the_vault_never_heard_of_is_a_new_upload() {
        let d = diff(&[local("a.txt", 3, 9)], &[]);
        assert_eq!(
            d,
            vec![Action::Upload {
                name: "a.txt".to_owned(),
                why: Difference::New
            }]
        );
    }

    #[test]
    fn the_same_size_and_mtime_means_up_to_date_and_produces_nothing() {
        let d = diff(&[local("a.txt", 3, 9)], &[remote("a.txt", 3, 9)]);
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn the_same_size_written_at_another_time_is_re_uploaded_said_so() {
        let d = diff(&[local("a.txt", 3, 20)], &[remote("a.txt", 3, 9)]);
        assert_eq!(
            d,
            vec![Action::Upload {
                name: "a.txt".to_owned(),
                why: Difference::Mtime
            }]
        );
    }

    #[test]
    fn a_different_size_is_its_own_reason_even_when_times_match() {
        let d = diff(&[local("a.txt", 5, 9)], &[remote("a.txt", 3, 9)]);
        assert_eq!(
            d,
            vec![Action::Upload {
                name: "a.txt".to_owned(),
                why: Difference::Size
            }]
        );
    }

    #[test]
    fn a_remote_file_with_no_local_counterpart_is_prune_not_upload() {
        let d = diff(&[], &[remote("gone.txt", 1, 1)]);
        assert_eq!(
            d,
            vec![Action::Prune {
                name: "gone.txt".to_owned()
            }]
        );
    }

    #[test]
    fn pruning_only_sees_names_that_are_gone_locally_not_merely_changed() {
        let d = diff(
            &[local("keep.txt", 2, 2)],
            &[remote("keep.txt", 9, 9), remote("extra.txt", 1, 1)],
        );
        assert_eq!(d.len(), 2);
        assert!(d.contains(&Action::Upload {
            name: "keep.txt".to_owned(),
            why: Difference::Size
        }));
        assert!(d.contains(&Action::Prune {
            name: "extra.txt".to_owned()
        }));
    }

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-sync-{tag}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn dir(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
        fn put(&self, rel: &str, bytes: &[u8]) {
            if let Some(parent) = self.0.join(rel).parent() {
                std::fs::create_dir_all(parent).expect("nested");
            }
            std::fs::write(self.0.join(rel), bytes).expect("written");
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_walk_takes_top_level_files_and_skips_the_rest_out_loud() {
        let s = Scratch::new("walk");
        s.put("kept.txt", b"1234");
        s.put(".hidden", b"x");
        s.put("sub/nested.txt", b"x");

        let (locals, skipped) = walk(&s.dir()).expect("readable");
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].name, "kept.txt");
        assert_eq!(locals[0].bytes, 4);

        let whys: Vec<(&str, SkipReason)> =
            skipped.iter().map(|s| (s.name.as_str(), s.why)).collect();
        assert!(whys.contains(&(".hidden", SkipReason::Hidden)));
        assert!(whys.contains(&("sub", SkipReason::Nested)));
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_in_the_folder_is_skipped_as_nested_not_followed() {
        use std::os::unix::fs::symlink;
        let s = Scratch::new("link");
        s.put("elsewhere.bin", b"outside");
        symlink(s.0.join("elsewhere.bin"), s.0.join("link.bin")).expect("a link");

        let (locals, skipped) = walk(&s.dir()).expect("readable");
        assert_eq!(locals.len(), 1, "{locals:?}");
        assert_eq!(locals[0].name, "elsewhere.bin");
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        assert_eq!(skipped[0].name, "link.bin");
    }

    #[test]
    fn a_folder_that_cannot_be_opened_is_an_error_not_an_empty_plan() {
        let s = Scratch::new("dark");
        let dir = s.dir();
        std::fs::remove_dir_all(&s.0).expect("removes it");
        assert!(walk(&dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_name_the_vault_could_not_store_is_reported_as_unaddressable() {
        // Windows cannot create a name ending in a dot — it strips it — so
        // this case only exists where the filesystem allows the thing the
        // vault refuses.
        let s = Scratch::new("odd");
        s.put("fine.txt", b"x");
        s.put("ends.", b"x");

        let (_, skipped) = walk(&s.dir()).expect("readable");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].why, SkipReason::Unaddressable);
        assert!(skipped[0].to_string().contains("ends."));
    }
}
