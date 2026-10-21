// SPDX-License-Identifier: Apache-2.0

//! Tests for the tier-3 image manifest, and for the check of a running
//! system against it.
//!
//! The property under test throughout is the one PLAN §11 puts on this phase,
//! stated as a software property: **"this device is running a verified image"
//! has to be something the software can check and report**, and what it
//! reports must never be better than what it actually looked at.
//!
//! Every device here is a [`FakeHost`]. That is deliberate: the images this
//! module checks do not exist yet, so a test that needed real hardware could
//! not be written at all, and a check that cannot be exercised cannot be
//! trusted to be honest.

use crate::host::FakeHost;
use crate::image::{
    check, volts, Finding, ImageError, Manifest, Outcome, Report, Status, Subject,
    CHARGE_TOLERANCE_MV, COMPATIBLE_PROPERTY, DEVICE_TREE_ROOTS, MAX_CHARGE_VOLTAGE_PROPERTY,
    OSRELEASE,
};

/// The manifest header this build writes.
///
/// Written out here rather than imported, because the thing under test is
/// what a file *looks like* on disk. A test that borrowed the constant would
/// keep passing if the constant were wrong, and a manifest nobody can read is
/// a manifest nobody can check.
const HEADER: &str = "vayucell image manifest 1";

/// The kernel release the test image expects to be running.
const KERNEL: &str = "6.9.0-1-postmarketos-trinity";

/// The board the test image targets.
const BOARD: &str = "samsung,exynos2100";

/// The charge ceiling the test image declares, in millivolts.
const CEILING_MV: u32 = 3800;

/// A manifest this build accepts.
fn manifest() -> Manifest {
    Manifest::new("postmarketOS-trinity", "2024.06", KERNEL, BOARD, CEILING_MV)
        .expect("a manifest this build accepts")
}

/// The path of a device-tree property under the first device-tree root.
fn tree_path(property: &str) -> String {
    format!("{}{}", DEVICE_TREE_ROOTS[0], property)
}

/// The path of a device-tree property under the second device-tree root.
fn sysfs_tree_path(property: &str) -> String {
    format!("{}{}", DEVICE_TREE_ROOTS[1], property)
}

/// A machine running the kernel the manifest expects, and nothing else.
fn machine_with_kernel(release: &str) -> FakeHost {
    FakeHost::new().with_file(OSRELEASE, release)
}

/// A machine running exactly what the manifest describes.
fn verified_machine() -> FakeHost {
    machine_with_kernel("6.9.0-1-postmarketos-trinity\n")
        .with_file(
            &tree_path(COMPATIBLE_PROPERTY),
            "samsung,exynos2100\0samsung,exynos2100-dtb\0",
        )
        .with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "3800\n")
}

/// The status a report recorded for one subject.
fn status_of(report: &Report, subject: Subject) -> Status {
    report
        .finding(subject)
        .expect("a finding for every subject")
        .status()
}

#[test]
fn a_manifest_round_trips_through_the_text_form_it_renders() {
    // The manifest is a file on disk, and a manifest that cannot be read back
    // is a manifest the operator cannot check. Round-tripping is the only
    // thing that keeps the rendered form and the parsed form the same
    // contract instead of two contracts that happen to agree today.
    let original = manifest();
    let text = original.render();
    let read = Manifest::parse(&text).expect("a manifest this build wrote");
    assert_eq!(read, original);
    assert_eq!(read.render(), text);
}

#[test]
fn an_unknown_key_is_refused_rather_than_ignored() {
    // A key this build does not know is a key whose value this build would
    // not know how to check. Ignoring it would let anything be smuggled past
    // a rule that was never taught, and the manifest is the one file that
    // decides what "verified" means.
    let text = format!(
        "{HEADER}\nname postmarketOS-trinity\nversion 2024.06\nkernel {KERNEL}\n\
         compatible {BOARD}\nmax-charge-mv 3800\ncolour blue\n"
    );
    assert!(matches!(
        Manifest::parse(&text),
        Err(ImageError::UnknownKey { .. })
    ));
}

#[test]
fn a_key_given_twice_is_refused() {
    // A manifest that says the same thing twice has said nothing, and
    // resolving it would be this program deciding which of the two the author
    // meant — including when the two happen to agree, because a file that
    // disagrees with itself is not a file anybody should read a ceiling out
    // of.
    let text = format!(
        "{HEADER}\nname postmarketOS-trinity\nname something-else\nversion 2024.06\n\
         kernel {KERNEL}\ncompatible {BOARD}\nmax-charge-mv 3800\n"
    );
    assert!(matches!(
        Manifest::parse(&text),
        Err(ImageError::DuplicateKey { .. })
    ));
}

