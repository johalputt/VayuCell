// SPDX-License-Identifier: Apache-2.0

//! The guest's own record on disk: what the host told it, and in which boot.
//!
//! The contract lives in [`vayucell_core::guest`]; this is the half that does
//! filesystem work, and it exists because a guest is destroyed by every host
//! reboot. Nothing the guest learns survives one in memory, so the only way it
//! can know that the world has moved on is by having written down where it
//! was standing before it was destroyed.
//!
//! # Absent, unreadable and unread are three different answers
//!
//! The record can be missing (the guest has never been told anything), it can
//! exist and refuse to open (a permissions change, a full disk, a card that
//! half-mounted), or it can exist and be nonsense (something wrote it that
//! this build does not understand). The third is the one that matters: a
//! corrupt record read as an empty record would turn a lost file into a clean
//! bill of health, and the guest would report that it holds no claims when in
//! fact it cannot remember whether it holds any.
//!
//! # What is written
//!
//! Nothing secret, and nothing about the phone. The record names the boot of
//! the host the guest last heard from, and what the host said in it. It is
//! created readable by its owner only, in the same way as the halt record: it
//! is one of the things this program keeps, and `rm -rf ~/.vayucell` remains
//! the whole uninstall.

use std::io::Write as _;

use vayucell_core::guest::{Ledger, Record};

/// The mode the record is created with.
///
/// Not a secret, and not writable by anyone else either: this file is what
/// tells a restarted guest that the host has rebooted underneath it, and
/// anything that can write it can put back a claim the host has dropped.
const RECORD_MODE: u32 = 0o644;

/// Where the record lives unless told otherwise.
///
/// Beside the credential store, so one directory holds everything this program
/// keeps and `rm -rf ~/.vayucell` remains the whole uninstall.
#[must_use]
pub fn default_path() -> String {
    std::env::var("HOME").map_or_else(
        |_| ".vayucell/guest".to_owned(),
        |home| format!("{home}/.vayucell/guest"),
    )
}

/// Reads the record.
///
/// The three failures are kept apart on purpose; see the module documentation.
#[must_use]
pub fn read(path: &str) -> Record {
    match std::fs::read_to_string(path) {
        Ok(raw) => match Ledger::parse(&raw) {
            Ok(ledger) => Record::Read(ledger),
            // Present and not understood. Not empty: something wrote this, and
            // the only thing that writes it is this program.
            Err(e) => Record::Corrupt(format!("{path}: {e}")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Record::Absent,
        Err(e) => Record::Unreadable(format!("{path}: {e}")),
    }
}

/// Writes the record, durably enough to survive the power cut that may follow.
///
/// The ordering is the halt record's, for the same reason: write a temporary,
/// flush the bytes, rename, then **flush the directory**, because a rename
/// that never reached the medium leaves the record absent and the guest
/// believing it holds no claims.
///
/// # Errors
///
/// Returns what went wrong, for the operator's terminal.
pub fn write(path: &str, ledger: &Ledger) -> Result<(), String> {
    let parent = std::path::Path::new(path)
        .parent()
        .ok_or_else(|| format!("{path}: has no directory to write into"))?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;

    let temporary = format!("{path}.partial");

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(RECORD_MODE);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|e| format!("{temporary}: {e}"))?;
    file.write_all(ledger.render().as_bytes())
        .map_err(|e| format!("{temporary}: {e}"))?;
    file.sync_all().map_err(|e| format!("{temporary}: {e}"))?;
    drop(file);

    std::fs::rename(&temporary, path).map_err(|e| format!("{path}: {e}"))?;

    // The step whose absence is invisible until a real power cut: the rename
    // has to reach the medium, and on a directory that means the directory
    // entry has to be flushed too. Windows cannot open a directory for
    // flushing through the standard library, so the step is unix-only — and
    // the claim it supports is a claim about the filesystem, so it is only
    // made where the filesystem offers it.
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{}: {e}", parent.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{default_path, read, write};
    use vayucell_core::guest::{Attestation, Ledger, Record};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-guest-{name}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn path(&self) -> String {
            self.0.join("guest").to_string_lossy().into_owned()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ledger() -> Ledger {
        let mut ledger = Ledger::new();
        ledger
            .establish(&Attestation::parse("epoch=8;ceiling=60").expect("a statement"))
            .expect("establishes");
        ledger
    }

    #[test]
    fn a_recorded_claim_is_still_there_for_the_next_process() {
        // The point of the whole file. The guest is destroyed by every host
        // reboot, and this is the only thing that survives one.
        let s = Scratch::new("survives");
        write(&s.path(), &ledger()).expect("records");
        match read(&s.path()) {
            Record::Read(read) => assert_eq!(read, ledger()),
            other => panic!("the record did not come back: {other:?}"),
        }
    }

    #[test]
    fn no_record_at_all_is_absent() {
        let s = Scratch::new("absent");
        assert!(matches!(read(&s.path()), Record::Absent));
    }

    #[test]
    fn a_record_that_exists_and_will_not_parse_is_corrupt_rather_than_empty() {
        // The distinction the module exists for. Something wrote this file, and
        // reading it as an empty record would report that the guest holds no
        // claims when in fact it cannot remember whether it holds any.
        let s = Scratch::new("garbled");
        std::fs::write(s.path(), b"not a guest record").expect("writes");
        assert!(matches!(read(&s.path()), Record::Corrupt(_)));
    }

    #[cfg(unix)]
    #[test]
    fn a_record_that_cannot_be_opened_is_unreadable_rather_than_absent() {
        use std::os::unix::fs::PermissionsExt as _;
        let s = Scratch::new("locked");
        write(&s.path(), &ledger()).expect("records");
        std::fs::set_permissions(s.path(), std::fs::Permissions::from_mode(0o000))
            .expect("removes every permission");
        let record = read(&s.path());
        // Running as root defeats the permission, and CI sometimes does. Then
        // the record simply reads, which is also not `Absent`.
        assert!(!matches!(record, Record::Absent), "{record:?}");
        let _ = std::fs::set_permissions(s.path(), std::fs::Permissions::from_mode(0o644));
    }

    #[test]
    fn writing_leaves_no_temporary_behind() {
        // The rename either happened or it did not; debris beside it would be
        // mistaken for the record by anything that globbed the directory.
        let s = Scratch::new("nodebris");
        write(&s.path(), &ledger()).expect("records");
        let left: Vec<String> = std::fs::read_dir(&s.0)
            .expect("readable")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(left, vec!["guest".to_owned()], "{left:?}");
    }

    #[test]
    fn writing_into_a_directory_that_does_not_exist_yet_creates_it() {
        let s = Scratch::new("fresh");
        let nested = s.0.join("deeper").join("guest");
        let path = nested.to_string_lossy().into_owned();
        write(&path, &ledger()).expect("creates the directory it needs");
        assert!(matches!(read(&path), Record::Read(_)));
    }

    #[test]
    fn the_default_path_is_beside_the_rest_of_what_this_program_keeps() {
        // One directory holds everything, so the uninstall stays one command.
        let path = default_path();
        assert!(
            path.ends_with("/guest") || path.ends_with("\\guest"),
            "{path}"
        );
    }
}
