// SPDX-License-Identifier: Apache-2.0

//! Shed ladder tests, in the attacker's voice.
//!
//! An outage is a twenty-minute event and none of these take twenty minutes,
//! because elapsed time is an argument rather than something this module waits
//! for.

use core::time::Duration;

use crate::battery::Percent;
use crate::shed::{Charge, PlanError, Shed, ShedPlan, ShedReason, Stage, UpsClaim};

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn healthy() -> Charge {
    Charge::Measured(Percent::clamped(72))
}

fn stages(t: &[crate::shed::ShedTransition]) -> Vec<Stage> {
    t.iter().map(|x| x.stage).collect()
}

// ── The ladder ────────────────────────────────────────────────────────────────

#[test]
fn the_ladder_runs_in_order_and_announces_before_it_sheds() {
    let mut s = Shed::new(ShedPlan::recommended());

    assert_eq!(stages(&s.on_tick(secs(0), &healthy())), [Stage::Announced]);
    assert_eq!(
        stages(&s.on_tick(secs(30), &healthy())),
        [] as [Stage; 0],
        "a rung already entered is not entered again"
    );
    assert_eq!(stages(&s.on_tick(secs(60), &healthy())), [Stage::Shed]);
    assert_eq!(stages(&s.on_tick(secs(180), &healthy())), [Stage::Quiesced]);
}

#[test]
fn a_late_tick_hands_back_every_rung_it_skipped_over() {
    // The failure this forecloses, and the reason on_tick returns a sequence
    // rather than a stage. A busy node whose ticks are three minutes apart would
    // otherwise be told "you are quiesced now" having never been told to stop
    // taking writes or to shut down its indexer. It would then flush a database
    // that services were still writing into, and call the result a clean
    // checkpoint.
    let mut s = Shed::new(ShedPlan::recommended());
    let entered = s.on_tick(secs(200), &healthy());

    assert_eq!(
        stages(&entered),
        [Stage::Announced, Stage::Shed, Stage::Quiesced],
        "every obligation between must be handed back, in order"
    );
    assert_eq!(s.stage(), Stage::Quiesced);
}

#[test]
fn the_ladder_never_walks_back_up_on_its_own() {
    // Clocks on these devices are not reliable — a resumed suspend, an NTP step
    // — and an elapsed time that goes backwards must not un-quiesce a database.
    let mut s = Shed::new(ShedPlan::recommended());
    s.on_tick(secs(180), &healthy());
    assert_eq!(s.stage(), Stage::Quiesced);

    assert_eq!(
        stages(&s.on_tick(secs(5), &healthy())),
        [] as [Stage; 0],
        "time going backwards must not reopen a closed database"
    );
    assert_eq!(s.stage(), Stage::Quiesced);
}

#[test]
fn time_alone_never_reaches_shutdown() {
    // A node that has been on battery for an hour and still holds 70% is doing
    // exactly what it was built to do. Shutting it down on a timer would throw
    // away the advantage the ladder exists to demonstrate — and would make the
    // §8 claim false in the one case where it is most impressive.
    let mut s = Shed::new(ShedPlan::recommended());
    s.on_tick(secs(3600), &healthy());
    assert_eq!(s.stage(), Stage::Quiesced);
    assert_ne!(s.stage(), Stage::ShuttingDown);
}

// ── Charge, which is what shutdown is actually about ──────────────────────────

#[test]
fn reaching_the_reserve_shuts_down_however_little_time_has_passed() {
    // The timings describe an orderly outage. They are not a minimum the node is
    // obliged to spend: a cell that was already low when mains went is a cell
    // with seconds, not three minutes, and a ladder that made it wait would run
    // out of energy partway through its own flush.
    let mut s = Shed::new(ShedPlan::recommended());
    let entered = s.on_tick(secs(10), &Charge::Measured(Percent::clamped(10)));

    assert_eq!(
        stages(&entered),
        [
            Stage::Announced,
            Stage::Shed,
            Stage::Quiesced,
            Stage::ShuttingDown
        ],
        "the whole ladder is still handed back — it is compressed, not skipped"
    );
    assert!(matches!(
        entered.last().unwrap().reason,
        ShedReason::ReserveReached { .. }
    ));
}

#[test]
fn a_cell_that_cannot_be_read_during_an_outage_is_treated_as_empty() {
    // Charter Article IV, at the moment it costs the most. A node that has lost
    // mains and cannot read its own charge does not have "probably enough"; it
    // has an unknown quantity and a database to close. The tempting reading —
    // no evidence of a low cell, so carry on — is the one that loses the write.
    let mut s = Shed::new(ShedPlan::recommended());
    let entered = s.on_tick(secs(1), &Charge::Unreadable("no such file".into()));

    assert_eq!(s.stage(), Stage::ShuttingDown);
    let last = entered.last().unwrap();
    assert!(matches!(last.reason, ShedReason::Unmeasurable(_)));
    assert!(
        last.to_string().contains("unknown is treated as empty"),
        "the operator must be told this was a decision, not a measurement: {last}"
    );
}

#[test]
fn the_reserve_is_a_floor_the_node_shuts_down_holding_not_one_it_spends() {
    // "Clean shutdown with charge remaining" is a claim about a race against the
    // kernel's own cut-off. It is only true if the ladder stops while there is
    // still charge, so the boundary is inclusive.
    let mut at = Shed::new(ShedPlan::recommended());
    at.on_tick(secs(0), &Charge::Measured(ShedPlan::MIN_RESERVE));
    assert_eq!(at.stage(), Stage::ShuttingDown, "at the reserve, not below");

    let mut above = Shed::new(ShedPlan::recommended());
    above.on_tick(secs(0), &Charge::Measured(Percent::clamped(11)));
    assert_eq!(above.stage(), Stage::Announced);
}