#[test]
fn a_manifest_missing_a_required_key_is_refused() {
    // A missing key is not a default. "This image targets no board" and "this
    // file forgot to say" are different claims, and only the second one is
    // true — so the file is refused rather than read as a manifest that
    // checks less.
    let text = format!(
        "{HEADER}\nname postmarketOS-trinity\nversion 2024.06\nkernel {KERNEL}\n\
         max-charge-mv 3800\n"
    );
    assert!(matches!(
        Manifest::parse(&text),
        Err(ImageError::MissingKey { .. })
    ));
}

#[test]
fn a_manifest_with_a_zero_maximum_charge_voltage_is_refused() {
    // Zero is not a very low ceiling. It is *no ceiling at all*, and it must
    // not be constructible by accident — a manifest that carried one would be
    // a manifest saying this image holds the cell at nothing, which is the
    // opposite of what a T3 image does.
    assert!(matches!(
        Manifest::new("postmarketOS-trinity", "2024.06", KERNEL, BOARD, 0),
        Err(ImageError::NoCeiling)
    ));
    let text = format!(
        "{HEADER}\nname postmarketOS-trinity\nversion 2024.06\nkernel {KERNEL}\n\
         compatible {BOARD}\nmax-charge-mv 0\n"
    );
    assert!(matches!(Manifest::parse(&text), Err(ImageError::NoCeiling)));
}

#[test]
fn a_manifest_with_no_name_is_refused() {
    // A manifest that names nothing is not a description of an image, and a
    // report that could not say which image it checked would be a report
    // nobody could act on.
    assert!(matches!(
        Manifest::new("", "2024.06", KERNEL, BOARD, CEILING_MV),
        Err(ImageError::Blank { .. })
    ));
}

#[test]
fn a_manifest_with_no_version_is_refused() {
    // The version is how an operator tells the image they are running from
    // the one they meant to run. Without it, "this is the verified image" has
    // no referent.
    assert!(matches!(
        Manifest::new("postmarketOS-trinity", "", KERNEL, BOARD, CEILING_MV),
        Err(ImageError::Blank { .. })
    ));
}

#[test]
fn a_manifest_that_expects_no_kernel_or_targets_no_board_is_refused() {
    // Two more holes in the same claim. A manifest with no kernel release
    // would match nothing, and one with no board would match every board and
    // so verify nothing at all — the kind of check that passes because it
    // cannot fail.
    assert!(matches!(
        Manifest::new("postmarketOS-trinity", "2024.06", "", BOARD, CEILING_MV),
        Err(ImageError::Blank { .. })
    ));
    assert!(matches!(
        Manifest::new("postmarketOS-trinity", "2024.06", KERNEL, "", CEILING_MV),
        Err(ImageError::Blank { .. })
    ));
}

#[test]
fn a_file_that_is_not_a_manifest_is_an_error_and_not_an_empty_manifest() {
    // The defect this forecloses: a manifest reader that returns a default
    // manifest for text it does not understand. That turns a corrupted file,
    // a wrong file, or an empty file into "this image declares nothing" —
    // which is a claim about a cell, made by a file that was never written by
    // anybody who meant it.
    assert!(matches!(
        Manifest::parse("not a manifest at all"),
        Err(ImageError::NotAManifest { .. })
    ));
    assert!(matches!(
        Manifest::parse(""),
        Err(ImageError::NotAManifest { .. })
    ));
    assert!(matches!(
        Manifest::parse(&format!("{HEADER}\n")),
        Err(ImageError::MissingKey { .. })
    ));
}

#[test]
fn a_value_carrying_words_this_build_does_not_know_is_refused() {
    // "max-charge-mv 3800 volts" is not a number, and reading it as one
    // would be a guess. The same rule catches a value that has picked up a
    // trailing word because something appended to the file.
    let text = format!(
        "{HEADER}\nname postmarketOS-trinity\nversion 2024.06\nkernel {KERNEL}\n\
         compatible {BOARD}\nmax-charge-mv 3800 volts\n"
    );
    assert!(matches!(
        Manifest::parse(&text),
        Err(ImageError::ExtraWords { .. })
    ));
}

#[test]
fn a_running_kernel_that_matches_the_manifest_matches() {
    // The ordinary case. A check that could not report a match would have no
    // reason to exist, and one that reported a match for the wrong kernel
    // would be worse than none.
    let report = check(&verified_machine(), &manifest());
    assert_eq!(status_of(&report, Subject::Kernel), Status::Matched);
    assert_eq!(report.outcome(), Outcome::Verified);
}

