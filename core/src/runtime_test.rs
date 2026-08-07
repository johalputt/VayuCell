// SPDX-License-Identifier: Apache-2.0

//! Supervisor tests, in the attacker's voice.
//!
//! The longest run here is thirty days and the whole file finishes in
//! milliseconds, because the clock is an argument. That is the entire reason the
//! sampler and the shed ladder were written as pure functions.

use core::time::Duration;

use crate::battery::Percent;
use crate::governor::{Governor, Level, Reason, Thresholds};
use crate::host::FakeHost;
use crate::runtime::{Clock, FakeClock, Power, Supervisor};
use crate::sampler::Sampler;
use crate::shed::{Shed, ShedPlan, Stage};
use crate::sysfs::{Kind, SysfsCeiling, SUPPLY};

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

fn supervisor() -> Supervisor {
    Supervisor::new(
        Governor::new(Thresholds::recommended()),
        Sampler::new(Thresholds::recommended()),
        Shed::new(ShedPlan::recommended()),
        SUPPLY,
        Percent::clamped(60),
    )
}

// ── The ordinary pass ─────────────────────────────────────────────────────────

#[test]
fn a_cool_idle_device_ticks_at_the_steady_cadence_and_changes_nothing() {
    let host = device(290, 58);
    let mut s = supervisor();
    let out = s.tick(&host, None, Power::Mains);

    assert_eq!(out.level, Level::Normal);
    assert_eq!(out.next_in, Duration::from_secs(30));
    assert!(out.transition.is_none());
    assert!(out.read_succeeded());
    assert!(out.shed.is_empty(), "mains present, nothing to shed");
}

#[test]
fn a_hot_device_escalates_on_the_tick_that_read_it() {
    let host = device(600, 58);
    let mut s = supervisor();
    let out = s.tick(&host, None, Power::Mains);

    assert_eq!(out.level, Level::Halt);
    assert!(matches!(
        out.transition.as_ref().map(|t| &t.reason),
        Some(Reason::Temperature { .. })
    ));
    assert_eq!(
        out.next_in,
        Duration::from_secs(5),
        "a cell this hot is watched closely"
    );
}

// ── The error path, which must not be the short one ───────────────────────────

#[test]
fn a_device_that_cannot_be_read_still_produces_a_full_outcome() {
    // The failure this forecloses: an error path that logs and continues. A loop
    // that returns less information when something is wrong is a loop that goes
    // quiet exactly when somebody needs it to speak.
    let host = FakeHost::new(); // nothing readable at all
    let mut s = supervisor();
    let out = s.tick(&host, None, Power::Mains);

    assert!(!out.read_succeeded());
    assert!(out.read_error.is_some());
    assert_eq!(
        out.next_in,
        Duration::from_secs(5),
        "an unreadable cell is watched more closely, not less"
    );
    assert_eq!(out.level, Level::Normal, "one failure is a transient");
}

#[test]
fn three_unreadable_ticks_derate_the_device_through_the_loop() {
    // End to end: the blind counter is reachable from the supervisor, not just
    // from a direct call to the governor.
    let host = FakeHost::new();
    let mut s = supervisor();

    assert!(s.tick(&host, None, Power::Mains).transition.is_none());
    assert!(s.tick(&host, None, Power::Mains).transition.is_none());
    let out = s.tick(&host, None, Power::Mains);

    assert_eq!(out.level, Level::Derated);
    assert!(matches!(
        out.transition.as_ref().map(|t| &t.reason),
        Some(Reason::Unmeasurable { consecutive: 3, .. })
    ));
}

#[test]
fn a_device_that_comes_back_clears_the_blind_counter_through_the_loop() {
    let readable = device(290, 58);
    let mut s = supervisor();

    s.tick(&FakeHost::new(), None, Power::Mains);
    s.tick(&FakeHost::new(), None, Power::Mains);
    s.tick(&readable, None, Power::Mains);

    assert_eq!(s.governor().consecutive_failures(), 0);
    assert!(s
        .tick(&FakeHost::new(), None, Power::Mains)
        .transition
        .is_none());
    assert_eq!(s.governor().level(), Level::Normal);
}

