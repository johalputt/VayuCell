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
mod cell;
mod device;
mod enrol;
mod halted;
mod listen;
mod onion;
mod report;
mod storage;
mod survey;

use std::process::ExitCode;

use args::{Args, Command};
use cell::{Cell, Governed};
use report::EXIT_USAGE;

use vayucell_core::battery::Percent;
use vayucell_core::governor::{Governor, Level, Thresholds};
use vayucell_core::host::RealHost;
use vayucell_core::runtime::{Clock, Power, RealClock, Supervisor};
use vayucell_core::sampler::Sampler;
use vayucell_core::shed::{Shed, ShedPlan, Stage};
use vayucell_core::site::SiteRoot;
use vayucell_core::sysfs::{detect_mechanism, Kind, SysfsCeiling};
use vayucell_core::vault::VaultRoot;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    let parsed = match args::parse(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return ExitCode::from(u8::try_from(EXIT_USAGE).unwrap_or(64));
        }
    };

    // One place, asked of every command. `run` and `all` each carried their own
    // copy of this check and the two commands added after them did not get one,
    // so a halted phone went on serving a website and accepting uploads the
    // moment somebody restarted it into a different subcommand — while the
    // binary's own halt message says no restart clears it.
    //
    // Before anything binds a socket or writes a ceiling.
    if parsed.command.serves_traffic() {
        if let Some(code) = refuse_if_halted(&parsed) {
            return ExitCode::from(u8::try_from(code).unwrap_or(64));
        }
    }

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
        Command::Vault => vault(&parsed),
        Command::All => all(&parsed),
        Command::Inspect => inspect(&parsed),
        Command::Report => {
            print!(
                "{}",
                survey::report(
                    &RealHost,
                    &parsed.supply_dir,
                    env!("CARGO_PKG_VERSION"),
                    &halted::read(&parsed.halt_record),
                    // A fresh clock. `report` prints once and exits, so nothing
                    // it shows has had time to go stale — but the storage
                    // posture demands both readings rather than assuming that,
                    // which is why the type asks for them.
                    vayucell_core::durability::Now {
                        since_start: core::time::Duration::ZERO,
                        today: {
                            use vayucell_core::runtime::Clock as _;
                            vayucell_core::runtime::RealClock::new().wall_clock_unix()
                        },
                    },
                )
            );
            0
        }
        Command::Enrol => enrol_device(&parsed),
        Command::Devices => list_devices(&parsed),
        Command::Revoke => revoke_device(&parsed),
        Command::Run { ticks } => run(&parsed, ticks),
    };

    ExitCode::from(u8::try_from(code).unwrap_or(64))
}

/// One read, one panel, one verdict.
fn status(a: &Args) -> i32 {
    let host = RealHost;
    // The level is derived from the same device this panel is about. It was
    // passed as a literal `Level::Normal` here, which made the governor row read
    // `VERIFIED — no threshold crossed` on a phone the governor would have
    // halted: the panel asserting, in green, a state nobody had computed.
    let panel = report::observed(
        &host,
        &a.supply_dir,
        Percent::clamped(i64::from(a.ceiling)),
        &halted::read(&a.halt_record),
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
    // process lives, which is the failure the whole project is built against —
    // and so is a panel that is rebuilt every time around a level that was never
    // read. The governor row now comes from the cell, per request, like the
    // rest of it.
    // The record is re-read per request, like everything else on this surface.
    // A panel that read it once at startup would keep reporting a halt after
    // somebody cleared it, and keep reporting health after one arrived.
    let record = a.halt_record.clone();
    let panel = move || report::observed(&RealHost, &dir, ceiling, &halted::read(&record)).render();
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

    // One cell, one ladder — see cell.rs. This command serves one surface, so
    // the sharing buys nothing here; it is written this way so that `all`, which
    // serves three, cannot end up with a second copy of the ladder by being
    // written separately.
    let cell = Cell::new(a.supply_dir.clone(), a.assume_outage);
    let availability = || cell.availability(&RealHost);

    announce_ungoverned();

    match listen::serve_site(&a.bind, &root, &availability) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("vayucell: {e}");
            report::EXIT_UNSAFE
        }
    }
}

