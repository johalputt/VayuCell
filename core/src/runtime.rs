// SPDX-License-Identifier: Apache-2.0

//! The supervisor loop — the piece that makes the other modules a running thing.
//!
//! Everything before this module is a pure function or a state machine. That was
//! deliberate: a sampler that owned a timer could only be tested in real time,
//! and a governor that read the clock could only be tested by waiting. The cost
//! of that decision is that nothing actually *ran*, and this is where it does.
//!
//! # The clock is still injected, so a month of operation is a test
//!
//! [`Clock`] is a trait. [`RealClock`] sleeps; [`FakeClock`] advances a counter
//! and returns immediately. The roadmap's P2 gate is "survives 30 days unattended
//! with Doze active", and thirty days of *loop logic* is a test that finishes in
//! milliseconds — 86,400 ticks at the steady cadence. What that test does not
//! establish is anything about Doze, about a real kernel, or about a real cell.
//! It establishes that the composition does not drift, leak, or stop escalating
//! over a long run, which is a smaller claim and an honest one.
//!
//! # A tick has one shape, and the unreadable case is not an early return
//!
//! [`Supervisor::tick`] always produces an [`Outcome`]. The path where the
//! battery could not be read is not a `continue` and not a logged warning: it
//! feeds [`crate::governor::Governor::observe_unreadable`], tightens the cadence,
//! and appears in the outcome like any other tick. A loop whose error path is
//! shorter than its success path is a loop that goes quiet exactly when something
//! is wrong.
//!
//! # This has never run against a device
//!
//! [`RealClock`] exists and sleeps on a real thread. Nothing in this repository
//! has pointed the supervisor at a [`crate::host::RealHost`], because there is no
//! binary that would — the daemon that would own this loop is not written. What
//! is written is the loop itself, and every tick of it that has ever executed has
//! been driven by a fake clock over a fake device.

use core::time::Duration;

use crate::battery::Percent;
use crate::governor::{ChargeMechanism, Governor, Level, Transition};
use crate::host::Host;
use crate::sampler::{Cadence, Sampler};
use crate::shed::{Charge, Shed, ShedTransition};
use crate::sysfs::{read_battery, ReadError};

/// The passage of time, injected.
///
/// Split from the loop for the same reason the sampler has no timer: a
/// supervisor that called `thread::sleep` directly could only be exercised by
/// waiting, and the interesting runs are a month long.
pub trait Clock {
    /// Waits.
    fn sleep(&mut self, how_long: Duration);

    /// Total time this clock has advanced since it was made.
    ///
    /// Monotonic and owned by the clock rather than read from the system, so a
    /// wall-clock step — an NTP correction, a resumed suspend — cannot make the
    /// supervisor believe an outage ran backwards.
    fn elapsed(&self) -> Duration;
}

/// A clock that really sleeps.
#[derive(Debug, Default)]
pub struct RealClock {
    elapsed: Duration,
}

impl RealClock {
    /// A clock at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
        }
    }
}

impl Clock for RealClock {
    fn sleep(&mut self, how_long: Duration) {
        std::thread::sleep(how_long);
        self.elapsed = self.elapsed.saturating_add(how_long);
    }

    fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

/// A clock that advances without waiting.
///
/// The reason a thirty-day run is a unit test. It records every interval it was
/// asked to wait, so a test can assert the *cadence* the supervisor chose rather
/// than only the state it ended in.
#[derive(Debug, Default, Clone)]
pub struct FakeClock {
    elapsed: Duration,
    slept: Vec<Duration>,
}

impl FakeClock {
    /// A clock at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            slept: Vec::new(),
        }
    }

    /// Every interval this clock was asked to wait, in order.
    #[must_use]
    pub fn intervals(&self) -> &[Duration] {
        &self.slept
    }
}

impl Clock for FakeClock {
    fn sleep(&mut self, how_long: Duration) {
        self.slept.push(how_long);
        self.elapsed = self.elapsed.saturating_add(how_long);
    }

    fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

/// Whether mains is present, as the supervisor was told.
///
/// Not detected here. Reading the charger state is a device concern and the
/// daemon that would do it is not written, so this is an argument — and being an
/// argument keeps the outage tests free of a fake charger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    /// Running on mains.
    Mains,
    /// Running on the cell, this long since mains was lost.
    Battery(Duration),
}

