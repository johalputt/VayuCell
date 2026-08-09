// SPDX-License-Identifier: Apache-2.0

//! One cell, one ladder, however many surfaces are serving from it.
//!
//! # Why this exists
//!
//! `site` and `vault` each built their own [`Shed`], each with its own start
//! instant. For one process serving one surface that is correct and invisible.
//!
//! It stops being either the moment somebody runs both, which is exactly what
//! `docs/INSTALL.md` asks a beginner to do: two ladders, latching independently,
//! measuring from two different clocks, describing one phone with one battery.
//! They can disagree about which rung the node is on — and the one that
//! disagrees in the reassuring direction is the one that carries on serving
//! while the other has already shed.
//!
//! So the ladder is owned here and the surfaces borrow it. Sharing is not an
//! optimisation; it is the only arrangement in which "the node is at
//! `Stage::Shed`" is a fact about the node rather than about whichever process
//! was asked.
//!
//! # What is shared and what is not
//!
//! The **ladder** is shared, because it latches and a latch is history.
//!
//! The **governor reading is not.** Every call reads the cell again through
//! [`crate::device::observe`], which builds a fresh governor that cannot latch.
//! That is the per-request question — what is the cell doing *now* — and caching
//! it would reintroduce, inside one process, precisely the staleness this module
//! exists to remove between two.

use core::time::Duration;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use vayucell_core::governor::{ChargeMechanism, Level};
use vayucell_core::host::Host;
use vayucell_core::runtime::{Outcome, Power, Supervisor};
use vayucell_core::shed::{Shed, ShedPlan, Stage};
use vayucell_core::site::Availability;

/// The device, as every serving surface in this process sees it.
pub struct Cell {
    supply_dir: String,
    /// How long ago mains was lost, if the operator says it was.
    ///
    /// `None` is not "mains is present" — it is "nobody is claiming an outage".
    /// Mains detection is not implemented anywhere in this project, and naming
    /// the assumption is the difference between an argument and a measurement.
    outage: Option<Duration>,
    started: Instant,
    ladder: Mutex<Shed>,
}

impl Cell {
    /// A cell with a ladder at the top rung, ready to be walked down once.
    #[must_use]
    pub fn new(supply_dir: String, outage: Option<Duration>) -> Self {
        Self {
            supply_dir,
            outage,
            started: Instant::now(),
            ladder: Mutex::new(Shed::new(ShedPlan::recommended())),
        }
    }

    /// The two facts every request needs: what the governor says, and which rung
    /// of the outage ladder this node has reached.
    #[must_use]
    pub fn context(&self, host: &dyn Host) -> (Level, Stage) {
        self.context_after(host, self.started.elapsed())
    }

    /// What a reader of this cell is permitted to be served.
    #[must_use]
    pub fn availability(&self, host: &dyn Host) -> Availability {
        let (level, stage) = self.context(host);
        Availability::of(level, stage)
    }

    /// [`Cell::context`] with the elapsed time supplied rather than read.
    ///
    /// Split out so the sharing property is testable without sleeping. A test
    /// that had to wait three minutes to watch a ladder latch is a test nobody
    /// runs.
    fn context_after(&self, host: &dyn Host, since_start: Duration) -> (Level, Stage) {
        let (level, charge) = crate::device::observe(host, &self.supply_dir);

        let stage = match self.outage {
            None => Stage::Serving,
            Some(since) => {
                // Poisoning is recovered from rather than propagated. A panic
                // while serving one request must not take every surface down,
                // and the ladder's own state is still whatever rung it reached —
                // there is no half-written rung to distrust.
                let mut ladder = self.ladder.lock().unwrap_or_else(PoisonError::into_inner);
                ladder.on_tick(since.saturating_add(since_start), &charge);
                ladder.stage()
            }
        };

        (level, stage)
    }
}

/// The device, with a supervisor actually governing it.
///
/// # The difference from [`Cell`], which is the whole point
///
/// [`Cell`] answers "what is this cell doing right now" and nothing else. It
/// holds no charge ceiling, and its governor is rebuilt per request so that it
/// **cannot latch** — right for a single question, and wrong for a phone left
/// running for months, because `HALT` is supposed to require a person who has
/// looked at the device. A per-request governor forgets it the moment the cell
/// cools.
///
/// A [`Supervisor`] holds the ceiling, samples on its own cadence and escalates
/// monotonically — `Governor::escalate` refuses to move down. So where one is
/// running, the surfaces must ask it rather than taking their own reading.
///
/// # Both answers, and the worse one wins
///
/// [`Governed::context`] takes a fresh reading *and* the supervisor's latched
/// level, and serves the more severe of the two. Neither alone is sufficient:
///
/// - The supervisor's level alone is up to one sampling interval old, so a cell
///   that spiked since the last tick would be served as though it had not.
/// - A fresh reading alone cannot latch, so a device that halted and then cooled
///   would quietly start serving again — turning a hard stop into a log line.
///
/// Taking the maximum cannot be wrong in the reassuring direction, which is the
/// only direction that matters here. `Level` is `Ord` precisely so this is
/// expressible; the ordering is load-bearing and documented as such.
pub struct Governed {
    supervisor: Mutex<Supervisor>,
    supply_dir: String,
}

