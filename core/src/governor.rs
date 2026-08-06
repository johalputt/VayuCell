// SPDX-License-Identifier: Apache-2.0

//! The Battery Safety Governor, per ADR-0002.
//!
//! CHARTER Article III.1: **no capability that serves traffic may ship before
//! this.** Everything else in the project is an engineering inconvenience; this
//! one is a fire in somebody's home.
//!
//! # What this claims, and what it does not
//!
//! It claims the risk is measured, bounded and reported. It does **not** claim
//! the battery is safe, and it does not claim swelling is detected — swelling is
//! a physical deformation and software cannot see it. Physical inspection is
//! named as the definitive check, here and everywhere it is discussed.
//!
//! # A ceiling that is never read back is not a control
//!
//! It is a configuration, and vendor charging daemons reset configurations on
//! cable events, thermal events and firmware updates. A ceiling that held for six
//! weeks and then quietly stopped is indistinguishable — from the operator's
//! side — from a governor that never worked at all.
//!
//! So [`Governor::enforce`] applies, then **reads the value back from the
//! hardware**, and treats three outcomes as distinct: it worked, something undid
//! it ([`Reason::Reverted`]), and it could not be checked
//! ([`Reason::Unverifiable`]). The last of those is never reported as the first.
//!
//! # Escalation is automatic; leaving PROTECT and HALT is not
//!
//! A cell that reached a critical threshold has told you something about itself.
//! Clearing that requires a person, who is prompted to look at the phone — so
//! there is no method here that lowers the level, at any level, for any reason.
//! Recovery is [`Governor::after_inspection`], which consumes the governor and
//! demands a recorded [`Inspection`] to produce a new one.
//!
//! ```compile_fail
//! # use vayucell_core::governor::{Governor, Thresholds};
//! let mut g = Governor::new(Thresholds::recommended());
//! // There is no such method, and adding one is the reviewable act.
//! g.set_level(vayucell_core::governor::Level::Normal);
//! ```
//!
//! A positive control:
//!
//! ```
//! use vayucell_core::governor::{Governor, Level, Thresholds};
//! let g = Governor::new(Thresholds::recommended());
//! assert_eq!(g.level(), Level::Normal);
//! ```

use core::fmt;

use crate::battery::{Celsius, Percent, Reading, StateOfHealth};

/// How far the governor has escalated.
///
/// Ordered, and the ordering is load-bearing: escalation compares levels, and a
/// variant inserted in the wrong place would silently reorder the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Ceiling held and verified; temperature nominal.
    Normal,
    /// Workload shed, charge current reduced, the panel warns.
    Derated,
    /// Serving stopped, charging suspended, data checkpointed.
    Protect,
    /// Clean shutdown. Requires a person to clear.
    Halt,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Normal => "NORMAL",
            Level::Derated => "DERATED",
            Level::Protect => "PROTECT",
            Level::Halt => "HALT",
        })
    }
}

/// Why the governor moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The ceiling could not be written.
    ApplyFailed(String),
    /// The ceiling was written but could not be read back. **Not** working.
    Unverifiable(String),
    /// The ceiling was written, read back, and did not match. Something undid it.
    Reverted {
        /// What was asked for.
        wanted: Percent,
        /// What the hardware actually reports.
        got: Percent,
    },
    /// Pack temperature crossed a threshold.
    Temperature {
        /// The threshold that was crossed.
        threshold: Celsius,
    },
    /// State of health fell below a threshold.
    Health {
        /// The threshold that was crossed.
        threshold: Percent,
    },
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::ApplyFailed(e) => write!(f, "the charge ceiling could not be set: {e}"),
            Reason::Unverifiable(e) => write!(
                f,
                "the charge ceiling could not be read back, so it is unverified, \
                 not working: {e}"
            ),
            Reason::Reverted { wanted, got } => write!(
                f,
                "the charge ceiling was set to {wanted} and the hardware now \
                 reports {got}; something reverted it"
            ),
            Reason::Temperature { threshold } => {
                write!(f, "pack temperature exceeded {threshold}")
            }
            Reason::Health { threshold } => {
                write!(f, "state of health fell below {threshold}")
            }
        }
    }
}

/// A state change, with the reading that caused it.
///
/// `evidence` is not an `Option`. A transition nobody can explain afterwards is
/// the thing this type exists to prevent — see ADR-0002 §4, "every transition
/// names its reading".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// Where it was.
    pub from: Level,
    /// Where it went. Always higher than `from`.
    pub to: Level,
    /// Why.
    pub reason: Reason,
    /// The reading at the moment of the change.
    pub evidence: String,
}

