// SPDX-License-Identifier: Apache-2.0

//! Enrolling a device, and reading the store back.
//!
//! This is the half of [`vayucell_core::auth`] that touches the machine: it
//! mints a secret from the kernel, appends it to a file only the owner can read,
//! and shows it to a person exactly once.
//!
//! # The secret is printed once and never again
//!
//! It is written to the store and to the terminal in the same breath, and there
//! is no command that prints it back. That is not an inconvenience to work
//! around — a credential a program will re-display is a credential that leaks
//! through a screen share, a scrollback, or somebody's shoulder, and a person
//! who lost one can enrol the device again in five seconds.

use std::io::Write as _;

use vayucell_core::auth::{
    parse_store, readable_by_others, Credentials, DeviceName, Secret, SECRET_BYTES,
};

/// The file mode a credential store must have.
///
/// Owner read and write. The store holds secrets in the clear — see
/// `auth`'s module documentation for why that is the right trade — so this is
/// the whole of the protection rather than a belt beside a brace.
pub const STORE_MODE: u32 = 0o600;

/// Mints a secret using the kernel's randomness.
///
/// # Errors
///
/// Returns why no secret could be produced. There is no fallback to a weaker
/// source: a credential from a predictable generator is worse than no
/// credential, because it reads as protection.
pub fn mint() -> Result<Secret, String> {
    use std::io::Read as _;
    // read_exact into a fixed buffer, never a whole-file read: /dev/urandom has
    // no end, and reading it to EOF allocates until the process is killed.
    let mut buf = [0u8; SECRET_BYTES];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .map_err(|e| format!("no randomness available to mint a secret: {e}"))?;
    Secret::new(&crate::listen::base64url(&buf))
        .map_err(|e| format!("the minted secret was refused by its own checks: {e}"))
}

/// Reads a credential store, refusing one that others can read.
///
/// # Errors
///
/// Returns why it could not be used. A missing store is **not** an error — it
/// parses as the empty store, which enrols nobody and therefore accepts nobody.
/// Treating "no file" as "no authentication" is the failure this whole path
/// exists to avoid, and `auth` refuses the empty store for exactly that reason.
pub fn load(path: &str) -> Result<Credentials, String> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(Credentials::empty());
    };
    let mode = mode_of(&meta);
    if readable_by_others(mode) {
        return Err(format!(
            "{path} is mode {mode:04o} and holds secrets in the clear, so anyone \
             on this device can read them.\n  Fix it with: chmod {STORE_MODE:o} {path}"
        ));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    parse_store(&text).map_err(|e| format!("{path}: {e}"))
}

