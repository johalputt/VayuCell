// SPDX-License-Identifier: Apache-2.0

//! `vayucell` — the binary that owns the supervisor loop.
//!
//! Everything of consequence lives in [`args`] and [`report`], which are
//! testable. This file is the part that cannot be: it reads the real process
//! environment, points a [`RealHost`] at real sysfs, and exits with a status.
//!
//! # What running this actually does
//!
//! `vayucell status` reads the device once and prints the safety panel, exiting
//! with a code that carries the verdict. `vayucell run` holds the charge ceiling
//! and watches the cell until the governor halts.
//!
//! # This has not been run on a phone
//!
//! On an ordinary laptop `/sys/class/power_supply/battery` does not exist, so
//! `status` prints a panel whose every device row is unverified and exits 1.
//! That is the correct output and it is also the only output anybody has ever
//! seen from it. No handset has run this binary.

mod args;
mod report;

use std::process::ExitCode;

use args::{Args, Command};
use report::EXIT_USAGE;

use vayucell_core::battery::Percent;
use vayucell_core::governor::{Governor, Level, Thresholds};
use vayucell_core::host::RealHost;
use vayucell_core::runtime::{Clock, Power, RealClock, Supervisor};
use vayucell_core::sampler::Sampler;
use vayucell_core::shed::{Shed, ShedPlan};
use vayucell_core::sysfs::{detect_mechanism, Kind, SysfsCeiling};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    let parsed = match args::parse(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return ExitCode::from(u8::try_from(EXIT_USAGE).unwrap_or(64));
        }
    };

    let code = match parsed.command {
        Command::Help => {
            print!("{}", args::USAGE);
            0
        }
        Command::Version => {
            println!("vayucell {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Command::Status => status(&parsed),
        Command::Run { ticks } => run(&parsed, ticks),
    };

    ExitCode::from(u8::try_from(code).unwrap_or(64))
}

/// One read, one panel, one verdict.
fn status(a: &Args) -> i32 {
    let host = RealHost;
    let panel = report::assemble(
        &host,
        &a.supply_dir,
        Percent::clamped(i64::from(a.ceiling)),
        Level::Normal,
    );
    print!("{}", panel.render());
    report::exit_code(panel.overall())
}

/// The supervisor loop, against the real machine and a clock that really sleeps.
fn run(a: &Args, ticks: Option<u32>) -> i32 {
    let mut host = RealHost;
    let thresholds = Thresholds::recommended();
    let ceiling = Percent::clamped(i64::from(a.ceiling));

    let mut supervisor = Supervisor::new(
        Governor::new(thresholds),
        Sampler::new(thresholds),
        Shed::new(ShedPlan::recommended()),
        &a.supply_dir,
        ceiling,
    );

    let kind = detect_mechanism(&host, &a.supply_dir);
    announce_mechanism(kind);

    let mut clock = RealClock::new();
    let mut done = 0u32;

    loop {
        let power = match a.assume_outage {
            Some(since) => Power::Battery(since.saturating_add(clock.elapsed())),
            None => Power::Mains,
        };

        // Rebound every pass. The mechanism borrows the host mutably, and
        // holding that borrow across the loop would mean the reading and the
        // write could not both happen — the read-back this whole project turns
        // on needs the same node available to both.
        let outcome = {
            let mut mech = kind
                .filter(|k| k.is_ceiling())
                .and_then(|k| SysfsCeiling::new(&mut host, &a.supply_dir, k));
            let m = mech
                .as_mut()
                .map(|c| c as &mut dyn vayucell_core::governor::ChargeMechanism);
            supervisor.tick(&RealHost, m, power)
        };

        if let Some(e) = &outcome.read_error {
            eprintln!("vayucell: {e}");
        }
        if let Some(t) = &outcome.transition {
            println!("vayucell: {t}");
        }
        for rung in &outcome.shed {
            println!("vayucell: {rung}");
        }

        if outcome.level == Level::Halt {
            // The governor halts and this process stops with it. Continuing to
            // loop after a hard stop would make HALT a log line rather than a
            // state, which is the one thing it must never become.
            eprintln!(
                "vayucell: the governor has halted. This requires a person who \
                 has looked at the phone; no restart clears it."
            );
            return report::EXIT_UNSAFE;
        }

        done += 1;
        if ticks.is_some_and(|n| done >= n) {
            let panel = report::assemble(
                &RealHost,
                &a.supply_dir,
                ceiling,
                supervisor.governor().level(),
            );
            print!("{}", panel.render());
            return report::exit_code(panel.overall());
        }

        clock.sleep(outcome.next_in);
    }
}

fn announce_mechanism(kind: Option<Kind>) {
    match kind {
        Some(k) if k.is_ceiling() => {
            println!("vayucell: holding a ceiling via {}", k.node());
        }
        Some(k) => {
            // Named rather than glossed. This device has a control and it is not
            // a ceiling, and the difference is the difference between a verified
            // limit and a hope.
            println!(
                "vayucell: {} is present but exposes no percentage to read back, \
                 so no ceiling is being verified",
                k.node()
            );
        }
        None => {
            println!(
                "vayucell: this device exposes no charge control, so no ceiling \
                 can be held. That is permanent on this hardware."
            );
        }
    }
}