impl fmt::Display for Transition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} -> {}: {} [{}]",
            self.from, self.to, self.reason, self.evidence
        )
    }
}

/// The thresholds the ladder is measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    warn_temp: Celsius,
    critical_temp: Celsius,
    hard_stop_temp: Celsius,
    degraded_health: Percent,
    retire_health: Percent,
}

/// Why a threshold set was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdError {
    /// Temperatures were not strictly increasing.
    TemperatureNotAscending,
    /// Health thresholds were not strictly decreasing.
    HealthNotDescending,
    /// A hard stop above the recommended maximum.
    HardStopTooHigh,
}

impl fmt::Display for ThresholdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ThresholdError::TemperatureNotAscending => {
                "temperature thresholds must be warn < critical < hard stop"
            }
            ThresholdError::HealthNotDescending => "health thresholds must be degraded > retire",
            ThresholdError::HardStopTooHigh => {
                "the hard stop may be configured downward, never above 60 °C"
            }
        })
    }
}

impl Thresholds {
    /// The hard stop this project will never allow to be raised above.
    ///
    /// ADR-0002 open decision 2: 60 °C pack, "configurable downward only". An
    /// operator who wants a safer number gets it; one who wants a laxer one does
    /// not, because the risk is not theirs alone to take — it is taken by
    /// everyone in the building.
    pub const MAX_HARD_STOP: Celsius = Celsius::new(60);

    /// ADR-0002's recommended values.
    #[must_use]
    pub fn recommended() -> Self {
        Self {
            warn_temp: Celsius::new(45),
            critical_temp: Celsius::new(52),
            hard_stop_temp: Self::MAX_HARD_STOP,
            degraded_health: Percent::clamped(80),
            retire_health: Percent::clamped(60),
        }
    }

    /// Builds a threshold set.
    ///
    /// # Errors
    ///
    /// Refuses a set whose ladder is not ordered, or a hard stop above
    /// [`Thresholds::MAX_HARD_STOP`].
    pub fn new(
        warn_temp: Celsius,
        critical_temp: Celsius,
        hard_stop_temp: Celsius,
        degraded_health: Percent,
        retire_health: Percent,
    ) -> Result<Self, ThresholdError> {
        if !(warn_temp < critical_temp && critical_temp < hard_stop_temp) {
            return Err(ThresholdError::TemperatureNotAscending);
        }
        if degraded_health <= retire_health {
            return Err(ThresholdError::HealthNotDescending);
        }
        if hard_stop_temp > Self::MAX_HARD_STOP {
            return Err(ThresholdError::HardStopTooHigh);
        }
        Ok(Self {
            warn_temp,
            critical_temp,
            hard_stop_temp,
            degraded_health,
            retire_health,
        })
    }
}

/// What a person saw when they looked at the phone.
///
/// ADR-0002 §6: software cannot measure a millimetre of deformation. This type
/// is how the one check that can is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inspection {
    /// Placed face-down on a flat table; it lay flat and no edge was lifting.
    LiesFlat,
    /// It rocked, or an edge was lifting. The cell is damaged.
    Deformed,
}

/// A charge-limiting mechanism on this device.
///
/// `verify` is not optional, for the same reason the capability registry's is
/// not: a control that cannot be read back is indistinguishable from one that
/// silently stopped working.
pub trait ChargeMechanism {
    /// Asks the hardware to hold the ceiling.
    ///
    /// # Errors
    ///
    /// Returns a description of what refused.
    fn apply(&mut self, ceiling: Percent) -> Result<(), String>;

    /// Reads the ceiling back **from the hardware**.
    ///
    /// # Errors
    ///
    /// Returns a description of why it could not be read. That is never the same
    /// as the ceiling not being held.
    fn verify(&self) -> Result<Percent, String>;
}

/// The governor.
#[derive(Debug, Clone)]
pub struct Governor {
    level: Level,
    thresholds: Thresholds,
    history: Vec<Transition>,
}

impl Governor {
    /// A governor at [`Level::Normal`].
    #[must_use]
    pub fn new(thresholds: Thresholds) -> Self {
        Self {
            level: Level::Normal,
            thresholds,
            history: Vec::new(),
        }
    }

