// SPDX-License-Identifier: Apache-2.0

//! Storage durability, per ADR-0004.
//!
//! # There is no way to say "your data is safe"
//!
//! The type this module is built around is [`RecoveryPoint`], and it has no
//! variant meaning *durable*. That is the whole design. ADR-0004 §1.1: a phone
//! is a replica, never the only copy — which is a durability guarantee **only
//! for data older than the replication lag**. Data written in the last N seconds
//! exists on exactly one device, and that device may be lying about having
//! written it.
//!
//! So the guarantee is stated as a number an operator can act on — *"the
//! off-device copy is 47 seconds behind"* — rather than an adjective they can
//! only believe. An adjective cannot be checked, cannot be graphed, and cannot
//! warn.
//!
//! # Assume the flash lies, and make it not matter
//!
//! ADR-0004 §0 records a feature that was designed, then withdrawn: a flush
//! honesty test. A sealed-battery phone cannot drop its own storage rail, and
//! the kernel's shutdown path flushes the device cache on the way out — so an
//! honest device and a maximally dishonest one produce byte-identical results.
//! Whatever shipped under that name would have been a green light from a test
//! that structurally could not go red for the reason it claimed to.
//!
//! [`DurabilityClass::AssumedUntrusted`] is therefore the default and the honest
//! answer for essentially every device. It is **not** a fault, and
//! [`DurabilityClass::describe`] says so in neutral language, because a posture
//! rendered as a warning teaches its reader to dismiss warnings.
//!
//! # Test what you control; assume the worst about what you do not
//!
//! Of the four things ADR-0004 §2 records, the one that is genuinely testable on
//! a device is [`GracefulShutdown`] — because it measures *this software's*
//! behaviour, not the flash controller's honesty. That is the rule the whole ADR
//! produces, and [`Posture`] is arranged so the testable field is the only one
//! that can ever read as verified.

use core::time::Duration;

/// How much data would be lost if this device stopped existing right now.
///
/// There is deliberately no `Durable` variant:
///
/// ```compile_fail
/// use vayucell_core::durability::RecoveryPoint;
/// // Nothing on a phone is entitled to say this.
/// let r: RecoveryPoint = RecoveryPoint::Durable;
/// ```
///
/// The closest thing to good news this type can express is
/// [`RecoveryPoint::Behind`] with a small duration, and that still names the
/// window in which data exists on one device only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryPoint {
    /// An off-device copy exists and is this far behind.
    Behind(Duration),
    /// Replication is configured but has never completed a cycle.
    ///
    /// Distinct from a large [`RecoveryPoint::Behind`] on purpose: "twelve hours
    /// behind" means twelve hours of data is at risk, and *this* means all of it
    /// is.
    NeverReplicated,
    /// The replica could not be reached, so the lag is unknown.
    ///
    /// Charter Article IV. The tempting reading — no news since the last
    /// successful sync, so probably still fine — is the one that reports a
    /// broken backup as a working one.
    Unreachable(String),
    /// No off-device copy is configured at all.
    ///
    /// The worst state and the easiest one to arrive in by doing nothing, which
    /// is why it is a named variant rather than the absence of a value.
    NoReplica,
}

impl RecoveryPoint {
    /// Whether an operator should be told about this now.
    ///
    /// Every state except a lag inside the target is worth surfacing. In
    /// particular an unreachable replica is *not* filtered out as noise — it is
    /// the state in which the number on the panel stops meaning anything.
    #[must_use]
    pub fn needs_attention(&self, target: Duration) -> bool {
        match self {
            RecoveryPoint::Behind(lag) => *lag > target,
            _ => true,
        }
    }
}

impl core::fmt::Display for RecoveryPoint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RecoveryPoint::Behind(d) => write!(
                f,
                "the off-device copy is {}s behind; anything written since then \
                 exists on this device only",
                d.as_secs()
            ),
            RecoveryPoint::NeverReplicated => f.write_str(
                "replication is configured but has never completed a cycle, so \
                 every byte here exists on this device only",
            ),
            RecoveryPoint::Unreachable(why) => write!(
                f,
                "the off-device copy could not be reached ({why}), so the lag is \
                 unknown and unknown is not small"
            ),
            RecoveryPoint::NoReplica => f.write_str(
                "no off-device copy is configured, so this phone is the only copy \
                 — which is the one thing ADR-0004 says a phone must never be",
            ),
        }
    }
}

/// What a lab fixture actually did. ADR-0004 §2.
///
/// Every field is required, so [`DurabilityClass::LabVerified`] cannot be
/// claimed bare. A class that could be set without naming the rig would be set
/// by somebody who rebooted a phone and watched the database survive, which is
/// the warm-reboot test §0 withdrew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabVerification {
    /// What was done.
    pub method: String,
    /// The physical fixture. A relay on the rail, a dummy-battery supply.
    pub fixture: String,
    /// When.
    pub date: String,
}

/// How much this software is willing to trust the flash beneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurabilityClass {
    /// The default, and the correct posture toward all consumer flash.
    AssumedUntrusted,
    /// One physical part, on one bench, on one day.
    LabVerified(LabVerification),
}

impl Default for DurabilityClass {
    /// [`DurabilityClass::AssumedUntrusted`].
    ///
    /// The default matters more here than usual: a `Default` that resolved to
    /// the trusting value would make every device that nobody configured look
    /// lab-verified, which is the exact inversion of ADR-0004.
    fn default() -> Self {
        Self::AssumedUntrusted
    }
}

