// SPDX-License-Identifier: Apache-2.0

//! The mains-loss shed ladder, per ADR-0002 §8.
//!
//! # The inversion
//!
//! The cell is this project's largest risk, and once it is governed it becomes
//! the clearest advantage the platform has: a governed phone is a server with an
//! integrated uninterruptible power supply. A single-board computer at any price
//! cannot say that without buying a UPS costing more than the board.
//!
//! That claim is worth exactly as much as the ladder behind it. "The battery
//! carries the node" is a marketing sentence; "the database is consistent on
//! restart" is an engineering one, and only the second is testable. This module
//! is the second.
//!
//! # No rung may be skipped, and the API is what enforces it
//!
//! [`Shed::on_tick`] returns **every rung newly entered, in order**, rather than
//! the stage the node ended up in. The difference matters on a device that was
//! busy: if a tick is late and three minutes pass between two calls, a caller
//! reading only the final stage would quiesce the database without ever having
//! flushed it, and would do so believing it followed the ladder. Returning the
//! sequence means the flush cannot be dropped without the caller visibly
//! discarding it.
//!
//! # Nothing here owns a clock
//!
//! As with [`crate::sampler`], elapsed time is an argument. An outage is a
//! twenty-minute event and a test suite should not sit through one.
//!
//! # An unmeasurable cell during an outage is an empty one
//!
//! Charter Article IV: absence is never protection. A node that has lost mains
//! and cannot read its own state of charge does not have "probably enough" —
//! it has an unknown quantity of energy and a database to close. It shuts down
//! immediately and says why. The alternative is a node that rides an outage on
//! optimism and loses the write it was holding.

use core::time::Duration;

use crate::battery::Percent;

/// How far down the ladder the node has gone.
///
/// The order is load-bearing in the same way [`crate::governor::Level`]'s is:
/// this only ever moves toward shutdown. Mains returning is a real recovery,
/// but it is a deliberate one — see [`Shed::restored`] — not the ladder walking
/// backwards on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Mains present. Normal service.
    Serving,
    /// Mains lost. The fleet has been told and no new work is accepted.
    Announced,
    /// Non-essential services stopped: media, indexing, inference.
    Shed,
    /// State checkpointed, writes flushed and fsynced, the database quiesced.
    Quiesced,
    /// Clean shutdown, with charge remaining.
    ShuttingDown,
}

impl Stage {
    /// What the node is expected to have done on entering this rung.
    ///
    /// Carried so an operator reading a log sees the obligation rather than an
    /// enum name, and so the panel has one place to render it from.
    #[must_use]
    pub const fn obligation(self) -> &'static str {
        match self {
            Stage::Serving => "serving normally",
            Stage::Announced => "told the fleet and stopped accepting new work",
            Stage::Shed => "stopped non-essential services",
            Stage::Quiesced => "checkpointed, flushed and quiesced the database",
            Stage::ShuttingDown => "shut down cleanly with charge remaining",
        }
    }
}

/// What the cell had to say when the ladder was consulted.
///
/// [`Charge::Unreadable`] is deliberately not `Option<Percent>` and deliberately
/// not a sentinel percentage. A `None` invites a caller to write
/// `unwrap_or(100)` — or worse, `unwrap_or_default()`, which is zero and would
/// look like the right answer by accident on this particular ladder while being
/// wrong for the reason that matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Charge {
    /// The cell answered.
    Measured(Percent),
    /// It did not, and this is why.
    Unreadable(String),
}

/// Why the node moved down a rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShedReason {
    /// The rung's time came.
    Elapsed(Duration),
    /// The cell reached the reserve the ladder shuts down at.
    ReserveReached {
        /// The floor in force.
        floor: Percent,
        /// What the cell actually read.
        measured: Percent,
    },
    /// The cell could not be read, so the node is treated as empty.
    Unmeasurable(String),
    /// There is no cell to ride the outage on.
    NoUps,
}

impl core::fmt::Display for ShedReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ShedReason::Elapsed(d) => write!(f, "{}s since mains was lost", d.as_secs()),
            ShedReason::ReserveReached { floor, measured } => write!(
                f,
                "cell at {measured}, at or below the {floor} reserve this node \
                 shuts down with rather than spends"
            ),
            ShedReason::Unmeasurable(why) => write!(
                f,
                "the cell could not be read during an outage ({why}), so its \
                 charge is unknown and unknown is treated as empty"
            ),
            ShedReason::NoUps => write!(
                f,
                "no battery is carrying this node, so mains loss is an immediate \
                 stop and not an outage to ride out"
            ),
        }
    }
}

