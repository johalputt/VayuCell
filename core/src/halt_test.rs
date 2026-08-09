// SPDX-License-Identifier: Apache-2.0

//! Halt-record tests, in the voice of somebody who wants their phone back.
//!
//! The interesting case is not a halt that reads cleanly. It is the record that
//! exists and cannot be read, because that is the one where "probably fine" is
//! the tempting answer and the wrong one.

use crate::governor::{Governor, Inspection, Level, Thresholds};
use crate::halt::{Halt, HaltError, Standing};

fn halted() -> Halt {
    Halt::new("pack temperature exceeded 60 °C").expect("an ordinary reason")
}

// ── The record ────────────────────────────────────────────────────────────────

#[test]
fn a_record_round_trips_through_the_form_it_is_stored_in() {
    let parsed = Halt::parse(&halted().render()).expect("what was written reads back");
    assert_eq!(parsed, halted());
}

#[test]
fn an_empty_record_is_refused_rather_than_read_as_a_halt_with_no_reason() {
    // An operator being told to go and look at their phone deserves to be told
    // what the device saw. A halt naming nothing is not evidence of anything.
    for raw in ["", "   ", "\n", "\t\n "] {
        assert_eq!(Halt::parse(raw), Err(HaltError::Empty), "{raw:?}");
    }
}

#[test]
fn a_record_carrying_a_control_character_is_refused() {
    // The reason is printed to a terminal. A newline in the middle of it
    // rewrites the following line, which turns a record into a way to forge
    // whatever the operator reads next.
    for raw in ["hot\nvayucell: all clear", "a\0b", "a\rb"] {
        assert_eq!(Halt::parse(raw), Err(HaltError::Control), "{raw:?}");
    }
}

#[test]
fn every_refusal_says_what_is_wrong_rather_than_that_it_is_invalid() {
    for e in [HaltError::Empty, HaltError::Control] {
        let msg = e.to_string();
        assert!(msg.split_whitespace().count() >= 6, "{msg}");
        assert!(!msg.to_lowercase().contains("invalid"), "{msg}");
    }
}

#[test]
fn the_record_is_one_line_so_a_partial_write_looks_partial() {
    let out = halted().render();
    assert_eq!(out.lines().count(), 1);
    assert!(out.ends_with('\n'), "{out:?}");
}

// ── The decision ──────────────────────────────────────────────────────────────

#[test]
fn only_a_clear_standing_may_start_serving() {
    assert!(Standing::Clear.may_start_serving());
    assert!(!Standing::Halted(halted()).may_start_serving());
    assert!(!Standing::Unreadable("permission denied".to_owned()).may_start_serving());
}

#[test]
fn a_record_nobody_could_read_is_not_treated_as_no_record() {
    // The whole reason this type has three variants. "The file is there and I
    // could not open it" and "there is no file" are different facts, and only
    // one of them is evidence that nothing happened.
    let unreadable = Standing::Unreadable("permission denied".to_owned());
    assert_ne!(unreadable, Standing::Clear);
    assert!(!unreadable.may_start_serving());

    let said = unreadable.describe();
    assert!(said.contains("could not be read"), "{said}");
    assert!(said.contains("treated as halted"), "{said}");
}

#[test]
fn a_standing_says_which_of_the_three_it_is_rather_than_just_refusing() {
    let clear = Standing::Clear.describe();
    assert!(clear.contains("no halt"), "{clear}");

    let held = Standing::Halted(halted()).describe();
    assert!(held.contains("60 °C"), "{held}");
    assert!(held.contains("looking at it"), "{held}");
}

#[test]
fn a_standing_halt_floors_any_report_at_halt() {
    // The panel reads the cell, and the cell has usually cooled by the time
    // anybody looks at it. Without a floor, `vayucell status` on a phone with a
    // halt recorded prints "governor at NORMAL; no threshold crossed" — which is
    // exactly the defect the governor row was fixed for once already, arriving
    // from a different direction.
    assert_eq!(Standing::Clear.floor(), Level::Normal);
    assert_eq!(Standing::Halted(halted()).floor(), Level::Halt);
    assert_eq!(
        Standing::Unreadable("permission denied".to_owned()).floor(),
        Level::Halt
    );
}

#[test]
fn the_floor_never_lowers_a_reading_that_is_already_worse() {
    // Taken as a maximum, so a device that is hot *now* is not reported as
    // merely halted-earlier, and one that is halted-earlier is not reported as
    // fine because it has cooled.
    let clear = Standing::Clear;
    assert_eq!(Level::Halt.max(clear.floor()), Level::Halt);
    assert_eq!(Level::Derated.max(clear.floor()), Level::Derated);
    assert_eq!(
        Level::Normal.max(Standing::Halted(halted()).floor()),
        Level::Halt
    );
}

// ── The governor it produces ──────────────────────────────────────────────────

#[test]
fn a_recorded_halt_produces_a_governor_that_is_already_halted() {
    // The property the binary has been claiming. Before this existed, every
    // start was Governor::new at NORMAL, so a restart cleared a hard stop that
    // the same binary said no restart could clear.
    let g = Governor::halted(Thresholds::recommended());
    assert_eq!(g.level(), Level::Halt);
    assert!(!g.may_serve());
}

#[test]
fn an_inherited_halt_claims_no_history_it_did_not_witness() {
    // It inherited a conclusion; it did not watch the escalation. Manufacturing
    // a transition here would put a reading in the record that nobody took.
    let g = Governor::halted(Thresholds::recommended());
    assert!(g.history().is_empty(), "{:?}", g.history());
    assert_eq!(g.consecutive_failures(), 0);
}

#[test]
fn a_person_who_looked_and_found_it_flat_clears_the_halt() {
    // The only way down the ladder, and it requires somebody to have looked.
    let g = Governor::halted(Thresholds::recommended())
        .after_inspection(Inspection::LiesFlat)
        .expect("a flat cell may resume");
    assert_eq!(g.level(), Level::Normal);
}

#[test]
fn a_person_who_looked_and_found_it_deformed_does_not_clear_it() {
    // The case where the answer must be no whatever the sensors say afterwards.
    // A cell somebody has watched deform is not a cell to resume serving on.
    let refused = Governor::halted(Thresholds::recommended())
        .after_inspection(Inspection::Deformed)
        .expect_err("a deformed cell never resumes");
    assert_eq!(refused.level(), Level::Halt);
}