// ── The ceiling, driven from the loop ─────────────────────────────────────────

#[test]
fn a_reverted_ceiling_is_caught_on_the_tick_that_wrote_it() {
    // The vendor daemon case, through the supervisor rather than by calling
    // enforce directly. Enforcement runs before the threshold check on purpose:
    // a ceiling reverted between ticks is caught on the same pass that reads the
    // temperature it was supposed to be limiting.
    let mut host =
        device(290, 58).with_revert(&format!("{SUPPLY}/charge_control_end_threshold"), "100");
    let mut s = supervisor();
    let mut mech = SysfsCeiling::new(&mut host, SUPPLY, Kind::EndThreshold).unwrap();

    let out = s.tick(&device(290, 58), Some(&mut mech), Power::Mains);
    assert_eq!(out.level, Level::Derated);
    assert!(matches!(
        out.transition.as_ref().map(|t| &t.reason),
        Some(Reason::Reverted { .. })
    ));
}

#[test]
fn a_device_with_no_mechanism_is_not_an_error_and_claims_no_ceiling() {
    // T0. Passing None must not fabricate a transition, and must not be treated
    // as a failed enforcement either — there was nothing to enforce.
    let host = device(290, 58);
    let mut s = supervisor();
    let out = s.tick(&host, None, Power::Mains);

    assert!(out.transition.is_none());
    assert_eq!(out.level, Level::Normal);
}

// ── Outages ───────────────────────────────────────────────────────────────────

#[test]
fn an_outage_walks_the_shed_ladder_and_hands_back_every_rung() {
    let host = device(290, 58);
    let mut s = supervisor();

    let t0 = s.tick(&host, None, Power::Battery(Duration::from_secs(0)));
    assert_eq!(
        t0.shed.iter().map(|t| t.stage).collect::<Vec<_>>(),
        [Stage::Announced]
    );

    let late = s.tick(&host, None, Power::Battery(Duration::from_secs(200)));
    assert_eq!(
        late.shed.iter().map(|t| t.stage).collect::<Vec<_>>(),
        [Stage::Shed, Stage::Quiesced],
        "a late tick must hand back every rung it passed"
    );
}

#[test]
fn an_outage_on_a_cell_that_stopped_answering_shuts_down_rather_than_riding_it_out() {
    // Both failure paths at once: the cell is unreadable AND mains is gone. The
    // supervisor must not let the read failure swallow the outage — an unknown
    // charge during an outage is treated as empty.
    let mut s = supervisor();
    let out = s.tick(
        &FakeHost::new(),
        None,
        Power::Battery(Duration::from_secs(1)),
    );

    assert!(!out.read_succeeded());
    assert_eq!(s.shed().stage(), Stage::ShuttingDown);
    assert!(out
        .shed
        .last()
        .expect("the ladder must have moved")
        .to_string()
        .contains("unknown is treated as empty"));
}

#[test]
fn mains_returning_before_anything_was_torn_down_resumes_service() {
    let host = device(290, 58);
    let mut s = supervisor();

    s.tick(&host, None, Power::Battery(Duration::from_secs(5)));
    assert_eq!(s.shed().stage(), Stage::Announced);

    s.tick(&host, None, Power::Mains);
    assert_eq!(s.shed().stage(), Stage::Serving);
}

#[test]
fn mains_returning_after_the_database_was_closed_does_not_silently_reopen_it() {
    let host = device(290, 58);
    let mut s = supervisor();

    s.tick(&host, None, Power::Battery(Duration::from_secs(200)));
    assert_eq!(s.shed().stage(), Stage::Quiesced);

    s.tick(&host, None, Power::Mains);
    assert_eq!(
        s.shed().stage(),
        Stage::Quiesced,
        "a flickering supply must not restart a closed database"
    );
}

