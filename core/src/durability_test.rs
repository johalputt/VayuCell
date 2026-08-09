// SPDX-License-Identifier: Apache-2.0

//! Durability tests, in the attacker's voice.
//!
//! The attacker is the ordinary wish to see a green row. Every assertion here
//! forecloses a way of arriving at one without having earned it.

use core::time::Duration;

use crate::durability::{
    BackupState, DurabilityClass, GracefulShutdown, LabVerification, Posture, RecoveryPoint,
    WearIndicator, MEASUREMENT_STANDS_FOR,
};

fn target() -> Duration {
    Duration::from_secs(60)
}

/// The clock reading every test below treats as "now".
///
/// Non-zero so a measurement can sit both before and after it, which is what
/// makes the ahead-of-the-clock case expressible at all.
const NOW: Duration = Duration::from_secs(10_000);

/// A lag measured at `NOW`, so its age is zero and the figure is live.
fn just_measured(secs: u64) -> RecoveryPoint {
    RecoveryPoint::Behind {
        lag: Duration::from_secs(secs),
        measured_at: NOW,
    }
}

// ── The recovery point ────────────────────────────────────────────────────────

#[test]
fn a_lag_inside_the_target_is_the_only_quiet_state() {
    // And even this one names a window in which data exists on one device. There
    // is no state that means "safe" — the compile_fail proof on RecoveryPoint
    // pins that there is no variant for it.
    let ok = just_measured(47);
    assert!(!ok.needs_attention(target(), NOW));
    assert!(ok.describe(NOW).contains("exists on this device only"));

    assert!(just_measured(61).needs_attention(target(), NOW));
}

#[test]
fn an_unreachable_replica_is_not_filtered_out_as_noise() {
    // The tempting reading: no news since the last successful sync, so probably
    // still fine. That is how a backup that stopped working three weeks ago goes
    // on reporting the lag it had when it stopped.
    let r = RecoveryPoint::Unreachable("connection refused".into());
    assert!(r.needs_attention(Duration::from_secs(86_400), NOW));
    let said = r.describe(NOW);
    assert!(said.contains("unknown is not small"), "{said}");
}

#[test]
fn never_replicated_is_distinct_from_a_large_lag() {
    // "Twelve hours behind" means twelve hours of data is at risk. This means all
    // of it is, and collapsing the two would present the worse state in the
    // gentler language.
    let never = RecoveryPoint::NeverReplicated;
    assert!(never.needs_attention(Duration::from_secs(0), NOW));
    assert!(never.describe(NOW).contains("every byte"));
    assert_ne!(never, just_measured(43_200));
}

#[test]
fn having_no_replica_at_all_is_a_named_state_rather_than_an_absence() {
    // The worst state, and the one arrived at by doing nothing — so it must be a
    // value the panel can render, not a missing field it renders nothing for.
    let none = RecoveryPoint::NoReplica;
    assert!(none.needs_attention(Duration::from_secs(0), NOW));
    assert!(none.describe(NOW).contains("the only copy"));
}

// ── The lag has to still be being measured ────────────────────────────────────

#[test]
fn a_lag_nobody_has_re_measured_stops_being_a_live_figure() {
    // ADR-0004 §1.1 does not promise a lag. It promises one shown "continuously,
    // as a live figure", and the argument for a number over an adjective is that
    // a number can be checked. 47 renders identically whether it was taken a
    // second ago or the morning the replicator died.
    //
    // An hour, as a literal rather than a multiple of MEASUREMENT_STANDS_FOR: a
    // test written against the constant it pins stays green when somebody widens
    // that constant, which is the change that puts the defect back.
    let hour_later = NOW + Duration::from_secs(60 * 60);
    let stale = just_measured(47);

    assert!(!stale.is_live(hour_later));
    assert!(
        stale.needs_attention(target(), hour_later),
        "a dead replicator's last good number is not 'no concern'"
    );

    let said = stale.describe(hour_later);
    assert!(said.contains("no longer live"), "{said}");
    assert!(
        said.contains("47s behind when it was last measured"),
        "{said}"
    );
    assert!(said.contains("3600s ago"), "{said}");
}

#[test]
fn a_lag_measured_a_moment_ago_is_still_live() {
    // The other side of the same literal. A standing period of zero would make
    // every figure permanently stale — safe, useless, and it would leave the
    // test above green.
    let minute_later = NOW + Duration::from_secs(60);
    assert!(just_measured(47).is_live(minute_later));
    assert!(!just_measured(47).needs_attention(target(), minute_later));
    assert!(just_measured(47)
        .describe(minute_later)
        .contains("is 47s behind"));
}

