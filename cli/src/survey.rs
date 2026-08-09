// SPDX-License-Identifier: Apache-2.0

//! A device report somebody can paste, for a project that has never seen a phone.
//!
//! # Why this exists
//!
//! Every device-facing claim in this repository rests on a fake host describing
//! handsets nobody here is holding, and the README carries that as a permanent
//! row. The only thing that changes it is a person running this on real
//! hardware and saying what happened.
//!
//! Until now the issue template asked four free-text questions and never asked
//! for the program's own output. The program knows far more than anybody will
//! type: which nodes the kernel exposes, which of the four charge mechanisms are
//! present, what the tier probes actually found. **That is the data a hardware
//! database is made of, and it was being thrown away at the point of collection.**
//!
//! # It prints. It does not send.
//!
//! There is no network code here and there is not going to be. The README's
//! headline is "No account. No telemetry.", and a device report that phoned home
//! would make that sentence false — quietly, and in the one direction nobody
//! would check. So the report goes to standard output and a person decides what
//! to do with it.
//!
//! ## What the report is allowed to say about that
//!
//! It used to print *"this program has no network code"*, which is not true of
//! the program. The same binary runs three HTTP listeners — that is most of what
//! it is for — so an operator who has ever run `vayucell site` reads that
//! sentence, knows it is wrong, and has been given a reason to discount every
//! other assurance in the block. **An overstated reassurance is worse than a
//! narrow one, because the reader can check it.**
//!
//! What is true, and is the property that actually matters, is stronger and
//! narrower: **nothing in this binary dials out.** There is no `connect` outside
//! the test module — only `bind`, and only when somebody asks for a surface. A
//! report cannot phone home because nothing here can, and a test pins that the
//! sentence claims listening-not-connecting rather than an absence of sockets.
//!
//! # What it contains is listed in what it prints
//!
//! A phone is a personal device and this report is going into a public issue.
//! Rather than a promise in a document nobody reads, the report opens by naming
//! what it holds and what it deliberately leaves out — so the person about to
//! paste it can check the claim against the text underneath it.
//!
//! Nothing here reads a serial number, an IMEI, a MAC address, a hostname, a
//! username, any network configuration, or anything from the operator's site or
//! vault directories. The only path that can carry anything personal is a
//! `--supply-dir` the operator chose themselves, and the report says so.

use core::fmt::Write as _;

use vayucell_core::battery::Percent;
use vayucell_core::halt::Standing;
use vayucell_core::host::Host;
use vayucell_core::sysfs::{detect_mechanism, NODES, PROBE_ORDER, SUPPLY};
use vayucell_core::tier::{detect, Verdict, SHELL_ASSERTION_ENV};

