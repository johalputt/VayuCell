// SPDX-License-Identifier: Apache-2.0

//! Governor tests, in the attacker's voice.
//!
//! ADR-0002 §Test-gates lists these, and every one is mutation-tested — the
//! defence is re-broken and the test confirmed to go red again.
//!
//! Note what is NOT here: there is no test for "the level cannot be lowered by a
//! method call", because no such method exists. That proof is a `compile_fail`
//! doctest on the `governor` module, where rustdoc collects and runs it.

use crate::battery::{Celsius, DeciCelsius, Percent, Reading, StateOfHealth, VendorHealth};
use crate::governor::{
    ChargeMechanism, Governor, Inspection, Level, Reason, ThresholdError, Thresholds,
};

/// A healthy cell at room temperature.
fn nominal() -> Reading {
    Reading {
        capacity: Percent::clamped(58),
        voltage_now_uv: 3_820_000,
        current_now_ua: -180_000,
        temp: DeciCelsius::from_sysfs(292),
        cycle_count: 412,
        charge_full_uah: 3_600_000,
        charge_full_design_uah: 4_000_000,
        vendor_health: VendorHealth::Reported("Good"),
    }
}

fn at_temp(deci: i32) -> Reading {
    Reading {
        temp: DeciCelsius::from_sysfs(deci),
        ..nominal()
    }
}

fn at_health(full: i64, design: i64) -> Reading {
    Reading {
        charge_full_uah: full,
        charge_full_design_uah: design,
        ..nominal()
    }
}

/// A mechanism that does exactly what it is told.
struct Working {
    held: Percent,
}
impl ChargeMechanism for Working {
    fn apply(&mut self, ceiling: Percent) -> Result<(), String> {
        self.held = ceiling;
        Ok(())
    }
    fn verify(&self) -> Result<Percent, String> {
        Ok(self.held)
    }
}

/// A vendor daemon that quietly puts the ceiling back.
struct Reverting;
impl ChargeMechanism for Reverting {
    fn apply(&mut self, _ceiling: Percent) -> Result<(), String> {
        Ok(())
    }
    fn verify(&self) -> Result<Percent, String> {
        Ok(Percent::clamped(100))
    }
}

/// A node that accepts writes and refuses reads.
struct Unreadable;
impl ChargeMechanism for Unreadable {
    fn apply(&mut self, _ceiling: Percent) -> Result<(), String> {
        Ok(())
    }
    fn verify(&self) -> Result<Percent, String> {
        Err("permission denied".into())
    }
}

/// A device-tree limit set below the runtime request: the write is accepted and
/// changes nothing, because the real ceiling lives below userspace. ADR-0002's
/// preferred T3 configuration.
struct DeviceTreeLimited(Percent);
impl ChargeMechanism for DeviceTreeLimited {
    fn apply(&mut self, _ceiling: Percent) -> Result<(), String> {
        Ok(())
    }
    fn verify(&self) -> Result<Percent, String> {
        Ok(self.0)
    }
}

/// A node that refuses the write outright.
struct Refusing;
impl ChargeMechanism for Refusing {
    fn apply(&mut self, _ceiling: Percent) -> Result<(), String> {
        Err("read-only file system".into())
    }
    fn verify(&self) -> Result<Percent, String> {
        Ok(Percent::clamped(60))
    }
}

fn governor() -> Governor {
    Governor::new(Thresholds::recommended())
}

// ── The verification loop ─────────────────────────────────────────────────────

#[test]
fn a_ceiling_that_was_quietly_reverted_is_detected() {
    // The failure this whole loop exists for. A vendor charging daemon resets
    // the node on a cable event; the ceiling held for six weeks and then
    // stopped. From the operator's side that is indistinguishable from a
    // governor that never worked, and a one-shot writer cannot see it at all.
    let mut g = governor();
    let t = g
        .enforce(&mut Reverting, Percent::clamped(60), &nominal())
        .expect("a reverted ceiling must produce a transition");

    assert_eq!(g.level(), Level::Derated);
    assert!(
        matches!(t.reason, Reason::Reverted { .. }),
        "{:?}",
        t.reason
    );
    assert!(
        t.to_string().contains("reverted it"),
        "the operator must be told what happened: {t}"
    );
}

