// SPDX-License-Identifier: Apache-2.0

//! The halt record on disk: reading it, writing it, and clearing it.
//!
//! The decision lives in [`vayucell_core::halt`] and has no filesystem. This is
//! the half that does, and the half where the interesting failure is not a halt
//! that reads cleanly — it is a record that exists and will not open.
//!
//! # Absent and unreadable are answered differently, on purpose
//!
//! [`std::fs::read_to_string`] fails the same way for both as far as a caller in
//! a hurry is concerned, so the two are separated by asking the filesystem which
//! it is. `NotFound` is [`Standing::Clear`] — nothing was ever recorded.
//! Anything else is [`Standing::Unreadable`], which does not serve.
//!
//! That distinction is the whole module. Collapsing it — treating any read
//! failure as "no halt" — would mean a permissions change, a full disk or a
//! half-mounted card silently returned a halted phone to service.

use std::io::Write as _;

use vayucell_core::halt::{Halt, Standing};

/// The mode the record is created with.
///
/// Not a secret, and not writable by anyone else either: this file is the only
/// thing standing between a halted phone and a phone that serves again, so
/// anything that can write it can clear a hard stop.
const RECORD_MODE: u32 = 0o644;

/// Where the record lives unless told otherwise.
///
/// Beside the credential store, so one directory holds everything this program
/// keeps and `rm -rf ~/.vayucell` remains the whole uninstall.
#[must_use]
pub fn default_path() -> String {
    std::env::var("HOME").map_or_else(
        |_| ".vayucell/halted".to_owned(),
        |home| format!("{home}/.vayucell/halted"),
    )
}

/// What the record says about this device.
#[must_use]
pub fn read(path: &str) -> Standing {
    match std::fs::read_to_string(path) {
        Ok(raw) => match Halt::parse(&raw) {
            Ok(h) => Standing::Halted(h),
            // Present and not understood. Not clear: something wrote this, and
            // the only thing that writes it is a hard stop.
            Err(e) => Standing::Unreadable(format!("{path}: {e}")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Standing::Clear,
        Err(e) => Standing::Unreadable(format!("{path}: {e}")),
    }
}

/// Records a halt, durably enough to survive the power cut that may follow it.
///
/// A halt is written at the moment a device is in trouble, which is exactly
/// when it may lose power a second later. The ordering is the vault's, for the
/// same reason: write a temporary, flush the bytes, rename, then **flush the
/// directory**, because a rename that never reached the medium leaves the
/// record absent and the phone serving.
///
/// # Errors
///
/// Returns what went wrong, for the operator's terminal.
pub fn record(path: &str, halt: &Halt) -> Result<(), String> {
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
    file.write_all(halt.render().as_bytes())
        .map_err(|e| format!("{temporary}: {e}"))?;
    file.sync_all().map_err(|e| format!("{temporary}: {e}"))?;
    drop(file);

    std::fs::rename(&temporary, path).map_err(|e| format!("{path}: {e}"))?;

    // The step whose absence is invisible until a real power cut.
    std::fs::File::open(parent)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{}: {e}", parent.display()))
}

/// Removes the record, after a person has looked at the phone.
///
/// # Errors
///
/// Returns what went wrong. A record that was not there is not an error — the
/// caller wanted it gone and it is gone.
pub fn clear(path: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{path}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{clear, read, record};
    use vayucell_core::halt::{Halt, Standing};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-halt-{name}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn path(&self) -> String {
            self.0.join("halted").to_string_lossy().into_owned()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn halt() -> Halt {
        Halt::new("pack temperature exceeded 60 °C").expect("ordinary")
    }

    #[test]
    fn a_recorded_halt_is_still_there_for_the_next_process() {
        // The point of the whole thing: the restart does not clear it.
        let s = Scratch::new("survives");
        record(&s.path(), &halt()).expect("records");
        assert_eq!(read(&s.path()), Standing::Halted(halt()));
        assert!(!read(&s.path()).may_start_serving());
    }

    #[test]
    fn no_record_at_all_is_clear() {
        let s = Scratch::new("absent");
        assert_eq!(read(&s.path()), Standing::Clear);
        assert!(read(&s.path()).may_start_serving());
    }

    #[test]
    fn a_record_that_exists_and_will_not_parse_is_unreadable_rather_than_clear() {
        // The distinction the module exists for. Something wrote this file, and
        // the only thing that writes it is a hard stop.
        let s = Scratch::new("garbled");
        std::fs::write(s.path(), b"\n").expect("writes an empty record");
        let standing = read(&s.path());
        assert!(matches!(standing, Standing::Unreadable(_)), "{standing:?}");
        assert!(!standing.may_start_serving());
    }

    #[cfg(unix)]
    #[test]
    fn a_record_that_cannot_be_opened_is_unreadable_rather_than_clear() {
        // A permissions change must not return a halted phone to service. This
        // is the failure that a plain `read_to_string(..).unwrap_or_default()`
        // would have turned into "no halt was ever recorded".
        use std::os::unix::fs::PermissionsExt as _;
        let s = Scratch::new("locked");
        record(&s.path(), &halt()).expect("records");
        std::fs::set_permissions(s.path(), std::fs::Permissions::from_mode(0o000))
            .expect("removes every permission");

        let standing = read(&s.path());
        // Running as root defeats the permission, and CI sometimes does. Then
        // the record simply reads, which is also not `Clear` — either way the
        // device must not be free to serve.
        assert!(
            !standing.may_start_serving(),
            "an unopenable record let the device serve: {standing:?}"
        );

        let _ = std::fs::set_permissions(s.path(), std::fs::Permissions::from_mode(0o644));
    }

    #[test]
    fn clearing_removes_it_and_clearing_nothing_is_not_an_error() {
        // A person looked at the phone. That is the only way back.
        let s = Scratch::new("cleared");
        record(&s.path(), &halt()).expect("records");
        clear(&s.path()).expect("clears");
        assert_eq!(read(&s.path()), Standing::Clear);
        clear(&s.path()).expect("clearing an absent record is what the caller wanted");
    }

    #[test]
    fn recording_leaves_no_temporary_behind() {
        // The rename either happened or it did not; debris beside it would be
        // mistaken for the record by anything that globbed the directory.
        let s = Scratch::new("nodebris");
        record(&s.path(), &halt()).expect("records");
        let left: Vec<String> = std::fs::read_dir(&s.0)
            .expect("readable")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(left, vec!["halted".to_owned()], "{left:?}");
    }

    #[test]
    fn recording_into_a_directory_that_does_not_exist_yet_creates_it() {
        // The first halt on a fresh install arrives before anything else has
        // had reason to make ~/.vayucell.
        let s = Scratch::new("fresh");
        let nested = s.0.join("deeper").join("halted");
        let path = nested.to_string_lossy().into_owned();
        record(&path, &halt()).expect("creates the directory it needs");
        assert_eq!(read(&path), Standing::Halted(halt()));
    }
}
