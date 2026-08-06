// SPDX-License-Identifier: Apache-2.0

//! Tests in the attacker's voice, with the consequence in the name.
//!
//! Note what is NOT here, and why that is the point of ADR-0005: there is no
//! test for "a capability with no verify is refused", because such a capability
//! cannot be written down. That proof lives as a `compile_fail` doctest on the
//! `capability` module, where rustdoc actually collects and runs it.

// These helpers must match the DetectFn/VerifyFn/ApplyFn signatures exactly, so
// their Result wrappers are load-bearing even though a linter sees them as
// unnecessary for these particular bodies.
#![allow(clippy::unnecessary_wraps)]

use super::capability::*;

fn ok_detect() -> Result<Observation, ProbeError> {
    Ok(Observation::new(Result_::Present, "test"))
}
fn ok_verify() -> Result<Observation, ProbeError> {
    Ok(Observation::new(Result_::Present, "test"))
}
fn ok_apply(_: &str) -> Result<(), ProbeError> {
    Ok(())
}

/// A capability that passes `check`, so each test can break exactly one thing.
fn valid() -> Capability {
    Capability {
        id: "test.capability",
        floor: Tier::T1,
        class: Class::Serving,
        detect: ok_detect,
        apply: Some(ok_apply),
        verify: ok_verify,
        on_absent: Disposition::Refuse,
        rationale: "exists to exercise the registry",
    }
}

#[test]
fn safety_capability_cannot_degrade_quietly() {
    // The attack: register the battery charge ceiling with Degrade so a device
    // that cannot limit charging keeps serving behind a soft warning nobody
    // reads. CHARTER Article III forbids it.
    let mut c = valid();
    c.class = Class::Safety;
    c.on_absent = Disposition::Degrade;

    let errs = c
        .check()
        .expect_err("a degrading safety capability must be refused");
    assert!(errs.contains(&Invalid::SafetyDegrades), "got {errs:?}");

    let mut r = Registry::new();
    assert!(
        r.register(c).is_err(),
        "registry accepted a silently-degrading safety capability"
    );
}

#[test]
fn safety_capability_may_refuse() {
    // The corollary: refusing IS allowed, or the rule above would make safety
    // capabilities unregisterable rather than strict.
    let mut c = valid();
    c.class = Class::Safety;
    c.on_absent = Disposition::Refuse;
    c.check()
        .expect("a refusing safety capability must be registerable");
}

#[test]
fn only_observe_capabilities_may_omit_apply() {
    // A controlling capability with no apply is a report wearing a control's
    // clothes.
    let mut c = valid();
    c.apply = None;
    let errs = c.check().expect_err("a controlling capability needs apply");
    assert!(errs.contains(&Invalid::NoApply), "got {errs:?}");

    c.class = Class::Observe;
    c.check().expect("an observe capability may omit apply");
}

#[test]
fn blank_identifiers_and_rationales_are_refused() {
    for id in ["", "   "] {
        let mut c = valid();
        c.id = id;
        assert!(c.check().expect_err("blank id").contains(&Invalid::NoId));
    }
    for r in ["", "  "] {
        let mut c = valid();
        c.rationale = r;
        assert!(c
            .check()
            .expect_err("blank rationale")
            .contains(&Invalid::NoRationale));
    }
}

#[test]
fn an_undetected_tier_satisfies_nothing() {
    // The attack: a device nobody could probe gets treated as satisfying a T0
    // floor, silently granting capabilities to unknown hardware. ADR-0001 §2:
    // detection is positive evidence only.
    assert!(
        !tier_satisfies(None, Tier::T0),
        "an undetected tier must satisfy nothing"
    );
    assert!(!tier_satisfies(None, Tier::T3));
    assert!(
        tier_satisfies(Some(Tier::T2), Tier::T1),
        "a higher tier satisfies a lower floor"
    );
    assert!(
        tier_satisfies(Some(Tier::T1), Tier::T1),
        "a tier satisfies its own floor"
    );
    assert!(
        !tier_satisfies(Some(Tier::T0), Tier::T3),
        "a lower tier must not satisfy a higher floor"
    );
}

#[test]
fn unverified_is_never_absent_and_absence_is_never_protection() {
    // CHARTER Article IV. These are distinct values that must never collapse.
    assert_ne!(Result_::Unverified, Result_::Absent);
    assert_ne!(Result_::Unverified, Result_::Present);
    assert_eq!(Result_::Unverified.to_string(), "unverified");
    assert_eq!(Result_::Absent.to_string(), "absent");
}

#[test]
fn an_observation_carries_its_evidence() {
    // A bare verdict cannot be audited.
    let o = Observation::new(
        Result_::Absent,
        "/sys/class/power_supply/battery: no such file",
    );
    assert!(
        !o.evidence.is_empty(),
        "an observation must say what was looked for"
    );
}

#[test]
fn registry_refuses_duplicates() {
    let mut r = Registry::new();
    r.register(valid())
        .expect("first registration should succeed");
    let errs = r.register(valid()).expect_err("duplicate must be refused");
    assert!(errs.contains(&Invalid::Duplicate), "got {errs:?}");
    assert_eq!(r.len(), 1, "duplicate must not be stored");
}

#[test]
fn available_at_reports_nothing_for_an_undetected_device() {
    let mut r = Registry::new();
    let mut low = valid();
    low.id = "low";
    low.floor = Tier::T0;
    r.register(low).unwrap();

    let mut high = valid();
    high.id = "high";
    high.floor = Tier::T3;
    r.register(high).unwrap();

    assert_eq!(
        r.available_at(None).len(),
        0,
        "an undetected device gets nothing"
    );
    assert_eq!(r.available_at(Some(Tier::T0)), vec!["low"]);
    assert_eq!(r.available_at(Some(Tier::T3)).len(), 2);
}

#[test]
fn ids_are_ordered_so_reports_are_deterministic() {
    let mut r = Registry::new();
    for id in ["z.last", "a.first", "m.middle"] {
        let mut c = valid();
        c.id = id;
        r.register(c).unwrap();
    }
    assert_eq!(r.ids(), vec!["a.first", "m.middle", "z.last"]);
}
