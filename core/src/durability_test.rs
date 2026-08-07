// SPDX-License-Identifier: Apache-2.0

//! Durability tests, in the attacker's voice.
//!
//! The attacker is the ordinary wish to see a green row. Every assertion here
//! forecloses a way of arriving at one without having earned it.

use core::time::Duration;

use crate::durability::{
    BackupState, DurabilityClass, GracefulShutdown, LabVerification, Posture, RecoveryPoint,
    WearIndicator,
};

fn target() -> Duration {
    Duration::from_secs(60)
}

// ── The recovery point ────────────────────────────────────────────────────────

#[test]
fn a_lag_inside_the_target_is_the_only_quiet_state() {
    // And even this one names a window in which data exists on one device. There
    // is no state that means "safe" — the compile_fail proof on RecoveryPoint
    // pins that there is no variant for it.
    let ok = RecoveryPoint::Behind(Duration::from_secs(47));
    assert!(!ok.needs_attention(target()));
    assert!(ok.to_string().contains("exists on this device only"));

    assert!(RecoveryPoint::Behind(Duration::from_secs(61)).needs_attention(target()));
}

#[test]
fn an_unreachable_replica_is_not_filtered_out_as_noise() {
    // The tempting reading: no news since the last successful sync, so probably
    // still fine. That is how a backup that stopped working three weeks ago goes
    // on reporting the lag it had when it stopped.
    let r = RecoveryPoint::Unreachable("connection refused".into());
    assert!(r.needs_attention(Duration::from_secs(86_400)));
    assert!(r.to_string().contains("unknown is not small"), "{r}");
}

#[test]
fn never_replicated_is_distinct_from_a_large_lag() {
    // "Twelve hours behind" means twelve hours of data is at risk. This means all
    // of it is, and collapsing the two would present the worse state in the
    // gentler language.
    let never = RecoveryPoint::NeverReplicated;
    assert!(never.needs_attention(Duration::from_secs(0)));
    assert!(never.to_string().contains("every byte"));
    assert_ne!(never, RecoveryPoint::Behind(Duration::from_secs(43_200)));
}

#[test]
fn having_no_replica_at_all_is_a_named_state_rather_than_an_absence() {
    // The worst state, and the one arrived at by doing nothing — so it must be a
    // value the panel can render, not a missing field it renders nothing for.
    let none = RecoveryPoint::NoReplica;
    assert!(none.needs_attention(Duration::from_secs(0)));
    assert!(none.to_string().contains("the only copy"));
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
    assert!(!p.concerns(target()).iter().any(|c| c.contains("wear")));
}

#[test]
fn a_wear_indicator_that_returned_nonsense_is_surfaced() {
    let p = Posture {
        wear: WearIndicator::Unreliable("65535".into()),
        ..Posture::unconfigured()
    };
    assert!(p
        .concerns(target())
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
        recovery_point: RecoveryPoint::Behind(Duration::from_secs(2)),
        graceful_shutdown: GracefulShutdown::Verified,
        ..Posture::unconfigured()
    };
    let concerns = p.concerns(target());
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
        .concerns(target())
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
        .concerns(target())
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
        p.concerns(target()).len() >= 3,
        "an unconfigured device has several: {:?}",
        p.concerns(target())
    );
}

#[test]
fn a_settled_device_still_required_somebody_to_restore_a_backup() {
    // The only way to an empty concern list. Note what it takes: a lag inside
    // target, a shed ladder observed completing, and a restore somebody actually
    // performed. Trusting the flash is not among the requirements and never
    // becomes one.
    let p = Posture {
        recovery_point: RecoveryPoint::Behind(Duration::from_secs(12)),
        durability: DurabilityClass::AssumedUntrusted,
        wear: WearIndicator::Readable(4),
        graceful_shutdown: GracefulShutdown::Verified,
        backup: BackupState::Restored {
            when: "2026-08-07".into(),
        },
    };
    assert!(
        p.concerns(target()).is_empty(),
        "{:?}",
        p.concerns(target())
    );
}

#[test]
fn assuming_the_flash_lies_is_never_itself_a_concern() {
    // The design does not depend on the flash being honest, so the posture that
    // says so is not a problem to be resolved. Listing it beside real problems is
    // how a list of real problems stops being read.
    let settled = Posture {
        recovery_point: RecoveryPoint::Behind(Duration::from_secs(12)),
        durability: DurabilityClass::AssumedUntrusted,
        wear: WearIndicator::Absent,
        graceful_shutdown: GracefulShutdown::Verified,
        backup: BackupState::Restored {
            when: "2026-08-07".into(),
        },
    };
    assert!(settled.concerns(target()).is_empty());
}
