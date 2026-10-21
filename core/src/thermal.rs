// SPDX-License-Identifier: Apache-2.0

//! What the cell has been observed to do, and what that permits.
//!
//! # Why this module exists
//!
//! The catalogue compares a service's thermal envelope against the device's
//! thermal class, and until now the device's class could only ever be absent:
//! the picker read the tier and the memory, and every row that needed a
//! thermal answer said *cannot be told*. That was honest, and it was also the
//! end of the road — a catalogue that can never place a cell is a catalogue
//! that never says anything about the battery.
//!
//! PLAN §11 asks for the class and says what it needs: *"a governor that has
//! watched a cell heat"*. So the class is not a number read from a node. It is
//! a reading of the cell's own temperature, seen over time, put through a
//! policy — and the policy is this module.
//!
//! # The class is a ceiling, not a description
//!
//! [`ThermalClass`] orders work from idle to heavy, and the class a device
//! reports is **the heaviest work this cell may now do**. The catalogue asks
//! `service.worse_than(device)`: a service that wants heavier work than the
//! cell may do does not fit. A cool cell is therefore a *capable* cell, and a
//! hot one is a restricted one — which is the only direction that makes the
//! comparison mean anything.
//!
//! # The thresholds are the governor's own
//!
//! The policy uses [`Thresholds`] — the same warn, critical and hard-stop
//! rungs the governor escalates on — and introduces no numbers of its own. A
//! second set of temperature limits would be a second ladder, and this
//! repository has one ladder for this.
//!
//! # An unobserved class is not a low class
//!
//! A cell that has never been read has no class at all. [`ThermalHistory::class`]
//! answers `None`, and the catalogue turns that into *cannot be told* — not
//! into `Idle`, and not into a class that would make an unread device look
//! capable. Nothing about a cell that was never observed may be inferred, and
//! the one thing that may not be inferred about a hot cell is that it is
//! comfortable.
//!
//! # The history keeps the worst, and why
//!
//! The governor raises its level and never lowers it: a cell that has been
//! warned stays warned until somebody looks at the phone
//! ([`crate::governor::Governor::after_inspection`]). The class follows the
//! same rule, so it is computed from the **worst** reading observed rather
//! than the latest. A cell that cools by a degree has not recovered, and a
//! catalogue that forgot a warning thirty seconds after it was issued would be
//! offering heavy work to a cell the governor is still derating.
//!
//! Keeping the worst rather than the whole series is deliberate. The policy
//! needs the worst, and a list that grows for the life of a device is a list
//! nobody reads; the count of observations is kept so that "we have watched
//! this cell" can be said without keeping every reading.

use core::fmt;

use crate::battery::{Celsius, Reading};
use crate::governor::Thresholds;

/// How hard a workload leans on the cell.
///
/// The order is the point: the governor compares what a service wants against
/// what the device is willing to give, and "worse" means hotter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThermalClass {
    /// The cell does essentially nothing: a report, a wake-up, a ping.
    Idle,
    /// Steady background work, such as a sync daemon answering a request a minute.
    Light,
    /// Continuous work that will run for hours, such as transcoding.
    Sustained,
    /// A flat-out multi-core workload with the accelerator running.
    Heavy,
}

impl ThermalClass {
    /// Every class, lightest first.
    ///
    /// The order is the order of [`worse_than`](ThermalClass::worse_than), and
    /// a test holds it: a class added in the wrong place would silently change
    /// what the catalogue allows.
    pub const ALL: [Self; 4] = [Self::Idle, Self::Light, Self::Sustained, Self::Heavy];

    /// The class's place in the order, lowest first.
    const fn rank(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Light => 1,
            Self::Sustained => 2,
            Self::Heavy => 3,
        }
    }

    /// Whether this class is worse — hotter — than `other`.
    ///
    /// The comparison the governor wants: a device willing to run only
    /// [`ThermalClass::Light`] work cannot carry a [`ThermalClass::Heavy`]
    /// service, and a device willing to run heavy work can carry anything.
    #[must_use]
    pub const fn worse_than(self, other: Self) -> bool {
        self.rank() > other.rank()
    }
}

impl fmt::Display for ThermalClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Idle => "idle",
            Self::Light => "light",
            Self::Sustained => "sustained",
            Self::Heavy => "heavy",
        })
    }
}

/// What this cell has been observed to do.
///
/// A history is empty when nothing has been read, and that is not the same as
/// a history of a cool cell: the first is *no answer*, the second is an answer
/// that permits heavy work. The two are kept apart by
/// [`class`](ThermalHistory::class) returning `None` for the first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ThermalHistory {
    worst: Option<Celsius>,
    observed: u32,
}

impl ThermalHistory {
    /// A history in which nothing has been observed.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            worst: None,
            observed: 0,
        }
    }

    /// Records one reading of the cell.
    ///
    /// The reading's own temperature is what is kept; everything else about it
    /// belongs to the governor, which has its own opinion about charge current
    /// and state of health.
    pub fn observe(&mut self, reading: &Reading) {
        self.observe_celsius(reading.temp.to_celsius());
    }

    /// Records one temperature.
    ///
    /// Separate from [`observe`](ThermalHistory::observe) so that a caller
    /// holding a temperature rather than a whole reading — the tests, and a
    /// caller that has already unwrapped what it needed — does not have to
    /// build a [`Reading`] to say what the cell was.
    pub fn observe_celsius(&mut self, temp: Celsius) {
        self.observed = self.observed.saturating_add(1);
        self.worst = Some(match self.worst {
            Some(worst) => worst.max(temp),
            None => temp,
        });
    }

    /// How many readings have been observed.
    #[must_use]
    pub const fn observed(&self) -> u32 {
        self.observed
    }

    /// The hottest this cell has been seen to be, if it has been seen.
    #[must_use]
    pub const fn worst(&self) -> Option<Celsius> {
        self.worst
    }

    /// The heaviest work this cell may now do, as far as it has been observed.
    ///
    /// `None` when the cell has never been read: an unobserved class is not a
    /// class, and the catalogue answers *cannot be told* for every row that
    /// needs one.
    ///
    /// The mapping is the governor's own ladder, read as a ceiling:
    ///
    /// | the hottest reading | the class |
    /// |---|---|
    /// | below the warn threshold | [`ThermalClass::Heavy`] — the cell is cool, and the governor has not intervened |
    /// | at or above warn | [`ThermalClass::Sustained`] — the cell has been warned, and the heavy work is off |
    /// | at or above critical | [`ThermalClass::Light`] — the cell has been critical, and only light work is left |
    /// | at or above the hard stop | [`ThermalClass::Idle`] — the cell has been stopped, and nothing runs |
    #[must_use]
    pub fn class(&self, thresholds: &Thresholds) -> Option<ThermalClass> {
        let worst = self.worst?;
        let [warn, critical, hard_stop] = thresholds.temperatures();
        Some(if worst >= hard_stop {
            ThermalClass::Idle
        } else if worst >= critical {
            ThermalClass::Light
        } else if worst >= warn {
            ThermalClass::Sustained
        } else {
            ThermalClass::Heavy
        })
    }
}
