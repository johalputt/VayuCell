// SPDX-License-Identifier: Apache-2.0

//! Sampling tests, in the attacker's voice.
//!
//! Nothing here sleeps. The cadence is a function of the reading, so a cell
//! warming toward a threshold over an hour is a handful of assertions rather
//! than an hour of waiting.

use core::time::Duration;

use crate::battery::{Celsius, DeciCelsius, Percent, Reading, VendorHealth};
use crate::governor::{Governor, Level, Reason, Thresholds};
use crate::sampler::{Cadence, Sampler};

fn idle_at(deci: i32) -> Reading {
    Reading {
        capacity: Percent::clamped(58),
        voltage_now_uv: 3_820_000,
        current_now_ua: -180_000, // discharging
        temp: DeciCelsius::from_sysfs(deci),
        cycle_count: 412,
        charge_full_uah: 3_600_000,
        charge_full_design_uah: 4_000_000,
        vendor_health: VendorHealth::Unavailable,
    }
}

fn sampler() -> Sampler {
    Sampler::new(Thresholds::recommended())
}

#[test]
fn a_cool_idle_cell_is_left_alone() {
    // The device must not be kept awake by its own monitor. Polling every five
    // seconds for a cell at 29 °C burns the battery this subsystem exists to
    // preserve — a monitor causing the harm it was written to measure.
    let s = sampler();
    assert_eq!(s.cadence_for(&idle_at(290)), Cadence::Steady);
    assert_eq!(s.interval_for(&idle_at(290)), Duration::from_secs(30));
}

#[test]
fn approaching_a_threshold_tightens_the_cadence_before_it_is_crossed() {
    // Watching only once a threshold is crossed means the first sample after a
    // crossing can be thirty seconds late. The margin buys back that thirty
    // seconds at exactly the moment it is worth having.
    let s = sampler();
    // warn is 45 °C, margin is 5 °C.
    assert_eq!(
        s.cadence_for(&idle_at(390)),
        Cadence::Steady,
        "39 °C is far"
    );
    assert_eq!(
        s.cadence_for(&idle_at(400)),
        Cadence::Alert,
        "40 °C is close"
    );
    assert_eq!(s.interval_for(&idle_at(400)), Duration::from_secs(5));
}

#[test]
fn the_alert_band_follows_the_lowest_rung_rather_than_a_hardcoded_temperature() {
    // This test replaced one that iterated every rung and asserted each was
    // Alert. That version passed no matter what the code did: the ladder is
    // ordered, so 47 °C is already past the 45 °C warn margin and every later
    // rung it "checked" was subsumed by the first. It looked thorough and
    // asserted nothing the earlier case had not.
    //
    // What actually distinguishes a correct implementation is that the band
    // moves when the ladder does. An operator who lowers warn to 35 °C is asking
    // to be watched from 30 °C, and a sampler with 40 hardcoded — or one reading
    // the wrong field — keeps sampling that cell every thirty seconds.
    let strict = Thresholds::new(
        Celsius::new(35),
        Celsius::new(48),
        Celsius::new(55),
        Percent::clamped(80),
        Percent::clamped(60),
    )
    .expect("an ordered ladder");
    let s = Sampler::new(strict);

    assert_eq!(s.alert_from(), Celsius::new(30));
    assert_eq!(
        s.cadence_for(&idle_at(290)),
        Cadence::Steady,
        "29 °C is far"
    );
    assert_eq!(
        s.cadence_for(&idle_at(300)),
        Cadence::Alert,
        "30 °C is a margin below the lowered warn rung"
    );
    assert_eq!(
        sampler().cadence_for(&idle_at(300)),
        Cadence::Steady,
        "the same reading under the recommended ladder is not close to anything"
    );
}

#[test]
fn a_cell_past_the_first_rung_and_climbing_stays_under_close_watch() {
    // The case sampling frequency matters most in: already derated, heading for
    // the critical rung. Subsumed by the band above rather than checked
    // separately, which is exactly why it is worth pinning.
    let s = sampler();
    for deci in [470, 500, 520, 550, 590] {
        assert_eq!(
            s.cadence_for(&idle_at(deci)),
            Cadence::Alert,
            "{}.{} °C must be watched closely",
            deci / 10,
            deci % 10
        );
    }
}

#[test]
fn a_charging_cell_is_watched_closely_however_cool_it_is() {
    // Charging is when the cell's state changes fastest, and it is the condition
    // this project asks people to leave a device in for years.
    let s = sampler();
    let charging = Reading {
        current_now_ua: 900_000,
        ..idle_at(250)
    };
    assert!(charging.is_charging());
    assert_eq!(s.cadence_for(&charging), Cadence::Alert);
    assert_eq!(
        s.cadence_for(&idle_at(250)),
        Cadence::Steady,
        "the same temperature idle must not be alert"
    );
}

#[test]
fn a_device_that_cannot_be_read_is_watched_more_closely_not_less() {
    // The direction a retry backoff would naturally take this, and the wrong
    // one. A device whose battery nodes stopped answering is a device nobody is
    // measuring; backing off turns "we cannot see the cell" into "we look at it
    // less often".
    assert_eq!(Sampler::cadence_when_unreadable(), Cadence::Alert);
}

// ── Going blind ───────────────────────────────────────────────────────────────

#[test]
fn one_failed_reading_is_a_transient_and_does_not_cry_wolf() {
    let mut g = Governor::new(Thresholds::recommended());
    assert!(g.observe_unreadable("resource busy").is_none());
    assert_eq!(g.level(), Level::Normal);
    assert_eq!(g.consecutive_failures(), 1);
}

#[test]
fn a_governor_that_has_gone_blind_says_so_rather_than_reporting_health() {
    // The gap this closes: before it existed, a device whose power-supply nodes
    // vanished — a kernel update, a permission change — produced no readings, no
    // transitions, and a panel that still said NORMAL. A monitor that has stopped
    // measuring and says nothing is reporting a healthy device.
    let mut g = Governor::new(Thresholds::recommended());
    assert!(g.observe_unreadable("no such file").is_none());
    assert!(g.observe_unreadable("no such file").is_none());
    let t = g
        .observe_unreadable("no such file")
        .expect("a run of failures must escalate");

    assert_eq!(g.level(), Level::Derated);
    assert!(
        matches!(t.reason, Reason::Unmeasurable { consecutive: 3, .. }),
        "{:?}",
        t.reason
    );
    assert!(
        t.to_string().contains("unmeasured rather than healthy"),
        "the operator must be told which it is: {t}"
    );
}

#[test]
fn a_reading_that_arrives_is_the_only_thing_that_clears_the_blind_counter() {
    // A retry timer or a restart resetting it would let a device managing one
    // reading an hour look continuously monitored.
    let mut g = Governor::new(Thresholds::recommended());
    g.observe_unreadable("busy");
    g.observe_unreadable("busy");
    assert_eq!(g.consecutive_failures(), 2);

    g.observe(&idle_at(290));
    assert_eq!(g.consecutive_failures(), 0);

    // And the count really did reset: two more failures must not escalate.
    assert!(g.observe_unreadable("busy").is_none());
    assert!(g.observe_unreadable("busy").is_none());
    assert_eq!(g.level(), Level::Normal);
}

#[test]
fn going_blind_does_not_undo_an_earlier_escalation() {
    let mut g = Governor::new(Thresholds::recommended());
    g.observe(&idle_at(600));
    assert_eq!(g.level(), Level::Halt);
    for _ in 0..5 {
        g.observe_unreadable("no such file");
    }
    assert_eq!(
        g.level(),
        Level::Halt,
        "blindness must not lower the ladder"
    );
}
