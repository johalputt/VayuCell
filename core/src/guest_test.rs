// SPDX-License-Identifier: Apache-2.0

//! Tests for the tier-2 guest contract.
//!
//! The property under test throughout is the one PLAN §11 puts on this phase:
//! **the guest survives a host reboot**, which in software terms means the
//! claims it made before the reboot do not survive it, and the guest says so
//! rather than carrying them forward.

use crate::battery::Percent;
use crate::guest::{
    Attestation, Claim, Fault, Guest, HostEpoch, Ledger, Mediated, Observation, Record, Standing,
    HOST_ATTESTATION_ENV,
};
use crate::host::FakeHost;

fn epoch(counter: u64) -> HostEpoch {
    HostEpoch::new(counter).expect("a boot counter above zero")
}

fn statement(text: &str) -> Attestation {
    Attestation::parse(text).expect("a well-formed host statement")
}

fn host_saying(text: &str) -> FakeHost {
    FakeHost::new().with_env(HOST_ATTESTATION_ENV, text)
}

#[test]
fn a_host_reboot_voids_the_claims_made_in_the_earlier_boot() {
    // The gate on this phase, in software. The guest said the cell was held at
    // 60% on the host's word; the host rebooted; the ceiling it was holding
    // went with it. A guest that carried the claim forward would be reporting
    // a control that no longer exists, which is the failure Charter Article IV
    // is written against.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement("epoch=8;ceiling=60"))
        .expect("a current statement");
    assert!(matches!(
        ledger.standing(Mediated::Ceiling),
        Standing::Attested { .. }
    ));

    // The host comes back in a new boot and says nothing about the ceiling.
    ledger
        .establish(&statement("epoch=9"))
        .expect("a current statement");

    match ledger.standing(Mediated::Ceiling) {
        Standing::Void {
            established_in,
            host_now,
            ..
        } => {
            assert_eq!(established_in, epoch(8));
            assert_eq!(host_now, epoch(9));
        }
        other => panic!("a void claim was reported as {other:?}"),
    }
}

#[test]
fn a_reboot_is_noticed_even_when_the_host_says_nothing_at_all() {
    // The other direction, and the one a mutation would break first. The guest
    // learns which boot the host is in *separately* from what the host says
    // about the cell, so a host that reboots and says nothing at all still
    // invalidates the old claim.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement("epoch=8;ceiling=60"))
        .expect("a current statement");
    assert_eq!(ledger.observe(epoch(9)), Observation::Rebooted);
    assert!(matches!(
        ledger.standing(Mediated::Ceiling),
        Standing::Void { .. }
    ));
}

#[test]
fn the_same_boot_is_not_reported_as_a_reboot() {
    // Otherwise every poll of the host would look like a power cut, and the
    // guest would void its claims continuously and never hold anything.
    let mut ledger = Ledger::new();
    ledger.observe(epoch(8));
    assert_eq!(ledger.observe(epoch(8)), Observation::SameBoot);
    assert_eq!(ledger.observe(epoch(9)), Observation::Rebooted);
}

#[test]
fn a_boot_counter_that_goes_backwards_is_refused() {
    // A counter that moves backwards is either a replayed statement or a
    // restored snapshot, and neither may be believed: believing one would put
    // back a ceiling the host released when it rebooted.
    let mut ledger = Ledger::new();
    ledger.observe(epoch(9));
    assert_eq!(ledger.observe(epoch(8)), Observation::Replayed);
    assert_eq!(ledger.observed_epoch(), Some(epoch(9)));
}

#[test]
fn a_statement_from_an_earlier_boot_is_refused() {
    // The same protection one layer up, where the statement itself carries the
    // boot it belongs to.
    let mut ledger = Ledger::new();
    ledger.observe(epoch(9));
    let outcome = ledger.establish(&statement("epoch=8;ceiling=60"));
    assert!(matches!(outcome, Err(Fault::Replayed(_))), "{outcome:?}");
    // And the refusal changes nothing.
    assert_eq!(ledger.standing(Mediated::Ceiling), Standing::Unknown);
}

#[test]
fn a_control_the_host_never_mentioned_is_unknown_rather_than_a_negative() {
    // "The host said nothing about its Doze policy" and "the host says Doze
    // applies to this guest" are different answers, and only the second one is
    // a statement. Collapsing them would make a host that was merely silent
    // into a host that has promised to sleep the guest.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement("epoch=8;ceiling=60"))
        .expect("a current statement");
    assert_eq!(ledger.standing(Mediated::Doze), Standing::Unknown);
    assert_eq!(ledger.standing(Mediated::Reachability), Standing::Unknown);
}

