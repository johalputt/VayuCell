// SPDX-License-Identifier: Apache-2.0

//! Supervising the system tor daemon as one more surface of `vayucell all`.
//!
//! The contract lives in [`vayucell_core::onion`]; this file is the part that
//! touches the real machine, and it is written against two facts about this
//! project:
//!
//! - **The binary never dials.** Charter Article V.2 is a gate on this crate's
//!   source. Every packet that reaches the Tor network is sent by the daemon,
//!   which is somebody else's program doing what it is for; this process only
//!   writes a configuration file, starts the daemon, reads the address it
//!   publishes, and stops it when the governor says so.
//! - **What cannot be verified is never claimed.** Reading the published
//!   hostname proves the daemon came up — nothing more. Until a request has
//!   arrived from outside through the path, the word used out loud stays
//!   *unverified*, exactly as ADR-0003 §4 defines it.

use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use vayucell_core::governor::Level;
use vayucell_core::host::RealHost;
use vayucell_core::onion::{self, Plan};

/// How often the supervision loop looks at the world.
///
/// One second is far below every human-relevant timescale here — bootstrap
/// takes minutes, restarts wait seconds — and cheap enough that nobody will
/// be tempted to make it cleverer.
pub const POLL: Duration = Duration::from_secs(1);

/// How long a freshly started daemon has to publish an address.
///
/// Bootstrap on a first-ever run builds state from nothing and can take
/// minutes on a slow link; a restart over warm state takes seconds. Three
/// minutes covers the cold case without making a genuinely broken daemon
/// look merely slow for hours.
pub const PUBLISH_TIMEOUT: Duration = Duration::from_secs(180);

/// How long a daemon must stay up before its crash streak stops counting.
///
/// Without this, one bad night counts against every morning afterwards and
/// the restart delay sits at its cap forever — punishing a cell that
/// recovered as if it had not.
pub const STABLE_AFTER: Duration = Duration::from_secs(60);

/// Set when the process has decided to stop, so the supervision thread kills
/// the daemon and exits instead of restarting it behind a shutting-down cell.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// The supervised daemon, if one is running right now.
///
/// Owned here rather than by the thread so the halt path can stop the daemon
/// **before** `std::process::exit` — which runs no destructors — leaves an
/// orphan publishing a site nobody is governing any more.
static LIVE: Mutex<Option<Child>> = Mutex::new(None);

/// Marks the end of the process, and stops the daemon now rather than never.
///
/// Called from the paths that are about to `std::process::exit` — a halt, or
/// the outage ladder's last rung. An orphaned daemon would keep the onion
/// address alive and answering after the cell that governs it has stopped,
/// which is precisely the state ADR-0003 §5 forbids: ingress running while
/// the governor is no longer watching.
pub fn begin_shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    stop("the cell is stopping");
}