impl Governed {
    /// A supervisor, ready to be ticked by one thread and read by the rest.
    #[must_use]
    pub fn new(supervisor: Supervisor, supply_dir: String) -> Self {
        Self {
            supervisor: Mutex::new(supervisor),
            supply_dir,
        }
    }

    /// One supervisor pass: read, govern, enforce the ceiling, walk the ladder.
    ///
    /// The caller logs the outcome and waits for `next_in`, exactly as the
    /// standalone supervisor loop does. It is not done here because a lock held
    /// across a sleep is a lock no request can take.
    pub fn tick(
        &self,
        host: &dyn Host,
        mechanism: Option<&mut dyn ChargeMechanism>,
        power: Power,
    ) -> Outcome {
        let mut supervisor = self
            .supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        supervisor.tick(host, mechanism, power)
    }

    /// What every serving surface in this process sees.
    #[must_use]
    pub fn context(&self, host: &dyn Host) -> (Level, Stage) {
        // Read the cell *before* taking the lock. The reading is I/O and the
        // lock is contended by every request; doing them the other way round
        // would hold it across a sysfs read for no benefit.
        let (fresh, _) = crate::device::observe(host, &self.supply_dir);

        let supervisor = self
            .supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let governed = supervisor.governor().level();
        let stage = supervisor.shed().stage();
        drop(supervisor);

        (fresh.max(governed), stage)
    }

    /// What a reader of this cell is permitted to be served.
    #[must_use]
    pub fn availability(&self, host: &dyn Host) -> Availability {
        let (level, stage) = self.context(host);
        Availability::of(level, stage)
    }
}

#[cfg(test)]
mod tests {
    use super::Cell;
    use core::time::Duration;
    use vayucell_core::governor::Level;
    use vayucell_core::host::FakeHost;
    use vayucell_core::shed::Stage;
    use vayucell_core::sysfs::SUPPLY;

    fn device(temp_deci: i32, capacity: i64) -> FakeHost {
        FakeHost::new()
            .with_file(&format!("{SUPPLY}/capacity"), &format!("{capacity}\n"))
            .with_file(&format!("{SUPPLY}/voltage_now"), "3820000\n")
            .with_file(&format!("{SUPPLY}/current_now"), "-180000\n")
            .with_file(&format!("{SUPPLY}/temp"), &format!("{temp_deci}\n"))
            .with_file(&format!("{SUPPLY}/cycle_count"), "412\n")
            .with_file(&format!("{SUPPLY}/charge_full"), "3600000\n")
            .with_file(&format!("{SUPPLY}/charge_full_design"), "4000000\n")
    }

    fn outage() -> Cell {
        Cell::new(SUPPLY.to_owned(), Some(Duration::ZERO))
    }

    use super::Governed;
    use vayucell_core::battery::Percent;
    use vayucell_core::governor::{Governor, Thresholds};
    use vayucell_core::runtime::{Power, Supervisor};
    use vayucell_core::sampler::Sampler;
    use vayucell_core::shed::{Shed, ShedPlan};

    fn governed() -> Governed {
        let thresholds = Thresholds::recommended();
        Governed::new(
            Supervisor::new(
                Governor::new(thresholds),
                Sampler::new(thresholds),
                Shed::new(ShedPlan::recommended()),
                SUPPLY,
                Percent::clamped(60),
            ),
            SUPPLY.to_owned(),
        )
    }

    #[test]
    fn a_halted_supervisor_keeps_refusing_after_the_cell_cools() {
        // The reason `all` runs a supervisor at all. HALT is meant to require a
        // person who has looked at the phone; a per-request governor forgets it
        // the moment the temperature drops, turning a hard stop into a log line.
        let g = governed();
        g.tick(&device(600, 58), None, Power::Mains);

        let (level, _) = g.context(&device(290, 58));
        assert_eq!(
            level,
            Level::Halt,
            "a cooled cell cleared a hard stop nobody had looked at"
        );
        assert!(!g.availability(&device(290, 58)).is_serving());
    }

    #[test]
    fn a_cell_that_spikes_between_ticks_is_refused_on_the_fresh_reading() {
        // The other half of taking the worse of two answers. The supervisor's
        // level is up to one sampling interval old, and a cell that went hot
        // since the last tick must not be served on the strength of it.
        let g = governed();
        g.tick(&device(290, 58), None, Power::Mains);

        let (level, _) = g.context(&device(600, 58));
        assert_eq!(
            level,
            Level::Halt,
            "a spike since the last tick was served as though it had not happened"
        );
    }

