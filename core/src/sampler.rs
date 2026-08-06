// SPDX-License-Identifier: Apache-2.0

//! Sampling cadence, per ADR-0002 §3.
//!
//! # Why this is a function and not a loop
//!
//! Nothing here sleeps, and nothing here owns a clock. [`Sampler::cadence_for`]
//! answers "how long until the next reading" from the reading in front of it,
//! and the caller does the waiting.
//!
//! A sampler that owned its own timer would be a sampler nobody could test
//! except in real time — and the interesting cases are a cell warming toward a
//! threshold over an hour and a device whose battery nodes disappear for a day.
//! Neither is something a test suite should be asked to sit through.
//!
//! # The device must not be kept awake by its own monitor
//!
//! ADR-0002 §3: a phone doing nothing is a phone that should be idle. Polling
//! every five seconds for a cell sitting at 29 °C burns the battery this whole
//! subsystem exists to preserve, which would be a monitor causing the harm it
//! was written to measure.
//!
//! So the cadence is adaptive: [`Cadence::Steady`] far from any threshold, and
//! [`Cadence::Alert`] only when something is actually happening — within
//! [`Sampler::ALERT_MARGIN`] of a temperature threshold, or while the cell is
//! taking charge, which is when its state changes fastest.

use core::time::Duration;

use crate::battery::{Celsius, Reading};
use crate::governor::Thresholds;

/// How often to sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    /// Nothing is close to happening.
    Steady,
    /// Near a threshold, or the state is changing.
    Alert,
}

impl Cadence {
    /// The interval this cadence implies.
    #[must_use]
    pub const fn interval(self) -> Duration {
        match self {
            Cadence::Steady => Duration::from_secs(30),
            Cadence::Alert => Duration::from_secs(5),
        }
    }
}

/// Decides how often to look at the cell.
#[derive(Debug, Clone, Copy)]
pub struct Sampler {
    thresholds: Thresholds,
}

impl Sampler {
    /// How close to a threshold counts as close.
    ///
    /// ADR-0002 §3 says within 5 °C. The margin is generous on purpose: a cell
    /// that crosses from 40 °C to a hard stop between two thirty-second samples
    /// was never going to be caught by a tighter one, and the cost of watching
    /// too early is some wakefulness, while the cost of watching too late is the
    /// thing this project exists to prevent.
    pub const ALERT_MARGIN: Celsius = Celsius::new(5);

    /// Builds a sampler for a threshold set.
    #[must_use]
    pub const fn new(thresholds: Thresholds) -> Self {
        Self { thresholds }
    }

    /// The cadence this reading calls for.
    #[must_use]
    pub fn cadence_for(&self, reading: &Reading) -> Cadence {
        // Charging is when the cell's state changes fastest, and it is the
        // condition this project asks people to leave a device in for years.
        if reading.is_charging() {
            return Cadence::Alert;
        }

        if reading.temp.to_celsius() >= self.alert_from() {
            return Cadence::Alert;
        }
        Cadence::Steady
    }

    /// The temperature at which watching tightens.
    ///
    /// A margin below the **lowest** rung, and only that one. An earlier version
    /// of this looped over every rung, which reads as more thorough and is in
    /// fact vacuous: [`Thresholds::new`] enforces `warn < critical < hard_stop`,
    /// so a cell within the margin of any higher rung is already past the margin
    /// of the lowest one and the later iterations can never be the branch that
    /// fires. Taking the minimum says the same thing once, and says it without
    /// depending on which field happens to be lowest.
    #[must_use]
    pub fn alert_from(&self) -> Celsius {
        // Destructured rather than `.into_iter().min()`, which returns an Option
        // this function would have to unwrap. The ladder is three rungs by
        // construction, and a `min` that cannot fail is better than an `expect`
        // explaining why it will not.
        let [warn, critical, hard_stop] = self.thresholds.temperatures();
        let lowest = warn.min(critical).min(hard_stop);
        Celsius::new(lowest.whole_degrees() - Self::ALERT_MARGIN.whole_degrees())
    }

    /// How long to wait before the next reading.
    #[must_use]
    pub fn interval_for(&self, reading: &Reading) -> Duration {
        self.cadence_for(reading).interval()
    }

    /// The cadence to use when a reading could not be taken at all.
    ///
    /// Always [`Cadence::Alert`]. A device whose battery nodes have stopped
    /// answering is a device nobody is measuring, and backing off to a
    /// thirty-second poll turns "we cannot see the cell" into "we look at it
    /// less often" — which is the wrong direction, and the direction a retry
    /// backoff would naturally take it.
    #[must_use]
    pub const fn cadence_when_unreadable() -> Cadence {
        Cadence::Alert
    }
}