/// Adds a device to the credential store and shows its secret once.
fn enrol_device(a: &Args) -> i32 {
    let Some(device) = a.device.as_deref() else {
        eprintln!("vayucell: enrol needs --device <NAME>");
        return report::EXIT_USAGE;
    };
    match enrol::enrol(&a.store, device) {
        Ok(secret) => {
            let Ok(text) = core::str::from_utf8(secret.expose_for_comparison()) else {
                eprintln!("vayucell: the minted secret was not text");
                return report::EXIT_UNSAFE;
            };
            println!("Enrolled {device}. Its secret is:\n");
            println!("    {text}\n");
            // Said plainly, because the next thing the person does decides
            // whether this credential survives.
            println!(
                "This is the only time it is shown. There is no command that prints \n\
                 it back — a credential a program will re-display is one that leaks \n\
                 through a scrollback or a screen share. If you lose it, enrol the \n\
                 device again; that takes five seconds.\n"
            );
            println!("Use it like this:\n");
            // No client is named here on purpose. The charter gate refuses
            // any outbound-connection vocabulary in this source because this
            // program never dials out — it only listens — so the request is
            // described in the protocol's own words and the command that
            // sends it lives wherever the file does.
            println!(
                "    PUT http://<phone>:8080/report.pdf\n\
                 \x20   Authorization: Bearer {text}\n\
                 \n\
                 Any HTTP client speaks that; the command that sends the file\n\
                 runs on your machine, not this one."
            );
            println!("Revoke it with: vayucell revoke --device {device}");
            0
        }
        Err(e) => {
            eprintln!("vayucell: {e}");
            report::EXIT_USAGE
        }
    }
}

/// Lists the devices enrolled here. Never prints a secret.
fn list_devices(a: &Args) -> i32 {
    let creds = match enrol::load(&a.store) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
    };
    if creds.is_empty() {
        // Said as a state with a consequence, not as an empty list. In this
        // condition the vault refuses everything, which is correct and is also
        // not what somebody who just started it wanted.
        println!("No device is enrolled, so the vault will refuse every request.");
        println!("Enrol one with: vayucell enrol --device <name>");
        return 0;
    }
    println!("{} device(s) enrolled in {}:\n", creds.len(), a.store);
    for device in creds.devices() {
        println!("    {device}");
    }
    println!("\nRevoke one with: vayucell revoke --device <name>");
    println!("Secrets are never printed back; enrol again if one is lost.");
    0
}

/// Removes a device, so its credential stops working.
fn revoke_device(a: &Args) -> i32 {
    let Some(device) = a.device.as_deref() else {
        eprintln!("vayucell: revoke needs --device <NAME>");
        return report::EXIT_USAGE;
    };
    match enrol::revoke(&a.store, device) {
        Ok(true) => {
            println!("Revoked {device}. Its credential no longer works.");
            // The vault reads the store once at start, so a running one is still
            // holding the old list. Said plainly rather than left to surprise.
            println!("A vault already running still holds the old list; restart it.");
            0
        }
        Ok(false) => {
            eprintln!(
                "vayucell: {device} is not enrolled. Run `vayucell devices` to see which are."
            );
            report::EXIT_USAGE
        }
        Err(e) => {
            eprintln!("vayucell: {e}");
            report::EXIT_USAGE
        }
    }
}

/// Serves the vault, with the governor consulted on every request.
fn vault(a: &Args) -> i32 {
    let host = RealHost;
    let Some(dir) = a.vault_dir.as_deref() else {
        eprintln!("vayucell: vault needs --dir <DIR>");
        return report::EXIT_USAGE;
    };

    let root = match VaultRoot::open(&host, dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
    };

    // Loaded once at start rather than per request. A credential store that is
    // re-read on every request is a file opened by anything that can send
    // traffic, and revocation is already a restart-shaped act.
    let credentials = match enrol::load(&a.store) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
    };

    let cell = Cell::new(a.supply_dir.clone(), a.assume_outage);
    let context = || cell.context(&RealHost);

    announce_ungoverned();

    match listen::serve_vault(&a.bind, &root, &credentials, &context, a.quota) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("vayucell: {e}");
            report::EXIT_UNSAFE
        }
    }
}

