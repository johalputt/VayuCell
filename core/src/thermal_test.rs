// SPDX-License-Identifier: Apache-2.0

//! Tests for the thermal class and the policy that produces it.
//!
//! Each test names the failure it forecloses. The failure mode here is never a
//! crash: it is a cell that is reported as comfortable when it was not, or a
//! device that is reported as unreadable when it was merely cold — and both of
//! those are a catalogue offering the wrong thing to somebody's battery.

use crate::battery::{DeciCelsius, Percent, Reading, VendorHealth};
use crate::governor::Thresholds;
use crate::thermal::{ThermalClass, ThermalHistory};

/// A healthy cell, at a temperature the caller chooses.
fn at_temp(deci: i32) -> Reading {
    Reading {
        capacity: Percent::clamped(58),
        voltage_now_uv: 3_820_000,
        current_now_ua: -180_000,
        temp: DeciCelsius::from_sysfs(deci),
        cycle_count: 412,
        charge_full_uah: 3_600_000,
        charge_full_design_uah: 4_000_000,
        vendor_health: VendorHealth::Reported("Good"),
    }
}

/// The thresholds the governor uses by default.
fn thresholds() -> Thresholds {
    Thresholds::recommended()
}

#[test]
fn a_cell_that_has_never_been_read_has_no_class_at_all() {
    // The defect this forecloses: an unobserved cell defaulting to the lowest
    // class. "Idle" is a *reading* — it says this cell does nothing — and a
    // cell nobody has looked at has not been shown to do nothing. Worse, a
    // catalogue that treats "cannot be told" as "idle" offers every service
    // whose envelope is idle, on the strength of a reading nobody took.
    let history = ThermalHistory::new();
    assert_eq!(history.observed(), 0);
    assert_eq!(history.worst(), None);
    assert_eq!(
        history.class(&thresholds()),
        None,
        "an unobserved cell has no class, and the class it has is not a low one"
    );
}

#[test]
fn a_cool_cell_is_a_capable_one() {
    // The direction of the mapping. The class is a ceiling on the work the
    // cell may do, so a cell the governor has never warned may do the heavy
    // work — and a catalogue that read the class as a description of the cell
    // rather than a limit on it would grey out the inference service on a
    // phone that had never been warm.
    let mut history = ThermalHistory::new();
    history.observe(&at_temp(292));
    assert_eq!(history.class(&thresholds()), Some(ThermalClass::Heavy));
}

#[test]
fn the_class_follows_the_governors_own_ladder() {
    // One rung per rung, and the governor's own thresholds rather than a
    // second set of numbers: a cell at the warn temperature has been warned,
    // and the heavy work is off.
    for (deci, expected) in [
        (440, ThermalClass::Heavy),     // 44 °C, below the warn rung
        (450, ThermalClass::Sustained), // 45 °C, at the warn rung
        (519, ThermalClass::Sustained), // 51.9 °C, still below critical
        (520, ThermalClass::Light),     // 52 °C, at the critical rung
        (599, ThermalClass::Light),     // 59.9 °C, still below the hard stop
        (600, ThermalClass::Idle),      // 60 °C, at the hard stop
    ] {
        let mut history = ThermalHistory::new();
        history.observe(&at_temp(deci));
        assert_eq!(
            history.class(&thresholds()),
            Some(expected),
            "{deci} deci-degrees should read as {expected}"
        );
    }
}

#[test]
fn a_cell_that_has_been_hot_stays_restricted_after_it_cools() {
    // The defect this forecloses: a class computed from the *latest* reading.
    // The governor raises its level and never lowers it, because a cell that
    // has been warned has not recovered the moment the number comes down. A
    // catalogue that forgot a warning thirty seconds after it was issued would
    // be offering heavy work to a cell the governor is still derating.
    let mut history = ThermalHistory::new();
    history.observe(&at_temp(470));
    history.observe(&at_temp(295));
    assert_eq!(history.worst(), Some(crate::battery::Celsius::new(47)));
    assert_eq!(
        history.class(&thresholds()),
        Some(ThermalClass::Sustained),
        "the warning stands until somebody looks at the phone"
    );
}

#[test]
fn the_hottest_reading_is_the_one_that_counts() {
    // The history keeps the worst rather than the whole series, and this holds
    // that decision to the order the readings arrived in: a cooler cell first
    // and a hotter one second must be the hotter one that is remembered.
    let mut history = ThermalHistory::new();
    history.observe(&at_temp(300));
    history.observe(&at_temp(500));
    assert_eq!(history.worst(), Some(crate::battery::Celsius::new(50)));
    assert_eq!(history.observed(), 2);

    let mut other = ThermalHistory::new();
    other.observe(&at_temp(500));
    other.observe(&at_temp(300));
    assert_eq!(
        other.worst(),
        history.worst(),
        "the order the readings arrived in must not change the worst of them"
    );
}

#[test]
fn the_class_order_is_the_order_the_catalogue_compares() {
    // The catalogue asks `service.worse_than(device)`, so the order of the
    // classes and the order of `worse_than` have to be the same order. A class
    // added in the wrong place would silently change what the catalogue
    // offers, and nothing else in the code would notice.
    for (index, class) in ThermalClass::ALL.iter().enumerate() {
        for (other_index, other) in ThermalClass::ALL.iter().enumerate() {
            assert_eq!(
                class.worse_than(*other),
                index > other_index,
                "{class} against {other}"
            );
        }
    }
    assert_eq!(ThermalClass::ALL.len(), 4);
}

#[test]
fn a_temperature_the_kernel_reports_in_tenths_is_truncated_the_way_the_governor_truncates_it() {
    // The governor compares whole degrees, and truncates toward zero so that
    // 60.9 °C fires a threshold at 60. A class computed from a different
    // rounding would put the same cell in two different classes depending on
    // which code did the arithmetic.
    let mut history = ThermalHistory::new();
    history.observe(&at_temp(609));
    assert_eq!(
        history.class(&thresholds()),
        Some(ThermalClass::Idle),
        "60.9 °C must be 60 °C, and 60 °C is the hard stop"
    );
}
