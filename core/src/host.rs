// SPDX-License-Identifier: Apache-2.0

//! The seam between the probes and the machine.
//!
//! Every probe in this crate reads the world through [`Host`] rather than
//! touching the filesystem directly. That is not architecture for its own sake:
//! a detection routine that can only be exercised on the device it detects is a
//! routine nobody can write a failing test for, and this project's discipline
//! requires the failing test first.
//!
//! [`RealHost`] is the implementation that reads the running machine.
//! [`FakeHost`] is a map, and is what the tier tests use to describe a handset
//! nobody in this room is holding.

use std::collections::BTreeMap;
use std::path::Path;

/// Read-only access to the machine a probe is running on.
pub trait Host {
    /// Contents of a file, or `None` if it cannot be read for any reason.
    ///
    /// Unreadable and absent are deliberately collapsed here, because at this
    /// layer they are the same fact: this process could not see it. The
    /// *interpretation* of that fact belongs to the probe, which knows whether
    /// it means absent or unverified.
    fn read(&self, path: &str) -> Option<String>;

    /// Whether a path exists.
    fn exists(&self, path: &str) -> bool;

    /// Effective user id of this process.
    fn euid(&self) -> u32;

    /// An environment variable.
    fn env(&self, key: &str) -> Option<String>;
}

/// Write access to the machine — a separate trait, deliberately.
///
/// Everything else in this crate reads. This is the one thing that *changes a
/// device*, and on this project that means changing how a lithium cell is
/// charged in somebody's home. Folding it into [`Host`] would make every probe
/// in the codebase capable of it, and the capability would stop being visible
/// at the call site.
///
/// A function taking `&dyn Host` cannot write. A function that can write says so
/// in its signature, and that is the point.
pub trait Writer {
    /// Writes a value to a path.
    ///
    /// # Errors
    ///
    /// Returns what refused: a missing node, a read-only filesystem, a
    /// permission denial. None of these may be treated as success — a charge
    /// ceiling that could not be written is not a charge ceiling.
    fn write(&mut self, path: &str, value: &str) -> Result<(), String>;
}

/// Reads the machine this process is actually running on.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealHost;

impl Host for RealHost {
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    fn euid(&self) -> u32 {
        parse_euid(self.read("/proc/self/status").as_deref())
    }

    fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// The effective uid named by `/proc/self/status`, or [`NOT_ROOT`] if that file
/// did not answer.
///
/// Split out from [`RealHost::euid`] so the failure paths are testable without
/// a filesystem that refuses to answer. Reading the file at all — rather than
/// calling `geteuid` — keeps this crate free of libc and of `unsafe`, which
/// `#![forbid(unsafe_code)]` requires anyway. The line is
/// `Uid:\treal\teffective\tsaved\tfilesystem`, so the effective uid is field 2.
fn parse_euid(status: Option<&str>) -> u32 {
    status
        .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")))
        .and_then(|l| l.split_whitespace().nth(2))
        .and_then(|f| f.parse().ok())
        // A host that will not say who we are is never treated as root.
        .unwrap_or(NOT_ROOT)
}

/// The uid reported when the machine would not say. Deliberately the highest
/// representable id rather than `0`: an unanswered question must fall to the
/// least privilege, not the most.
pub const NOT_ROOT: u32 = u32::MAX;

#[cfg(test)]
mod tests {
    use super::{parse_euid, NOT_ROOT};

    #[test]
    fn a_status_file_that_will_not_say_who_we_are_is_never_root() {
        // The attack this forecloses: any parse failure defaulting to 0. Every
        // tier and capability gate downstream reads uid 0 as root, so a single
        // unwrap_or(0) here silently grants root to a machine that answered
        // nothing at all.
        for silent in [
            None,
            Some(""),
            Some("Name:\tvayucell\n"),           // no Uid line
            Some("Uid:\n"),                      // Uid line with no fields
            Some("Uid:\t0\n"),                   // real uid only, no effective
            Some("Uid:\t0\tnotanumber\t0\t0\n"), // unparseable effective uid
        ] {
            assert_eq!(
                parse_euid(silent),
                NOT_ROOT,
                "a silent or malformed status file must not read as root: {silent:?}"
            );
        }
        assert_ne!(NOT_ROOT, 0, "the fallback must not itself be root");
    }