/// Records that a person looked at the phone, and clears a halt if it was sound.
///
/// The only way down from `HALT`, and it deliberately takes a human observation
/// rather than a reading. ADR-0002 §6: software cannot measure a millimetre of
/// deformation, so the check that matters is somebody putting the phone
/// face-down on a table — and this is where that observation is recorded.
fn inspect(a: &Args) -> i32 {
    let Some(seen) = a.inspection else {
        eprintln!("vayucell: inspect needs --lies-flat or --deformed");
        return report::EXIT_USAGE;
    };

    match Governor::new(Thresholds::recommended()).after_inspection(seen) {
        Ok(_) => {
            if let Err(e) = halted::clear(&a.halt_record) {
                eprintln!("vayucell: the halt record would not clear: {e}");
                // Not reported as success. The person did their part; the
                // device is still refusing, and saying otherwise would send
                // them away believing it had resumed.
                return report::EXIT_UNSAFE;
            }
            println!("Recorded: the phone lies flat and no edge is lifting.");
            println!("The halt is cleared. `vayucell all` will serve again.");
            println!();
            println!("Check again next month. Nothing in this program can see a");
            println!("cell beginning to swell; you can, in five seconds, by looking.");
            0
        }
        Err(_) => {
            // The record is left exactly where it is. A deformed cell does not
            // resume, whatever any sensor says afterwards, and there is no flag
            // that overrides this.
            eprintln!("Recorded: the phone is deformed. The halt stands and will not clear.");
            eprintln!();
            eprintln!("Stop using it now. Unplug it, and take it to hazardous-waste or");
            eprintln!("electronics recycling. Do not puncture it and do not put it in");
            eprintln!("household rubbish.");
            report::EXIT_UNSAFE
        }
    }
}

/// Refuses to start when a halt is standing, and says how to clear it.
///
/// Called before anything binds a socket or writes a ceiling, so a device that
/// a person has not looked at never begins serving and then stops.
fn refuse_if_halted(a: &Args) -> Option<i32> {
    let standing = halted::read(&a.halt_record);
    if standing.may_start_serving() {
        return None;
    }
    eprintln!("vayucell: {}", standing.describe());
    eprintln!();
    eprintln!("Put the phone face-down on a flat table and look at it.");
    eprintln!("  it lies flat, nothing lifting   ->  vayucell inspect --lies-flat");
    eprintln!("  it rocks, or an edge is lifting ->  vayucell inspect --deformed");
    Some(report::EXIT_UNSAFE)
}

/// The sentence a halt is recorded under.
///
/// The governor's own transition where there is one — it names the threshold
/// and the reading that crossed it, which is what the operator needs. A halt
/// reached without a transition on that tick still has to say something, and
/// says that rather than nothing: an empty record is refused, and a refused
/// record is a phone that comes back serving.
fn halt_reason(transition: Option<&vayucell_core::governor::Transition>) -> String {
    transition.map_or_else(
        || "the governor reached HALT; the transition was not on this pass".to_owned(),
        ToString::to_string,
    )
}

/// Writes the halt down, so the next start inherits it.
///
/// A halt that is only ever printed is a log line. This is what makes the
/// sentence beside it — *no restart clears it* — a true statement about the
/// program rather than an intention.
fn record_halt(path: &str, reason: &str) {
    match vayucell_core::halt::Halt::new(reason).map(|h| halted::record(path, &h)) {
        Ok(Ok(())) => eprintln!("vayucell: the halt is recorded in {path}."),
        // Said out loud. If this failed, the next start will come back serving,
        // and the operator is the only one who can know that.
        Ok(Err(e)) => eprintln!(
            "vayucell: THE HALT COULD NOT BE RECORDED: {e}\n\
             \x20         Restarting will clear it. Do not restart until somebody has \n\
             \x20         looked at the phone."
        ),
        Err(e) => eprintln!("vayucell: the halt reason was not usable as a record: {e}"),
    }
}

