// SPDX-License-Identifier: Apache-2.0

//! Ingress tests, in the attacker's voice.
//!
//! ADR-0003 §0 records that its own draft was taken apart by an adversarial pass,
//! and that two of the corrections invalidated its organising claim. Most of what
//! follows pins those corrections, because a correction with no test is a
//! paragraph somebody will helpfully undo.

use core::time::Duration;

use crate::governor::Level;
use crate::ingress::{
    disclosures, shed_for, CompromiseStory, Dependency, Mode, Reachability, ThermalClass, DEFAULT,
    FRESH_FOR,
};

/// A completed round trip, stamped at the clock's origin.
///
/// Every expiry test below then reads as an age, because `now` *is* the age.
fn verified_at_start() -> Reachability {
    Reachability::Verified { at: Duration::ZERO }
}

const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

// ── The default ───────────────────────────────────────────────────────────────

#[test]
fn a_newly_installed_cell_publishes_nothing() {
    // Charter Article VIII.5 forbids an irreversible action without explicit
    // confirmation, and publishing a device to the world is irreversible in the
    // way that matters: you cannot un-disclose it. It is also the only default
    // executable on T0, the tier most retired phones are permanently stuck at.
    assert_eq!(DEFAULT, Mode::LocalOnly);
    assert!(!DEFAULT.publishes());
    assert_eq!(DEFAULT.profile().dependency, Dependency::None);
}

// ── The corrections ADR-0003 §0 records ───────────────────────────────────────

#[test]
fn an_onion_is_not_recorded_as_dependency_free() {
    // The draft ranked it as having none, and made it the default on that basis.
    // It depends on reaching the Tor network. That is a better dependency than a
    // supplier — a commons cannot evict you — but it is not nothing, and calling
    // it nothing is a ruler chosen to flatter the default.
    let d = Mode::Onion.profile().dependency;
    assert_ne!(d, Dependency::None);
    assert_eq!(d, Dependency::Commons);
    assert!(d.who_can_evict_you().contains("no single party"));

    // And the distinction is preserved rather than flattened into "depends on
    // something".
    assert_ne!(d, Mode::Relay.profile().dependency);
    assert!(Mode::Relay
        .profile()
        .dependency
        .who_can_evict_you()
        .contains("at will"));
}

#[test]
fn an_onion_is_recorded_as_unreachable_by_ordinary_browsers() {
    // RFC 7686: .onion is a reserved special-use name, not in DNS, and it never
    // resolves. The headline claim — serve a real site from a drawer with no
    // rented infrastructure — is true about the transport and overstated about
    // the audience, and this is where that correction has to live so no copy can
    // restate it.
    assert!(!Mode::Onion.profile().ordinary_browsers);
    for reachable in [Mode::Relay, Mode::Direct, Mode::LocalOnly] {
        assert!(
            reachable.profile().ordinary_browsers,
            "{reachable:?} is reachable by an ordinary browser"
        );
    }
}

#[test]
fn the_most_sovereign_mode_is_recorded_as_having_the_worst_compromise_story() {
    // The draft did not mention it. The ed25519 identity key *is* the address:
    // steal it and the impersonation is permanent, with no revocation, no
    // authority to appeal to, and no way for a visitor to notice.
    assert_eq!(Mode::Onion.profile().compromise, CompromiseStory::Permanent);
    assert_eq!(
        Mode::Relay.profile().compromise,
        CompromiseStory::Recoverable
    );
}

#[test]
fn a_relay_records_that_it_can_read_everything_it_terminates() {
    let p = Mode::Relay.profile();
    assert_eq!(p.dependency, Dependency::Supplier);
    assert!(p.middle_sees.contains("can read everything"));
    assert!(p.costs_money);
}

// ── The thermal contract, which is the repair for the worst defect ────────────

#[test]
fn the_high_thermal_mode_is_the_one_that_was_almost_the_default() {
    // §5 exists because its absence was the design's worst defect: the default
    // ingress mode maximised sustained CPU, sustained CPU is heat, and heat is
    // the ageing ADR-0002 exists to suppress — and neither ADR mentioned the
    // other.
    assert_eq!(Mode::Onion.profile().thermal, ThermalClass::High);
    for cool in [Mode::Relay, Mode::Direct, Mode::LocalOnly] {
        assert!(cool.profile().thermal < ThermalClass::High, "{cool:?}");
    }
}