#[cfg(unix)]
fn mode_of(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode_of(_meta: &std::fs::Metadata) -> u32 {
    // Nothing to check and nothing pretended. On a platform without POSIX modes
    // this returns a value that always fails the readable-by-others test, so the
    // store is refused rather than trusted on a check that did not happen.
    0o777
}

/// Adds a device to the store, creating it with a private mode if absent.
///
/// # Errors
///
/// Returns why the device could not be enrolled. A name already present is
/// refused rather than duplicated: two rows with one name means revoking it
/// leaves the other behind, and the operator has no way to see that.
pub fn enrol(path: &str, name: &str) -> Result<Secret, String> {
    let device = DeviceName::new(name).map_err(|e| format!("{name:?}: {e}"))?;

    // Read first, so an existing store's permissions are checked before this
    // appends a secret to it.
    let existing = load(path)?;
    if existing.devices().iter().any(|d| **d == device) {
        return Err(format!(
            "{device} is already enrolled. Remove its line from {path} first, \
             which is also how a device is revoked."
        ));
    }

    let secret = mint()?;
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }

    // Created with the private mode from the start rather than chmod'ed after.
    // A file that exists world-readable for even an instant has been readable.
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(STORE_MODE);
    }
    let mut file = options.open(path).map_err(|e| format!("{path}: {e}"))?;

    // The secret leaves `auth`'s custody for exactly one line, and goes straight
    // to a file whose mode was set before it was opened.
    writeln!(
        file,
        "{device} {}",
        core::str::from_utf8(secret.expose_for_comparison())
            .map_err(|e| format!("the minted secret was not text: {e}"))?
    )
    .map_err(|e| format!("{path}: {e}"))?;
    file.flush().map_err(|e| format!("{path}: {e}"))?;

    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::{enrol, load, mint, STORE_MODE};
    use vayucell_core::auth::SECRET_CHARS;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-enrol-{name}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn store(&self) -> String {
            self.0.join("devices").to_string_lossy().into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_minted_secret_passes_the_checks_that_guard_the_store() {
        let a = mint().expect("randomness");
        let b = mint().expect("randomness");
        assert_eq!(a.expose_for_comparison().len(), SECRET_CHARS);
        assert_ne!(
            a.expose_for_comparison(),
            b.expose_for_comparison(),
            "two mints produced the same secret"
        );
    }

    #[test]
    fn a_missing_store_is_the_empty_store_rather_than_an_error() {
        // And the empty store accepts nobody, which is the point: "no file" must
        // never become "no authentication".
        let s = Scratch::new("missing");
        let creds = load(&s.store()).expect("a missing store is not a failure");
        assert!(creds.is_empty());
    }

    #[test]
    fn enrolling_creates_the_store_private_from_the_first_instant() {
        // Not created and then chmod'ed: a file that was world-readable for an
        // instant has been read by anything that was looking.
        let s = Scratch::new("mode");
        enrol(&s.store(), "laptop").expect("enrols");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(s.store())
                .expect("exists")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, STORE_MODE, "mode was {mode:04o}");
        }
    }

    #[test]
    fn an_enrolled_device_verifies_against_the_store_it_was_written_to() {
        let s = Scratch::new("roundtrip");
        let secret = enrol(&s.store(), "laptop").expect("enrols");
        let creds = load(&s.store()).expect("loads");
        let offered =
            core::str::from_utf8(secret.expose_for_comparison()).expect("base64url is text");
        assert!(creds.verify(Some(offered)).is_authenticated());
    }

    #[test]
    fn a_second_device_is_added_without_disturbing_the_first() {
        let s = Scratch::new("two");
        let first = enrol(&s.store(), "laptop").expect("enrols");
        let second = enrol(&s.store(), "phone").expect("enrols");
        let creds = load(&s.store()).expect("loads");
        assert_eq!(creds.len(), 2);
        for secret in [&first, &second] {
            let offered = core::str::from_utf8(secret.expose_for_comparison()).expect("text");
            assert!(creds.verify(Some(offered)).is_authenticated());
        }
    }

    #[test]
    fn a_name_already_enrolled_is_refused_rather_than_duplicated() {
        // Two rows with one name means revoking it leaves the other behind, and
        // the operator has no way to see that.
        let s = Scratch::new("dup");
        enrol(&s.store(), "laptop").expect("enrols");
        let e = enrol(&s.store(), "laptop").expect_err("already enrolled");
        assert!(e.contains("already enrolled"), "{e}");
        assert_eq!(load(&s.store()).expect("loads").len(), 1);
    }

    #[test]
    fn a_store_others_can_read_is_refused_with_the_command_that_fixes_it() {
        let s = Scratch::new("exposed");
        enrol(&s.store(), "laptop").expect("enrols");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(s.store(), std::fs::Permissions::from_mode(0o644))
                .expect("chmod");
            let e = load(&s.store()).expect_err("world-readable");
            assert!(e.contains("chmod"), "{e}");
            assert!(e.contains("0644"), "{e}");
            // And enrolling into it is refused too, before another secret is
            // appended to a file everyone can read.
            assert!(enrol(&s.store(), "phone").is_err());
        }
    }

    #[test]
    fn a_device_name_the_store_could_not_represent_is_refused_before_minting() {
        let s = Scratch::new("badname");
        for bad in ["my laptop", "", "a\nb"] {
            assert!(enrol(&s.store(), bad).is_err(), "{bad:?} was enrolled");
        }
        assert!(
            load(&s.store()).expect("loads").is_empty(),
            "a refused enrolment must leave nothing behind"
        );
    }
}