/// Serves the panel, the site and the vault together, under one governor.
///
/// # Why this is a command rather than three terminals
///
/// Because three processes are three [`Cell`]s, and a [`Cell`] owns a shed
/// ladder that latches. Two of them on one phone measure from two different
/// start instants and can disagree about which rung the node has reached — and
/// the one that disagrees in the reassuring direction is the one still serving
/// after the other has shed. There is one battery, so there is one ladder.
///
/// Each surface still gets its own port, because the same-origin policy is what
/// stops a page on somebody's website reading the panel that reports whether
/// their battery is safe, and it counts ports rather than paths.
fn all(a: &Args) -> i32 {
    let host = RealHost;

    let [panel_addr, site_addr, vault_addr] = match args::adjacent_ports(&a.bind) {
        Ok(addrs) => addrs,
        Err(e) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
    };

    // Everything that can be refused is refused here, before a single listener
    // exists. A process that binds two ports and then discovers the third
    // directory is missing has already published two things on the strength of
    // an invocation that was never valid.
    let site = match a.site_dir.as_deref().map(|dir| SiteRoot::open(&host, dir)) {
        Some(Ok(root)) => Some(root),
        Some(Err(e)) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
        None => None,
    };

    let vault = match a
        .vault_dir
        .as_deref()
        .map(|dir| VaultRoot::open(&host, dir))
    {
        Some(Ok(root)) => Some(root),
        Some(Err(e)) => {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
        None => None,
    };

    let credentials = if vault.is_some() {
        match enrol::load(&a.store) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("vayucell: {e}");
                return report::EXIT_USAGE;
            }
        }
    } else {
        None
    };

    if let Some(root) = site.as_ref() {
        if !root.has_index(&host) {
            eprintln!(
                "vayucell: there is no {} in {}, so / will not serve anything",
                vayucell_core::site::INDEX,
                root.dir()
            );
        }
    }

    let ceiling = Percent::clamped(i64::from(a.ceiling));
    let supply = a.supply_dir.clone();

    // A real supervisor, not a reading taken per request. `all` is the command
    // the install guide tells somebody to leave running for months, and until
    // this existed it was the one command that never wrote a charge ceiling —
    // while the panel it served told the operator to run `vayucell run` to hold
    // one, which the guide never asked them to do.
    let thresholds = Thresholds::recommended();
    let governed = Governed::new(
        Supervisor::new(
            Governor::new(thresholds),
            Sampler::new(thresholds),
            Shed::new(ShedPlan::recommended()),
            &supply,
            ceiling,
        ),
        supply.clone(),
    );

    let kind = detect_mechanism(&host, &supply);
    announce_mechanism(kind);

    // Everything about publishing is decided, written and said BEFORE a
    // single thread starts: the operator reads the disclosures of ADR-0003
    // §5 while nothing has been disclosed to the world yet, and an unusable
    // onion directory refuses this invocation instead of quietly degrading
    // into local-only after three lines claimed otherwise.
    let onion_plan = a.onion_dir.as_ref().map(|dir| {
        crate::onion::build_plan(
            dir,
            site.as_ref()
                .and_then(|_| crate::onion::port_of(&site_addr)),
            vault
                .as_ref()
                .and_then(|_| crate::onion::port_of(&vault_addr)),
        )
    });
    if let Some(plan) = &onion_plan {
        if let Err(e) = crate::onion::prepare(plan) {
            eprintln!("vayucell: {e}");
            return report::EXIT_USAGE;
        }
    }
    let daemon = crate::onion::find_daemon(std::env::var("PATH").ok().as_deref());
    match (&onion_plan, &daemon) {
        (Some(_), Some(found)) => {
            // Said before anything publishes. The choice was made on the
            // command line; these are the consequences of it, in full, per
            // ADR-0003 §5.4 and §6.
            for d in vayucell_core::ingress::disclosures(
                vayucell_core::ingress::Mode::Onion,
                kind.is_some_and(|k| k.is_ceiling()),
            ) {
                println!("vayucell: {d}");
            }
            println!(
                "vayucell: publishing through {found}; it is a dependency of \
                 this mode ({}), not a part of this program",
                vayucell_core::onion::DAEMON_DEPENDENCY
            );
        }
        (Some(_), None) => {
            // Degrade, loudly: the cell stays reachable on its own network
            // only, which is exactly what the default posture promises —
            // but not what was asked for here, so that difference is said
            // rather than left to be discovered.
            println!(
                "vayucell: no tor daemon was found on PATH, so nothing will \
                 be published to the Tor network; the cell stays local-only. \
                 Install tor to publish — it is an external dependency of \
                 this mode, and this program never dials out on its own."
            );
        }
        _ => {}
    }

    let onion_live = onion_plan.is_some() && daemon.is_some();

    println!(
        "vayucell: one governor, {} surface(s):",
        1 + usize::from(site.is_some()) + usize::from(vault.is_some()) + usize::from(onion_live)
    );
    println!("  panel  http://{panel_addr}/   is the battery safe");
    match site.as_ref() {
        Some(root) => println!("  site   http://{site_addr}/   {}", root.dir()),
        None => println!("  site   not served — pass --site-dir <DIR> to publish a folder"),
    }
    match vault.as_ref() {
        Some(root) => println!("  vault  http://{vault_addr}/   {}", root.dir()),
        None => println!("  vault  not served — pass --vault-dir <DIR> to store files"),
    }
    if onion_live {
        // The address is printed by the supervision thread the moment the
        // daemon publishes one — which can be minutes into a cold start.
        println!("  onion  through your system's tor daemon; UNVERIFIED until a request arrives from outside");
    }

    // Scoped threads, so every surface borrows the one cell rather than each
    // being handed a copy. `scope` also joins on the way out, which is what
    // makes "a listener stopped" observable rather than a process that quietly
    // serves less than it announced.
    let outcome = std::thread::scope(|scope| {
        // The supervisor, in its own thread, holding the ceiling and owning the
        // governor the other three read. Spawned first, so a device already in
        // trouble is known to be in trouble before anything binds a socket.
        scope.spawn(|| supervise(&governed, &supply, kind, a.assume_outage, &a.halt_record));

        let panel = scope.spawn(|| {
            // The governed level rather than a fresh one, so the panel agrees
            // with what the other surfaces are enforcing. A panel reporting
            // NORMAL beside a site answering 503 is two answers about one cell.
            let render = || {
                let (level, _) = governed.context(&RealHost);
                report::assemble(&RealHost, &supply, ceiling, level).render()
            };
            listen::serve(&panel_addr, &render)
        });

        let site_thread = site.as_ref().map(|root| {
            scope.spawn(|| {
                let availability = || governed.availability(&RealHost);
                listen::serve_site(&site_addr, root, &availability)
            })
        });

        let vault_thread = vault
            .as_ref()
            .zip(credentials.as_ref())
            .map(|(root, creds)| {
                scope.spawn(|| {
                    let context = || governed.context(&RealHost);
                    listen::serve_vault(&vault_addr, root, creds, &context, a.quota)
                })
            });

        // The onion supervisor, when publishing was asked for and the daemon
        // to do it with exists. It owns no listener; it owns a child process,
        // and it is the one thread here that must not outlive the governor —
        // which is why the exit paths below stop the daemon first. Its handle
        // is dropped unjoined, exactly like the governor's own thread: the
        // loop runs until the cell stops, and the scope waits for it here as
        // for any other.
        if let (Some(plan), Some(found)) = (onion_plan.as_ref(), daemon.as_ref()) {
            scope.spawn(|| {
                crate::onion::supervise(
                    plan.clone(),
                    || governed.context(&RealHost).0,
                    found.clone(),
                )
            });
        }

        // A panicking listener is reported as one. Joining a panicked thread
        // yields Err, and treating that as "finished" would let a surface
        // disappear while the summary above still claims it is being served.
        let mut failed = Vec::new();
        let mut collect = |name: &str, joined: std::thread::Result<Result<(), String>>| match joined
        {
            Ok(Ok(())) => failed.push(format!("the {name} listener stopped on its own")),
            Ok(Err(e)) => failed.push(format!("the {name} listener: {e}")),
            Err(_) => failed.push(format!("the {name} listener panicked")),
        };
        collect("panel", panel.join());
        if let Some(t) = site_thread {
            collect("site", t.join());
        }
        if let Some(t) = vault_thread {
            collect("vault", t.join());
        }
        failed
    });

    // Normal way out — a surface stopped. The daemon is stopped explicitly
    // because statics are not guaranteed to drop on return any more than on
    // exit, and an orphaned publisher is the one thing this mode must never
    // leave behind.
    crate::onion::begin_shutdown();

    for why in &outcome {
        eprintln!("vayucell: {why}");
    }
    report::EXIT_UNSAFE
}