#[test]
fn a_running_kernel_that_does_not_match_is_a_mismatch_that_names_both() {
    // The defect this forecloses: a mismatch that reports only what the
    // machine said, leaving the operator to guess which image was expected.
    // Both versions are in the evidence, because the operator's next action
    // depends on which of the two is wrong.
    let host = machine_with_kernel("5.15.0-generic\n")
        .with_file(
            &tree_path(COMPATIBLE_PROPERTY),
            "samsung,exynos2100\0samsung,exynos2100-dtb\0",
        )
        .with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "3800\n");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::Kernel), Status::Differed);
    assert_eq!(report.outcome(), Outcome::Mismatched);
    let evidence = report
        .finding(Subject::Kernel)
        .expect("a kernel finding")
        .evidence()
        .to_owned();
    assert!(evidence.contains("5.15.0-generic"), "{evidence}");
    assert!(evidence.contains(KERNEL), "{evidence}");
}

#[test]
fn a_kernel_that_merely_starts_with_the_manifest_kernel_is_not_that_kernel() {
    // The defect: a prefix comparison, which would accept a kernel this image
    // was never built against — the "-dirty" build of the right kernel, or a
    // vendor kernel carrying the same version string and nothing else. The
    // manifest names the whole release string precisely so that this cannot
    // pass, and a check that compared only the version number would say
    // "verified" about a kernel that is not the one anybody checked.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity-dirty\n");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::Kernel), Status::Differed);
    assert_eq!(report.outcome(), Outcome::Mismatched);
    let evidence = report
        .finding(Subject::Kernel)
        .expect("a kernel finding")
        .evidence()
        .to_owned();
    assert!(evidence.contains("dirty"), "{evidence}");
}

#[test]
fn an_absent_kernel_node_is_unverified_rather_than_a_mismatch() {
    // "We could not look" is not "it is different". A machine that does not
    // publish its release string has not been shown to be the wrong image; it
    // has been shown to be a machine this check cannot speak about.
    let report = check(&FakeHost::new(), &manifest());
    assert_eq!(status_of(&report, Subject::Kernel), Status::Absent);
    assert_eq!(
        status_of(&report, Subject::Kernel).outcome(),
        Outcome::Unverified
    );
}

#[test]
fn an_absent_device_tree_is_unverified_rather_than_a_mismatch() {
    // The case PLAN §3 makes a T3 machine distinct by. There is no device
    // tree here, so there is no T3 image running here — but the check did not
    // find a *different* image either, and reporting a mismatch would be
    // accusing a machine of being something it never claimed to be.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity\n");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Absent);
    assert_eq!(report.outcome(), Outcome::Unverified);
}

#[test]
fn an_unreadable_device_tree_is_unverified_and_says_so_differently_from_absent() {
    // The two are never collapsed. A device tree this process may not open
    // is a device tree that exists, and the difference matters: one says
    // "this is not a T3 machine", the other says "this is a T3 machine whose
    // device tree I was not allowed to read". Both are unverified; they are
    // not the same unverified.
    let unreadable = machine_with_kernel("6.9.0-1-postmarketos-trinity\n")
        .with_unreadable(&tree_path(COMPATIBLE_PROPERTY));
    let absent = machine_with_kernel("6.9.0-1-postmarketos-trinity\n");

    let unreadable_report = check(&unreadable, &manifest());
    let absent_report = check(&absent, &manifest());

    assert_eq!(
        status_of(&unreadable_report, Subject::DeviceTree),
        Status::Unreadable
    );
    assert_eq!(
        status_of(&absent_report, Subject::DeviceTree),
        Status::Absent
    );
    assert_eq!(unreadable_report.outcome(), Outcome::Unverified);
    assert_eq!(absent_report.outcome(), Outcome::Unverified);

    let unreadable_evidence = unreadable_report
        .finding(Subject::DeviceTree)
        .expect("a device tree finding")
        .evidence()
        .to_owned();
    let absent_evidence = absent_report
        .finding(Subject::DeviceTree)
        .expect("a device tree finding")
        .evidence()
        .to_owned();
    assert_ne!(
        unreadable_evidence, absent_evidence,
        "an unreadable device tree and an absent one must not read the same"
    );
}

#[test]
fn a_device_tree_that_names_the_manifest_board_matches() {
    // The T3 check proper: the device tree names the board this image was
    // built for, so the running system is the image this manifest describes.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity\n").with_file(
        &tree_path(COMPATIBLE_PROPERTY),
        "samsung,exynos2100\0samsung,exynos2100-dtb\0",
    );
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Matched);
}

