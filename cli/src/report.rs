// SPDX-License-Identifier: Apache-2.0

//! Assembling a panel from a device, and turning its verdict into an exit code.
//!
//! Kept out of `main` because every interesting case here is a device that does
//! not answer, and none of those can be exercised by running the binary on the
//! machine that happens to be building it.
//!
//! # The exit code is the verdict
//!
//! [`exit_code`] maps the panel's headline onto a process status, so a monitor
//! can read the result without parsing prose. A non-zero exit here is not a
//! crash — it is the answer. That distinction is the whole reason
//! [`EXIT_UNVERIFIED`] and [`EXIT_UNSAFE`] are different numbers: a device
//! nobody could measure and a device that failed its checks call for different
//! responses, and collapsing them into "non-zero" would lose exactly the
//! distinction Charter Article IV exists to preserve.

use vayucell_core::battery::Percent;
use vayucell_core::governor::Level;
use vayucell_core::halt::Standing;
use vayucell_core::host::Host;
use vayucell_core::panel::{
    Confidence, Evidence, Finding, Overall, Panel, RiskLevel, SwellingRisk,
};
use vayucell_core::shed::UpsClaim;
use vayucell_core::sysfs::{detect_mechanism, read_battery, Kind};
use vayucell_core::tier::detect;

/// Every row was checked and holds.
pub const EXIT_PROTECTED: i32 = 0;
/// Something could not be checked.
pub const EXIT_UNVERIFIED: i32 = 1;
/// Something was checked and does not hold.
pub const EXIT_UNSAFE: i32 = 2;
/// The arguments were not usable. Follows the conventional `EX_USAGE`.
pub const EXIT_USAGE: i32 = 64;

/// The process status a panel calls for.
#[must_use]
pub const fn exit_code(overall: Overall) -> i32 {
    match overall {
        Overall::Protected => EXIT_PROTECTED,
        Overall::Unverified => EXIT_UNVERIFIED,
        Overall::Unsafe => EXIT_UNSAFE,
    }
}

/// The panel for a device, with the governor's level read from that same device.
///
/// [`assemble`] takes the level as an argument because the supervisor holds one
/// that has latched across ticks, and handing it a fresh reading would throw
/// that history away. Every *other* caller has no governor to speak of and has
/// to derive the level from the cell in front of it.
///
/// Both of them passed a literal `Level::Normal` instead. The governor row is
/// the one row rendering `Verified` — *"no threshold crossed"* — so `status` and
/// `serve` printed a green, positively worded assertion about a comparison
/// nobody had made, on a phone the governor would have halted. This exists so
/// that the correct call is also the shorter one.
///
/// `standing` floors the level. A halt recorded earlier is a fact about the
/// device that no fresh reading can see — the cell has usually cooled by the
/// time anybody looks — and without the floor this function printed
/// `VERIFIED  battery governor  governor at NORMAL; no threshold crossed` on a
/// phone whose halt record, sitting on the same disk, said a threshold had been
/// crossed and nobody had been to look at it.
///
/// The panel is deliberately still *served* on a halted device rather than
/// withheld: `run` and `all` refuse to start, and the one surface that reports
/// whether the battery is safe is the last thing to take away from somebody
/// whose battery is not. So it stays, and tells the truth.
#[must_use]
pub fn observed(host: &dyn Host, supply_dir: &str, ceiling: Percent, standing: &Standing) -> Panel {
    let (level, _) = crate::device::observe(host, supply_dir);
    assemble(host, supply_dir, ceiling, level.max(standing.floor()))
}