#[test]
fn a_derated_governor_sheds_high_thermal_ingress_first() {
    // Before storage, before serving work — because it is the load making the
    // device hot, and shedding anything else first would be shedding something
    // that was not the problem.
    let running = [Mode::Onion, Mode::Relay, Mode::LocalOnly];
    let left = shed_for(Level::Derated, &running);
    assert!(!left.contains(&Mode::Onion), "{left:?}");
    assert!(left.contains(&Mode::Relay));
    assert!(left.contains(&Mode::LocalOnly));
}

#[test]
fn protect_and_halt_stop_everything_outward_facing() {
    let running = [Mode::Onion, Mode::Relay, Mode::Direct, Mode::LocalOnly];
    for level in [Level::Protect, Level::Halt] {
        let left = shed_for(level, &running);
        assert_eq!(left, vec![Mode::LocalOnly], "at {level}");
        for m in &left {
            assert!(!m.publishes(), "{m:?} must not still be publishing");
        }
    }
}

#[test]
fn local_only_survives_every_level_because_it_is_not_what_is_heating_the_device() {
    // And because stopping it would take the panel away from the person who most
    // needs to read it — at exactly the moment the governor has halted.
    for level in [Level::Normal, Level::Derated, Level::Protect, Level::Halt] {
        assert!(
            shed_for(level, &[Mode::LocalOnly]).contains(&Mode::LocalOnly),
            "at {level}"
        );
    }
}

#[test]
fn nothing_is_shed_while_the_governor_is_normal() {
    let running = [Mode::Onion, Mode::Relay, Mode::Direct, Mode::LocalOnly];
    assert_eq!(shed_for(Level::Normal, &running), running.to_vec());
}

// ── Disclosure, before the choice ─────────────────────────────────────────────

#[test]
fn the_heat_cost_is_disclosed_before_the_mode_is_chosen() {
    let d = disclosures(Mode::Onion, true);
    assert!(
        d.iter()
            .any(|x| x.contains("may shed it under thermal load")),
        "{d:?}"
    );
}

#[test]
fn a_device_that_cannot_hold_a_ceiling_is_told_there_is_no_mitigation_at_all() {
    // ADR-0003 §5.5: the one combination with nothing available to do about it.
    // Said at the moment of choosing, because afterwards there is nothing to be
    // done — and a T0 handset is the most common device there is.
    let with = disclosures(Mode::Onion, false);
    assert!(
        with.iter().any(|x| x.contains("no mitigation available")),
        "{with:?}"
    );
    let without = disclosures(Mode::Onion, true);
    assert!(!without
        .iter()
        .any(|x| x.contains("no mitigation available")));
    assert!(with.len() > without.len());
}

#[test]
fn choosing_an_onion_discloses_the_audience_limit_and_the_permanent_compromise() {
    let d = disclosures(Mode::Onion, true);
    assert!(d.iter().any(|x| x.contains("will not resolve")), "{d:?}");
    assert!(d.iter().any(|x| x.contains("no revocation")), "{d:?}");
}

#[test]
fn choosing_a_relay_discloses_who_can_end_it_and_what_it_can_read() {
    let d = disclosures(Mode::Relay, true);
    assert!(d.iter().any(|x| x.contains("at will")), "{d:?}");
    assert!(d.iter().any(|x| x.contains("can read everything")), "{d:?}");
    assert!(d.iter().any(|x| x.contains("costs money")), "{d:?}");
}

#[test]
fn the_default_mode_has_nothing_to_disclose() {
    // Which is the argument for it being the default, stated as a property
    // rather than as a preference.
    assert!(disclosures(Mode::LocalOnly, true).is_empty());
    assert!(disclosures(Mode::LocalOnly, false).is_empty());
}

// ── Verification ──────────────────────────────────────────────────────────────

#[test]
fn only_a_round_trip_from_outside_counts_as_verified() {
    // Not "the tunnel process is running", not "the address was published", not
    // "the daemon returned success" — the compile_fail proof on Reachability
    // pins that there is no variant for any of them. A loopback test proves
    // nothing about a path whose entire difficulty is external.
    assert!(verified_at_start().is_verified(MINUTE));
    assert!(!Reachability::Failed("connection timed out".into()).is_verified(MINUTE));
    assert!(!Reachability::Unverified("never attempted".into()).is_verified(MINUTE));
}

