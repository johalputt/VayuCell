// SPDX-License-Identifier: Apache-2.0

//! Tier detection tests, in the attacker's voice.
//!
//! Every case is a device described by a [`FakeHost`], so the suite runs
//! anywhere and covers handsets nobody here is holding.

use crate::capability::Tier;
use crate::host::FakeHost;
use crate::tier::{detect, Verdict, SHELL_ASSERTION_ENV};

/// The most common device in the world: stock Android, no root.
fn stock_android() -> FakeHost {
    FakeHost::new()
        .with_file("/system/build.prop", "ro.product.model=Example\n")
        .as_uid(10_234)
}

#[test]
fn a_machine_with_no_recognised_evidence_is_unknown_not_t0() {
    // The attack: treat "we are a userspace process, therefore T0" as a
    // baseline. That silently grants a tier to any machine the probe failed on,
    // including a developer's laptop and a CI runner.
    let d = detect(&FakeHost::new());
    assert_eq!(
        d.verdict,
        Verdict::Unknown,
        "an unrecognised machine must not be assigned a tier"
    );
    assert_eq!(d.verdict.tier(), None, "Unknown must yield no tier");
}

#[test]
fn stock_android_without_root_is_t0() {
    let d = detect(&stock_android());
    assert_eq!(d.verdict, Verdict::Established(Tier::T0));
}

#[test]
fn root_is_concluded_from_the_kernels_answer_not_from_a_missing_restriction() {
    let d = detect(&stock_android().as_root());
    assert_eq!(d.verdict, Verdict::Established(Tier::T1));

    let f = d
        .findings
        .iter()
        .find(|f| f.what == "root")
        .expect("root must be probed");
    assert!(f.found);
    assert!(
        f.evidence.contains('0'),
        "the evidence must be the uid itself, got {:?}",
        f.evidence
    );
}

#[test]
fn a_guest_that_cannot_see_the_phone_reports_unverified_rather_than_guessing() {
    // The honest limit: inside a virtual machine the real hardware is hidden,
    // so "virtualised" alone cannot mean "a phone". Concluding T2 here would be
    // a guess wearing a verdict's clothes.
    let vm = FakeHost::new().with_file("/sys/hypervisor/type", "kvm\n");
    let d = detect(&vm);

    match &d.verdict {
        Verdict::Unverified(why) => {
            assert!(
                why.contains(SHELL_ASSERTION_ENV),
                "the reason must say how to resolve it: {why}"
            );
        }
        other => panic!("a bare VM must be Unverified, got {other:?}"),
    }
    assert_eq!(d.verdict.tier(), None, "Unverified must yield no tier");
}

#[test]
fn a_guest_the_shell_vouches_for_is_t2() {
    let vm = FakeHost::new()
        .with_file("/sys/hypervisor/type", "kvm\n")
        .with_env(SHELL_ASSERTION_ENV, "android-guest");
    assert_eq!(detect(&vm).verdict, Verdict::Established(Tier::T2));
}

#[test]
fn an_unrecognised_shell_assertion_is_refused_rather_than_believed() {
    // The attack: set the variable to anything at all and be promoted to T2.
    let vm = FakeHost::new()
        .with_file("/sys/hypervisor/type", "kvm\n")
        .with_env(SHELL_ASSERTION_ENV, "yes-please");
    assert!(
        matches!(detect(&vm).verdict, Verdict::Unverified(_)),
        "an unrecognised assertion value must not promote the tier"
    );
}

#[test]
fn the_assertion_alone_cannot_manufacture_a_tier() {
    // The attack: export the variable on a machine that is not virtualised at
    // all and claim T2. The assertion resolves an ambiguity; it does not create
    // evidence.
    let host = FakeHost::new().with_env(SHELL_ASSERTION_ENV, "android-guest");
    assert_eq!(
        detect(&host).verdict,
        Verdict::Unknown,
        "an assertion with no virtualisation signal must establish nothing"
    );
}