/// The supervisor loop that runs beside the surfaces in `all`.
///
/// The same passes `run` makes, in a thread, against the same [`Governed`] the
/// surfaces are reading. It never returns while the device is well.
///
/// # Halting stops the process, exactly as it does in `run`
///
/// The governor escalates monotonically, so a latched `HALT` would already
/// refuse every request for the life of the process. Exiting anyway is the
/// difference between a hard stop and a state the operator can leave running
/// and forget. `run` says it plainly — *"no restart clears it"* — and a second
/// command that treats the same level as something to keep serving through
/// would make that sentence untrue of the product rather than of one command.
fn supervise(
    governed: &Governed,
    supply_dir: &str,
    kind: Option<Kind>,
    assume_outage: Option<core::time::Duration>,
    halt_record: &str,
) {
    let mut host = RealHost;
    let mut clock = RealClock::new();

    loop {
        let power = match assume_outage {
            Some(since) => Power::Battery(since.saturating_add(clock.elapsed())),
            None => Power::Mains,
        };

        // Rebound every pass, for the reason `run` records: the mechanism
        // borrows the host mutably, and the read-back this project turns on
        // needs the same node available to both the read and the write.
        let outcome = {
            let mut mech = kind
                .filter(|k| k.is_ceiling())
                .and_then(|k| SysfsCeiling::new(&mut host, supply_dir, k));
            let m = mech
                .as_mut()
                .map(|c| c as &mut dyn vayucell_core::governor::ChargeMechanism);
            governed.tick(&RealHost, m, power)
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

        // The ladder's last rung is an instruction, not a report. Printing
        // "shut down cleanly with charge remaining" and then continuing to tick
        // is the node announcing an action and not taking it — and the whole
        // claim behind ADR-0002 §8, a phone that is a server with an integrated
        // UPS, rests on the node actually stopping while there is charge left.
        // One that keeps running drains to zero and dies the ungraceful way the
        // ladder exists to prevent.
        //
        // No halt record is written. An outage is not a governor halt: mains
        // returning is the fix, and requiring somebody to inspect the phone
        // before it may serve again would be a hard stop earned by a power cut.
        if outage_shutdown(&outcome.shed) {
            println!("{STOPPING_FOR_OUTAGE}");
            // The same shape HALT uses here: this runs in a thread, so returning
            // would leave the surfaces serving on a node that just said it was
            // shutting down. Flushed first, because the last line is the one the
            // operator needs to still be there. The daemon is stopped before
            // exiting: `process::exit` runs no destructors, and a publisher
            // outliving its governor is the one orphan this mode must never
            // leave behind.
            crate::onion::begin_shutdown();
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());
            std::process::exit(report::EXIT_PROTECTED);
        }

        if outcome.level == Level::Halt {
            // Written before the message, so a power cut between the two leaves
            // the record rather than only the sentence claiming there is one.
            record_halt(halt_record, &halt_reason(outcome.transition.as_ref()));
            eprintln!(
                "vayucell: the governor has halted. This requires a person who \
                 has looked at the phone; no restart clears it."
            );
            eprintln!(
                "vayucell: clear it with `vayucell inspect --lies-flat` once you have looked."
            );
            // Same reason as the outage path above: no destructors run here,
            // so the daemon is stopped by hand before the process ends.
            crate::onion::begin_shutdown();
            // Flushed before exiting: stdout is block-buffered when it is not a
            // terminal, and the transition that explains the halt is the one
            // line the operator most needs to still be there afterwards.
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());
            std::process::exit(report::EXIT_UNSAFE);
        }

        clock.sleep(outcome.next_in);
    }
}