#[test]
fn the_host_saying_it_holds_no_ceiling_is_not_the_same_as_saying_nothing() {
    // The other direction. A host that says "I hold no ceiling" has told the
    // guest something definite, and the guest must not report it as silence.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement("epoch=8;ceiling=none"))
        .expect("a current statement");
    match ledger.standing(Mediated::Ceiling) {
        Standing::Attested { claim, .. } => assert_eq!(claim, Claim::HoldingNothing),
        other => panic!("a definite negative was reported as {other:?}"),
    }
}

#[test]
fn a_statement_with_no_boot_counter_is_refused() {
    // Every claim has to belong to a boot, or the reboot that invalidates it
    // cannot be noticed. A statement with no counter is a statement the guest
    // cannot date, and an undated claim is one that can never be invalidated.
    assert!(matches!(
        Attestation::parse("ceiling=60"),
        Err(Fault::Malformed(_))
    ));
}

#[test]
fn a_statement_with_an_unknown_key_is_refused_rather_than_partly_believed() {
    // A statement this build does not understand is refused outright. Reading
    // the parts it recognises and dropping the rest would let anything the
    // host said be smuggled past a rule this build has not been taught.
    assert!(matches!(
        Attestation::parse("epoch=8;charge_control_end_threshold=60"),
        Err(Fault::Malformed(_))
    ));
}

#[test]
fn a_statement_that_claims_two_things_about_one_control_is_refused() {
    // A host that says both "the cell is held at 60%" and "the host holds no
    // ceiling" has said nothing, and picking one of them would be this program
    // deciding which.
    assert!(matches!(
        Attestation::parse("epoch=8;ceiling=60;ceiling=none"),
        Err(Fault::Contradictory(_))
    ));
}

#[test]
fn a_ceiling_above_a_hundred_is_refused_rather_than_clamped() {
    // A ceiling of 150 is not a clamp away from a ceiling of 100. Clamping it
    // would ship a limit on somebody's lithium cell that nobody chose, on the
    // one setting that governs it.
    assert!(matches!(
        Attestation::parse("epoch=8;ceiling=150"),
        Err(Fault::Malformed(_))
    ));
    assert!(matches!(
        Attestation::parse("epoch=8;ceiling=-1"),
        Err(Fault::Malformed(_))
    ));
}

#[test]
fn a_boot_counter_of_zero_is_refused() {
    // A counter that has never been set reads as zero, and a zero epoch
    // compares as older than every real boot — so it would silently void every
    // fact the guest holds.
    assert!(matches!(
        Attestation::parse("epoch=0;ceiling=60"),
        Err(Fault::Malformed(_))
    ));
    assert!(HostEpoch::new(0).is_none());
}

#[test]
fn the_record_round_trips() {
    // The record is the only thing that survives a host reboot, so a record
    // that cannot be written back is a guest that forgets everything.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement(
            "epoch=8;ceiling=60;doze=exempt;ingress=published",
        ))
        .expect("a current statement");
    let text = ledger.render();

    let read = Ledger::parse(&text).expect("a record this build wrote");
    assert_eq!(read, ledger);
    assert_eq!(read.observed_epoch(), Some(epoch(8)));
    assert!(matches!(
        read.standing(Mediated::Ceiling),
        Standing::Attested { .. }
    ));
}

#[test]
fn a_record_that_cannot_be_understood_is_not_read_as_an_empty_record() {
    // Something wrote this file, and treating it as "the guest has
    // established nothing" would turn a lost record into a clean bill of
    // health.
    assert!(matches!(
        Ledger::parse("not a guest record at all"),
        Err(Fault::Malformed(_))
    ));
    assert!(matches!(Ledger::parse(""), Err(Fault::Malformed(_))));
}

#[test]
fn a_record_holding_two_facts_about_one_control_is_refused() {
    let text = "vayucell guest ledger 1\nobserved 8\nfact 8 holding 60\nfact 8 holding nothing\n";
    assert!(matches!(Ledger::parse(text), Err(Fault::Contradictory(_))));
}

#[test]
fn a_record_holding_a_fact_from_a_boot_it_never_observed_is_refused() {
    // This program cannot write such a record, so one that says it was
    // written by something else.
    let text = "vayucell guest ledger 1\nobserved 8\nfact 9 holding 60\n";
    assert!(matches!(Ledger::parse(text), Err(Fault::Malformed(_))));
}

