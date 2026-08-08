// SPDX-License-Identifier: Apache-2.0

//! Turning a real device into the two facts every serving command needs.
//!
//! `site` and `vault` both have to answer the same question before every request —
//! what does the governor say, and where is the outage ladder — and both had
//! their own copy of the answer. Two copies of a safety decision is one more
//! than can be kept in step, and the copy that drifts is the one nobody is
//! looking at.
//!
//! It lives here rather than in `main.rs` for a second reason: `main.rs` is the
//! composition root, it is never unit-tested, and logic that matters should not
//! sit in the one file no test can reach.

use vayucell_core::governor::{Governor, Level, Thresholds};
use vayucell_core::host::Host;
use vayucell_core::shed::Charge;
use vayucell_core::sysfs::read_battery;

/// Reads the cell once and says what the governor makes of it.
///
/// A fresh governor per call, observing a fresh reading. It carries no history,
/// so it cannot latch — which is right for a per-request question and wrong for
/// `run`, where the escalation has to survive a cool reading. The two are
/// deliberately different, and this is the per-request one.
///
/// A cell that could not be read yields [`Level::Protect`], never "probably
/// fine". Absence is never protection, and a request answered on the strength of
/// a reading nobody took is the failure this project is built against.
#[must_use]
pub fn observe(host: &dyn Host, supply_dir: &str) -> (Level, Charge) {
    match read_battery(host, supply_dir) {
        Ok(reading) => {
            let mut governor = Governor::new(Thresholds::recommended());
            governor.observe(&reading);
            (governor.level(), Charge::Measured(reading.capacity))
        }
        Err(e) => (Level::Protect, Charge::Unreadable(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::observe;
    use vayucell_core::governor::Level;
    use vayucell_core::host::FakeHost;
    use vayucell_core::shed::Charge;
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

    #[test]
    fn a_cool_cell_reads_as_normal_and_reports_its_charge() {
        let (level, charge) = observe(&device(290, 58), SUPPLY);
        assert_eq!(level, Level::Normal);
        assert!(matches!(charge, Charge::Measured(_)), "{charge:?}");
    }

    #[test]
    fn a_hot_cell_halts_on_the_reading_that_saw_it() {
        let (level, _) = observe(&device(600, 58), SUPPLY);
        assert_eq!(level, Level::Halt);
    }

    #[test]
    fn a_cell_that_cannot_be_read_is_protect_rather_than_probably_fine() {
        // The whole reason this is a function rather than an expression inlined
        // twice. A request answered on the strength of a reading nobody took is
        // the failure this project exists to prevent.
        let (level, charge) = observe(&FakeHost::new(), SUPPLY);
        assert_eq!(level, Level::Protect);
        assert!(matches!(charge, Charge::Unreadable(_)), "{charge:?}");
    }

    #[test]
    fn the_unreadable_charge_carries_the_reason_rather_than_a_sentinel() {
        // The shed ladder treats an unknown charge as empty, and an operator
        // reading a log needs to know which node would not answer.
        let (_, charge) = observe(&FakeHost::new(), SUPPLY);
        let Charge::Unreadable(why) = charge else {
            panic!("an unreadable cell must say so")
        };
        assert!(!why.is_empty(), "the reason was blank");
    }

    #[test]
    fn each_call_starts_a_fresh_governor_so_a_request_cannot_latch() {
        // Right here and wrong for `run`. A per-request question is about the
        // cell now; the escalation that must survive a cool reading belongs to
        // the supervisor, which owns its governor across ticks.
        assert_eq!(observe(&device(600, 58), SUPPLY).0, Level::Halt);
        assert_eq!(
            observe(&device(290, 58), SUPPLY).0,
            Level::Normal,
            "a previous hot reading latched into a later cool one"
        );
    }
}