    #[test]
    fn a_well_cell_under_a_well_supervisor_still_serves() {
        // So that "take the worse of two" cannot be satisfied by a function
        // that refuses everything.
        let g = governed();
        g.tick(&device(290, 58), None, Power::Mains);
        let (level, stage) = g.context(&device(290, 58));
        assert_eq!(level, Level::Normal);
        assert_eq!(stage, Stage::Serving);
        assert!(g.availability(&device(290, 58)).is_serving());
    }

    #[test]
    fn an_unreadable_cell_is_refused_whichever_half_notices_first() {
        // Absence is never protection, on either input.
        let g = governed();
        let (level, _) = g.context(&FakeHost::new());
        assert_eq!(level, Level::Protect);
    }

    #[test]
    fn the_supervisors_ladder_is_the_one_the_surfaces_read() {
        // Not a second copy alongside it. The supervisor owns a Shed; a
        // Governed that built its own would be the two-ladder defect again,
        // wearing different clothes.
        let g = governed();
        g.tick(
            &device(290, 58),
            None,
            Power::Battery(Duration::from_secs(3600)),
        );
        let (_, stage) = g.context(&device(290, 58));
        assert_ne!(
            stage,
            Stage::Serving,
            "an hour of outage on the supervisor was invisible to the surfaces"
        );
    }

    #[test]
    fn a_rung_entered_by_one_surface_is_seen_by_the_next() {
        // The whole reason this module exists. The site and the vault are two
        // callers of one cell; if each held its own ladder, the second would
        // still believe the node was serving.
        let cell = outage();
        let host = device(290, 58);

        let (_, first) = cell.context_after(&host, Duration::from_secs(3600));
        assert_ne!(first, Stage::Serving, "an hour of outage shed nothing");

        let (_, second) = cell.context_after(&host, Duration::ZERO);
        assert_eq!(
            second, first,
            "the ladder walked back up for the surface that asked second"
        );
    }

    #[test]
    fn the_ladder_never_walks_back_up_however_many_times_it_is_asked() {
        // Latching is history, and history is the thing that cannot be recovered
        // from a fresh reading. Asked repeatedly with the clock running
        // backwards, the rung must only ever hold or descend further.
        let cell = outage();
        let host = device(290, 58);
        let mut worst = Stage::Serving;
        for seconds in [600_u64, 30, 1800, 5, 3600, 0] {
            let (_, stage) = cell.context_after(&host, Duration::from_secs(seconds));
            assert!(stage >= worst, "went back up to {stage:?} from {worst:?}");
            worst = stage;
        }
    }

    #[test]
    fn the_governor_is_read_afresh_and_does_not_latch_with_the_ladder() {
        // The asymmetry this module is careful about. The rung is history; the
        // cell's temperature is not, and a device that has cooled must be
        // reported as cool even though the ladder it walked stays walked.
        let cell = outage();

        let (hot, _) = cell.context_after(&device(600, 58), Duration::from_secs(1));
        assert_eq!(hot, Level::Halt);

        let (cooled, stage) = cell.context_after(&device(290, 58), Duration::from_secs(2));
        assert_eq!(cooled, Level::Normal, "a stale hot reading latched");
        assert_ne!(
            stage,
            Stage::Serving,
            "the ladder should still hold what it walked"
        );
    }

    #[test]
    fn without_a_declared_outage_the_ladder_is_never_walked_at_all() {
        // `None` means nobody is claiming mains was lost. Walking the ladder on
        // that would be this program inventing an outage it cannot detect.
        let cell = Cell::new(SUPPLY.to_owned(), None);
        let host = device(290, 58);
        let (_, stage) = cell.context_after(&host, Duration::from_secs(86_400));
        assert_eq!(stage, Stage::Serving);
    }

    #[test]
    fn a_cell_that_cannot_be_read_shuts_the_node_down_rather_than_riding_it_out() {
        // Absence is never protection. A node that cannot see its charge has no
        // idea how long it can last, so it does not spend the ladder guessing.
        let cell = outage();
        let (level, stage) = cell.context_after(&FakeHost::new(), Duration::from_secs(1));
        assert_eq!(level, Level::Protect);
        assert_eq!(stage, Stage::ShuttingDown);
    }

    #[test]
    fn availability_is_the_same_decision_the_site_would_have_made_alone() {
        // The surfaces must not each re-derive this from the parts. One cell,
        // one answer, whoever asks.
        let cell = Cell::new(SUPPLY.to_owned(), None);
        let host = device(290, 58);
        let (level, stage) = cell.context(&host);
        assert_eq!(
            cell.availability(&host),
            vayucell_core::site::Availability::of(level, stage)
        );
    }
}