/// Builds the panel for a device as it is right now.
///
/// Nothing here defaults. A battery that could not be read produces an
/// `Unverified` ceiling row naming the node that refused, not a ceiling row that
/// quietly says nothing — and the headline moves off `PROTECTED` as a result.
#[must_use]
pub fn assemble(host: &dyn Host, supply_dir: &str, ceiling: Percent, level: Level) -> Panel {
    let tier = detect(host);
    let mechanism: Option<Kind> = detect_mechanism(host, supply_dir);

    let ceiling_row = match read_battery(host, supply_dir) {
        Err(e) => Finding::Unverified(evidence(&format!(
            "the cell could not be read, so no ceiling can be confirmed: {e}"
        ))),
        Ok(_) => match mechanism {
            // A mechanism exists and the cell reads, but this process has not
            // written and re-read the node in this pass. Saying "verified" here
            // would be the panel reporting an intention.
            Some(k) if k.is_ceiling() => Finding::Unverified(evidence(&format!(
                "{} is present and holds a percentage; run `vayucell run` to \
                 write {}% and read it back",
                k.node(),
                ceiling.get()
            ))),
            Some(k) => Finding::Unverified(evidence(&format!(
                "{} influences charging but exposes no percentage to read back",
                k.node()
            ))),
            None => Finding::Unverified(evidence(
                "no mechanism exists to hold a ceiling on this device",
            )),
        },
    };

    // The cell either carries this node or it does not, and this process cannot
    // tell without a reading. An unreadable cell is not credited with a reserve.
    let ups = match read_battery(host, supply_dir) {
        Ok(_) => UpsClaim::Backed { reserve: ceiling },
        Err(_) => UpsClaim::Unbacked {
            why: "the cell could not be read, so nothing is known to be carrying this node",
        },
    };

    Panel::build(
        &tier.verdict,
        level,
        mechanism,
        &ceiling_row,
        &ups,
        // ADR-0002 §6 lists five proxies. This process has computed none of
        // them — there is no history to compute them from on a first run — so
        // the estimate rests on nothing and says so, rather than reporting
        // Nominal as though the proxies had been consulted and were quiet.
        SwellingRisk {
            level: RiskLevel::Nominal,
            confidence: Confidence::Low,
            basis: Vec::new(),
        },
    )
}

fn evidence(what: &str) -> Evidence {
    Evidence::new(what).unwrap_or_else(|| {
        Evidence::new("a check reported no detail, which is a defect in this software")
            .expect("the fallback is not blank")
    })
}

#[cfg(test)]
mod tests {
    use super::observed;
    use super::{assemble, exit_code, EXIT_PROTECTED, EXIT_UNSAFE, EXIT_UNVERIFIED};
    use vayucell_core::battery::Percent;
    use vayucell_core::governor::Level;
    use vayucell_core::halt::{Halt, Standing};
    use vayucell_core::host::FakeHost;
    use vayucell_core::panel::Overall;
    use vayucell_core::sysfs::SUPPLY;

    fn readable_device() -> FakeHost {
        FakeHost::new()
            .with_file(&format!("{SUPPLY}/capacity"), "58\n")
            .with_file(&format!("{SUPPLY}/voltage_now"), "3820000\n")
            .with_file(&format!("{SUPPLY}/current_now"), "-180000\n")
            .with_file(&format!("{SUPPLY}/temp"), "290\n")
            .with_file(&format!("{SUPPLY}/cycle_count"), "412\n")
            .with_file(&format!("{SUPPLY}/charge_full"), "3600000\n")
            .with_file(&format!("{SUPPLY}/charge_full_design"), "4000000\n")
    }

    /// The same device, hot enough that the governor halts.
    fn overheating_device() -> FakeHost {
        readable_device().with_file(&format!("{SUPPLY}/temp"), "600\n")
    }

    #[test]
    fn the_governor_row_comes_from_the_cell_rather_than_from_a_literal() {
        // The defect this function exists to prevent. `status` and `serve` both
        // passed Level::Normal, and the governor row is the one row that renders
        // Verified — so a 60 degree phone printed a green "no threshold crossed"
        // while every other row was busy being honest.
        let hot = observed(
            &overheating_device(),
            SUPPLY,
            Percent::clamped(60),
            &Standing::Clear,
        )
        .render();
        assert!(
            hot.contains("governor at HALT"),
            "a halting cell reported no threshold crossed: {hot}"
        );
        assert!(
            !hot.contains("no threshold crossed"),
            "the green governor row survived on a halting cell: {hot}"
        );
    }

