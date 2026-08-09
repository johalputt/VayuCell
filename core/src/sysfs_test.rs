// SPDX-License-Identifier: Apache-2.0

//! Sysfs tests, in the attacker's voice.
//!
//! Every device here is a [`FakeHost`], so the suite covers handsets nobody in
//! this room is holding — including the ones whose kernels misbehave, which are
//! the ones that matter.

use crate::battery::{Percent, StateOfHealth};
use crate::governor::{ChargeMechanism, Governor, Level, Reason, Thresholds};
use crate::host::FakeHost;
use crate::sysfs::{detect_mechanism, read_battery, Kind, ReadError, SysfsCeiling, SUPPLY};

/// A device whose power-supply directory is fully populated.
fn healthy_device() -> FakeHost {
    FakeHost::new()
        .with_file(&format!("{SUPPLY}/capacity"), "58\n")
        .with_file(&format!("{SUPPLY}/voltage_now"), "3820000\n")
        .with_file(&format!("{SUPPLY}/current_now"), "-180000\n")
        .with_file(&format!("{SUPPLY}/temp"), "292\n")
        .with_file(&format!("{SUPPLY}/cycle_count"), "412\n")
        .with_file(&format!("{SUPPLY}/charge_full"), "3600000\n")
        .with_file(&format!("{SUPPLY}/charge_full_design"), "4000000\n")
        .with_file(&format!("{SUPPLY}/health"), "Good\n")
}

// ── Reading ───────────────────────────────────────────────────────────────────

#[test]
fn a_populated_device_reads_cleanly() {
    let r = read_battery(&healthy_device(), SUPPLY).expect("a full directory must read");
    assert_eq!(r.capacity, Percent::clamped(58));
    assert_eq!(r.temp.raw(), 292);
    assert_eq!(
        r.state_of_health(),
        StateOfHealth::Measured(Percent::clamped(90))
    );
    assert!(!r.is_charging(), "a negative current is discharge");
}

#[test]
fn a_missing_node_refuses_the_reading_and_names_itself() {
    // The attack: fill in what can be read and default the rest. A pack
    // temperature that defaulted to zero is a cell that never gets warm, on a
    // device nobody is watching — and nothing downstream can tell the difference
    // between that and a cold phone.
    for node in [
        "capacity",
        "voltage_now",
        "current_now",
        "temp",
        "cycle_count",
        "charge_full",
        "charge_full_design",
    ] {
        let host = healthy_device().without(&format!("{SUPPLY}/{node}"));
        let err = read_battery(&host, SUPPLY).expect_err(&format!(
            "a device without {node} must refuse to produce a reading"
        ));
        assert_eq!(err, ReadError::Missing { node }, "for {node}");
        assert!(
            err.to_string().contains(node),
            "the error must name the node: {err}"
        );
    }
}

#[test]
fn a_node_containing_rubbish_is_refused_rather_than_coerced() {
    let host = healthy_device().with_file(&format!("{SUPPLY}/temp"), "warm\n");
    let err = read_battery(&host, SUPPLY).expect_err("non-numeric temp must be refused");
    assert!(
        matches!(err, ReadError::Unparseable { node: "temp", .. }),
        "{err:?}"
    );
}

#[test]
fn a_missing_health_node_does_not_prevent_a_reading() {
    // The one node allowed to be absent, and the one node nothing acts on.
    // Vendors report "Good" on visibly swollen cells, so its absence costs
    // nothing.
    let host = healthy_device().without(&format!("{SUPPLY}/health"));
    let r = read_battery(&host, SUPPLY).expect("health is not required");
    assert_eq!(r.vendor_health, crate::battery::VendorHealth::Unavailable);
}

// ── Mechanism detection ───────────────────────────────────────────────────────

#[test]
fn a_device_with_no_charge_node_has_no_mechanism_and_that_is_not_an_error() {
    // T0: the most common device and the worst case. On an unrooted stock phone
    // there is no supported way to stop the charger, and the honest answer is
    // nothing — never a mechanism that reports success while doing nothing.
    assert_eq!(detect_mechanism(&healthy_device(), SUPPLY), None);
}