/// Builds the report.
///
/// Returns a `String` rather than printing, so every line of it is reachable in
/// a test against a fake device — which is the only kind this project has.
#[must_use]
pub fn report(host: &dyn Host, supply_dir: &str, version: &str, standing: &Standing) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "VayuCell device report — v{version}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Nothing is sent anywhere: nothing in this binary dials out. It listens when\n\
         you ask it to serve and never connects to anything, and this report is\n\
         printed for you to paste. Read it before you do."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "CONTAINS   the version, what was probed on this device, which power-supply\n\
        \x20          files exist and their values, and what the panel concluded.\n\
         OMITS      no serial number, no IMEI, no MAC or Wi-Fi details, no account,\n\
        \x20          no hostname, no username, and nothing from your site or vault\n\
        \x20          folders."
    );
    // The values in this report that the operator chose themselves, and so the
    // only ones that can carry anything personal. The OMITS block above is a
    // claim, and a claim with an exception nobody mentions is a false one — so
    // each exception is named where the claim is made rather than left for a
    // reader to find further down.
    if supply_dir != SUPPLY {
        let _ = writeln!(
            out,
            "YOURS      you passed --supply-dir, so the path below is one you chose."
        );
    }
    if let Some(value) = host.env(SHELL_ASSERTION_ENV) {
        // An unrecognised assertion is echoed verbatim by the tier probe, which
        // is right — an operator who set it to the wrong thing needs to see what
        // they set. It also means whatever they put in it lands in a report
        // going into a public issue.
        let _ = writeln!(
            out,
            "YOURS      you set {SHELL_ASSERTION_ENV}={value:?}; it appears below because\n\
            \x20          the tier probe quotes what it was given."
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "TIER");
    let detection = detect(host);
    // Matched rather than formatted, so the two answers that are not a tier keep
    // their reasons. "Unverified" with the reason dropped is the shape this
    // whole project refuses.
    let _ = match &detection.verdict {
        Verdict::Established(t) => {
            writeln!(out, "  verdict  {t:?} established from positive evidence")
        }
        Verdict::Unverified(why) => writeln!(out, "  verdict  UNVERIFIED: {why}"),
        Verdict::Unknown => writeln!(
            out,
            "  verdict  UNKNOWN: nothing recognised this machine as a target device"
        ),
    };
    for f in &detection.findings {
        // The mark carries the answer and the evidence carries the reason. A
        // report that said only "not found" would send somebody to guess which
        // file was missing on a device they cannot see.
        let mark = if f.found { "yes" } else { "no " };
        let _ = writeln!(out, "  [{mark}]  {:<22} {}", f.what, f.evidence);
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "POWER SUPPLY  {supply_dir}");
    for node in NODES {
        match host.read(&format!("{supply_dir}/{node}")) {
            // Trimmed, because a trailing newline in a pasted report reads as a
            // blank field rather than as the value it is.
            Some(raw) => {
                let _ = writeln!(out, "  present  {node:<22} {}", raw.trim());
            }
            // Absence is the most useful line in this whole report. A node this
            // handset does not have is exactly what a hardware database is for,
            // and omitting the line would make it indistinguishable from a node
            // nobody looked at.
            None => {
                let _ = writeln!(out, "  ABSENT   {node}");
            }
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "CHARGE LIMITING");
    for kind in PROBE_ORDER {
        let present = host
            .read(&format!("{supply_dir}/{}", kind.node()))
            .is_some();
        let mark = if present { "present" } else { "ABSENT " };
        let ceiling = if kind.is_ceiling() {
            "holds a percentage"
        } else {
            "influences charging, exposes no percentage to read back"
        };
        let _ = writeln!(out, "  {mark}  {:<30} {ceiling}", kind.node());
    }
    let _ = match detect_mechanism(host, supply_dir) {
        Some(k) => writeln!(out, "  concluded  {} ({:?})", k.node(), k),
        None => writeln!(
            out,
            "  concluded  no mechanism; no ceiling can be held on this device"
        ),
    };

    let _ = writeln!(out);
    let _ = writeln!(out, "PANEL");
    let panel = crate::report::observed(host, supply_dir, Percent::clamped(60), standing);
    for line in panel.render().lines() {
        let _ = writeln!(out, "  {line}");
    }
    let _ = writeln!(out, "  exit {}", crate::report::exit_code(panel.overall()));

    out
}

#[cfg(test)]
mod tests {
    use super::report;
    use vayucell_core::halt::{Halt, Standing};
    use vayucell_core::host::FakeHost;
    use vayucell_core::sysfs::{NODES, SUPPLY};
    use vayucell_core::tier::SHELL_ASSERTION_ENV;

    fn phone() -> FakeHost {
        let mut h = FakeHost::new();
        for (node, value) in [
            ("capacity", "58"),
            ("voltage_now", "3820000"),
            ("current_now", "-180000"),
            ("temp", "290"),
            ("cycle_count", "412"),
            ("charge_full", "3600000"),
            ("charge_full_design", "4000000"),
        ] {
            h = h.with_file(&format!("{SUPPLY}/{node}"), &format!("{value}\n"));
        }
        h
    }

    #[test]
    fn a_node_this_handset_does_not_have_is_named_rather_than_omitted() {
        // The single most useful line in the report. `health` is absent on this
        // fixture, and a report that simply left the line out would make "this
        // device has no such node" indistinguishable from "nobody looked".
        let out = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(out.contains("ABSENT   health"), "{out}");
        assert!(out.contains("present  capacity               58"), "{out}");
    }

    #[test]
    fn every_node_the_reader_consults_appears_in_the_report() {
        // Pinned to the published list rather than to a copy, so a node added to
        // the reader cannot go unreported.
        let out = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        for node in NODES {
            assert!(
                out.contains(node),
                "{node} is missing from the report:\n{out}"
            );
        }
    }

    #[test]
    fn values_are_trimmed_so_a_pasted_report_does_not_show_blank_fields() {
        // The fixture writes each node with a trailing newline, as the kernel
        // does. Untrimmed, every value would land on the following line.
        let out = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(!out.contains("capacity               \n"), "{out}");
    }

    #[test]
    fn all_four_charge_mechanisms_are_reported_including_the_absent_ones() {
        // Which mechanisms a handset does *not* have is the answer to "why does
        // this phone say UNSAFE", and it is the same answer for most of them.
        let out = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(out.contains("charge_control_end_threshold"), "{out}");
        assert!(
            out.contains("no mechanism; no ceiling can be held"),
            "{out}"
        );
    }

    #[test]
    fn a_device_that_answers_nothing_still_produces_a_report() {
        // The case a first tester is most likely to hit, and the one where a
        // panic or an empty page would waste the only run anybody made.
        let out = report(&FakeHost::new(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(out.contains("TIER"), "{out}");
        assert!(out.contains("ABSENT   capacity"), "{out}");
        assert!(out.contains("BATTERY SAFETY"), "{out}");
    }

    #[test]
    fn the_report_says_what_it_holds_and_what_it_leaves_out() {
        // A promise in a document nobody reads is not a control. The claim
        // travels with the text it describes, so whoever is about to paste it
        // can check one against the other.
        let out = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(out.contains("CONTAINS"), "{out}");
        assert!(out.contains("OMITS"), "{out}");
        for absent in ["IMEI", "MAC", "hostname", "username"] {
            assert!(
                out.contains(absent),
                "{absent} is not named as omitted:\n{out}"
            );
        }
        // The claim about the network is stated narrowly on purpose. It used to
        // say "this program has no network code", which is false — the same
        // binary runs three HTTP listeners. An operator who has run
        // `vayucell site` can check that sentence, find it wrong, and discount
        // the rest of the block with it.
        assert!(
            !out.contains("no network code"),
            "the report claims something about this binary that is not true:\n{out}"
        );
        assert!(out.contains("dials out"), "{out}");
        assert!(
            out.contains("listens when") || out.contains("listens"),
            "the true property is listening rather than connecting:\n{out}"
        );
    }

    #[test]
    fn an_operator_set_assertion_is_flagged_because_the_probe_quotes_it() {
        // The tier probe echoes an unrecognised VAYUCELL_HOST_ASSERTION verbatim,
        // which is right — somebody who set it wrong needs to see what they set.
        // It also means whatever they typed lands in a report going into a public
        // issue, so the claim above it has to say so.
        let mine = phone().with_env(SHELL_ASSERTION_ENV, "alices-spare-pixel");
        let out = report(&mine, SUPPLY, "0.0.0", &Standing::Clear);
        assert!(
            out.contains("alices-spare-pixel"),
            "the probe stopped quoting it:\n{out}"
        );
        assert!(
            out.contains(&format!("you set {SHELL_ASSERTION_ENV}")),
            "an operator-set value reached the report unflagged:\n{out}"
        );
    }

    #[test]
    fn a_report_with_nothing_operator_set_claims_no_exceptions() {
        // The other direction, so "flag what they chose" cannot be satisfied by
        // a line that is always printed.
        let out = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(!out.contains("YOURS"), "{out}");
    }

    #[test]
    fn a_supply_directory_the_operator_chose_is_flagged_as_theirs() {
        // The only path here that can carry a person's name. Silently including
        // it would make the OMITS block above a slightly false statement.
        let mine = report(&phone(), "/home/somebody/fake", "0.0.0", &Standing::Clear);
        assert!(mine.contains("path below is one you chose"), "{mine}");

        let standard = report(&phone(), SUPPLY, "0.0.0", &Standing::Clear);
        assert!(
            !standard.contains("path below is one you chose"),
            "{standard}"
        );
    }

    #[test]
    fn a_standing_halt_reaches_the_report_as_it_reaches_the_panel() {
        // A report from a halted phone is the most interesting one anybody will
        // ever send, and by the time they send it the cell has cooled.
        let halted = Standing::Halted(Halt::new("pack temperature exceeded 60 °C").expect("ok"));
        let out = report(&phone(), SUPPLY, "0.0.0", &halted);
        assert!(out.contains("governor at HALT"), "{out}");
    }
}