/// Whether this tick's rungs oblige the node to stop.
///
/// The ladder's last rung is an instruction, not a report. `run` and `all` both
/// printed *"shut down cleanly with charge remaining"* and went on ticking —
/// verified against the built binary, which had to be killed after forty-five
/// seconds. A node that announces the shutdown and keeps running drains to zero
/// and dies the ungraceful way ADR-0002 §8 exists to prevent, and the claim that
/// a governed phone is a server with an integrated UPS rests on it stopping
/// while there is charge left.
///
/// A free function so the decision is testable without a clock, a socket or a
/// forty-five-second wait. The behaviour lived inline in two loops in `main`,
/// where nothing could reach it and no mutation could turn a test red.
///
/// Asked of **the rungs entered this tick** rather than the current stage,
/// because that is the moment to act: [`Stage::ShuttingDown`] is terminal, so a
/// later tick reports no rungs at all and a check reading only those would never
/// fire again.
fn outage_shutdown(rungs: &[vayucell_core::shed::ShedTransition]) -> bool {
    rungs.iter().any(|r| r.stage == Stage::ShuttingDown)
}

/// What the node says as it stops for an outage.
///
/// Not a halt: no record is written, and starting again when mains returns is
/// the whole remedy. Requiring somebody to inspect the phone before it may serve
/// again would be a hard stop earned by a power cut, and it would make the halt
/// record mean two different things.
const STOPPING_FOR_OUTAGE: &str = "vayucell: stopping now, with charge remaining. Start it again \
     when mains is back — this is an outage, not a halt, and nothing needs to be inspected.";

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

        // The ladder's last rung is an instruction, not a report. Printing
        // "shut down cleanly with charge remaining" and then continuing to tick
        // is the node announcing an action and not taking it — and the whole
        // claim behind ADR-0002 §8, a phone that is a server with an integrated
        // UPS, rests on the node actually stopping while there is charge left.
        // One that keeps running drains to zero and dies the ungraceful way the
        // ladder exists to prevent.
        //
        // No halt record is written. An outage is not a governor halt: mains
        // returning is the fix, and requiring somebody to inspect the phone
        // before it may serve again would be a hard stop earned by a power cut.
        if outage_shutdown(&outcome.shed) {
            println!("{STOPPING_FOR_OUTAGE}");
            return report::EXIT_PROTECTED;
        }

        if outcome.level == Level::Halt {
            // The governor halts and this process stops with it. Continuing to
            // loop after a hard stop would make HALT a log line rather than a
            // state, which is the one thing it must never become.
            record_halt(&a.halt_record, &halt_reason(outcome.transition.as_ref()));
            eprintln!(
                "vayucell: the governor has halted. This requires a person who \
                 has looked at the phone; no restart clears it."
            );
            eprintln!(
                "vayucell: clear it with `vayucell inspect --lies-flat` once you have looked."
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

/// Says, out loud, that this command governs nothing.
///
/// `site` and `vault` consult the governor before every request, which is what
/// stops them serving a phone in trouble. Neither *runs* one: no charge ceiling
/// is written, nothing samples on a cadence, and a `HALT` reached while they are
/// running is forgotten as soon as the cell cools, because the per-request
/// governor carries no history.
///
/// That is the correct behaviour for a command somebody is trying out, and the
/// wrong thing to leave running for a year. The difference is invisible from
/// the outside — both commands look like they are serving happily — so it is
/// said rather than left to be discovered.
fn announce_ungoverned() {
    println!(
        "vayucell: this command consults the governor but does not run one, so no \n\
         \x20         charge ceiling is being held. To leave something running, use\n\
         \x20         `vayucell all`, which supervises the cell as well as serving it."
    );
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

#[cfg(test)]
mod tests {
    use super::outage_shutdown;
    use vayucell_core::battery::Percent;
    use vayucell_core::shed::{ShedReason, ShedTransition, Stage};

    fn rung(stage: Stage) -> ShedTransition {
        ShedTransition {
            stage,
            reason: ShedReason::ReserveReached {
                floor: Percent::clamped(10),
                measured: Percent::clamped(8),
            },
        }
    }

    #[test]
    fn the_ladders_last_rung_stops_the_node() {
        // It printed "shut down cleanly with charge remaining" and kept ticking.
        // The built binary had to be killed after forty-five seconds.
        assert!(outage_shutdown(&[rung(Stage::ShuttingDown)]));
    }

    #[test]
    fn a_rung_reached_on_the_way_down_does_not_stop_the_node() {
        // Every earlier rung is work to do while still running. Stopping at
        // Announced would turn a mains blip into an outage the node never
        // rode out.
        for stage in [
            Stage::Serving,
            Stage::Announced,
            Stage::Shed,
            Stage::Quiesced,
        ] {
            assert!(!outage_shutdown(&[rung(stage)]), "{stage:?}");
        }
    }

    #[test]
    fn the_last_rung_is_found_among_the_ones_walked_with_it() {
        // A late tick walks several rungs at once, and the terminal one arrives
        // in the same batch as the flush. Reading only the first would keep the
        // node running on a cell at the reserve.
        let walked = [
            rung(Stage::Announced),
            rung(Stage::Shed),
            rung(Stage::Quiesced),
            rung(Stage::ShuttingDown),
        ];
        assert!(outage_shutdown(&walked));
    }

    #[test]
    fn a_tick_that_walked_nothing_does_not_stop_the_node() {
        assert!(!outage_shutdown(&[]));
    }
}