/// What one pass of the loop did.
///
/// Every field is populated on every tick, including the ticks where the battery
/// could not be read. A struct whose fields go absent on the error path is a
/// struct that reports least when there is most to say.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The governor's level after this tick.
    pub level: Level,
    /// How long the supervisor will wait before the next one.
    pub next_in: Duration,
    /// The cadence that interval came from.
    pub cadence: Cadence,
    /// Any level change this tick caused.
    pub transition: Option<Transition>,
    /// Any shed rungs entered this tick, in the order they must be discharged.
    pub shed: Vec<ShedTransition>,
    /// The read error, when the cell could not be read at all.
    pub read_error: Option<ReadError>,
}

impl Outcome {
    /// Whether this tick saw the cell.
    #[must_use]
    pub const fn read_succeeded(&self) -> bool {
        self.read_error.is_none()
    }
}

/// Drives one device.
#[derive(Debug)]
pub struct Supervisor {
    governor: Governor,
    sampler: Sampler,
    shed: Shed,
    supply_dir: String,
    ceiling: Percent,
}

impl Supervisor {
    /// Builds a supervisor around an already-configured governor and ladder.
    ///
    /// The governor is passed in rather than constructed here so a device that
    /// was halted before a restart comes back halted. A supervisor that built a
    /// fresh `Level::Normal` governor on every start would turn "this cell is
    /// unsafe" into "this cell is unsafe until somebody restarts the daemon".
    #[must_use]
    pub fn new(
        governor: Governor,
        sampler: Sampler,
        shed: Shed,
        supply_dir: &str,
        ceiling: Percent,
    ) -> Self {
        Self {
            governor,
            sampler,
            shed,
            supply_dir: supply_dir.to_owned(),
            ceiling,
        }
    }

    /// The governor, for rendering a panel or recording a physical inspection.
    #[must_use]
    pub const fn governor(&self) -> &Governor {
        &self.governor
    }

    /// The shed ladder.
    #[must_use]
    pub const fn shed(&self) -> &Shed {
        &self.shed
    }

    /// Runs one pass: read, govern, enforce, shed, and decide when to look again.
    ///
    /// `mechanism` is `None` on a device that exposes no charge control — the T0
    /// case, and the most common device. It is not an error and nothing here
    /// pretends a ceiling is being held.
    pub fn tick(
        &mut self,
        host: &dyn Host,
        mechanism: Option<&mut dyn ChargeMechanism>,
        power: Power,
    ) -> Outcome {
        match read_battery(host, &self.supply_dir) {
            Ok(reading) => {
                // Enforce before observing thresholds, so a ceiling that was
                // reverted between ticks is caught on the same pass that reads
                // the temperature it was supposed to be limiting.
                let mut transition =
                    mechanism.and_then(|m| self.governor.enforce(m, self.ceiling, &reading));
                transition = transition.or_else(|| self.governor.observe(&reading));

                let cadence = self.sampler.cadence_for(&reading);
                let shed = self.advance_shed(power, &Charge::Measured(reading.capacity));

                Outcome {
                    level: self.governor.level(),
                    next_in: cadence.interval(),
                    cadence,
                    transition,
                    shed,
                    read_error: None,
                }
            }
            Err(e) => {
                // Not an early return and not a warning. A cell nobody can read
                // is the case the blind counter exists for, and the cadence
                // tightens rather than backing off.
                let transition = self.governor.observe_unreadable(&e.to_string());
                let cadence = Sampler::cadence_when_unreadable();
                let shed = self.advance_shed(power, &Charge::Unreadable(e.to_string()));

                Outcome {
                    level: self.governor.level(),
                    next_in: cadence.interval(),
                    cadence,
                    transition,
                    shed,
                    read_error: Some(e),
                }
            }
        }
    }

    /// One tick, then the wait it decided on.
    pub fn tick_and_wait(
        &mut self,
        host: &dyn Host,
        mechanism: Option<&mut dyn ChargeMechanism>,
        power: Power,
        clock: &mut dyn Clock,
    ) -> Outcome {
        let outcome = self.tick(host, mechanism, power);
        clock.sleep(outcome.next_in);
        outcome
    }

    fn advance_shed(&mut self, power: Power, charge: &Charge) -> Vec<ShedTransition> {
        match power {
            Power::Mains => {
                self.shed.restored();
                Vec::new()
            }
            Power::Battery(since) => self.shed.on_tick(since, charge),
        }
    }
}