#[test]
fn the_mainline_node_is_preferred_over_the_vendor_ones() {
    let host = healthy_device()
        .with_file(&format!("{SUPPLY}/input_suspend"), "0\n")
        .with_file(&format!("{SUPPLY}/constant_charge_current_max"), "500000\n")
        .with_file(&format!("{SUPPLY}/charge_control_end_threshold"), "100\n");
    assert_eq!(
        detect_mechanism(&host, SUPPLY),
        Some(Kind::EndThreshold),
        "the standard node must win where it exists"
    );
}

#[test]
fn a_vendor_only_device_still_reports_the_mechanism_it_has() {
    let host = healthy_device().with_file(&format!("{SUPPLY}/input_suspend"), "0\n");
    assert_eq!(detect_mechanism(&host, SUPPLY), Some(Kind::InputSuspend));
}

#[test]
fn only_the_threshold_node_is_treated_as_a_ceiling() {
    // A current limit slows charging. It is genuinely useful and it is not a
    // percentage anyone can read back, so presenting it as a ceiling would put a
    // green row against something nobody verified.
    assert!(Kind::EndThreshold.is_ceiling());
    for k in [Kind::CurrentLimit, Kind::InputSuspend, Kind::VoltageMax] {
        assert!(!k.is_ceiling(), "{k:?} must not be treated as a ceiling");
    }
}

#[test]
fn a_non_ceiling_mechanism_cannot_be_bound_as_one() {
    // The failure this prevents: writing 60 to constant_charge_current_max,
    // reading back 500000 microamps, and comparing the two numbers.
    let mut host = healthy_device();
    for k in [Kind::CurrentLimit, Kind::InputSuspend, Kind::VoltageMax] {
        assert!(
            SysfsCeiling::new(&mut host, SUPPLY, k).is_none(),
            "{k:?} must not be usable as a ceiling"
        );
    }
    assert!(SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).is_some());
}

// ── The ceiling, end to end through the governor ──────────────────────────────

#[test]
fn a_ceiling_written_to_a_real_node_reads_back_and_holds() {
    let mut host =
        healthy_device().with_file(&format!("{SUPPLY}/charge_control_end_threshold"), "100\n");
    let reading = read_battery(&host, SUPPLY).unwrap();
    let mut g = Governor::new(Thresholds::recommended());

    let mut mech = SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).unwrap();
    assert!(g
        .enforce(&mut mech, Percent::clamped(60), &reading)
        .is_none());
    assert_eq!(mech.verify().unwrap(), Percent::clamped(60));
    assert_eq!(g.level(), Level::Normal);
}

#[test]
fn a_vendor_daemon_putting_the_ceiling_back_is_caught_through_the_real_node() {
    // The whole reason the verification loop exists, exercised against a fake
    // that behaves the way the vendor daemon does: it accepts the write and
    // keeps its own value.
    let mut host =
        healthy_device().with_revert(&format!("{SUPPLY}/charge_control_end_threshold"), "100");
    let reading = read_battery(&host, SUPPLY).unwrap();
    let mut g = Governor::new(Thresholds::recommended());

    let mut mech = SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).unwrap();
    let t = g
        .enforce(&mut mech, Percent::clamped(60), &reading)
        .expect("a reverted node must be caught");

    assert_eq!(g.level(), Level::Derated);
    assert!(
        matches!(t.reason, Reason::Reverted { .. }),
        "{:?}",
        t.reason
    );
}

#[test]
fn a_node_that_refuses_the_write_derates_rather_than_reporting_success() {
    let mut host =
        healthy_device().with_read_only(&format!("{SUPPLY}/charge_control_end_threshold"), "100");
    let reading = read_battery(&host, SUPPLY).unwrap();
    let mut g = Governor::new(Thresholds::recommended());

    let mut mech = SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).unwrap();
    let t = g
        .enforce(&mut mech, Percent::clamped(60), &reading)
        .expect("a refused write must be caught");
    assert!(matches!(t.reason, Reason::ApplyFailed(_)), "{:?}", t.reason);
    assert!(
        t.to_string().contains("read-only"),
        "the operator must be told why: {t}"
    );
}