#[test]
fn the_reserve_may_be_raised_but_never_lowered() {
    // The mirror of the hard stop, which is configurable downward only. In both
    // cases the direction that is safe for everyone is the direction left open,
    // and the operator who wants the laxer setting is the one told no.
    assert!(ShedPlan::new(secs(60), secs(180), Percent::clamped(40)).is_ok());
    assert_eq!(
        ShedPlan::new(secs(60), secs(180), Percent::clamped(3)),
        Err(PlanError::ReserveTooLow)
    );
    assert!(
        ShedPlan::new(secs(60), secs(180), Percent::clamped(3))
            .unwrap_err()
            .to_string()
            .contains("kernel's own cut-off"),
        "the refusal must say what it is protecting against"
    );
}

#[test]
fn a_plan_that_quiesces_before_it_sheds_is_refused() {
    // Flushing a database while the indexer is still writing into it is not a
    // checkpoint. The ordering is the whole content of the ladder, so an
    // unordered plan is not a stricter configuration — it is a broken one.
    assert_eq!(
        ShedPlan::new(secs(180), secs(60), ShedPlan::MIN_RESERVE),
        Err(PlanError::RungsNotAscending)
    );
    assert_eq!(
        ShedPlan::new(secs(60), secs(60), ShedPlan::MIN_RESERVE),
        Err(PlanError::RungsNotAscending),
        "simultaneous is not ordered"
    );
}

// ── Mains coming back ─────────────────────────────────────────────────────────

#[test]
fn a_brief_flicker_resumes_service_because_nothing_was_torn_down() {
    let mut s = Shed::new(ShedPlan::recommended());
    s.on_tick(secs(5), &healthy());
    assert_eq!(s.stage(), Stage::Announced);
    assert!(s.restored());
    assert_eq!(s.stage(), Stage::Serving);
}

#[test]
fn mains_returning_after_the_database_was_closed_does_not_silently_reopen_it() {
    // A supply that flickers every thirty seconds would otherwise have this node
    // starting and stopping its database all evening, and each of those is a
    // start-up with its own recovery. Coming back from here is an operation
    // somebody performs, not something the power supply performs on their
    // behalf.
    for reached in [secs(60), secs(180)] {
        let mut s = Shed::new(ShedPlan::recommended());
        s.on_tick(reached, &healthy());
        let before = s.stage();
        assert!(!s.restored(), "must not resume from {before:?}");
        assert_eq!(s.stage(), before);
    }
}

#[test]
fn mains_returning_does_not_call_off_a_shutdown_already_decided() {
    // That decision was made about a cell with a known amount of energy left,
    // and mains arriving a second later does not put the energy back.
    let mut s = Shed::new(ShedPlan::recommended());
    s.on_tick(secs(0), &Charge::Measured(Percent::clamped(4)));
    assert_eq!(s.stage(), Stage::ShuttingDown);
    assert!(!s.restored());
    assert_eq!(s.stage(), Stage::ShuttingDown);
}

// ── The claim itself ──────────────────────────────────────────────────────────

#[test]
fn a_node_with_no_cell_does_not_claim_a_ups_and_does_not_pretend_to_ride_it_out() {
    // ADR-0002's battery-absent mode. Some retired handsets run with the pack
    // removed, powered from the pad. A node in that state presenting a shed
    // ladder is promising three minutes it does not have — and the §8 claim is
    // the one most likely to be copied onto a page and left there, which is why
    // it is computed here rather than written down.
    let mut s = Shed::without_cell(ShedPlan::recommended());
    assert!(matches!(s.ups_claim(), UpsClaim::Unbacked { .. }));

    let entered = s.on_tick(secs(0), &healthy());
    assert_eq!(s.stage(), Stage::ShuttingDown);
    assert_eq!(
        entered.last().unwrap().reason,
        ShedReason::NoUps,
        "a healthy-looking charge reading must not manufacture an outage to ride"
    );
}

#[test]
fn a_node_with_a_cell_claims_the_ups_and_names_the_reserve_it_stops_at() {
    let plan = ShedPlan::new(secs(60), secs(180), Percent::clamped(25)).unwrap();
    let s = Shed::new(plan);
    assert_eq!(
        s.ups_claim(),
        UpsClaim::Backed {
            reserve: Percent::clamped(25)
        },
        "the claim carries the number, so a panel cannot render it without one"
    );
}

#[test]
fn every_rung_entered_is_recorded_with_why() {
    // The ladder's output is an operator's account of an outage. A history that
    // recorded stages without reasons would leave "shut down at 22:04" next to
    // no way of telling a planned reserve stop from a cell nobody could read.
    let mut s = Shed::new(ShedPlan::recommended());
    s.on_tick(secs(0), &healthy());
    s.on_tick(secs(200), &healthy());
    s.on_tick(secs(400), &Charge::Measured(Percent::clamped(9)));

    assert_eq!(
        stages(s.history()),
        [
            Stage::Announced,
            Stage::Shed,
            Stage::Quiesced,
            Stage::ShuttingDown
        ]
    );
    for t in s.history() {
        assert!(
            !t.to_string().is_empty() && t.to_string().contains(t.stage.obligation()),
            "each entry must state the obligation it recorded: {t}"
        );
    }
}