#[test]
fn an_unverified_path_is_neither_up_nor_down() {
    // The word "up" is what this type exists to prevent. A path nobody has
    // completed a round trip over is not down — it may well work — and it is
    // certainly not up.
    let d = Reachability::Unverified("no check has run yet".into()).describe(MINUTE);
    assert!(d.contains("not the same as down"), "{d}");
    assert!(d.contains("not the same as up"), "{d}");
}

#[test]
fn a_verified_path_says_what_verified_meant() {
    let d = verified_at_start().describe(MINUTE);
    assert!(d.contains("from outside this device"), "{d}");
}

// ── Verification expires, which is the whole of ADR-0003 §4's schedule ────────

#[test]
fn a_day_old_round_trip_does_not_still_stand() {
    // The defect this section exists for. `Verified` carried a free-form string
    // that no code compared to a clock, so a path that worked once was verified
    // for ever — and ADR-0003 §4 says in as many words that the failure that
    // matters is the path that worked for six weeks and then stopped.
    //
    // A literal day, deliberately, and not `FRESH_FOR * 96`: a test written
    // against the constant it is pinning stays green when somebody widens that
    // constant to a century, which is precisely the change that puts the defect
    // back.
    assert!(!verified_at_start().is_verified(DAY));
    assert!(verified_at_start().due_in(DAY).is_none());
}

#[test]
fn a_round_trip_from_a_minute_ago_still_stands() {
    // The other side of the same literal. An expiry set to zero would make every
    // path permanently unverified, which is safe, useless, and would leave the
    // test above green.
    assert!(verified_at_start().is_verified(MINUTE));
    assert_eq!(verified_at_start().as_of(MINUTE), verified_at_start());
    assert!(verified_at_start().due_in(MINUTE).is_some());
}

#[test]
fn a_standing_verification_is_short_enough_to_notice_a_path_that_stopped() {
    // Both bounds literal for the same reason. Inside an hour, so a path that
    // stopped is noticed within one sitting; over a minute, so re-checking is
    // not itself the sustained load the governor exists to shed.
    assert!(FRESH_FOR <= Duration::from_secs(60 * 60), "{FRESH_FOR:?}");
    assert!(FRESH_FOR >= MINUTE, "{FRESH_FOR:?}");
}

#[test]
fn a_lapsed_verification_reports_unverified_rather_than_failed() {
    // Nothing failed. Nobody looked. Reporting a stale standing as a failure
    // would send an operator to debug a path that may be working perfectly.
    let lapsed = verified_at_start().as_of(DAY);
    assert!(
        matches!(lapsed, Reachability::Unverified(_)),
        "{lapsed:?} — a check nobody ran is not a check that failed"
    );

    let d = verified_at_start().describe(DAY);
    assert!(d.contains("1440 minutes"), "{d}");
    assert!(d.contains("not the same as down"), "{d}");
}

#[test]
fn a_round_trip_stamped_ahead_of_the_clock_is_not_evidence() {
    // The clock is monotonic and owned by this process, so this should not
    // happen — which is exactly why it must be decided rather than assumed. An
    // age that cannot be established is not an age, and this project does not
    // report what it could not check as clean.
    let ahead = Reachability::Verified { at: DAY };
    assert!(!ahead.is_verified(MINUTE));
    assert!(ahead.due_in(MINUTE).is_none());

    let d = ahead.describe(MINUTE);
    assert!(d.contains("ahead of this cell's clock"), "{d}");
    assert!(d.contains("unverified"), "{d}");
}

#[test]
fn a_check_falls_due_exactly_when_the_standing_lapses() {
    // The scheduler and the panel read the same arithmetic. Two answers to
    // "is it current?" is how a path ends up re-checked on a timer that
    // disagrees with the row the operator is looking at.
    let r = verified_at_start();
    let due = r.due_in(MINUTE).expect("a fresh standing has time left");
    let a_moment_before = (MINUTE + due).saturating_sub(Duration::from_secs(1));
    assert!(r.is_verified(a_moment_before));
    assert!(!r.is_verified(MINUTE + due));
}

#[test]
fn a_path_that_never_had_a_check_is_due_one_now() {
    assert!(Reachability::Unverified("never attempted".into())
        .due_in(MINUTE)
        .is_none());
    assert!(Reachability::Failed("connection timed out".into())
        .due_in(MINUTE)
        .is_none());
}
