// SPDX-License-Identifier: Apache-2.0

//! Panel tests, in the attacker's voice.
//!
//! The attacker here is not a person. It is the ordinary pressure that makes
//! every status display in the world drift toward green: a row nobody had data
//! for, a headline computed from the rows that were easy, an estimate that got
//! rounded into a fact somewhere between the model and the screen.

use crate::capability::Tier;
use crate::governor::Level;
use crate::panel::{
    Confidence, Evidence, Finding, Overall, Panel, RiskLevel, SwellingRisk, INSPECTION,
};
use crate::shed::UpsClaim;
use crate::sysfs::Kind;
use crate::tier::Verdict;

use crate::battery::Percent;

fn ev(s: &str) -> Evidence {
    Evidence::new(s).expect("non-blank")
}

fn nominal_risk() -> SwellingRisk {
    SwellingRisk {
        level: RiskLevel::Nominal,
        confidence: Confidence::Low,
        basis: vec!["cycle count against age"],
    }
}

/// A device where everything was checked and everything holds.
fn good_panel() -> Panel {
    Panel::build(
        &Verdict::Established(Tier::T1),
        Level::Normal,
        Some(Kind::EndThreshold),
        &Finding::Verified(ev("60% written and read back from the node")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        nominal_risk(),
    )
}

// ── Evidence ──────────────────────────────────────────────────────────────────

#[test]
fn a_row_cannot_be_built_on_blank_evidence() {
    // A blank cell renders as a confident claim with nothing behind it, and it
    // renders that way most convincingly next to rows that do have evidence.
    for blank in ["", " ", "\t\n  "] {
        assert!(
            Evidence::new(blank).is_none(),
            "{blank:?} must not become evidence"
        );
    }
    assert_eq!(
        ev("  read back from the node  ").as_str(),
        "read back from the node"
    );
}

#[test]
fn every_finding_carries_its_evidence_including_the_ones_admitting_ignorance() {
    // The variant that most wants to be evidence-free is Unverified — there was
    // nothing to see, so there is nothing to write. That is exactly backwards:
    // "could not read /sys/class/power_supply/battery/temp" is the most useful
    // line on the panel, and "unknown" on its own is the least.
    let findings = [
        Finding::Verified(ev("a")),
        Finding::Refused(ev("b")),
        Finding::Unverified(ev("c")),
    ];
    for f in &findings {
        assert!(!f.evidence().as_str().is_empty());
    }
}

// ── The headline ──────────────────────────────────────────────────────────────

#[test]
fn a_fully_checked_device_is_the_only_one_that_reads_protected() {
    assert_eq!(good_panel().overall(), Overall::Protected);
}

#[test]
fn one_unverified_row_takes_the_headline_off_protected() {
    // Charter Article IV, at the only place a user ever reads it. Four green
    // rows and one that could not be checked is not a protected device; it is a
    // device that is four-fifths checked, and the headline has to say so.
    let p = Panel::build(
        &Verdict::Unverified("the device tree could not be read".into()),
        Level::Normal,
        Some(Kind::EndThreshold),
        &Finding::Verified(ev("60% written and read back")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        nominal_risk(),
    );
    assert_eq!(p.overall(), Overall::Unverified);
    assert!(p.render().contains("NOT FULLY VERIFIED"), "{}", p.render());
}

#[test]
fn a_failure_outranks_an_unverified_row() {
    // Both are off-green, and which one wins matters: a device that has one
    // unreadable probe and one confirmed failure must not be filed under
    // "not fully verified", which reads like a paperwork problem.
    let p = Panel::build(
        &Verdict::Unverified("no device tree".into()),
        Level::Halt,
        Some(Kind::EndThreshold),
        &Finding::Verified(ev("held")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        nominal_risk(),
    );
    assert_eq!(p.overall(), Overall::Unsafe);
}

#[test]
fn an_unknown_device_is_never_protected() {
    let p = Panel::build(
        &Verdict::Unknown,
        Level::Normal,
        Some(Kind::EndThreshold),
        &Finding::Verified(ev("held")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        nominal_risk(),
    );
    assert_ne!(p.overall(), Overall::Protected);
}

#[test]
fn the_headline_is_computed_from_the_rows_and_cannot_be_set_beside_them() {
    // Structural rather than behavioural, and worth pinning: there is no
    // constructor, field or setter that takes an Overall. A headline that could
    // be set independently of the rows will eventually disagree with them, and
    // the disagreement will be in the reassuring direction, because that is the
    // direction nobody files a bug about.
    let p = good_panel();
    let rendered = p.render();
    for row in p.rows() {
        assert!(
            rendered.contains(row.finding.evidence().as_str()),
            "every row's evidence must appear in what the user reads: {row}"
        );
    }
    assert!(rendered.starts_with(&format!("BATTERY SAFETY: {}", p.overall())));
}

// ── Rows that want to disappear ───────────────────────────────────────────────

#[test]
fn a_device_with_no_charge_control_says_so_rather_than_omitting_the_row() {
    // T0, the most common device and the worst case. Dropping the row would
    // leave a panel that never mentions charge ceilings, and a user who assumes
    // one is being held — the software would have said something otherwise.
    let p = Panel::build(
        &Verdict::Established(Tier::T0),
        Level::Normal,
        None,
        &Finding::Unverified(ev("no mechanism exists to hold a ceiling on this device")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        nominal_risk(),
    );

    let mech = p
        .rows()
        .iter()
        .find(|r| r.subject == "charge mechanism")
        .expect("the row must exist even where the mechanism does not");
    assert!(matches!(mech.finding, Finding::Refused(_)));
    assert!(mech
        .finding
        .evidence()
        .as_str()
        .contains("no charge control"));
    assert_eq!(p.overall(), Overall::Unsafe);
}

#[test]
fn a_control_that_is_not_a_readable_ceiling_is_not_presented_as_one() {
    // A current limit genuinely slows ageing and genuinely is not a percentage
    // anybody can read back. The row has to carry that difference, because it is
    // the difference between a verified ceiling and a hope.
    let p = Panel::build(
        &Verdict::Established(Tier::T1),
        Level::Normal,
        Some(Kind::CurrentLimit),
        &Finding::Unverified(ev("this mechanism exposes no percentage to read back")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        nominal_risk(),
    );
    let mech = p
        .rows()
        .iter()
        .find(|r| r.subject == "charge mechanism")
        .unwrap();
    assert!(mech
        .finding
        .evidence()
        .as_str()
        .contains("not a readable ceiling"));
    assert_ne!(p.overall(), Overall::Protected);
}

#[test]
fn a_node_with_no_cell_is_not_credited_with_an_outage_reserve() {
    let p = Panel::build(
        &Verdict::Established(Tier::T1),
        Level::Normal,
        Some(Kind::EndThreshold),
        &Finding::Verified(ev("held")),
        &UpsClaim::Unbacked {
            why: "no battery is carrying this node",
        },
        nominal_risk(),
    );
    let row = p
        .rows()
        .iter()
        .find(|r| r.subject == "outage reserve")
        .unwrap();
    assert!(matches!(row.finding, Finding::Refused(_)));
}

#[test]
fn a_governor_that_has_left_normal_is_never_a_verified_row() {
    for level in [Level::Derated, Level::Protect, Level::Halt] {
        let p = Panel::build(
            &Verdict::Established(Tier::T1),
            level,
            Some(Kind::EndThreshold),
            &Finding::Verified(ev("held")),
            &UpsClaim::Backed {
                reserve: Percent::clamped(10),
            },
            nominal_risk(),
        );
        assert_eq!(p.overall(), Overall::Unsafe, "at {level}");
        assert!(
            p.render().contains(&level.to_string()),
            "the panel must name the level it is in: {level}"
        );
    }
}

// ── The swelling estimate ─────────────────────────────────────────────────────

#[test]
fn the_swelling_estimate_is_rendered_as_an_estimate_and_never_as_a_measurement() {
    // The failure this forecloses is a rounding, not a bug: a risk that reaches
    // the screen as a number, or without its confidence, is read as something an
    // instrument produced. Nothing here measured a millimetre of anything.
    let p = good_panel();
    let out = p.render();
    assert!(out.contains("not a measurement"), "{out}");
    assert!(out.contains("low confidence"), "{out}");
    assert!(out.contains("cycle count against age"), "{out}");
}

#[test]
fn the_inspection_instruction_appears_at_every_risk_level_including_nominal() {
    // Making it conditional on an elevated estimate puts the prompt exactly
    // where it is useless: absent in the case where the estimate is wrong. The
    // estimate is built from proxies precisely because the thing it estimates
    // cannot be measured here, so it being nominal is not evidence of a flat
    // cell.
    for (level, confidence) in [
        (RiskLevel::Nominal, Confidence::Low),
        (RiskLevel::Elevated, Confidence::Moderate),
    ] {
        let p = Panel::build(
            &Verdict::Established(Tier::T1),
            Level::Normal,
            Some(Kind::EndThreshold),
            &Finding::Verified(ev("held")),
            &UpsClaim::Backed {
                reserve: Percent::clamped(10),
            },
            SwellingRisk {
                level,
                confidence,
                basis: vec!["capacity fade"],
            },
        );
        assert!(
            p.render().contains(INSPECTION),
            "the instruction must survive a {level:?} estimate"
        );
    }
}

#[test]
fn the_inspection_instruction_says_the_software_cannot_do_it() {
    // Left implicit, the prompt reads as a suggestion beside a panel that has
    // already checked several things. It has to say which of the two of you can
    // actually see the cell.
    assert!(INSPECTION.contains("cannot perform it"));
    assert!(INSPECTION.contains("flat table"));
    assert!(INSPECTION.contains("hazardous-waste"));
}

#[test]
fn an_estimate_resting_on_nothing_says_so_rather_than_rendering_an_empty_list() {
    let p = Panel::build(
        &Verdict::Established(Tier::T1),
        Level::Normal,
        Some(Kind::EndThreshold),
        &Finding::Verified(ev("held")),
        &UpsClaim::Backed {
            reserve: Percent::clamped(10),
        },
        SwellingRisk {
            level: RiskLevel::Nominal,
            confidence: Confidence::Low,
            basis: vec![],
        },
    );
    assert!(
        p.render().contains("no proxies at all"),
        "a nominal estimate built on nothing must not read like a clean bill: {}",
        p.render()
    );
}

// ── The committed snapshot ────────────────────────────────────────────────────

#[test]
fn the_rendered_panels_match_the_committed_snapshot() {
    // docs/panel-snapshot.txt is what a user reads, written down.
    //
    // Every test above pins one property of the panel in isolation, which is
    // right and is not enough. The way a panel goes wrong is not that one
    // assertion breaks; it is that the wording drifts, a row moves, a hedge is
    // dropped for brevity — and every individual assertion still passes. In a
    // diff that reads as a small edit to a Rust file, and nobody reviewing it
    // sees what the user will now be told.
    //
    // With the snapshot committed, softening anything produces a diff in a plain
    // text file that shows both the reassuring case and the alarming one side by
    // side. That is a thing a reviewer notices without knowing the codebase.
    //
    // On failure: read the diff before regenerating. The regeneration command is
    // the easy part and it is not the point.
    //   cargo test --workspace -- --ignored regenerate_the_panel_snapshot
    let expected = include_str!("../../docs/panel-snapshot.txt");
    assert_eq!(
        render_snapshot(),
        expected,
        "\nWhat the safety panel tells a user has changed.\n\
         Read the diff above and satisfy yourself the change is intended, then \
         regenerate docs/panel-snapshot.txt.\n"
    );
}

/// Both panels as a stable, human-readable block.
///
/// Two devices deliberately: the one where everything holds, and the T0 handset
/// with no charge control, a derated governor and no cell behind it. A snapshot
/// of only the good case would let the alarming panel be softened without any
/// diff at all — and the alarming panel is the one that matters.
fn render_snapshot() -> String {
    use crate::panel::{Confidence, RiskLevel};

    let mut out = String::from(
        "# What the VayuCell safety panel tells a user\n\
         #\n\
         # Generated from core/src/panel.rs. Do not edit by hand.\n\
         # Regenerate: cargo test --workspace -- --ignored regenerate_the_panel_snapshot\n\
         #\n\
         # Two devices: one where every check holds, and one T0 handset with no\n\
         # charge control, a derated governor and no cell behind it.\n\n",
    );

    out.push_str("--- a device where everything was checked and holds ---\n\n");
    out.push_str(
        &Panel::build(
            &Verdict::Established(Tier::T1),
            Level::Normal,
            Some(Kind::EndThreshold),
            &Finding::Verified(ev(
                "60% written to charge_control_end_threshold and read back",
            )),
            &UpsClaim::Backed {
                reserve: Percent::clamped(10),
            },
            SwellingRisk {
                level: RiskLevel::Nominal,
                confidence: Confidence::Low,
                basis: vec!["cycle count against age", "capacity fade"],
            },
        )
        .render(),
    );

    out.push_str("\n--- a stock handset with no charge control and no cell ---\n\n");
    out.push_str(
        &Panel::build(
            &Verdict::Established(Tier::T0),
            Level::Derated,
            None,
            &Finding::Unverified(ev("no mechanism exists to hold a ceiling on this device")),
            &UpsClaim::Unbacked {
                why: "no battery is carrying this node; mains loss stops it immediately",
            },
            SwellingRisk {
                level: RiskLevel::Elevated,
                confidence: Confidence::Moderate,
                basis: vec![
                    "capacity fade",
                    "time above 40 °C",
                    "charge acceptance falling",
                ],
            },
        )
        .render(),
    );
    out
}

#[test]
#[ignore = "writes docs/panel-snapshot.txt; run deliberately after reviewing a change to what the panel says"]
fn regenerate_the_panel_snapshot() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/panel-snapshot.txt");
    std::fs::write(path, render_snapshot()).expect("could not write the snapshot");
    println!("wrote {path}");
}