/// Looks for the system's tor daemon along `PATH`.
///
/// `PATH` is a parameter rather than read here so a test can hand over its
/// own directories. Existence is the whole check: whether the found program
/// actually runs, speaks Tor, and holds no surprises is exactly what this
/// project cannot verify from inside — which is why the daemon is named to
/// the operator as a dependency instead of being pretended away.
#[must_use]
pub fn find_daemon(path_var: Option<&str>) -> Option<String> {
    let path_var = path_var?;
    let wanted: &[&str] = if cfg!(windows) {
        &["tor.exe"]
    } else {
        &["tor"]
    };
    for dir in std::env::split_paths(path_var) {
        for name in wanted {
            let candidate = dir.join(name);
            // A directory called `tor` is not a daemon. Asking the filesystem
            // whether this entry is a file is the difference between finding
            // the program and finding somebody's folder.
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// The local port part of an address like `0.0.0.0:8081` or `[::1]:8082`.
///
/// The onion mapping targets the port, never the interface: the daemon
/// connects over loopback regardless of what the listener bound, so parsing
/// only what is needed is also the honest amount.
#[must_use]
pub fn port_of(addr: &str) -> Option<u16> {
    addr.parse::<SocketAddr>().ok().map(|a| a.port())
}

/// The plan for this cell: publish whichever surfaces were asked for.
///
/// The site goes out on the conventional web port and the vault on 8080;
/// the panel goes nowhere. It reports whether the battery in somebody's
/// house is safe, and publishing that to the world is a disclosure this
/// mode has no reason to make.
#[must_use]
pub fn build_plan(data_dir: &str, site_port: Option<u16>, vault_port: Option<u16>) -> Plan {
    use vayucell_core::onion::Mapping;

    let mut mappings = Vec::new();
    if let Some(port) = site_port {
        mappings.push(Mapping {
            virtual_port: 80,
            target_port: port,
        });
    }
    if let Some(port) = vault_port {
        mappings.push(Mapping {
            virtual_port: 8080,
            target_port: port,
        });
    }
    Plan {
        data_dir: data_dir.to_owned(),
        mappings,
    }
}

/// Writes the configuration into the key directory.
///
/// # Errors
///
/// Names whatever refused: a directory that could not be created or made
/// private, a configuration that could not be written.
pub fn prepare(plan: &Plan) -> Result<(), String> {
    std::fs::create_dir_all(&plan.data_dir)
        .map_err(|e| format!("{} could not be created: {e}", plan.data_dir))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(&plan.data_dir, perms)
            .map_err(|e| format!("{} could not be made private: {e}", plan.data_dir))?;
    }
    std::fs::write(plan.rc_path(), plan.torrc())
        .map_err(|e| format!("{} could not be written: {e}", plan.rc_path()))
}

/// Starts the daemon under the written configuration, logging beside it.
///
/// Output goes to the file [`Plan::log_path`] names, appended across
/// restarts — the last thing an operator needs when nothing outside can
/// reach them is for the reason to have been truncated away by the restart
/// they ran while investigating.
fn start(plan: &Plan, daemon: &str) -> Result<Child, String> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(plan.log_path())
        .map_err(|e| format!("{} could not be opened for the log: {e}", plan.log_path()))?;
    let err = log
        .try_clone()
        .map_err(|e| format!("the log could not be shared by the daemon: {e}"))?;

    Command::new(daemon)
        .arg("-f")
        .arg(plan.rc_path())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("{daemon} could not be started: {e}"))
}

/// How long to wait before attempt *n*, counting from the first failure.
///
/// Two, four, eight, sixteen, sixteen — the same ladder pushes use, because
/// the situation is the same shape: something out there is unhappy, hammering
/// it faster makes it worse, and backing off must never become giving up.
#[must_use]
pub fn next_wait(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(3);
    Duration::from_secs(2_u64.saturating_mul(1 << shift))
}

/// Kills the daemon, whoever asks, and says why out loud.
fn stop(reason: &str) {
    let mut live = LIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(child) = live.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
        println!("vayucell: the daemon was stopped: {reason}.");
    }
}

fn with_live<T>(f: impl FnOnce(&mut Option<Child>) -> T) -> T {
    let mut live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut live)
}