#[test]
fn a_ceiling_that_cannot_be_read_back_is_unverified_never_working() {
    // CHARTER Article IV.3 and IV.5. A control that cannot be read back may not
    // be reported at all, because it is indistinguishable from one that silently
    // stopped working.
    let mut g = governor();
    let t = g
        .enforce(&mut Unreadable, Percent::clamped(60), &nominal())
        .expect("an unverifiable ceiling must produce a transition");

    assert_eq!(g.level(), Level::Derated);
    assert!(
        matches!(t.reason, Reason::Unverifiable(_)),
        "{:?}",
        t.reason
    );
    assert!(
        t.to_string().contains("unverified, not working"),
        "the distinction must reach the operator: {t}"
    );
}

#[test]
fn a_ceiling_that_could_not_be_written_derates() {
    let mut g = governor();
    let t = g
        .enforce(&mut Refusing, Percent::clamped(60), &nominal())
        .expect("a failed write must produce a transition");
    assert_eq!(g.level(), Level::Derated);
    assert!(matches!(t.reason, Reason::ApplyFailed(_)), "{:?}", t.reason);
}

#[test]
fn a_ceiling_that_holds_produces_no_transition() {
    // The corollary. If every path escalated, the ladder would be noise and the
    // rule above would be satisfied for the wrong reason.
    let mut g = governor();
    let mut mech = Working {
        held: Percent::clamped(100),
    };
    assert!(g
        .enforce(&mut mech, Percent::clamped(60), &nominal())
        .is_none());
    assert_eq!(g.level(), Level::Normal);
    assert!(g.may_serve());
}

#[test]
fn a_hardware_ceiling_below_what_was_asked_for_is_satisfying_it() {
    // A device-tree limit set lower than the runtime request is ADR-0002's
    // preferred T3 configuration, not a fault. Treating "not exactly equal" as
    // reverted would derate the safest devices this project supports.
    // The mechanism must IGNORE the write, or the fixture cannot exhibit the
    // case at all. An earlier version used one whose apply() overwrote its own
    // held value, so verify() always returned exactly what was asked for and
    // this test passed against a mechanism that could never fail it. The
    // mutation gate is what found that.
    let mut g = governor();
    let mut mech = DeviceTreeLimited(Percent::clamped(40));
    assert!(g
        .enforce(&mut mech, Percent::clamped(60), &nominal())
        .is_none());
    assert_eq!(g.level(), Level::Normal);
}

// ── The ladder ────────────────────────────────────────────────────────────────

#[test]
fn each_temperature_threshold_fires_at_its_own_level() {
    for (deci, expected) in [
        (440, Level::Normal),
        (450, Level::Derated),
        (519, Level::Derated),
        (520, Level::Protect),
        (599, Level::Protect),
        (600, Level::Halt),
        (612, Level::Halt),
    ] {
        let mut g = governor();
        g.observe(&at_temp(deci));
        assert_eq!(
            g.level(),
            expected,
            "{}.{} °C should reach {expected}",
            deci / 10,
            deci % 10
        );
    }
}

#[test]
fn a_reading_crossing_several_thresholds_lands_at_the_highest_in_one_step() {
    // Walking the ladder would log three transitions for one event and bury the
    // one that matters under two that no longer do.
    let mut g = governor();
    let t = g.observe(&at_temp(650)).expect("must escalate");
    assert_eq!(g.level(), Level::Halt);
    assert_eq!(t.from, Level::Normal);
    assert_eq!(g.history().len(), 1);
}