/// One rung, and why it was entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShedTransition {
    /// The rung entered.
    pub stage: Stage,
    /// Why.
    pub reason: ShedReason,
}

impl core::fmt::Display for ShedTransition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.stage.obligation(), self.reason)
    }
}

/// The timings and the reserve the ladder runs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShedPlan {
    shed_after: Duration,
    quiesce_after: Duration,
    reserve: Percent,
}

/// Why a plan was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    /// Quiescing was scheduled at or before shedding.
    RungsNotAscending,
    /// The reserve was set below [`ShedPlan::MIN_RESERVE`].
    ReserveTooLow,
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanError::RungsNotAscending => write!(
                f,
                "the ladder must quiesce after it sheds; a plan that does not is \
                 one where flushing the database competes with the services \
                 still writing to it"
            ),
            PlanError::ReserveTooLow => write!(
                f,
                "the reserve may be raised but never lowered past {}%: below it \
                 the shutdown races the kernel's own cut-off, and a shutdown \
                 that loses that race is the corrupted database this ladder \
                 exists to prevent",
                ShedPlan::MIN_RESERVE.get()
            ),
        }
    }
}

impl ShedPlan {
    /// The lowest reserve this project will shut down at.
    ///
    /// Configurable **upward only** — the mirror of
    /// [`crate::governor::Thresholds::MAX_HARD_STOP`], which is configurable
    /// downward only, and for the same reason: the direction that is safe for
    /// everyone is the direction that stays open.
    ///
    /// Ten percent, because "clean shutdown with charge remaining" is a claim
    /// about a race. Android platforms force a shutdown of their own at low
    /// single digits, and quiescing a database on eMMC is tens of seconds of
    /// work under exactly the load that makes it slowest. A reserve chosen so
    /// the two are close is a reserve that will lose sometimes, and the failure
    /// when it loses is the one this whole ladder was built to avoid.
    pub const MIN_RESERVE: Percent = Percent::literal(10);

    /// ADR-0002 §8's timings: shed at 60 seconds, quiesce at 180.
    #[must_use]
    pub fn recommended() -> Self {
        Self {
            shed_after: Duration::from_secs(60),
            quiesce_after: Duration::from_secs(180),
            reserve: Self::MIN_RESERVE,
        }
    }

    /// Builds a plan.
    ///
    /// # Errors
    ///
    /// Refuses a ladder whose rungs are out of order, or a reserve below
    /// [`ShedPlan::MIN_RESERVE`].
    pub fn new(
        shed_after: Duration,
        quiesce_after: Duration,
        reserve: Percent,
    ) -> Result<Self, PlanError> {
        if quiesce_after <= shed_after {
            return Err(PlanError::RungsNotAscending);
        }
        if reserve < Self::MIN_RESERVE {
            return Err(PlanError::ReserveTooLow);
        }
        Ok(Self {
            shed_after,
            quiesce_after,
            reserve,
        })
    }

    /// The reserve in force.
    #[must_use]
    pub const fn reserve(&self) -> Percent {
        self.reserve
    }

    /// The rung this much elapsed time alone calls for.
    ///
    /// Time only ever reaches [`Stage::Quiesced`]. Shutdown is a decision about
    /// charge, never about the clock: a node that has been on battery for an
    /// hour and still holds 70% is a node doing exactly what it was built to do,
    /// and shutting it down on a timer would throw away the advantage the ladder
    /// exists to demonstrate.
    #[must_use]
    fn stage_for_elapsed(&self, elapsed: Duration) -> Stage {
        if elapsed >= self.quiesce_after {
            Stage::Quiesced
        } else if elapsed >= self.shed_after {
            Stage::Shed
        } else {
            Stage::Announced
        }
    }
}

/// Whether this node may describe itself as having an integrated UPS.
///
/// The claim in ADR-0002 §8 is the project's strongest, and it is the kind that
/// gets copied onto a landing page and left there. Computing it here means the
/// page can only say it where the ladder can actually be run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsClaim {
    /// A cell is carrying the node, and the ladder will run to this reserve.
    Backed {
        /// The reserve the node will shut down holding.
        reserve: Percent,
    },
    /// It may not, and this is what to say instead.
    Unbacked {
        /// Why.
        why: &'static str,
    },
}

/// The running ladder.
#[derive(Debug, Clone)]
pub struct Shed {
    plan: ShedPlan,
    stage: Stage,
    has_cell: bool,
    history: Vec<ShedTransition>,
}

