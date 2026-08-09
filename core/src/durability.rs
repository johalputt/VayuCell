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
//!
//! # A number is only honest while somebody is still taking it
//!
//! ADR-0004 §1.1 does not say the panel shows a lag. It says it shows one
//! *"continuously, as a live figure"*, and the whole argument of §1.1 is that a
//! number beats an adjective because a number can be checked. A number nobody
//! has re-measured is an adjective wearing a number's clothes: `47` renders
//! identically whether it was taken a second ago or the morning the replicator
//! died, and the morning the replicator died is exactly when an operator most
//! needs to be told.
//!
//! So [`RecoveryPoint::Behind`] carries **when it was measured**, not only what
//! was measured, and this module deliberately **does not implement `Display`**.
//! `Display` is the hole: `format!("{rp}")` renders a recovery point with no
//! clock in scope and no way for the type to object. [`RecoveryPoint::describe`]
//! takes the clock's reading instead, so the age travels with the figure and a
//! measurement past [`MEASUREMENT_STANDS_FOR`] says so in the sentence the
//! operator reads.
//!
//! This is the same repair, and the same defect, as
//! [`crate::ingress::Reachability`]. It is written here before the replicator
//! exists rather than after, which is the only difference and the one worth
//! having.

use core::time::Duration;

/// How long a lag measurement stands before it stops being a live figure.
///
/// ADR-0004 §2 sets the default lag *target* at 60 seconds, so a replicator that
/// is working reports many times inside this window. Five minutes without a new
/// measurement is therefore not a slow cycle; it is something having stopped,
/// and the panel says so rather than going on showing the last good number.
///
/// The tests that pin this use literal durations rather than this constant, for
/// the reason [`crate::ingress::FRESH_FOR`] does: a test written against the
/// constant it is pinning stays green when the constant is widened.
pub const MEASUREMENT_STANDS_FOR: Duration = Duration::from_secs(5 * 60);

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
/// And there is deliberately no `Display`, so a recovery point cannot be
/// rendered without a clock in scope:
///
/// ```compile_fail
/// use core::time::Duration;
/// use vayucell_core::durability::RecoveryPoint;
/// let r = RecoveryPoint::NoReplica;
/// let s = format!("{r}");
/// ```
///
/// The closest thing to good news this type can express is
/// [`RecoveryPoint::Behind`] with a small lag and a recent measurement, and that
/// still names the window in which data exists on one device only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryPoint {
    /// An off-device copy exists and was this far behind when last measured.
    Behind {
        /// How far behind the off-device copy was.
        lag: Duration,
        /// The clock's reading when that was measured.
        ///
        /// Monotonic, from [`crate::runtime::Clock::elapsed`]. Present because
        /// ADR-0004 §1.1 promises a *live* figure, and a number with no age
        /// cannot be told apart from one taken this morning.
        measured_at: Duration,
    },
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
    /// Whether this figure is current enough to be shown as a live one.
    ///
    /// Only a [`RecoveryPoint::Behind`] measured within
    /// [`MEASUREMENT_STANDS_FOR`] is. A measurement stamped ahead of the clock
    /// cannot be aged, so it is not live either — an age that cannot be
    /// established is not an age.
    #[must_use]
    pub const fn is_live(&self, now: Duration) -> bool {
        match self {
            Self::Behind { measured_at, .. } => match now.checked_sub(*measured_at) {
                Some(age) => age.checked_sub(MEASUREMENT_STANDS_FOR).is_none(),
                None => false,
            },
            Self::NeverReplicated | Self::Unreachable(_) | Self::NoReplica => false,
        }
    }

    /// Whether an operator should be told about this now.
    ///
    /// Every state except a live lag inside the target is worth surfacing. Two
    /// of those states are easy to miss and both are here deliberately:
    ///
    /// - An unreachable replica is *not* filtered out as noise. It is the state
    ///   in which the number on the panel stops meaning anything.
    /// - **A lag nobody has re-measured is not a lag that is fine.** Without
    ///   this, a replicator that died an hour ago goes on presenting its last
    ///   good reading — 47 seconds, inside target, no concern raised — for as
    ///   long as the process lives.
    ///
    /// `now` is required rather than optional, so the second case cannot be
    /// skipped by a caller who did not think of it.
    #[must_use]
    pub const fn needs_attention(&self, target: Duration, now: Duration) -> bool {
        match self {
            Self::Behind { lag, .. } => {
                if !self.is_live(now) {
                    return true;
                }
                // `lag > target` without a const comparison operator.
                lag.checked_sub(target).is_some()
            }
            Self::NeverReplicated | Self::Unreachable(_) | Self::NoReplica => true,
        }
    }

    /// What to tell the operator, as of `now`.
    ///
    /// Deliberately a method and not a [`core::fmt::Display`] impl. `Display` is
    /// the hole this module closed: it renders with no clock in scope and no way
    /// for the type to object, which is how a figure ADR-0004 §1.1 promises will
    /// be *live* gets printed hours after anybody measured it.
    #[must_use]
    pub fn describe(&self, now: Duration) -> String {
        match self {
            Self::Behind { lag, measured_at } => {
                if self.is_live(now) {
                    return format!(
                        "the off-device copy is {}s behind; anything written since then \
                         exists on this device only",
                        lag.as_secs()
                    );
                }
                match now.checked_sub(*measured_at) {
                    Some(age) => format!(
                        "the off-device copy was {}s behind when it was last measured, \
                         and that was {}s ago — the figure is no longer live, and a lag \
                         nobody is still measuring is not a lag that is fine",
                        lag.as_secs(),
                        age.as_secs()
                    ),
                    None => format!(
                        "the off-device copy was {}s behind at a moment stamped ahead of \
                         this cell's clock, so how old that figure is cannot be \
                         established and it is not a live one",
                        lag.as_secs()
                    ),
                }
            }
            Self::NeverReplicated => {
                "replication is configured but has never completed a cycle, so \
                 every byte here exists on this device only"
                    .to_owned()
            }
            Self::Unreachable(why) => format!(
                "the off-device copy could not be reached ({why}), so the lag is \
                 unknown and unknown is not small"
            ),
            Self::NoReplica => "no off-device copy is configured, so this phone is the only copy \
                 — which is the one thing ADR-0004 says a phone must never be"
                .to_owned(),
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

/// How long a completed restore drill stands before another is due.
///
/// ADR-0004 §4 says the backup system "restores an archive **on a schedule**"
/// and reports the time of the last verified restore. It does not set the
/// interval. A month: long enough that the drill is not a standing thermal load
/// on a phone — §4 makes it shed by the governor for exactly that reason — and
/// short enough that a backup chain which broke is found inside a month rather
/// than inside a year.
///
/// Pinned by tests written against literal durations, not against this constant.
pub const DRILL_STANDS_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Whether a backup has ever been restored from, and whether that is still
/// recent enough to mean anything.
///
/// The P6 gate, and the reason this type is not a boolean. A backup nobody has
/// restored is a file of the expected size — every property anybody checked is a
/// property of the file, not of the restore. `NeverRestored` is therefore a
/// first-class answer and it renders as unverified, permanently, until somebody
/// does the one thing that settles it.
///
/// And a drill that ran once is not a schedule. ADR-0004 §4 says the archive is
/// restored *on a schedule* precisely because a backup chain breaks silently —
/// the upload keeps succeeding, and the thing that would notice is the restore
/// nobody has run since March. So [`BackupState::Restored`] carries a wall-clock
/// stamp, [`BackupState::is_proven`] takes the current date, and there is no
/// `Display` impl to render one without it:
///
/// ```compile_fail
/// use vayucell_core::durability::BackupState;
/// let b = BackupState::NotConfigured;
/// let s = format!("{b}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupState {
    /// A restore was performed and the result was checked.
    Restored {
        /// Seconds since the Unix epoch when the drill ran and was checked.
        ///
        /// Wall clock, not [`crate::runtime::Clock::elapsed`], and deliberately:
        /// this fact outlived the process that is reading it, and a monotonic
        /// clock that started at zero on boot cannot date something that
        /// happened last month.
        at_unix: u64,
    },
    /// Backups are being written and none has been restored.
    NeverRestored,
    /// A restore was attempted and did not produce a usable result.
    RestoreFailed(String),
    /// Nothing is being backed up.
    NotConfigured,
}

impl BackupState {
    /// Whether this backup has been shown to work, **as of `today`**.
    ///
    /// Only a [`BackupState::Restored`] whose drill ran within
    /// [`DRILL_STANDS_FOR`] qualifies. A written backup is evidence that bytes
    /// were written, and a drill from eighteen months ago is evidence about an
    /// archive that no longer exists.
    ///
    /// `today` is `None` when the host would not say what day it is, and that is
    /// **not proven**. A cell that cannot date the drill cannot tell whether it
    /// is current, and Charter Article IV.3 does not permit reporting what could
    /// not be checked as clean. A stamp ahead of the clock is refused for the
    /// same reason: an age that cannot be established is not an age.
    #[must_use]
    pub const fn is_proven(&self, today: Option<u64>) -> bool {
        match (self, today) {
            (Self::Restored { at_unix }, Some(now)) => match now.checked_sub(*at_unix) {
                Some(age) => age <= DRILL_STANDS_FOR.as_secs(),
                None => false,
            },
            (Self::Restored { .. }, None)
            | (Self::NeverRestored | Self::RestoreFailed(_) | Self::NotConfigured, _) => false,
        }
    }

    /// What to tell the operator, as of `today`.
    ///
    /// A method rather than a [`core::fmt::Display`] impl, for the reason
    /// [`RecoveryPoint::describe`] is: `Display` renders with no clock in scope,
    /// and "a restore was performed and checked" is a sentence whose whole
    /// meaning depends on when.
    #[must_use]
    pub fn describe(&self, today: Option<u64>) -> String {
        match self {
            Self::Restored { at_unix } => match today {
                Some(now) if self.is_proven(today) => format!(
                    "a restore was performed and checked {} days ago",
                    (now.saturating_sub(*at_unix)) / (24 * 60 * 60)
                ),
                Some(now) if now >= *at_unix => format!(
                    "the last restore drill was {} days ago and one is due every {}; \
                     what is verified is that a backup worked then, not that this one \
                     does",
                    (now - *at_unix) / (24 * 60 * 60),
                    spell_days(DRILL_STANDS_FOR)
                ),
                Some(_) => "the last restore drill is stamped ahead of this cell's clock, so \
                     how old it is cannot be established and it is not evidence"
                    .to_owned(),
                None => "a restore drill was recorded and this cell cannot tell what day it \
                     is, so whether it is still current is unknown — and unknown is not \
                     recent"
                    .to_owned(),
            },
            Self::NeverRestored => {
                "backups are being written and none has ever been restored, so what \
                 is verified is that files exist, not that they can be recovered"
                    .to_owned()
            }
            Self::RestoreFailed(why) => {
                format!("the last restore attempt did not produce a usable result: {why}")
            }
            Self::NotConfigured => "nothing is being backed up".to_owned(),
        }
    }
}

/// A duration in whole days, for the one sentence that needs it.
fn spell_days(d: Duration) -> String {
    format!("{} days", d.as_secs() / (24 * 60 * 60))
}

/// The two readings a storage posture needs to judge itself.
///
/// Bundled rather than passed as two arguments so a caller cannot supply one and
/// forget the other, and so the reason there are two is written down where it is
/// used: the replication lag is a duration inside this process and wants the
/// monotonic clock; the restore drill happened before this process existed and
/// only a wall clock can date it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    /// How long this process's clock has advanced, from
    /// [`crate::runtime::Clock::elapsed`].
    pub since_start: Duration,
    /// What day it is, from [`crate::runtime::Clock::wall_clock_unix`].
    ///
    /// `None` when the host would not say. Never read as recent.
    pub today: Option<u64>,
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
    /// things, a replication lag somebody is still measuring and a backup
    /// somebody has actually restored, recently enough for it to still be about
    /// this backup.
    ///
    /// `now` carries both clock readings and is required: without the monotonic
    /// one this would report a dead replicator's last good number as no concern,
    /// and without the wall-clock one it would report a restore drill from
    /// eighteen months ago as a working backup.
    #[must_use]
    pub fn concerns(&self, lag_target: Duration, now: Now) -> Vec<String> {
        let mut out = Vec::new();

        if self
            .recovery_point
            .needs_attention(lag_target, now.since_start)
        {
            out.push(self.recovery_point.describe(now.since_start));
        }
        if !self.backup.is_proven(now.today) {
            out.push(self.backup.describe(now.today));
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