#[test]
fn a_device_tree_that_names_another_board_is_a_mismatch() {
    // A mainline image for one board does not run correctly on another, and
    // a device tree that names a different board is a definite fact about
    // this machine, not a gap in what we know.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity\n")
        .with_file(&tree_path(COMPATIBLE_PROPERTY), "qcom,snapdragon845\0");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Differed);
    assert_eq!(report.outcome(), Outcome::Mismatched);
    let evidence = report
        .finding(Subject::DeviceTree)
        .expect("a device tree finding")
        .evidence()
        .to_owned();
    assert!(evidence.contains("qcom,snapdragon845"), "{evidence}");
}

#[test]
fn the_nul_separated_compatible_property_is_read_as_a_list() {
    // On Linux this property holds several strings, most specific first, each
    // terminated by a NUL. A check that compared the whole file against one
    // string would never match any real device tree, and a check that
    // compared only the first string would miss an image built for a
    // compatible that is not the most specific one.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity\n").with_file(
        &tree_path(COMPATIBLE_PROPERTY),
        "samsung,exynos2100-dtb\0samsung,exynos2100\0",
    );
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Matched);
}

#[test]
fn a_device_tree_published_only_under_the_sysfs_root_is_still_read() {
    // Which root holds the flattened device tree depends on the kernel and
    // on the mount. A check that read only one of them would report a T3
    // machine as having no device tree at all, which is a mismatch-shaped
    // answer to a question about where the tree happens to live.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity\n").with_file(
        &sysfs_tree_path(COMPATIBLE_PROPERTY),
        "samsung,exynos2100\0",
    );
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Matched);
}

#[test]
fn an_unreadable_first_root_does_not_stop_the_second_from_answering() {
    // A machine that publishes the tree in two places and opens one of them
    // is still a machine that publishes it. The check reads what it can read
    // and says what it could not.
    let host = machine_with_kernel("6.9.0-1-postmarketos-trinity\n")
        .with_unreadable(&tree_path(COMPATIBLE_PROPERTY))
        .with_file(
            &sysfs_tree_path(COMPATIBLE_PROPERTY),
            "samsung,exynos2100\0",
        );
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Matched);
}

#[test]
fn a_charge_voltage_inside_the_tolerance_matches() {
    // The kernel rounds what it publishes, so a device tree that says 3800
    // mV may read back a few millivolts away. A check that failed on that
    // would report a disagreement that is not one, and would train its reader
    // to ignore it.
    let host = verified_machine().with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "3820\n");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::ChargeVoltage), Status::Matched);
    assert_eq!(report.outcome(), Outcome::Verified);
}

#[test]
fn a_charge_voltage_outside_the_tolerance_is_a_mismatch() {
    // The other side of the same rule. A ceiling at 4.4 V is not 3.8 V, and
    // the whole point of a T3 image is that the cell is not held full — so a
    // device tree publishing the vendor's maximum is a definite disagreement
    // with the manifest, and it must be reported as one.
    let host = verified_machine().with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "4400\n");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::ChargeVoltage), Status::Differed);
    assert_eq!(report.outcome(), Outcome::Mismatched);
    let evidence = report
        .finding(Subject::ChargeVoltage)
        .expect("a charge voltage finding")
        .evidence()
        .to_owned();
    assert!(evidence.contains("4.400 V"), "{evidence}");
    assert!(evidence.contains("3.800 V"), "{evidence}");
}

#[test]
fn the_tolerance_is_exactly_the_named_one_and_nothing_wider() {
    // The boundary, pinned with literals rather than with the constant, so
    // that widening the tolerance cannot silently widen what passes. The
    // constant is used to say *which* boundary is meant, and the literals
    // decide whether it holds.
    let inside = verified_machine().with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "3825\n");
    let outside = verified_machine().with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "3826\n");
    assert_eq!(
        status_of(&check(&inside, &manifest()), Subject::ChargeVoltage),
        Status::Matched
    );
    assert_eq!(
        status_of(&check(&outside, &manifest()), Subject::ChargeVoltage),
        Status::Differed
    );
    assert_eq!(CHARGE_TOLERANCE_MV, 25);
}

#[test]
fn a_charge_voltage_node_that_is_not_a_number_is_unreadable_and_not_a_mismatch() {
    // The node was there and it was read, but it does not say what it is
    // supposed to say. Calling that a mismatch would be a false accusation
    // against the image; calling it a match would be a claim nobody made.
    // The honest answer is that the ceiling in force could not be read.
    let host =
        verified_machine().with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "not-a-voltage");
    let report = check(&host, &manifest());
    assert_eq!(
        status_of(&report, Subject::ChargeVoltage),
        Status::Unreadable
    );
    assert_eq!(report.outcome(), Outcome::Unverified);
}