    #[test]
    fn a_cool_cell_still_reports_the_governor_as_verified() {
        // The other direction, so that "read the cell" cannot be satisfied by a
        // function that reports trouble unconditionally.
        let cool = observed(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            &Standing::Clear,
        )
        .render();
        assert!(
            cool.contains("no threshold crossed"),
            "a cool cell was reported as having crossed one: {cool}"
        );
    }

    #[test]
    fn a_cell_that_cannot_be_read_is_never_reported_as_a_quiet_governor() {
        // Absence is never protection, on the row most likely to be skimmed.
        let out = observed(
            &FakeHost::new(),
            SUPPLY,
            Percent::clamped(60),
            &Standing::Clear,
        )
        .render();
        assert!(
            !out.contains("no threshold crossed"),
            "an unreadable cell produced a green governor row: {out}"
        );
    }

    fn standing_halt() -> Standing {
        Standing::Halted(Halt::new("pack temperature exceeded 60 °C").expect("ordinary"))
    }

    #[test]
    fn a_recorded_halt_reaches_the_panel_even_though_the_cell_has_cooled() {
        // The defect this argument exists for, and one I introduced myself: the
        // halt record made a restart refuse, and left the panel — the surface a
        // person actually reads to decide whether their phone is safe — printing
        // a green "no threshold crossed" beside a record saying one was crossed.
        // By the time anybody looks, the cell has always cooled.
        let out = observed(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            &standing_halt(),
        )
        .render();
        assert!(out.contains("governor at HALT"), "{out}");
        assert!(!out.contains("no threshold crossed"), "{out}");
    }

    #[test]
    fn a_record_nobody_could_read_reaches_the_panel_the_same_way() {
        let out = observed(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            &Standing::Unreadable("permission denied".to_owned()),
        )
        .render();
        assert!(out.contains("governor at HALT"), "{out}");
    }

    #[test]
    fn the_standing_floors_the_reading_rather_than_replacing_it() {
        // A phone that is hot *now* and also has a halt on record is reported on
        // what it is doing now. Taking the standing instead of the maximum would
        // lose a live reading behind an old one.
        let hot = observed(
            &overheating_device(),
            SUPPLY,
            Percent::clamped(60),
            &standing_halt(),
        )
        .render();
        assert!(hot.contains("governor at HALT"), "{hot}");

        // The case that actually distinguishes the two, and which the first
        // version of this test was missing: a live reading worse than the
        // standing. Replacing the level with the floor instead of taking the
        // maximum passes both assertions above and loses this one entirely —
        // the mutation survived until this line existed.
        let hot_but_never_recorded = observed(
            &overheating_device(),
            SUPPLY,
            Percent::clamped(60),
            &Standing::Clear,
        )
        .render();
        assert!(
            hot_but_never_recorded.contains("governor at HALT"),
            "a cell that is hot right now was reported from the record instead: \
             {hot_but_never_recorded}"
        );

        // And a clear standing never invents trouble.
        let fine = observed(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            &Standing::Clear,
        )
        .render();
        assert!(fine.contains("no threshold crossed"), "{fine}");
    }

    #[test]
    fn the_exit_code_distinguishes_unmeasured_from_failed() {
        // Collapsing these into "non-zero" would lose exactly the distinction
        // Charter Article IV exists to preserve: a device nobody could measure
        // and a device that failed its checks call for different responses.
        assert_eq!(exit_code(Overall::Protected), EXIT_PROTECTED);
        assert_eq!(exit_code(Overall::Unverified), EXIT_UNVERIFIED);
        assert_eq!(exit_code(Overall::Unsafe), EXIT_UNSAFE);
        assert_ne!(EXIT_UNVERIFIED, EXIT_UNSAFE);
        assert_eq!(EXIT_PROTECTED, 0, "only a fully checked device exits zero");
    }