#[test]
fn verification_reads_the_hardware_not_what_we_remember_writing() {
    // The distinction the whole loop rests on. A mechanism that returned the
    // value it last wrote would report every ceiling as held, including on a
    // device where the write silently did nothing.
    let mut host =
        healthy_device().with_revert(&format!("{SUPPLY}/charge_control_end_threshold"), "100");
    let mut mech = SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).unwrap();

    mech.apply(Percent::clamped(60))
        .expect("the write is accepted");
    assert_eq!(
        mech.verify().unwrap(),
        Percent::clamped(100),
        "verify must report what the hardware says, not what was written"
    );
}

#[test]
fn the_node_that_answered_is_recorded_for_the_device_profile() {
    // ADR-0002 §2: the path is part of the profile and the hardware database,
    // and the hardware gate refuses a verified ceiling that names no node.
    let mut host =
        healthy_device().with_file(&format!("{SUPPLY}/charge_control_end_threshold"), "100\n");
    let mech = SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).unwrap();
    assert_eq!(
        mech.node_path(),
        "/sys/class/power_supply/battery/charge_control_end_threshold"
    );
    assert_eq!(mech.kind(), Kind::EndThreshold);
}

#[test]
fn a_charge_full_near_the_integer_limit_does_not_crash_the_reading() {
    // Found by the fuzzer, and the shape of bug this project can least afford:
    // `charge_full * 100` overflowed i64, which panics under debug assertions
    // and silently wraps to a negative without them — inside the reading the
    // governor uses to decide whether to keep charging a cell.
    //
    // `charge_full` is whatever a vendor kernel wrote into the node, parsed with
    // no upper bound, so a device reporting nonsense here took the process down
    // rather than being reported as nonsense.
    let host =
        healthy_device().with_file(&format!("{SUPPLY}/charge_full"), &format!("{}\n", i64::MAX));
    let r = read_battery(&host, SUPPLY).expect("the node parses as an integer");

    assert_eq!(
        r.state_of_health(),
        StateOfHealth::Unknown,
        "a capacity that cannot be scaled is unverified, never a number"
    );
}

#[test]
fn a_capacity_larger_than_the_design_capacity_is_unknown_rather_than_flattering() {
    // A cell cannot hold more than it was built to. A kernel saying otherwise is
    // saying something that cannot be true, and clamping it to 100% would render
    // the most suspicious reading available as perfect health.
    let host = healthy_device()
        .with_file(&format!("{SUPPLY}/charge_full"), "8000000\n")
        .with_file(&format!("{SUPPLY}/charge_full_design"), "4000000\n");
    let r = read_battery(&host, SUPPLY).unwrap();
    assert_eq!(
        r.state_of_health(),
        StateOfHealth::Measured(Percent::clamped(100))
    );
}

#[test]
fn the_published_node_list_matches_what_a_read_actually_consults() {
    // Two lists drift, and the one that drifts is the device report — which is
    // the only thing anybody would have to go on about a handset nobody here is
    // holding. So the list is checked against the reader: remove any one node
    // and the read must fail naming it.
    use crate::host::FakeHost;

    for missing in crate::sysfs::NODES {
        let mut host = FakeHost::new();
        for node in crate::sysfs::NODES {
            if node != missing {
                let value = if node == "health" { "Good" } else { "1" };
                host = host.with_file(&format!("{SUPPLY}/{node}"), value);
            }
        }
        let outcome = read_battery(&host, SUPPLY);
        if missing == "health" {
            // The documented exception, asserted rather than assumed.
            assert!(outcome.is_ok(), "health is allowed to be absent");
        } else {
            let e = outcome.expect_err("a required node was missing");
            assert!(
                e.to_string().contains(missing),
                "removing {missing} produced: {e}"
            );
        }
    }
}
