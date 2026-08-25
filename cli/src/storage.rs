// SPDX-License-Identifier: Apache-2.0

//! What this device can honestly say about its own storage.
//!
//! # The types had no caller
//!
//! [`vayucell_core::durability`] holds the honesty machinery for ADR-0004: a
//! replication lag that goes stale, a restore drill that expires, a
//! [`Posture`] whose `Default` is every field at its least reassuring value, and
//! no variant anywhere meaning *durable*. Every rule in it was written, tested
//! and mutation-proofed — and **nothing ever built one**. `Posture::unconfigured`
//! had no caller outside its own tests, so an operator running `vayucell vault`
//! was never told the thing ADR-0004 exists to tell them.
//!
//! This is the caller. It is deliberately small, because most of what P6
//! describes does not exist yet and this says so rather than inventing it.
//!
//! # What is knowable here, and what is not
//!
//! There is no replicator and no backup system in this repository. That is not a
//! gap in this module; it is the state of the project, and the honest rendering
//! of it is [`RecoveryPoint::NoReplica`] and [`BackupState::NotConfigured`] —
//! which the panel already describes as *"this phone is the only copy — which is
//! the one thing ADR-0004 says a phone must never be"*.
//!
//! **That sentence is the point.** Somebody storing files in the vault today is
//! keeping the only copy on a phone, and until this ran nothing said so.
//!
//! The one field a device can genuinely answer about itself is wear, and
//! [`vayucell_core::wear::observe`] answers it.
//!
//! # Nothing here guesses in the reassuring direction
//!
//! `graceful_shutdown` is [`GracefulShutdown::NeverObserved`] and stays there
//! until something records a clean shutdown, which nothing does. Reporting
//! `Verified` because no failure was seen would be exactly the reading Charter
//! Article IV.3 forbids — absence taken as evidence.

use vayucell_core::durability::{DurabilityClass, GracefulShutdown, Now, Posture};
use vayucell_core::host::Host;
use vayucell_core::wear;

/// The header the storage section puts on quoted replica claims, once, so no
/// line below it can be misread as something this device measured itself.
pub const EVIDENCE_PREAMBLE: &str = "as claimed by the replica's own receipt";

/// The same posture when a replica HAS been pointed at this cell.
///
/// `evidence` is the receipt file's text, or `None` when no file exists —
/// both mean nobody has claimed anything, and both get yesterday's answer.
/// When a claim IS present, every field below comes from
/// [`vayucell_core::replica::posture_parts`], whose whole job is turning
/// another machine's dated claims into these types without improving them.
/// Wear stays first-party: the flash is on THIS device, and this device
/// reads it.
#[must_use]
pub fn observed_with(host: &dyn Host, evidence: Option<&str>, now: Now) -> Posture {
    let (recovery_point, backup) =
        vayucell_core::replica::posture_parts(evidence, now.today, now.since_start);
    Posture {
        recovery_point,
        // ADR-0004 §0: the correct posture toward all consumer flash, and not a
        // fault. Rendered in neutral language for that reason.
        durability: DurabilityClass::AssumedUntrusted,
        wear: wear::observe(host),
        // Nothing records a clean shutdown yet. Never seen is not the same as
        // never happened, and neither is evidence that it works.
        graceful_shutdown: GracefulShutdown::NeverObserved,
        backup,
    }
}