#[test]
fn the_standing_period_is_short_enough_to_notice_a_replicator_that_stopped() {
    // Both bounds literal, for the same reason. Under fifteen minutes, so a
    // stopped replicator is noticed while the operator is still in the room;
    // over the 60s default lag target, so an ordinary cycle is not called stale.
    assert!(
        MEASUREMENT_STANDS_FOR <= Duration::from_secs(15 * 60),
        "{MEASUREMENT_STANDS_FOR:?}"
    );
    assert!(
        MEASUREMENT_STANDS_FOR > Duration::from_secs(60),
        "{MEASUREMENT_STANDS_FOR:?}"
    );
}

#[test]
fn a_measurement_stamped_ahead_of_the_clock_is_not_a_live_figure() {
    // The clock is monotonic and owned by this process, so this should not
    // happen — which is why it must be decided rather than assumed. An age that
    // cannot be established is not an age, and Article IV.3 does not allow what
    // could not be checked to read as clean.
    let ahead = RecoveryPoint::Behind {
        lag: Duration::from_secs(1),
        measured_at: NOW + Duration::from_secs(60),
    };
    assert!(!ahead.is_live(NOW));
    assert!(ahead.needs_attention(target(), NOW));
    assert!(ahead.describe(NOW).contains("ahead of this cell's clock"));
}

#[test]
fn a_stale_lag_reaches_the_operator_through_the_posture_too() {
    // The panel is what an operator reads, and a rule enforced only on the type
    // it wraps is a rule the panel can route around. Same failure the governor
    // row had twice: the mechanism was right and the surface did not use it.
    let p = Posture {
        recovery_point: just_measured(2),
        durability: DurabilityClass::AssumedUntrusted,
        wear: WearIndicator::Absent,
        graceful_shutdown: GracefulShutdown::Verified,
        backup: BackupState::Restored {
            when: "2026-08-09".into(),
        },
    };
    assert!(
        p.concerns(target(), NOW).is_empty(),
        "{:?}",
        p.concerns(target(), NOW)
    );

    let hour_later = NOW + Duration::from_secs(60 * 60);
    let concerns = p.concerns(target(), hour_later);
    assert_eq!(concerns.len(), 1, "{concerns:?}");
    assert!(concerns[0].contains("no longer live"), "{concerns:?}");
}

// ── Trusting the flash ────────────────────────────────────────────────────────

#[test]
fn the_default_posture_toward_flash_is_untrusted() {
    // A Default resolving to the trusting value would make every device nobody
    // configured look lab-verified, which is the exact inversion of ADR-0004.
    assert_eq!(
        DurabilityClass::default(),
        DurabilityClass::AssumedUntrusted
    );
    assert!(!DurabilityClass::default().is_lab_verified());
}

#[test]
fn assumed_untrusted_is_described_neutrally_and_not_as_a_fault() {
    // It is the correct posture toward all consumer flash and true of every
    // device. Rendered as a warning it would appear on every panel forever, and a
    // warning that is always on is a warning nobody reads.
    let d = DurabilityClass::AssumedUntrusted.describe();
    for alarming in ["fail", "error", "unsafe", "danger", "defect"] {
        assert!(
            !d.to_lowercase().contains(alarming),
            "the default posture must not read as a fault: {d}"
        );
    }
    assert!(d.contains("does not depend on it being honest"));
}

#[test]
fn lab_verified_cannot_be_claimed_without_naming_the_rig() {
    // ADR-0004 §0 withdrew the flush-honesty test because a warm reboot cannot
    // distinguish an honest device from a lying one. A class that could be set
    // without naming a fixture would be set by somebody who rebooted a phone and
    // watched the database survive — the withdrawn test, under a new name.
    let v = DurabilityClass::LabVerified(LabVerification {
        method: "power-fault injection, 200 cycles".into(),
        fixture: "a relay on the 3V3 rail with a dummy-battery supply".into(),
        date: "2026-08-07".into(),
    });
    assert!(v.is_lab_verified());
    let d = v.describe();
    assert!(d.contains("relay on the 3V3 rail"), "{d}");
    assert!(
        d.contains("never the model"),
        "one part on one bench is not a population: {d}"
    );
}

// ── Wear ──────────────────────────────────────────────────────────────────────

#[test]
fn a_device_exposing_no_wear_indicator_is_absent_rather_than_healthy() {
    // The variants record whether the device says anything, not whether it said
    // something good. Absent collapsing into healthy is Charter Article IV's
    // central failure, in the one place a number would look reassuring.
    assert_ne!(WearIndicator::Absent, WearIndicator::Readable(0));

    let p = Posture {
        wear: WearIndicator::Absent,
        ..Posture::unconfigured()
    };
    // Absent is not a concern — it is not a fault — but it is also never
    // reported as a low wear figure.
    assert!(!p.concerns(target(), NOW).iter().any(|c| c.contains("wear")));
}

#[test]
fn a_wear_indicator_that_returned_nonsense_is_surfaced() {
    let p = Posture {
        wear: WearIndicator::Unreliable("65535".into()),
        ..Posture::unconfigured()
    };
    assert!(p
        .concerns(target(), NOW)
        .iter()
        .any(|c| c.contains("65535") && c.contains("not usable")));
}