#[test]
fn the_decidegree_reading_is_not_mistaken_for_degrees() {
    // The bug this forecloses: `temp` is decidegrees, the hard stop is written
    // in degrees, and 601 compared against 60 reads as nowhere near the limit.
    // A cell at 60.1 °C would keep serving in somebody's home.
    let mut g = governor();
    g.observe(&at_temp(601));
    assert_eq!(
        g.level(),
        Level::Halt,
        "60.1 °C must hard-stop; the raw reading is 601"
    );

    // And the other direction: 60 decidegrees is 6 °C, which is nothing.
    let mut cold = governor();
    cold.observe(&at_temp(60));
    assert_eq!(cold.level(), Level::Normal, "6.0 °C must not escalate");
}

#[test]
fn truncation_rounds_toward_the_threshold_not_away_from_it() {
    // 60.9 °C truncates to 60 and fires. Rounding to nearest would put the
    // boundary at half a degree, which is not a distinction any thermal design
    // should rest on.
    let mut g = governor();
    g.observe(&at_temp(609));
    assert_eq!(g.level(), Level::Halt);
}

#[test]
fn a_degraded_cell_derates_and_a_spent_one_stops_serving() {
    let mut degraded = governor();
    degraded.observe(&at_health(3_000_000, 4_000_000)); // 75%
    assert_eq!(degraded.level(), Level::Derated);

    let mut spent = governor();
    spent.observe(&at_health(2_000_000, 4_000_000)); // 50%
    assert_eq!(spent.level(), Level::Protect);
    assert!(!spent.may_serve(), "a spent cell must not keep serving");
}

#[test]
fn an_unmeasurable_state_of_health_is_unknown_not_zero() {
    // The attack: a device whose charge_full_design node is missing reads as
    // 0% health and gets retired, or the arithmetic lands somewhere harmless and
    // a genuinely dead cell keeps serving. Unknown is a third answer.
    let r = at_health(3_600_000, 0);
    assert_eq!(r.state_of_health(), StateOfHealth::Unknown);

    let mut g = governor();
    g.observe(&r);
    assert_eq!(
        g.level(),
        Level::Normal,
        "an unmeasurable cell must not be retired for being unmeasurable"
    );
}

#[test]
fn the_same_condition_reported_twice_does_not_fill_the_log() {
    // A repeated transition per sample would bury the next real event under
    // hundreds of copies of the last one.
    let mut g = governor();
    assert!(g.observe(&at_temp(460)).is_some());
    assert!(g.observe(&at_temp(465)).is_none());
    assert!(g.observe(&at_temp(470)).is_none());
    assert_eq!(g.history().len(), 1);
}

// ── Escalation only ───────────────────────────────────────────────────────────

#[test]
fn a_cooling_cell_does_not_walk_back_down_on_its_own() {
    // The rule ADR-0002 §4 puts first. A cell that reached a critical threshold
    // has told you something about itself, and a temperature that recovered does
    // not unsay it.
    let mut g = governor();
    g.observe(&at_temp(600));
    assert_eq!(g.level(), Level::Halt);

    g.observe(&nominal());
    g.observe(&at_temp(200));
    assert_eq!(
        g.level(),
        Level::Halt,
        "cooling down must not clear a hard stop"
    );

    // The version above is necessary and was not sufficient: at 20 °C no
    // threshold is crossed, so nothing ever ATTEMPTS to lower the level and the
    // guard against lowering is never reached. This reading maps to Derated —
    // a real attempt to move down the ladder, which is what the guard exists
    // for. The mutation gate found the gap.
    g.observe(&at_temp(460));
    assert_eq!(
        g.level(),
        Level::Halt,
        "a reading that maps to a lower level must not lower the level"
    );
    assert!(!g.may_serve());
}

#[test]
fn a_working_ceiling_does_not_undo_an_earlier_escalation() {
    // The subtle version: enforce() succeeding must not be read as "all is well
    // now" and quietly reset a level raised by temperature.
    let mut g = governor();
    g.observe(&at_temp(600));
    let mut mech = Working {
        held: Percent::clamped(60),
    };
    g.enforce(&mut mech, Percent::clamped(60), &nominal());
    assert_eq!(g.level(), Level::Halt);
}