/// The storage section when a replica may have spoken.
///
/// With a claim present, the section opens by naming it as a claim before
/// any line can be mistaken for a measurement taken here.
#[must_use]
pub fn describe_with(host: &dyn Host, now: Now, evidence: Option<&str>) -> Vec<String> {
    let posture = observed_with(host, evidence, now);
    let mut out = vec!["STORAGE".to_owned()];
    if evidence.is_some() {
        out.push(format!("  replica    quoting {} below", EVIDENCE_PREAMBLE));
    }

    out.push(format!("  flash      {}", posture.durability.describe()));
    out.push(match &posture.wear {
        vayucell_core::durability::WearIndicator::Readable(used) => {
            format!("  wear       {used}% of rated life used, as the device estimates it")
        }
        // Absent is printed, never omitted. A missing line cannot be told apart
        // from a node nobody looked for — the same rule the power-supply section
        // follows.
        vayucell_core::durability::WearIndicator::Absent => {
            "  wear       ABSENT   this device exposes no life-time estimate".to_owned()
        }
        vayucell_core::durability::WearIndicator::Unreliable(why) => {
            format!("  wear       UNRELIABLE   {why}")
        }
    });

    let concerns = posture.concerns(LAG_TARGET, now);
    if concerns.is_empty() {
        // Unreachable while there is no replicator, and written as a real branch
        // rather than an unreachable! so the day one exists this renders instead
        // of panicking on somebody's phone.
        out.push(
            "  settled    every storage question this device can answer is answered".to_owned(),
        );
    } else {
        for concern in concerns {
            out.push(format!("  concern    {concern}"));
        }
    }
    out
}

