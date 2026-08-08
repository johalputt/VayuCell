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
mod listen;
mod report;

use std::process::ExitCode;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use args::{Args, Command};
use report::EXIT_USAGE;

use vayucell_core::battery::Percent;
use vayucell_core::governor::{Governor, Level, Thresholds};
use vayucell_core::host::RealHost;
use vayucell_core::runtime::{Clock, Power, RealClock, Supervisor};
use vayucell_core::sampler::Sampler;
use vayucell_core::shed::{Charge, Shed, ShedPlan, Stage};
use vayucell_core::site::{Availability, SiteRoot};
use vayucell_core::sysfs::{detect_mechanism, read_battery, Kind, SysfsCeiling};

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
        Command::Serve => serve(&parsed),
        Command::Site => site(&parsed),
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

/// Serves the panel, re-assembled per request so it never goes stale.
fn serve(a: &Args) -> i32 {
    let dir = a.supply_dir.clone();
    let ceiling = Percent::clamped(i64::from(a.ceiling));
    // Re-assembled on every request rather than rendered once at startup. A
    // panel captured at boot is a panel that says NORMAL for as long as the
    // process lives, which is the failure the whole project is built against.
    let panel = move || report::assemble(&RealHost, &dir, ceiling, Level::Normal).render();
    match listen::serve(&a.bind, &panel) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("vayucell: {e}");
            report::EXIT_UNSAFE
        }
    }
}

/// Publishes a directory, with the governor consulted on every request.
///
/// The battery is read per request rather than once at startup. That is the
/// whole difference between a site that stops when the cell is in trouble and
/// one that keeps serving because the process happened to start while everything
/// was fine.
fn site(a: &Args) -> i32 {
    let host = RealHost;
    let Some(dir) = a.site_dir.as_deref() else {
        // args::parse refuses this, so reaching here means the two disagree.
        eprintln!("vayucell: site needs --dir <DIR>");
        return report::EXIT_USAGE;
    };

    let root = match SiteRoot::open(&host, dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
    };

    if !root.has_index(&host) {
        // Not a failure. A site whose top level is only subdirectories is a real
        // arrangement, and the operator should hear this from the program rather
        // than from whoever visits and gets a 404.
        eprintln!(
            "vayucell: there is no {} in {}, so / will not serve anything",
            vayucell_core::site::INDEX,
            root.dir()
        );
    }

    let supply = a.supply_dir.clone();
    let outage = a.assume_outage;
    // The real ladder, not a recomputation from the clock. It latches: once a
    // rung is entered it is never walked back up on its own, which is the
    // property that makes shedding mean something. A pure function of elapsed
    // time would quietly un-shed the moment the arithmetic said so.
    let ladder = Mutex::new(Shed::new(ShedPlan::recommended()));
    let started = Instant::now();

    let availability = move || {
        // A fresh governor per request, observing a fresh reading. It carries no
        // history, so it cannot latch — right here and wrong for `run`: this
        // decides whether to answer one request from what the cell says now,
        // while `run` owns the escalation that has to survive a cool reading.
        let (level, charge) = match read_battery(&RealHost, &supply) {
            Ok(reading) => {
                let mut governor = Governor::new(Thresholds::recommended());
                governor.observe(&reading);
                (governor.level(), Charge::Measured(reading.capacity))
            }
            // A cell nobody could read is not a cell that is fine. The site is
            // withheld rather than served on the strength of a reading nobody
            // took — absence is never protection.
            Err(e) => (Level::Protect, Charge::Unreadable(e.to_string())),
        };

        let stage = match outage {
            // Mains detection is not implemented anywhere in this project — see
            // the note on runtime::Power, where whether mains is present is an
            // argument rather than something read. Claiming Serving here is
            // therefore an assumption, and it is named as one rather than
            // dressed up as a measurement.
            None => Stage::Serving,
            Some(since) => {
                // Poisoning is recovered from rather than propagated: a panic in
                // one request must not take the site down for every later one.
                let mut ladder = ladder.lock().unwrap_or_else(PoisonError::into_inner);
                ladder.on_tick(since.saturating_add(started.elapsed()), &charge);
                ladder.stage()
            }
        };

        Availability::of(level, stage)
    };

    match listen::serve_site(&a.bind, &root, &availability) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("vayucell: {e}");
            report::EXIT_UNSAFE
        }
    }
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