    #[test]
    fn a_machine_that_is_not_a_phone_reports_unverified_rather_than_crashing_or_passing() {
        // What running `vayucell status` on an ordinary laptop does, which is
        // the only thing anybody has ever run it on. Both failure modes are
        // wrong: a panic reads as a bug in the tool, and a clean exit reads as a
        // healthy device.
        let panel = assemble(
            &FakeHost::new(),
            SUPPLY,
            Percent::clamped(60),
            Level::Normal,
        );
        assert_ne!(panel.overall(), Overall::Protected);
        let out = panel.render();
        assert!(out.contains("could not be read"), "{out}");

        // Asserted per row, not against the whole rendering. The version of this
        // test that only searched the rendered text passed a mutation that
        // credited an unreadable cell with a full outage reserve — the phrase it
        // was matching came from the ceiling row, which is unverified for its own
        // reasons, so the assertion was true for the wrong reason. A cell nobody
        // could read is not a cell known to be carrying anything.
        let reserve = panel
            .rows()
            .iter()
            .find(|r| r.subject == "outage reserve")
            .expect("the row exists even where the cell does not answer");
        assert!(
            matches!(reserve.finding, vayucell_core::panel::Finding::Refused(_)),
            "an unreadable cell must not be credited with a reserve: {reserve}"
        );
        assert!(reserve
            .finding
            .evidence()
            .as_str()
            .contains("nothing is known to be carrying this node"));
    }

    #[test]
    fn a_readable_cell_with_no_charge_control_still_does_not_claim_a_ceiling() {
        // T0, and the most common device. The cell reads fine; there is simply
        // no way to hold a ceiling, and the panel must not imply one is held.
        let panel = assemble(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            Level::Normal,
        );
        assert_eq!(panel.overall(), Overall::Unsafe);
        assert!(panel.rows().iter().any(|r| r.subject == "charge ceiling"
            && r.finding
                .evidence()
                .as_str()
                .contains("no mechanism exists")));
    }

    #[test]
    fn a_present_ceiling_node_is_not_reported_verified_before_anything_was_written() {
        // The subtle one. `status` detects the node but has not written a value
        // and read it back, and this project's whole definition of a verified
        // ceiling is the read-back. Reporting VERIFIED here would be the panel
        // announcing an intention.
        let host =
            readable_device().with_file(&format!("{SUPPLY}/charge_control_end_threshold"), "100\n");
        let panel = assemble(&host, SUPPLY, Percent::clamped(60), Level::Normal);

        let row = panel
            .rows()
            .iter()
            .find(|r| r.subject == "charge ceiling")
            .expect("the row exists");
        assert!(
            matches!(row.finding, vayucell_core::panel::Finding::Unverified(_)),
            "{row}"
        );
        assert!(row.finding.evidence().as_str().contains("read it back"));
        assert_ne!(panel.overall(), Overall::Protected);
    }

    #[test]
    fn the_swelling_estimate_rests_on_nothing_and_says_so_on_a_first_run() {
        // There is no history to compute the five proxies from. Reporting
        // Nominal as though they had been consulted and were quiet is the exact
        // shape of a clean bill nobody earned.
        let panel = assemble(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            Level::Normal,
        );
        assert!(
            panel.render().contains("no proxies at all"),
            "{}",
            panel.render()
        );
    }

    #[test]
    fn a_halted_governor_reaches_the_panel_as_unsafe() {
        let panel = assemble(
            &readable_device(),
            SUPPLY,
            Percent::clamped(60),
            Level::Halt,
        );
        assert_eq!(panel.overall(), Overall::Unsafe);
        assert_eq!(exit_code(panel.overall()), EXIT_UNSAFE);
    }
}