// ── The backup, and the P6 gate ───────────────────────────────────────────────

#[test]
fn a_backup_nobody_has_restored_is_never_proven() {
    // The roadmap's P6 gate: an unrestored backup reads unverified. Every
    // property anybody checked on a written backup is a property of the file —
    // its size, its checksum, that it appeared. None of them is a property of the
    // restore, which is the only thing anybody actually wants.
    assert!(!BackupState::NeverRestored.is_proven());
    assert!(!BackupState::NotConfigured.is_proven());
    assert!(!BackupState::RestoreFailed("truncated archive".into()).is_proven());
    assert!(BackupState::Restored {
        when: "2026-08-07".into()
    }
    .is_proven());
}

#[test]
fn an_unrestored_backup_says_what_was_actually_verified() {
    let s = BackupState::NeverRestored.to_string();
    assert!(
        s.contains("files exist, not that they can be recovered"),
        "the distinction has to reach the operator: {s}"
    );
}

#[test]
fn an_unrestored_backup_is_a_standing_concern_that_no_amount_of_backing_up_clears() {
    // Writing more backups is the thing somebody does instead of restoring one,
    // and it must not move this row.
    let p = Posture {
        backup: BackupState::NeverRestored,
        recovery_point: just_measured(2),
        graceful_shutdown: GracefulShutdown::Verified,
        ..Posture::unconfigured()
    };
    let concerns = p.concerns(target(), NOW);
    assert_eq!(concerns.len(), 1, "{concerns:?}");
    assert!(
        concerns[0].contains("none has ever been restored"),
        "{}",
        concerns[0]
    );
}

// ── The one thing that is ours to test ────────────────────────────────────────

#[test]
fn a_shed_ladder_nobody_has_watched_complete_is_not_credited() {
    // ADR-0004 §2: this is the one durability property genuinely testable
    // on-device, because it measures our behaviour rather than the flash
    // controller's honesty. NeverObserved is an absence, not a pass.
    let p = Posture::unconfigured();
    assert_eq!(p.graceful_shutdown, GracefulShutdown::NeverObserved);
    assert!(p
        .concerns(target(), NOW)
        .iter()
        .any(|c| c.contains("never been observed")));
}

#[test]
fn a_shed_ladder_that_ran_and_left_an_inconsistent_database_says_whose_failure_it_is() {
    let p = Posture {
        graceful_shutdown: GracefulShutdown::Failed,
        ..Posture::unconfigured()
    };
    assert!(p
        .concerns(target(), NOW)
        .iter()
        .any(|c| c.contains("ours rather than the")));
}

// ── The whole posture ─────────────────────────────────────────────────────────

#[test]
fn an_unconfigured_device_reports_every_field_at_its_least_reassuring_value() {
    // What is true before anything has been set up. A Default resolving to good
    // news would be this module telling a first-run device it was protected.
    let p = Posture::unconfigured();
    assert_eq!(p.recovery_point, RecoveryPoint::NoReplica);
    assert_eq!(p.durability, DurabilityClass::AssumedUntrusted);
    assert_eq!(p.backup, BackupState::NotConfigured);
    assert_eq!(p.graceful_shutdown, GracefulShutdown::NeverObserved);
    assert!(
        p.concerns(target(), NOW).len() >= 3,
        "an unconfigured device has several: {:?}",
        p.concerns(target(), NOW)
    );
}

#[test]
fn a_settled_device_still_required_somebody_to_restore_a_backup() {
    // The only way to an empty concern list. Note what it takes: a lag inside
    // target, a shed ladder observed completing, and a restore somebody actually
    // performed. Trusting the flash is not among the requirements and never
    // becomes one.
    let p = Posture {
        recovery_point: just_measured(12),
        durability: DurabilityClass::AssumedUntrusted,
        wear: WearIndicator::Readable(4),
        graceful_shutdown: GracefulShutdown::Verified,
        backup: BackupState::Restored {
            when: "2026-08-07".into(),
        },
    };
    assert!(
        p.concerns(target(), NOW).is_empty(),
        "{:?}",
        p.concerns(target(), NOW)
    );
}

#[test]
fn assuming_the_flash_lies_is_never_itself_a_concern() {
    // The design does not depend on the flash being honest, so the posture that
    // says so is not a problem to be resolved. Listing it beside real problems is
    // how a list of real problems stops being read.
    let settled = Posture {
        recovery_point: just_measured(12),
        durability: DurabilityClass::AssumedUntrusted,
        wear: WearIndicator::Absent,
        graceful_shutdown: GracefulShutdown::Verified,
        backup: BackupState::Restored {
            when: "2026-08-07".into(),
        },
    };
    assert!(settled.concerns(target(), NOW).is_empty());
}