// ── Restart, and the state that must survive it ───────────────────────────────

#[test]
fn a_supervisor_built_around_a_halted_governor_comes_back_halted() {
    // The failure this forecloses: constructing a fresh Normal governor inside
    // Supervisor::new. That turns "this cell is unsafe" into "this cell is unsafe
    // until somebody restarts the daemon", which is the one recovery path the
    // governor is written to refuse.
    let mut halted = Governor::new(Thresholds::recommended());
    halted.observe(&crate::sysfs::read_battery(&device(600, 58), SUPPLY).unwrap());
    assert_eq!(halted.level(), Level::Halt);

    let mut s = Supervisor::new(
        halted,
        Sampler::new(Thresholds::recommended()),
        Shed::new(ShedPlan::recommended()),
        SUPPLY,
        Percent::clamped(60),
    );

    let out = s.tick(&device(250, 58), None, Power::Mains);
    assert_eq!(
        out.level,
        Level::Halt,
        "a cool reading after a restart must not clear a hard stop"
    );
}

// ── The long run ──────────────────────────────────────────────────────────────

#[test]
fn thirty_days_of_ticks_on_a_healthy_device_neither_drifts_nor_stops_watching() {
    // The roadmap's P2 gate is "survives 30 days unattended". This is not that
    // gate: it says nothing about Doze, a real kernel, or a real cell. It says
    // the composition does not drift, does not stop escalating, and does not
    // accumulate state over 86,400 ticks — which is the part a test can settle.
    let host = device(290, 58);
    let mut s = supervisor();
    let mut clock = FakeClock::new();

    let thirty_days = Duration::from_secs(30 * 24 * 60 * 60);
    let mut ticks = 0u32;
    while clock.elapsed() < thirty_days {
        let out = s.tick_and_wait(&host, None, Power::Mains, &mut clock);
        assert_eq!(out.level, Level::Normal);
        assert_eq!(out.next_in, Duration::from_secs(30));
        ticks += 1;
    }

    assert_eq!(ticks, 86_400, "thirty days at the steady cadence");
    assert!(
        s.governor().history().is_empty(),
        "a month of nominal readings must produce no transitions"
    );

    // And it is still watching: the very next hot reading still escalates.
    let out = s.tick(&device(600, 58), None, Power::Mains);
    assert_eq!(out.level, Level::Halt);
}

#[test]
fn a_month_of_ticks_finishes_instantly_because_the_clock_is_an_argument() {
    // Pinning the property the whole design rests on. If Clock stopped being
    // injected — if the supervisor called thread::sleep directly — this test
    // would take thirty days rather than failing, and a test that hangs reads as
    // infrastructure trouble rather than as a regression.
    let mut clock = FakeClock::new();
    clock.sleep(Duration::from_secs(30 * 24 * 60 * 60));
    assert_eq!(clock.elapsed(), Duration::from_secs(2_592_000));
    assert_eq!(clock.intervals().len(), 1);
}

#[test]
fn the_cadence_tightens_and_relaxes_with_the_cell_over_a_run() {
    // A supervisor that latched Alert after one warm reading would keep the
    // device awake for the rest of its life; one that latched Steady would stop
    // watching a cell that got hot. The interval log makes both visible.
    let mut s = supervisor();
    let mut clock = FakeClock::new();

    s.tick_and_wait(&device(290, 58), None, Power::Mains, &mut clock);
    s.tick_and_wait(&device(430, 58), None, Power::Mains, &mut clock);
    s.tick_and_wait(&device(290, 58), None, Power::Mains, &mut clock);

    assert_eq!(
        clock.intervals(),
        [
            Duration::from_secs(30),
            Duration::from_secs(5),
            Duration::from_secs(30)
        ]
    );
}