#[test]
fn mainline_linux_on_mobile_silicon_is_t3() {
    let pmos = FakeHost::new()
        .with_file("/proc/device-tree/compatible", "qcom,msm8996\0qcom,mtp\0")
        .as_root();
    assert_eq!(detect(&pmos).verdict, Verdict::Established(Tier::T3));
}

#[test]
fn android_outranks_the_device_tree_so_a_rooted_handset_is_t1_not_t3() {
    // A real Android handset also has a device tree naming its chipset. The
    // presence of Android userspace is what distinguishes T1 from T3, and the
    // order must not be an accident of which probe ran first.
    let rooted = FakeHost::new()
        .with_file("/system/build.prop", "ro.product.model=Example\n")
        .with_file("/proc/device-tree/compatible", "qcom,sm8250\0")
        .as_root();
    assert_eq!(detect(&rooted).verdict, Verdict::Established(Tier::T1));
}

#[test]
fn an_unreadable_device_tree_is_not_read_as_absent_hardware() {
    // CHARTER Article IV: what cannot be checked is unverified, never clean.
    // A device tree we are not permitted to read must not be reported as
    // "no mobile hardware here".
    let host = FakeHost::new().with_unreadable("/proc/device-tree/compatible");
    let d = detect(&host);

    let f = d
        .findings
        .iter()
        .find(|f| f.what == "mobile hardware")
        .expect("mobile hardware must be probed");
    assert!(!f.found);
    assert!(
        f.evidence.contains("unreadable"),
        "the evidence must distinguish unreadable from absent, got {:?}",
        f.evidence
    );
}

#[test]
fn an_unreadable_device_tree_makes_the_verdict_unverified_not_unknown() {
    // The stronger half of the case above. Recording "unreadable" in a finding
    // while the verdict still says Unknown means the honesty stops at the log
    // and never reaches the decision. Unknown claims "nothing recognised this
    // machine"; here the one question that could have recognised it was locked,
    // which is a weaker claim and must be reported as the weaker one.
    let host = FakeHost::new().with_unreadable("/proc/device-tree/compatible");
    match detect(&host).verdict {
        Verdict::Unverified(why) => assert!(
            why.contains("could not be read"),
            "the reason must name the unread device tree: {why}"
        ),
        other => panic!("an unreadable device tree must be Unverified, got {other:?}"),
    }
}

#[test]
fn a_readable_device_tree_naming_nothing_mobile_is_unknown_not_unverified() {
    // The corollary, so the rule above cannot be satisfied by making everything
    // Unverified. A device tree we DID read, naming a desktop chipset, is a real
    // answer: this is not a handset. That must stay Unknown.
    let laptop = FakeHost::new().with_file("/proc/device-tree/compatible", "intel,x86\0");
    assert_eq!(
        detect(&laptop).verdict,
        Verdict::Unknown,
        "a device tree that was read and names nothing mobile is an answer, not a gap"
    );
}

#[test]
fn every_probe_records_a_finding_even_when_it_finds_nothing() {
    // A layer that silently checks nothing looks exactly like one that is
    // working. Each probe must leave a trace either way.
    let d = detect(&FakeHost::new());
    for what in [
        "android userspace",
        "root",
        "virtualised",
        "mobile hardware",
        "shell assertion",
    ] {
        assert!(
            d.findings.iter().any(|f| f.what == what),
            "{what} left no finding; a silent probe is indistinguishable from a working one"
        );
    }
    assert!(
        d.findings.iter().all(|f| !f.evidence.is_empty()),
        "every finding needs evidence"
    );
}

#[test]
fn android_is_detected_from_several_independent_markers() {
    // No single file is load-bearing: a device that lacks build.prop but has a
    // recognisable /proc/version is still Android.
    for host in [
        FakeHost::new().with_file("/system/bin/getprop", ""),
        FakeHost::new().with_file("/init.environ.rc", ""),
        FakeHost::new().with_file("/proc/version", "Linux version 5.10 (Android clang)"),
        FakeHost::new().with_env("ANDROID_DATA", "/data"),
    ] {
        assert_eq!(
            detect(&host).verdict,
            Verdict::Established(Tier::T0),
            "each Android marker alone should establish T0"
        );
    }
}