/// The replication lag this device would be held to, per ADR-0004 §2.
///
/// Carried even though nothing replicates, because [`Posture::concerns`]
/// requires it and a caller passing an arbitrary number would be inventing the
/// target rather than quoting it.
const LAG_TARGET: core::time::Duration = core::time::Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use super::{describe_with, observed_with, EVIDENCE_PREAMBLE};
    use vayucell_core::durability::{BackupState, GracefulShutdown, Now, RecoveryPoint};
    use vayucell_core::host::FakeHost;

    /// A fixed wall-clock second the receipt fixtures are dated against.
    const NOW_UNIX: u64 = 1_786_000_000;

    fn now() -> Now {
        Now {
            since_start: core::time::Duration::from_secs(10),
            today: Some(1_786_000_000),
        }
    }

    #[test]
    fn a_cell_with_no_replicator_says_it_is_the_only_copy() {
        // The whole reason this module exists. Somebody storing files in the
        // vault is keeping the only copy on a phone, and until this ran nothing
        // said so anywhere.
        let lines = describe_with(&FakeHost::new(), now(), None).join("\n");
        assert!(lines.contains("the only copy"), "{lines}");
        assert!(lines.contains("must never be"), "{lines}");
    }

    #[test]
    fn an_unbacked_up_device_is_told_so_as_well() {
        let lines = describe_with(&FakeHost::new(), now(), None).join("\n");
        assert!(lines.contains("nothing is being backed up"), "{lines}");
    }

    #[test]
    fn a_shed_ladder_nobody_has_watched_is_not_credited_here_either() {
        // Absence taken as evidence is the reading Article IV.3 forbids, and a
        // producer is exactly where that mistake gets made.
        assert_eq!(
            observed_with(&FakeHost::new(), None, now()).graceful_shutdown,
            GracefulShutdown::NeverObserved
        );
        let lines = describe_with(&FakeHost::new(), now(), None).join("\n");
        assert!(lines.contains("never been observed"), "{lines}");
    }

    #[test]
    fn nothing_here_reports_a_replica_or_a_backup_that_does_not_exist() {
        let p = observed_with(&FakeHost::new(), None, now());
        assert_eq!(p.recovery_point, RecoveryPoint::NoReplica);
        assert_eq!(p.backup, BackupState::NotConfigured);
    }

    #[test]
    fn a_device_that_exposes_no_wear_node_says_absent_rather_than_omitting_the_line() {
        // A missing line cannot be told apart from a node nobody looked for.
        let lines = describe_with(&FakeHost::new(), now(), None).join("\n");
        assert!(lines.contains("wear       ABSENT"), "{lines}");
    }

    #[test]
    fn a_device_that_does_expose_wear_reports_the_number() {
        let host = FakeHost::new().with_file("/sys/block/mmcblk0/device/life_time", "0x04 0x02\n");
        let lines = describe_with(&host, now(), None).join("\n");
        assert!(lines.contains("40% of rated life used"), "{lines}");
    }

    #[test]
    fn the_flash_posture_is_described_without_calling_it_a_fault() {
        // ADR-0004 §0: AssumedUntrusted is correct for essentially every device
        // and a posture rendered as a warning on every device teaches its reader
        // that warnings here mean nothing.
        let lines = describe_with(&FakeHost::new(), now(), None).join("\n");
        assert!(
            lines.contains("does not depend on it being honest"),
            "{lines}"
        );
        assert!(!lines.contains("flash      WARNING"), "{lines}");
    }

    #[test]
    fn a_receipt_whose_lag_is_past_target_is_quoted_as_exactly_that() {
        // The mirror last confirmed data 100 seconds old: past the 60-second
        // target, so this IS a concern, and it must read as a claim from the
        // receipt rather than something this phone measured.
        use vayucell_core::replica::Receipt;
        let receipt = Receipt::Replication {
            completed_unix: NOW_UNIX - 5,
            files: 2,
            bytes: 20,
            covered_mtime: NOW_UNIX - 100,
        };
        let drill = Receipt::RestoreDrill {
            completed_unix: NOW_UNIX - 5,
            files: 2,
            bytes: 20,
        };
        let evidence = format!("[{},{}]", receipt.render(), drill.render());
        let joined = describe_with(&FakeHost::new(), now(), Some(&evidence)).join("\n");
        // A fresh successful drill is proven work, and proven work is not a
        // concern line — its rendering lives in core's own tests.
        assert!(joined.contains(EVIDENCE_PREAMBLE), "{joined}");
        assert!(joined.contains("100s behind"), "{joined}");
    }

    #[test]
    fn a_lag_inside_the_target_produces_no_concern_line_at_all() {
        // Healthy is quiet here, same as every other section: a list that
        // repeats good news teaches its reader to stop reading it.
        use vayucell_core::replica::Receipt;
        let receipt = Receipt::Replication {
            completed_unix: NOW_UNIX - 5,
            files: 2,
            bytes: 20,
            covered_mtime: NOW_UNIX - 40,
        };
        let evidence = format!("[{}]", receipt.render());
        let lines = describe_with(&FakeHost::new(), now(), Some(&evidence));
        assert!(!lines.iter().any(|l| l.contains("behind")), "{lines:?}");
    }

    #[test]
    fn an_aged_receipt_says_nobody_is_still_measuring_rather_than_showing_the_number() {
        use vayucell_core::replica::Receipt;
        let receipt = Receipt::Replication {
            completed_unix: NOW_UNIX - 4_000,
            files: 2,
            bytes: 20,
            covered_mtime: NOW_UNIX - 4_000,
        };
        let evidence = format!("[{}]", receipt.render());
        let joined = describe_with(&FakeHost::new(), now(), Some(&evidence)).join("\n");
        assert!(joined.contains("nothing is still measuring"), "{joined}");
        assert!(!joined.contains("40s behind"), "{joined}");
    }

    #[test]
    fn an_unreadable_evidence_file_breaks_both_halves_openly() {
        let joined = describe_with(&FakeHost::new(), now(), Some("[{]")).join("\n");
        assert!(joined.contains("could not be read"), "{joined}");
        // A broken file must not downgrade to the calm sentence about there
        // being no backup at all.
        assert!(!joined.contains("nothing is being backed up"), "{joined}");
    }

    #[test]
    fn a_missing_file_is_the_same_as_no_claim_at_all() {
        let p = observed_with(
            &FakeHost::new(),
            None,
            Now {
                since_start: core::time::Duration::ZERO,
                today: Some(NOW_UNIX),
            },
        );
        assert_eq!(p.recovery_point, RecoveryPoint::NoReplica);
        assert_eq!(p.backup, BackupState::NotConfigured);
    }
}