    /// The current level.
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }

    /// Every transition so far, in order.
    #[must_use]
    pub fn history(&self) -> &[Transition] {
        &self.history
    }

    /// Whether the governor permits traffic to be served right now.
    ///
    /// The single question the rest of the system asks. `Protect` and `Halt`
    /// both mean no.
    #[must_use]
    pub fn may_serve(&self) -> bool {
        self.level <= Level::Derated
    }

    /// Raises the level. Never lowers it.
    ///
    /// Returns the transition if this actually escalated, and `None` if the
    /// governor was already at or above the target — which is not a failure, and
    /// must not be recorded as a fresh event or the log fills with repeats of
    /// one problem and buries the next one.
    fn escalate(&mut self, to: Level, reason: Reason, evidence: String) -> Option<Transition> {
        if to <= self.level {
            return None;
        }
        let t = Transition {
            from: self.level,
            to,
            reason,
            evidence,
        };
        self.level = to;
        self.history.push(t.clone());
        Some(t)
    }

    /// Considers one reading and escalates if it warrants it.
    ///
    /// Thresholds are checked from the most severe downward, so a reading that
    /// crosses several lands at the highest one in a single step rather than
    /// walking the ladder and logging three transitions for one event.
    pub fn observe(&mut self, reading: &Reading) -> Option<Transition> {
        let temp = reading.temp.to_celsius();
        let evidence = reading.evidence();

        if temp >= self.thresholds.hard_stop_temp {
            return self.escalate(
                Level::Halt,
                Reason::Temperature {
                    threshold: self.thresholds.hard_stop_temp,
                },
                evidence,
            );
        }
        if temp >= self.thresholds.critical_temp {
            return self.escalate(
                Level::Protect,
                Reason::Temperature {
                    threshold: self.thresholds.critical_temp,
                },
                evidence,
            );
        }

        // Health is only consulted where it was actually measured. Unknown is
        // not zero, and a device that cannot report design capacity must not be
        // retired for it.
        if let StateOfHealth::Measured(soh) = reading.state_of_health() {
            if soh < self.thresholds.retire_health {
                return self.escalate(
                    Level::Protect,
                    Reason::Health {
                        threshold: self.thresholds.retire_health,
                    },
                    evidence,
                );
            }
            if soh < self.thresholds.degraded_health {
                return self.escalate(
                    Level::Derated,
                    Reason::Health {
                        threshold: self.thresholds.degraded_health,
                    },
                    evidence,
                );
            }
        }

        if temp >= self.thresholds.warn_temp {
            return self.escalate(
                Level::Derated,
                Reason::Temperature {
                    threshold: self.thresholds.warn_temp,
                },
                evidence,
            );
        }
        None
    }

    /// Applies the ceiling and reads it back.
    ///
    /// The loop that makes this a control rather than a configuration. Any of
    /// the three failure outcomes derates; none of them is reported as success.
    pub fn enforce(
        &mut self,
        mechanism: &mut dyn ChargeMechanism,
        ceiling: Percent,
        reading: &Reading,
    ) -> Option<Transition> {
        let evidence = reading.evidence();

        if let Err(e) = mechanism.apply(ceiling) {
            return self.escalate(Level::Derated, Reason::ApplyFailed(e), evidence);
        }
        let got = match mechanism.verify() {
            Ok(v) => v,
            Err(e) => {
                return self.escalate(Level::Derated, Reason::Unverifiable(e), evidence);
            }
        };
        // A hardware ceiling at or below what was asked for is satisfying it.
        // Above it is not, however close.
        if got > ceiling {
            return self.escalate(
                Level::Derated,
                Reason::Reverted {
                    wanted: ceiling,
                    got,
                },
                evidence,
            );
        }
        None
    }

    /// Returns a fresh governor after a person has looked at the phone.
    ///
    /// Consumes this one, because there is no in-place path down the ladder and
    /// there should not be. [`Inspection::Deformed`] does not recover: a cell
    /// somebody has seen deforming is not a cell to resume serving on, whatever
    /// the sensors say afterwards.
    ///
    /// # Errors
    ///
    /// Returns the governor unchanged, at [`Level::Halt`], when the inspection
    /// found deformation.
    pub fn after_inspection(mut self, inspection: Inspection) -> Result<Self, Self> {
        match inspection {
            Inspection::LiesFlat => {
                self.level = Level::Normal;
                Ok(self)
            }
            Inspection::Deformed => {
                self.level = Level::Halt;
                Err(self)
            }
        }
    }
}