/// Runs until the process ends: keeps the daemon up, sheds it when the
/// governor says so, restarts it when it dies, and never lets it outlive the
/// cell.
///
/// One loop, one-second cadence, and every decision either delegated to the
/// contract ([`onion::should_run`]) or said out loud as it happens — a
/// supervisor that acts silently is indistinguishable from one that has
/// stopped acting.
pub fn supervise<F>(plan: Plan, level_of: F, daemon: String)
where
    F: Fn() -> Level + Send + Sync,
{
    println!(
        "vayucell: asking {daemon} to publish this cell; the first address can\n\
         \x20         take a few minutes. Its log goes to {}.",
        plan.log_path()
    );
    for line in onion::custody_lines(&plan.data_dir) {
        println!("vayucell: {line}");
    }

    let mut failures: u32 = 0;
    let mut retry_at: Option<Instant> = None;
    let mut started_at: Option<Instant> = None;
    let mut published = false;
    let mut stable_since: Option<Instant> = None;
    // A warning worth printing once — unreadable or nonsense where the
    // address should be — is not worth printing every second forever.
    let mut warned = false;

    loop {
        if SHUTTING_DOWN.load(Ordering::SeqCst) {
            stop("the cell is stopping");
            return;
        }

        // Exit noticed before anything else: a dead daemon changes every
        // branch below, and reporting a shed on top of a crash would tell
        // the operator two stories where one is true.
        let exited = with_live(|live| match live.as_mut() {
            Some(child) => child
                .try_wait()
                .ok()
                .flatten()
                .map(|status| status.to_string()),
            None => None,
        });

        if let Some(status) = exited {
            let was_published = published;
            published = false;
            started_at = None;
            stable_since = None;
            warned = false;
            failures += 1;
            retry_at = Some(Instant::now() + next_wait(failures));
            println!(
                "vayucell: the daemon exited ({status}); {}",
                if was_published {
                    "nothing outside can reach the cell until it comes back"
                } else {
                    "no address had been published yet"
                }
            );
        }

        let allowed = onion::should_run(level_of());

        if !allowed {
            if live_is_some() {
                stop(
                    "the governor shed high-thermal ingress; the onion goes \
                     first, before serving or storage",
                );
                published = false;
                started_at = None;
                stable_since = None;
                warned = false;
                // Shedding is not failing. The daemon did nothing wrong and
                // should come back at full speed when the cell cools, not
                // after a punishment delay for an outage it caused nobody.
                failures = 0;
                retry_at = None;
            }
        } else if live_is_none() && retry_at.map_or(true, |at| Instant::now() >= at) {
            match start(&plan, &daemon) {
                Ok(child) => {
                    with_live(|slot| *slot = Some(child));
                    started_at = Some(Instant::now());
                    published = false;
                    warned = false;
                    println!("vayucell: the daemon is coming up; waiting for it to publish");
                }
                Err(e) => {
                    failures += 1;
                    retry_at = Some(Instant::now() + next_wait(failures));
                    println!("vayucell: {e}");
                }
            }
        } else if live_is_some() && !published {
            match onion::read_hostname(&RealHost, &plan.data_dir) {
                Ok(hostname) => {
                    published = true;
                    stable_since = Some(Instant::now());
                    failures = 0;
                    retry_at = None;
                    println!(
                        "vayucell: onion  {}  — reachable wherever Tor works, and\n\
                         \x20         UNVERIFIED until a request has arrived from outside \
                         this device;\n\x20         that is all verification means here",
                        hostname.url()
                    );
                }
                Err(e) => {
                    // *Not yet* is the normal sound of bootstrap and is not
                    // printed at all. Everything else is said once, because
                    // a permission problem repeated every second is noise,
                    // and noise teaches operators to ignore this program.
                    if !matches!(e, onion::Unpublished::NotYet(_)) && !warned {
                        warned = true;
                        println!("vayucell: {e}");
                    }
                    let began = started_at.unwrap_or_else(Instant::now);
                    if Instant::now()
                        >= began
                            .checked_add(PUBLISH_TIMEOUT)
                            .unwrap_or_else(Instant::now)
                    {
                        println!(
                            "vayucell: no address after {}s; stopping the daemon and \
                             trying again",
                            PUBLISH_TIMEOUT.as_secs()
                        );
                        stop("publish did not complete");
                        failures += 1;
                        retry_at = Some(Instant::now() + next_wait(failures));
                        started_at = None;
                        warned = false;
                    }
                }
            }
        } else {
            // Published and running: the healthy stretch. Long enough awake
            // means the earlier crash stops counting against tonight's.
            if stable_since.is_some_and(|s| s.elapsed() >= STABLE_AFTER) {
                failures = 0;
            }
        }

        std::thread::sleep(POLL);
    }
}