    #[test]
    fn the_effective_uid_is_read_not_the_real_one() {
        // Fields are real, effective, saved, filesystem. A process that dropped
        // privileges has real 0 and effective non-zero, and reading field 1
        // would call it root after it stopped being root.
        assert_eq!(parse_euid(Some("Uid:\t0\t1000\t0\t0\n")), 1000);
        assert_eq!(parse_euid(Some("Name:\tx\nUid:\t0\t0\t0\t0\nGid:\t0\n")), 0);
    }
}

impl Writer for RealHost {
    fn write(&mut self, path: &str, value: &str) -> Result<(), String> {
        std::fs::write(path, value).map_err(|e| format!("{path}: {e}"))
    }
}

/// A machine described by a test rather than inhabited by one.
#[derive(Debug, Default, Clone)]
pub struct FakeHost {
    files: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    euid: u32,
    read_only: BTreeMap<String, String>,
    reverts: BTreeMap<String, String>,
}

impl FakeHost {
    /// An empty machine: no files, no environment, not root.
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            env: BTreeMap::new(),
            euid: 1000,
            read_only: BTreeMap::new(),
            reverts: BTreeMap::new(),
        }
    }

    /// Adds a readable file.
    #[must_use]
    pub fn with_file(mut self, path: &str, contents: &str) -> Self {
        self.files.insert(path.to_owned(), contents.to_owned());
        self
    }

    /// Adds a path that exists but cannot be read — the case that must report
    /// *unverified* rather than *absent*.
    #[must_use]
    pub fn with_unreadable(mut self, path: &str) -> Self {
        self.files
            .insert(format!("\0unreadable\0{path}"), String::new());
        self
    }

    /// Sets an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    /// Sets the effective user id.
    #[must_use]
    pub fn as_uid(mut self, uid: u32) -> Self {
        self.euid = uid;
        self
    }

    /// Convenience for `as_uid(0)`.
    #[must_use]
    pub fn as_root(self) -> Self {
        self.as_uid(0)
    }

    /// Removes a path, so a test can describe a device that lacks exactly one
    /// node.
    ///
    /// Building the missing-node cases by subtraction rather than by listing
    /// every other node keeps them honest: a node added to the fixture is
    /// automatically covered, where a hand-written list would silently stop
    /// testing the new one.
    #[must_use]
    pub fn without(mut self, path: &str) -> Self {
        self.files.remove(path);
        self.read_only.remove(path);
        self.reverts.remove(path);
        self
    }

    /// Adds a node that exists, reads back, and refuses every write.
    ///
    /// The ordinary case on a device whose kernel exposes a charge node but will
    /// not let this process set it.
    #[must_use]
    pub fn with_read_only(mut self, path: &str, contents: &str) -> Self {
        self.read_only.insert(path.to_owned(), contents.to_owned());
        self.files.insert(path.to_owned(), contents.to_owned());
        self
    }

    /// Adds a node that accepts a write and then reports something else.
    ///
    /// This is a vendor charging daemon putting the ceiling back after a cable
    /// event, and it is the failure the whole verification loop exists for.
    /// Without a fake that can do it, the loop could only ever be tested against
    /// mechanisms that behave.
    #[must_use]
    pub fn with_revert(mut self, path: &str, reverts_to: &str) -> Self {
        self.reverts.insert(path.to_owned(), reverts_to.to_owned());
        self.files.insert(path.to_owned(), reverts_to.to_owned());
        self
    }
}

impl Writer for FakeHost {
    fn write(&mut self, path: &str, value: &str) -> Result<(), String> {
        if self.read_only.contains_key(path) {
            return Err(format!("{path}: read-only file system"));
        }
        if !self.files.contains_key(path) {
            return Err(format!("{path}: no such file or directory"));
        }
        if let Some(back) = self.reverts.get(path).cloned() {
            self.files.insert(path.to_owned(), back);
            return Ok(());
        }
        self.files.insert(path.to_owned(), value.to_owned());
        Ok(())
    }
}

impl Host for FakeHost {
    fn read(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path) || self.files.contains_key(&format!("\0unreadable\0{path}"))
    }

    fn euid(&self) -> u32 {
        self.euid
    }

    fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }
}