impl Shed {
    /// A node on mains, with a cell behind it.
    #[must_use]
    pub fn new(plan: ShedPlan) -> Self {
        Self {
            plan,
            stage: Stage::Serving,
            has_cell: true,
            history: Vec::new(),
        }
    }

    /// A node with no cell to ride an outage on.
    ///
    /// Some retired handsets run with the pack removed, powered from the pad.
    /// ADR-0002 calls this battery-absent mode: there is no UPS, so mains loss
    /// is an immediate hard stop rather than an outage. Saying so is the whole
    /// point — a node in this state that presented a shed ladder would be
    /// promising three minutes it does not have.
    #[must_use]
    pub fn without_cell(plan: ShedPlan) -> Self {
        Self {
            plan,
            stage: Stage::Serving,
            has_cell: false,
            history: Vec::new(),
        }
    }

    /// The rung the node is on.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// Every rung entered so far, in order.
    #[must_use]
    pub fn history(&self) -> &[ShedTransition] {
        &self.history
    }

    /// Whether this node may claim an integrated UPS.
    #[must_use]
    pub fn ups_claim(&self) -> UpsClaim {
        if self.has_cell {
            UpsClaim::Backed {
                reserve: self.plan.reserve,
            }
        } else {
            UpsClaim::Unbacked {
                why: "no battery is carrying this node; mains loss stops it immediately",
            }
        }
    }

    /// Advances the ladder for one observation during an outage.
    ///
    /// `elapsed` is time since mains was lost. Returns **every rung newly
    /// entered, in order** — see the module documentation for why this is not
    /// the final stage.
    ///
    /// The returned rungs are obligations the caller has to discharge, in the
    /// order given. A caller that performs only the last one has not run the
    /// ladder; it has skipped the flush and shut down on top of an open
    /// database.
    pub fn on_tick(&mut self, elapsed: Duration, charge: &Charge) -> Vec<ShedTransition> {
        // A node with no cell is not riding anything out. It goes straight to
        // shutdown and the reason says so, rather than walking three minutes of
        // ladder it has no energy to walk.
        if !self.has_cell {
            return self.walk_to(Stage::ShuttingDown, &ShedReason::NoUps);
        }

        // Charge is checked before the clock, and the two answers are not
        // combined. A cell at the reserve after ten seconds shuts down after ten
        // seconds; the timings describe an orderly outage, not a minimum one
        // this node is obliged to spend.
        let (target, reason) = match charge {
            Charge::Unreadable(why) => (Stage::ShuttingDown, ShedReason::Unmeasurable(why.clone())),
            Charge::Measured(p) if *p <= self.plan.reserve => (
                Stage::ShuttingDown,
                ShedReason::ReserveReached {
                    floor: self.plan.reserve,
                    measured: *p,
                },
            ),
            Charge::Measured(_) => (
                self.plan.stage_for_elapsed(elapsed),
                ShedReason::Elapsed(elapsed),
            ),
        };

        self.walk_to(target, &reason)
    }

    /// Records that mains came back.
    ///
    /// Returns whether the node resumed serving on its own. It does so only from
    /// [`Stage::Announced`], where nothing has been torn down and resuming costs
    /// nothing. Past that rung, services have been stopped and a database has
    /// been flushed and closed; bringing those back is a start-up, and a node
    /// that quietly performed one on a flickering supply would be a node that
    /// starts and stops its database every time the lights blink.
    ///
    /// From [`Stage::ShuttingDown`] it does nothing at all. That decision was
    /// made about a cell with a known amount of energy left, and mains arriving
    /// a second later does not put the energy back.
    pub fn restored(&mut self) -> bool {
        if self.stage == Stage::Announced {
            self.stage = Stage::Serving;
            return true;
        }
        false
    }

    /// Enters every rung between the current one and `target`, inclusive.
    ///
    /// Never moves back up: a `target` at or below the current rung produces no
    /// transitions. That is what makes a late tick safe and a flickering mains
    /// supply uneventful.
    fn walk_to(&mut self, target: Stage, reason: &ShedReason) -> Vec<ShedTransition> {
        let mut entered = Vec::new();
        for rung in [
            Stage::Announced,
            Stage::Shed,
            Stage::Quiesced,
            Stage::ShuttingDown,
        ] {
            if rung > self.stage && rung <= target {
                let t = ShedTransition {
                    stage: rung,
                    reason: reason.clone(),
                };
                self.stage = rung;
                self.history.push(t.clone());
                entered.push(t);
            }
        }
        entered
    }
}