impl DurabilityClass {
    /// Neutral language, deliberately.
    ///
    /// `AssumedUntrusted` is not a defect in the device and must not be rendered
    /// as one. A posture that reads as a warning on every device teaches its
    /// reader that warnings here mean nothing, and the next warning is a real
    /// one.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            DurabilityClass::AssumedUntrusted => "the flash is assumed to be capable of \
                 acknowledging a flush it did not perform, as all consumer flash is; \
                 the design does not depend on it being honest"
                .to_owned(),
            DurabilityClass::LabVerified(v) => format!(
                "one physical part was power-fault tested on {} using {} ({}); this \
                 describes that part and never the model",
                v.date, v.fixture, v.method
            ),
        }
    }

    /// Whether this class was established by a real power-fault rig.
    ///
    /// Advisory only. ADR-0004 §2: it never grants a tier and never relaxes any
    /// other guarantee, because one part on one bench is not a population.
    #[must_use]
    pub const fn is_lab_verified(&self) -> bool {
        matches!(self, DurabilityClass::LabVerified(_))
    }
}

/// What the device says about its own wear, and whether it says anything.
///
/// The variant names record **whether the device exposes an indicator**, not
/// whether it reported good news — which is why [`WearIndicator::Absent`] is not
/// a synonym for healthy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WearIndicator {
    /// The device exposes a life-used estimate, in percent.
    Readable(u8),
    /// The device exposes nothing. Reported as absent, never as healthy.
    Absent,
    /// The device exposes something that did not make sense.
    Unreliable(String),
}

/// Whether the shed ladder has ever actually completed on this device.
///
/// ADR-0004 §2 calls this the one durability property genuinely testable
/// on-device, because it measures this software's behaviour rather than the flash
/// controller's honesty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GracefulShutdown {
    /// The ladder ran to completion and the database was consistent on restart.
    Verified,
    /// It has not been observed. Not a failure — an absence.
    NeverObserved,
    /// It ran and something was inconsistent afterwards.
    Failed,
}

/// Whether a backup has ever been restored from.
///
/// The P6 gate, and the reason this type is not a boolean. A backup nobody has
/// restored is a file of the expected size — every property anybody checked is a
/// property of the file, not of the restore. `NeverRestored` is therefore a
/// first-class answer and it renders as unverified, permanently, until somebody
/// does the one thing that settles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupState {
    /// A restore was performed and the result was checked.
    Restored {
        /// When.
        when: String,
    },
    /// Backups are being written and none has been restored.
    NeverRestored,
    /// A restore was attempted and did not produce a usable result.
    RestoreFailed(String),
    /// Nothing is being backed up.
    NotConfigured,
}

impl BackupState {
    /// Whether this backup has been shown to work.
    ///
    /// Only [`BackupState::Restored`] qualifies. A written backup is evidence
    /// that bytes were written.
    #[must_use]
    pub const fn is_proven(&self) -> bool {
        matches!(self, BackupState::Restored { .. })
    }
}

impl core::fmt::Display for BackupState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BackupState::Restored { when } => {
                write!(f, "a restore was performed and checked on {when}")
            }
            BackupState::NeverRestored => f.write_str(
                "backups are being written and none has ever been restored, so what \
                 is verified is that files exist, not that they can be recovered",
            ),
            BackupState::RestoreFailed(why) => {
                write!(
                    f,
                    "the last restore attempt did not produce a usable result: {why}"
                )
            }
            BackupState::NotConfigured => f.write_str("nothing is being backed up"),
        }
    }
}

/// Everything this software is willing to say about storage on this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posture {
    /// How far behind the off-device copy is.
    pub recovery_point: RecoveryPoint,
    /// How much the flash is trusted.
    pub durability: DurabilityClass,
    /// What the device says about its wear.
    pub wear: WearIndicator,
    /// Whether the shed ladder has completed here.
    pub graceful_shutdown: GracefulShutdown,
    /// Whether a backup has been restored from.
    pub backup: BackupState,
}

impl Posture {
    /// The posture of a device nobody has configured.
    ///
    /// Every field is at its least reassuring value, because that is what is
    /// true before anything has been set up — and because a `Default` that
    /// resolved to good news would be this module telling a first-run device it
    /// was protected.
    #[must_use]
    pub fn unconfigured() -> Self {
        Self {
            recovery_point: RecoveryPoint::NoReplica,
            durability: DurabilityClass::AssumedUntrusted,
            wear: WearIndicator::Absent,
            graceful_shutdown: GracefulShutdown::NeverObserved,
            backup: BackupState::NotConfigured,
        }
    }

    /// The lines an operator should read, worst first.
    ///
    /// Returns the reasons this device's storage is not settled. An empty slice
    /// means every one of them was answered — which requires, among other
    /// things, a backup somebody has actually restored.
    #[must_use]
    pub fn concerns(&self, lag_target: Duration) -> Vec<String> {
        let mut out = Vec::new();

        if self.recovery_point.needs_attention(lag_target) {
            out.push(self.recovery_point.to_string());
        }
        if !self.backup.is_proven() {
            out.push(self.backup.to_string());
        }
        match self.graceful_shutdown {
            GracefulShutdown::Failed => out.push(
                "the shed ladder ran and the database was inconsistent afterwards, \
                 which is the one durability failure that is ours rather than the \
                 device's"
                    .to_owned(),
            ),
            GracefulShutdown::NeverObserved => out.push(
                "the shed ladder has never been observed completing on this device".to_owned(),
            ),
            GracefulShutdown::Verified => {}
        }
        if let WearIndicator::Unreliable(what) = &self.wear {
            out.push(format!(
                "the wear indicator returned {what}, which is not usable"
            ));
        }

        // Deliberately NOT a concern: DurabilityClass::AssumedUntrusted, and a
        // wear indicator this device simply does not expose. Neither is a fault,
        // and listing them beside real problems is how a list of real problems
        // stops being read.
        out
    }
}