fn live_is_some() -> bool {
    with_live(|live| live.is_some())
}

fn live_is_none() -> bool {
    !live_is_some()
}

#[cfg(test)]
mod tests {
    use super::{build_plan, find_daemon, next_wait, port_of};
    use std::time::Duration;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vayucell-onion-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        dir
    }

    #[test]
    fn the_daemon_is_found_on_a_path_that_carries_it() {
        let dir = temp_dir("found");
        let name = if cfg!(windows) { "tor.exe" } else { "tor" };
        std::fs::write(dir.join(name), b"").expect("fake daemon writes");
        let path = std::env::join_paths([&dir]).expect("path joins");
        let found = find_daemon(Some(path.to_str().expect("temp path is text")))
            .expect("the fake daemon is found");
        assert!(found.ends_with(name), "{found}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_named_like_the_daemon_is_not_the_daemon() {
        // Finding somebody's folder and trying to run it would produce an
        // error nobody could act on; checking is_file is the whole defence.
        let dir = temp_dir("isdir");
        let name = if cfg!(windows) { "tor.exe" } else { "tor" };
        std::fs::create_dir(dir.join(name)).expect("impostor dir creates");
        let path = std::env::join_paths([&dir]).expect("path joins");
        assert!(find_daemon(Some(path.to_str().expect("text"))).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_daemon_anywhere_answers_nothing_rather_than_guessing() {
        // An empty PATH stands in for "not installed". Returning a guessed
        // default here would send the spawn half hunting for a program the
        // machine does not have, with a message less useful than this
        // function staying silent and the caller naming what is missing.
        assert!(find_daemon(Some("")).is_none());
        assert!(find_daemon(None).is_none());
    }

    #[test]
    fn the_target_port_comes_out_of_both_address_shapes() {
        assert_eq!(port_of("0.0.0.0:8081"), Some(8081));
        assert_eq!(port_of("[::1]:8082"), Some(8082));
        assert_eq!(port_of("127.0.0.1"), None);
        assert_eq!(port_of("localhost:8081"), None);
        assert_eq!(port_of(""), None);
    }

    #[test]
    fn the_plan_publishes_what_was_asked_and_nothing_more() {
        // The panel is deliberately absent from every mapping: it reports on
        // the battery in somebody's home, and publishing that to the world
        // is a disclosure this mode has no reason to make.
        let both = build_plan("/d", Some(8081), Some(8082));
        assert_eq!(both.mappings.len(), 2, "{both:?}");
        assert_eq!(both.mappings[0].virtual_port, 80);
        assert_eq!(both.mappings[0].target_port, 8081);
        assert_eq!(both.mappings[1].virtual_port, 8080);
        assert_eq!(both.mappings[1].target_port, 8082);

        let site_only = build_plan("/d", Some(8081), None);
        assert_eq!(site_only.mappings.len(), 1, "{site_only:?}");

        let vault_only = build_plan("/d", None, Some(8082));
        assert_eq!(vault_only.mappings.len(), 1, "{vault_only:?}");
        assert_eq!(vault_only.mappings[0].virtual_port, 8080);
    }

    #[test]
    fn restart_delays_double_and_stop_at_sixteen_seconds() {
        // Doubling without a cap grows past any patience; a cap without
        // doubling hammers a struggling network. Both halves asserted,
        // because either can rot silently on its own.
        assert_eq!(next_wait(1), Duration::from_secs(2));
        assert_eq!(next_wait(2), Duration::from_secs(4));
        assert_eq!(next_wait(3), Duration::from_secs(8));
        assert_eq!(next_wait(4), Duration::from_secs(16));
        assert_eq!(next_wait(50), Duration::from_secs(16));
    }
}