#[test]
fn a_guest_with_no_host_statement_says_the_host_was_silent() {
    // Absence of a statement is reported as absence, and the record the guest
    // already holds is still the record it holds.
    let guest = Guest::reconcile(&FakeHost::new(), Record::Absent);
    assert!(matches!(guest.host_fault(), Some(Fault::Absent)));
    assert_eq!(guest.standing(Mediated::Ceiling), Standing::Unknown);
}

#[test]
fn a_guest_whose_own_record_could_not_be_read_says_so_and_claims_nothing() {
    // A record that exists and will not open is not an empty record. The guest
    // says what went wrong and claims nothing, rather than claiming the
    // absence of claims.
    let guest = Guest::reconcile(
        &FakeHost::new(),
        Record::Unreadable("/home/somebody/.vayucell/guest: permission denied".to_owned()),
    );
    assert!(guest.record_fault().is_some());
    assert_eq!(guest.standing(Mediated::Ceiling), Standing::Unknown);
}

#[test]
fn a_guest_folds_the_hosts_statement_into_its_own_record() {
    // The end-to-end shape of the thing: a record from before a reboot, and a
    // host that has come back in a new boot.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement("epoch=8;ceiling=60"))
        .expect("a current statement");

    let guest = Guest::reconcile(&host_saying("epoch=9"), Record::Read(ledger));
    assert_eq!(guest.boot(), Some(epoch(9)));
    assert!(matches!(
        guest.standing(Mediated::Ceiling),
        Standing::Void { .. }
    ));
}

#[test]
fn a_guest_that_receives_a_statement_establishes_it() {
    let guest = Guest::reconcile(&host_saying("epoch=8;ceiling=60"), Record::Absent);
    assert!(guest.host_fault().is_none());
    match guest.standing(Mediated::Ceiling) {
        Standing::Attested { claim, epoch: boot } => {
            assert_eq!(claim, Claim::Holding(Percent::clamped(60)));
            assert_eq!(boot, epoch(8));
        }
        other => panic!("a current statement was reported as {other:?}"),
    }
}

#[test]
fn a_statement_this_build_cannot_read_is_reported_as_such_and_claims_nothing() {
    let guest = Guest::reconcile(&host_saying("epoch=8;ceiling=150"), Record::Absent);
    assert!(matches!(guest.host_fault(), Some(Fault::Malformed(_))));
    assert_eq!(guest.standing(Mediated::Ceiling), Standing::Unknown);
}

#[test]
fn the_doze_claim_carries_the_caveat_that_it_cannot_be_checked_from_inside() {
    // The Doze promise is the one a reader is most likely to take for a
    // measurement, and it is the one furthest from being one: it is a
    // statement about what the host will do next, made by the host.
    let mut ledger = Ledger::new();
    ledger
        .establish(&statement("epoch=8;doze=exempt"))
        .expect("a current statement");
    let evidence = ledger.standing(Mediated::Doze).evidence();
    assert!(evidence.contains("promise"), "{evidence}");
    assert!(evidence.contains("Doze"), "{evidence}");
}

#[test]
fn every_control_is_reported_in_order() {
    // The report lists them all, so a control nobody mentioned cannot be left
    // out of the report by forgetting to look at it.
    let mut ledger = Ledger::new();
    ledger.observe(epoch(8));
    let controls: Vec<Mediated> = ledger.standings().into_iter().map(|(c, _)| c).collect();
    assert_eq!(
        controls,
        vec![Mediated::Ceiling, Mediated::Doze, Mediated::Reachability]
    );
}

#[test]
fn a_claim_about_one_control_cannot_be_read_as_another() {
    // The control lives inside the claim, so pairing the reachability value
    // with the ceiling control is not expressible.
    assert_eq!(Claim::Published.control(), Mediated::Reachability);
    assert_eq!(Claim::DozeApplies.control(), Mediated::Doze);
    assert_eq!(Claim::HoldingNothing.control(), Mediated::Ceiling);
}

#[test]
fn a_claim_survives_the_wire_it_is_written_on() {
    // The record is written on disk, so every claim has to survive a trip
    // through text.
    for claim in [
        Claim::Holding(Percent::clamped(60)),
        Claim::HoldingNothing,
        Claim::DozeExempt,
        Claim::DozeApplies,
        Claim::Published,
        Claim::Withheld,
    ] {
        let wire = claim.wire();
        assert_eq!(Claim::from_wire(&wire).expect("a claim it wrote"), claim);
    }
}