#[test]
fn recovery_requires_a_person_who_looked_at_the_phone() {
    let mut g = governor();
    g.observe(&at_temp(600));
    assert_eq!(g.level(), Level::Halt);

    let g = g
        .after_inspection(Inspection::LiesFlat)
        .expect("a flat phone may resume");
    assert_eq!(g.level(), Level::Normal);
    assert!(g.may_serve());
}

#[test]
fn a_deformed_cell_does_not_recover_whatever_the_sensors_say_next() {
    // Software cannot see a millimetre of deformation, so when a person reports
    // seeing it, that observation outranks every reading that follows.
    let mut g = governor();
    g.observe(&at_temp(600));
    let g = g
        .after_inspection(Inspection::Deformed)
        .expect_err("a deformed cell must not resume");
    assert_eq!(g.level(), Level::Halt);
    assert!(!g.may_serve());
}

// ── Thresholds ────────────────────────────────────────────────────────────────

#[test]
fn the_hard_stop_may_be_lowered_but_never_raised() {
    // ADR-0002 open decision 2. An operator who wants a safer number gets it;
    // the risk of a laxer one is not theirs alone to take.
    Thresholds::new(
        Celsius::new(40),
        Celsius::new(48),
        Celsius::new(55),
        Percent::clamped(80),
        Percent::clamped(60),
    )
    .expect("a lower hard stop must be accepted");

    assert_eq!(
        Thresholds::new(
            Celsius::new(45),
            Celsius::new(52),
            Celsius::new(75),
            Percent::clamped(80),
            Percent::clamped(60),
        )
        .unwrap_err(),
        ThresholdError::HardStopTooHigh
    );
}

#[test]
fn an_unordered_ladder_is_refused_rather_than_silently_unreachable() {
    // A critical threshold above the hard stop is a rung that can never fire,
    // and nothing about the running system would say so.
    assert_eq!(
        Thresholds::new(
            Celsius::new(50),
            Celsius::new(45),
            Celsius::new(60),
            Percent::clamped(80),
            Percent::clamped(60),
        )
        .unwrap_err(),
        ThresholdError::TemperatureNotAscending
    );
    assert_eq!(
        Thresholds::new(
            Celsius::new(45),
            Celsius::new(52),
            Celsius::new(60),
            Percent::clamped(60),
            Percent::clamped(80),
        )
        .unwrap_err(),
        ThresholdError::HealthNotDescending
    );
}

// ── Evidence ──────────────────────────────────────────────────────────────────

#[test]
fn every_transition_names_the_reading_that_caused_it() {
    // "Halted" is useless. "Halted: pack temperature 61.2 °C exceeded 60 °C" is
    // actionable, and an operator has to be able to act at the moment they read
    // it, not after asking someone.
    let mut g = governor();
    let t = g.observe(&at_temp(612)).expect("must escalate");
    assert!(!t.evidence.is_empty());
    assert!(t.evidence.contains("61.2 °C"), "{}", t.evidence);
    let line = t.to_string();
    assert!(line.contains("NORMAL -> HALT"), "{line}");
    assert!(line.contains("60 °C"), "{line}");
}

#[test]
fn the_vendors_opinion_of_the_cell_changes_nothing() {
    // ADR-0002 open decision 5: vendors report Good on visibly swollen cells.
    // It is recorded so a disagreement with the measured trend is visible, and
    // it is kept out of every decision path.
    let mut a = governor();
    let mut b = governor();
    a.observe(&Reading {
        vendor_health: VendorHealth::Reported("Good"),
        ..at_temp(600)
    });
    b.observe(&Reading {
        vendor_health: VendorHealth::Unavailable,
        ..at_temp(600)
    });
    assert_eq!(a.level(), b.level(), "the vendor's opinion must not matter");
    assert_eq!(a.level(), Level::Halt);
}