#[test]
fn the_headline_is_computed_from_the_findings() {
    // The defect this forecloses: a headline that could be set independently
    // of the rows. It would eventually disagree with them, in the reassuring
    // direction, because that is the direction nobody files a bug about. So
    // the headline here is only ever the worst of the findings, and this test
    // says so for every combination of them.
    let verified = check(&verified_machine(), &manifest());
    assert_eq!(verified.outcome(), Outcome::Verified);

    let mismatched = check(
        &verified_machine().with_file(&tree_path(MAX_CHARGE_VOLTAGE_PROPERTY), "4400\n"),
        &manifest(),
    );
    assert_eq!(mismatched.outcome(), Outcome::Mismatched);

    let unverified = check(&FakeHost::new(), &manifest());
    assert_eq!(unverified.outcome(), Outcome::Unverified);

    // And in every case the headline is exactly the worst of the findings,
    // not a number anybody chose. The rule is restated here from the statuses
    // rather than copied from the implementation, so that a change to the
    // implementation that made the headline say something else would fail
    // this test rather than agree with it.
    for report in [&verified, &mismatched, &unverified] {
        let computed = if report
            .findings()
            .iter()
            .any(|f| f.status() == Status::Differed)
        {
            Outcome::Mismatched
        } else if report
            .findings()
            .iter()
            .any(|f| matches!(f.status(), Status::Absent | Status::Unreadable))
        {
            Outcome::Unverified
        } else {
            Outcome::Verified
        };
        assert_eq!(report.outcome(), computed);
    }
}

#[test]
fn a_machine_that_answers_nothing_is_not_a_machine_that_passed() {
    // The default this module refuses to have. A report with nothing in it
    // and no all-good variant to fall into on: "unverified" is the only
    // honest answer a silent machine can get, and the headline says so
    // instead of staying blank.
    let report = check(&FakeHost::new(), &manifest());
    assert_ne!(report.outcome(), Outcome::Verified);
    assert_eq!(report.outcome(), Outcome::Unverified);
    assert_eq!(report.findings().len(), 3);
}

#[test]
fn a_mismatch_outranks_an_unanswered_question_in_the_headline() {
    // A definite disagreement with the manifest is a fact about this
    // machine; an unanswered check is a gap in what we know. The fact is the
    // more alarming of the two, and a headline that hid it behind an
    // "unverified" would understate what we already know.
    let host = machine_with_kernel("5.15.0-generic\n");
    let report = check(&host, &manifest());
    assert_eq!(status_of(&report, Subject::Kernel), Status::Differed);
    assert_eq!(status_of(&report, Subject::DeviceTree), Status::Absent);
    assert_eq!(report.outcome(), Outcome::Mismatched);
}

#[test]
fn every_subject_is_checked_and_reported() {
    // A check nobody performed is a check nobody can rely on, so the report
    // always carries all three subjects — including on a machine where two
    // of them could not be performed at all.
    let report = check(&FakeHost::new(), &manifest());
    let subjects: Vec<Subject> = report.findings().iter().map(Finding::subject).collect();
    assert_eq!(subjects, Subject::ALL.to_vec());
    for subject in Subject::ALL {
        let finding = report
            .finding(subject)
            .expect("a finding for every subject");
        assert!(
            !finding.evidence().is_empty(),
            "{subject:?} was reported without evidence"
        );
    }
}

#[test]
fn the_charge_voltage_is_reported_in_volts_to_three_decimals() {
    // Volts, because that is the unit a ceiling is talked about in, and
    // three decimals, because "3.8" would hide the difference between a
    // rounding and a change.
    assert_eq!(volts(3800), "3.800 V");
    assert_eq!(volts(4420), "4.420 V");
    assert_eq!(volts(0), "0.000 V");
}

#[test]
fn the_report_names_the_image_it_was_made_against() {
    // A report that cannot say which image it checked leaves the operator
    // with a verdict and nothing to act on. The name and the version come
    // from the manifest, so they cannot drift from what was checked.
    let report = check(&verified_machine(), &manifest());
    assert_eq!(report.manifest().name(), "postmarketOS-trinity");
    assert_eq!(report.manifest().version(), "2024.06");
    let rendered = report.render();
    assert!(rendered.contains("postmarketOS-trinity"), "{rendered}");
    assert!(rendered.contains("VERIFIED"), "{rendered}");
}
