// SPDX-License-Identifier: Apache-2.0

//! `vayucell image` — check the running system against an image manifest.
//!
//! ADR-0016 says what a mainline image *is*: a name, a version, the kernel
//! release it was built against, the board it targets, and the maximum charge
//! voltage its device tree declares. This is the command that checks the
//! machine in front of you against such a description.
//!
//! # What it can and cannot say
//!
//! The check reads the running kernel, the running device tree and the
//! running charge-voltage node, and it compares each against the manifest.
//! Three answers stay apart: *matched*, *different*, and *could not be read*.
//! A machine that answers nothing is not a machine that matched, and the
//! exit code says so.
//!
//! # What it does not check
//!
//! **Who published the image.** There is no signature here, and the manifest
//! says what an image is rather than who vouched for it. A manifest is as
//! trustworthy as the file it came from, and until the images exist there is
//! nothing to sign. PLAN §11 keeps that part of P8 open.
//!
//! # What it has never done
//!
//! It has never run against a phone running a mainline image, because no such
//! image exists yet. Every path here is exercised against a fake host.

use vayucell_core::host::Host;
use vayucell_core::image::{self, Manifest, Outcome};

/// The command.
///
/// Reads the manifest at `path`, checks the running system against it, and
/// prints what it found.
///
/// The exit code is the verdict: `0` when every check matched, `1` when a
/// check could not be made, `2` when a check was made and did not hold. The
/// third is the one that matters — a machine running something other than
/// the image it claims is running has been checked and has failed, which is
/// not the same as a machine nobody could look at.
pub fn run(host: &dyn Host, path: &str) -> i32 {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("vayucell image: the manifest at {path} could not be read: {e}");
            return 64;
        }
    };
    let manifest = match Manifest::parse(&text) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("vayucell image: {path} is not a manifest: {e}");
            return 64;
        }
    };
    let report = image::check(host, &manifest);
    print!("{}", report.render());
    match report.outcome() {
        Outcome::Verified => 0,
        Outcome::Unverified => 1,
        Outcome::Mismatched => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vayucell_core::image::{Manifest, Outcome};

    /// A host that answers with fixed text.
    struct Fake {
        files: Vec<(String, String)>,
    }

    impl Fake {
        fn new() -> Self {
            Self { files: Vec::new() }
        }
        fn with(mut self, path: &str, text: &str) -> Self {
            self.files.push((path.to_owned(), text.to_owned()));
            self
        }
    }

    impl Host for Fake {
        fn read(&self, path: &str) -> Option<String> {
            self.files
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, text)| text.clone())
        }
        fn exists(&self, path: &str) -> bool {
            self.files.iter().any(|(name, _)| name == path)
        }
        fn is_file(&self, _path: &str) -> bool {
            true
        }
        fn euid(&self) -> u32 {
            0
        }
        fn env(&self, _key: &str) -> Option<String> {
            None
        }
    }

    /// A manifest on disk, in a temporary directory.
    fn write_manifest(text: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "vayucell-image-test-{}-{:?}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::write(&path, text).expect("the manifest is written");
        path
    }

    #[test]
    fn a_manifest_that_is_not_a_manifest_is_refused() {
        // The defect: a file that is not a manifest being read as one, so the
        // check would report on a description nobody wrote.
        let path = write_manifest("this is not a manifest\n");
        let code = run(&Fake::new(), path.to_str().expect("a path"));
        let _ = std::fs::remove_file(&path);
        assert_eq!(code, 64);
    }

    #[test]
    fn a_manifest_that_cannot_be_read_is_refused() {
        // The defect: a missing manifest treated as an empty one, which
        // would check the machine against nothing and call it a match.
        let code = run(&Fake::new(), "/nonexistent/vayucell/manifest.txt");
        assert_eq!(code, 64);
    }

    #[test]
    fn a_machine_that_matches_the_manifest_says_so() {
        let manifest =
            Manifest::new("vayu.test", "1.0", "6.1.0", "vayu,board", 4200).expect("a manifest");
        let path = write_manifest(&manifest.render());
        let host = Fake::new()
            .with(image::OSRELEASE, "6.1.0\n")
            .with(
                &format!("{}/compatible", image::DEVICE_TREE_ROOTS[0]),
                "vayu,board\0vayu,soc\0",
            )
            .with(
                &format!(
                    "{}{}",
                    image::DEVICE_TREE_ROOTS[0],
                    image::MAX_CHARGE_VOLTAGE_PROPERTY
                ),
                "4200",
            );
        let code = run(&host, path.to_str().expect("a path"));
        let _ = std::fs::remove_file(&path);
        assert_eq!(code, 0);
    }

    #[test]
    fn a_machine_that_cannot_be_read_says_it_could_not_be_checked() {
        // The defect: a machine that answers nothing reported as a machine
        // that matched, which is the difference between a check and a
        // compliment.
        let manifest =
            Manifest::new("vayu.test", "1.0", "6.1.0", "vayu,board", 4200).expect("a manifest");
        let path = write_manifest(&manifest.render());
        let code = run(&Fake::new(), path.to_str().expect("a path"));
        let _ = std::fs::remove_file(&path);
        assert_eq!(code, 1);
    }

    #[test]
    fn a_machine_running_something_else_is_told_it_does_not_match() {
        // The defect: a machine running another kernel reported as matched,
        // because the comparison took a prefix.
        let manifest =
            Manifest::new("vayu.test", "1.0", "6.1.0", "vayu,board", 4200).expect("a manifest");
        let path = write_manifest(&manifest.render());
        let host = Fake::new()
            .with(image::OSRELEASE, "6.1.0-rc1\n")
            .with(
                &format!("{}/compatible", image::DEVICE_TREE_ROOTS[0]),
                "vayu,board\0",
            )
            .with(
                &format!(
                    "{}{}",
                    image::DEVICE_TREE_ROOTS[0],
                    image::MAX_CHARGE_VOLTAGE_PROPERTY
                ),
                "4200",
            );
        let code = run(&host, path.to_str().expect("a path"));
        let _ = std::fs::remove_file(&path);
        assert_eq!(code, 2);
    }

    #[test]
    fn the_outcomes_are_ordered_so_that_a_mismatch_outranks_a_question() {
        // The defect: a machine that answered one question wrongly and
        // another question not at all reported as merely unverified, so the
        // definite failure was hidden behind the open one.
        assert!(Outcome::Mismatched > Outcome::Unverified);
        assert!(Outcome::Unverified > Outcome::Verified);
    }
}
