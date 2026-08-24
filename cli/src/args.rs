// SPDX-License-Identifier: Apache-2.0

//! Argument parsing, hand-rolled and kept separate so it can be tested.
//!
//! Thirty lines of `std` rather than a parser crate. A project whose headline
//! claim is that the core has no third-party runtime dependencies should not
//! acquire its first one in order to read `--ceiling`.
//!
//! Parsing lives in its own module because the interesting cases — a ceiling of
//! 200, a `--supply-dir` with no value after it — are exactly the ones nobody
//! exercises by running the binary, and every one of them has a way of turning
//! into a default that looks deliberate.

use core::time::Duration;

use vayucell_core::governor::Inspection;

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Read the device once, render the panel, exit with its verdict.
    Status,
    /// Run the supervisor loop.
    Run {
        /// Stop after this many ticks. `None` runs until the governor halts.
        ticks: Option<u32>,
    },
    /// Serve the panel on the local network.
    Serve,
    /// Serve a directory of files as a website.
    Site,
    /// Serve the vault: authenticated storage.
    Vault,
    /// Serve the panel, the site and the vault together, under one governor.
    All,
    /// Record that a person has looked at the phone, and clear a halt if they
    /// found it sound.
    Inspect,
    /// Print a device report to paste into an issue.
    Report,
    /// Enrol a device and print its secret once.
    Enrol,
    /// List the devices enrolled on this cell.
    Devices,
    /// Remove a device from the credential store.
    Revoke,
    /// Print usage.
    Help,
    /// Print the version.
    Version,
}

impl Command {
    /// Whether this command puts the device's storage or content in front of
    /// somebody other than the person holding it.
    ///
    /// # Why this is a predicate and not a line in each command
    ///
    /// `run` and `all` refused to start while a halt record stood. `site` and
    /// `vault` did not — they started, served `200`, and said nothing about it.
    /// So a phone whose cell crossed a hard threshold went on serving a website
    /// and **accepting uploads** as soon as somebody restarted it into a
    /// different subcommand, while the binary's own halt message says *"no
    /// restart clears it"*.
    ///
    /// Nothing was wrong with the check. It was written once per command, and
    /// the two commands added later did not get it. Asking the enum settles it
    /// for every command that exists and forces the question for every command
    /// added — a new variant does not compile until this match answers for it.
    ///
    /// [`Command::Serve`] is **deliberately false**, and it is the only
    /// interesting entry. It serves the panel, which is what a person needs to
    /// read at exactly the moment the device has halted; taking it away would
    /// be the shed ladder's mistake, made at the terminal. It already reports
    /// the halt — the panel floors its level at the standing — so it tells the
    /// truth rather than serving through it.
    #[must_use]
    pub const fn serves_traffic(&self) -> bool {
        match self {
            Self::Site | Self::Vault | Self::All | Self::Run { .. } => true,
            Self::Serve
            | Self::Status
            | Self::Inspect
            | Self::Report
            | Self::Enrol
            | Self::Devices
            | Self::Revoke
            | Self::Help
            | Self::Version => false,
        }
    }
}

/// A parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// What to do.
    pub command: Command,
    /// Where the power-supply directory is.
    pub supply_dir: String,
    /// The charge ceiling to hold, in percent.
    pub ceiling: u8,
    /// How long the outage clock has been running, for testing the shed ladder
    /// without unplugging anything.
    pub assume_outage: Option<Duration>,
    /// What address `serve` binds.
    pub bind: String,
    /// The directory `site` publishes. `None` means none was given.
    pub site_dir: Option<String>,
    /// The directory `vault` keeps files in. `None` means none was given.
    pub vault_dir: Option<String>,
    /// The hidden-service directory `all` publishes through the system's
    /// tor daemon. `None` means the cell publishes nothing.
    pub onion_dir: Option<String>,
    /// Where the credential store lives.
    pub store: String,
    /// The device name `enrol` adds. `None` means none was given.
    pub device: Option<String>,
    /// How many bytes the vault may hold.
    pub quota: u64,
    /// Where the halt record lives.
    pub halt_record: String,
    /// What `inspect` was told the person saw. `None` means neither flag.
    pub inspection: Option<Inspection>,
}

/// Why an invocation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgError(pub String);

impl core::fmt::Display for ArgError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The default power-supply directory.
pub const DEFAULT_SUPPLY: &str = vayucell_core::sysfs::SUPPLY;

/// How much the vault may hold unless told otherwise: one gibibyte.
///
/// A number rather than "the free space on the device". A vault that grows until
/// the filesystem is full takes the phone down with it, and the governor cannot
/// help with a disk.
pub const DEFAULT_QUOTA: u64 = 1024 * 1024 * 1024;

/// Where the credential store lives unless told otherwise.
///
/// Beside the binary's own directory rather than anywhere shared. Falls back to
/// a relative path when `HOME` is unset, so the value is always something the
/// operator can see in `--help` rather than an empty string.
#[must_use]
pub fn default_store() -> String {
    std::env::var("HOME").map_or_else(
        |_| ".vayucell/devices".to_owned(),
        |home| format!("{home}/.vayucell/devices"),
    )
}

/// The default ceiling. ADR-0002: 60% is the recommended long-term hold.
pub const DEFAULT_CEILING: u8 = 60;

/// What `serve` binds unless told otherwise.
///
/// Loopback. ADR-0003 §3 makes local-only the default because publishing is an
/// irreversible disclosure, and binding every interface by default would make a
/// weaker version of that decision on the operator's behalf — reachable by
/// anything on their network, including whatever else is on the guest Wi-Fi.
pub const DEFAULT_BIND: &str = "127.0.0.1:8080";

/// The three addresses `all` serves on, counted from `bind`.
///
/// Panel, then site, then vault, on consecutive ports. Consecutive rather than
/// three separate flags because three flags is three chances for a beginner to
/// bind something to a port they did not mean, and the summary this prints is
/// easier to read back than three arguments are to get right.
///
/// **Separate ports, not separate paths.** The panel reports whether the battery
/// is safe; the site serves whatever the operator put in a folder. A page on one
/// must never be able to read the other, and the browser rule that guarantees
/// that is the same-origin policy — which counts a differing port as a different
/// origin and a differing path as the same one.
///
/// # Errors
///
/// Returns the reason, phrased for somebody reading a terminal.
pub fn adjacent_ports(bind: &str) -> Result<[String; 3], ArgError> {
    let addr: std::net::SocketAddr = bind.parse().map_err(|_| {
        // `serve` accepts a hostname because it binds one thing. Counting to the
        // next port needs a number, and resolving a name here to find one would
        // mean the address bound depended on DNS at the moment of parsing.
        ArgError(format!(
            "all counts three ports from --bind, so it needs a numeric address \
             and port like 0.0.0.0:8080, not {bind:?}"
        ))
    })?;

    let base = addr.port();
    if base == 0 {
        // Port 0 means "whichever port the kernel has spare". There is no such
        // thing as the next one along.
        return Err(ArgError(
            "all cannot count from port 0, which asks the system to choose; name \
             the port you want, like 0.0.0.0:8080"
                .to_owned(),
        ));
    }
    // Checked, not wrapped. 65535 + 1 is not port 0, and port 0 would hand the
    // operator a listener on a port nobody chose.
    base.checked_add(2).ok_or_else(|| {
        ArgError(format!(
            "all needs three consecutive ports and {base} leaves room for fewer; \
             pick a port at or below 65533"
        ))
    })?;

    let at = |port: u16| {
        let mut a = addr;
        a.set_port(port);
        a.to_string()
    };
    Ok([at(base), at(base + 1), at(base + 2)])
}

/// What `--help` prints.
pub const USAGE: &str = "\
vayucell — report what a device can be trusted to do, and govern its cell

USAGE:
    vayucell <COMMAND> [OPTIONS]

COMMANDS:
    status              Read the device once, print the safety panel, and exit
                        with a code that reflects it
    run                 Run the supervisor loop until the governor halts
    serve               Serve the safety panel over HTTP, local only
    site                Serve a directory of files as a website, under the
                        governor: it stops serving when the cell is in trouble
    vault               Serve authenticated storage: GET reads a file, PUT
                        stores one. Every request is checked against the
                        enrolled devices and against the governor
    all                 Serve the panel, the site and the vault together, on
                        three consecutive ports, under one governor and one
                        outage ladder. This is the one to run on a phone.
                        With --onion-dir it also publishes through your
                        system's tor daemon
    report              Print a device report to paste into an issue. Nothing
                        is sent anywhere; it says what it contains and what it
                        leaves out, so you can check before pasting
    enrol               Add a device to the credential store and print its
                        secret. It is shown once and never again
    devices             List the devices enrolled here. Never prints a secret
    revoke              Remove a device, so its credential stops working
    help                Print this
    version             Print the version

OPTIONS:
    --supply-dir <DIR>  Power-supply directory
                        [default: /sys/class/power_supply/battery]
    --ceiling <PCT>     Charge ceiling to hold, 0-100 [default: 60]
    --ticks <N>         Stop `run` after N passes instead of running on
    --assume-outage <S> Treat mains as lost this many seconds ago, so the shed
                        ladder can be exercised without unplugging anything
    --bind <ADDR>       Address for `serve`, `site` and `vault` [default:
                        127.0.0.1:8080]. The default is loopback: reaching the
                        rest of your network is something you type, not
                        something you get. `all` counts three ports from it —
                        panel, then site, then vault — so it needs a numeric
                        address rather than a name
    --dir <DIR>         The directory `site` or `vault` uses. Required by
                        both. Hidden names are never served, no directory
                        listing is ever generated, and a symbolic link pointing
                        outside this directory is refused
    --site-dir <DIR>    The directory `all` publishes as a website. Omit it and
                        no site is served, which `all` says out loud
    --vault-dir <DIR>   The directory `all` keeps files in. Omit it and no
                        vault is served, which `all` says out loud
    --onion-dir <DIR>   The hidden-service directory `all` publishes through
                        your system's tor daemon: the site appears on port 80
                        of an .onion address and the vault on 8080, reachable
                        from anywhere Tor works. tor must be installed — it
                        is a dependency of this mode, not a part of this
                        program, and the panel is never published. `all`
                        only: publishing needs the governor running beside it
    --store <FILE>      Credential store for `vault` and `enrol`
                        [default: ~/.vayucell/devices]. It holds secrets in the
                        clear, so it must not be readable by anyone else and
                        both commands refuse it if it is
    --device <NAME>     The device `enrol` or `revoke` names. No spaces
    --quota <BYTES>     How much the vault may hold [default: 1073741824].
                        What it already holds is measured before every upload,
                        and an upload is refused if the directory cannot be
                        measured at all

EXIT CODES:
    0   the panel reads PROTECTED — every row was checked and held
    1   the panel reads NOT FULLY VERIFIED — something could not be checked
    2   the panel reads UNSAFE — something was checked and does not hold
    64  the arguments were not usable

    A non-zero exit is not a crash. It is the verdict, in the one form a
    monitor can read without parsing prose.

This binary has never been run against a phone. Every device-facing path in
this project is exercised through a fake host.
";

/// Parses an argument list, excluding the program name.
///
/// # Errors
///
/// Returns the reason, phrased for somebody reading a terminal.
pub fn parse(argv: &[String]) -> Result<Args, ArgError> {
    let mut command: Option<Command> = None;
    let mut supply_dir = DEFAULT_SUPPLY.to_owned();
    let mut ceiling = DEFAULT_CEILING;
    let mut ticks: Option<u32> = None;
    let mut assume_outage: Option<Duration> = None;
    let mut bind = DEFAULT_BIND.to_owned();
    let mut dir: Option<String> = None;
    let mut site_only: Option<String> = None;
    let mut vault_only: Option<String> = None;
    let mut onion_dir: Option<String> = None;
    let mut store = default_store();
    let mut device: Option<String> = None;
    let mut quota: u64 = DEFAULT_QUOTA;
    let mut halt_record = crate::halted::default_path();
    let mut inspection: Option<Inspection> = None;

    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        match arg {
            "status" => set_command(&mut command, Command::Status, arg)?,
            "run" => set_command(&mut command, Command::Run { ticks: None }, arg)?,
            "serve" => set_command(&mut command, Command::Serve, arg)?,
            "site" => set_command(&mut command, Command::Site, arg)?,
            "vault" => set_command(&mut command, Command::Vault, arg)?,
            "all" => set_command(&mut command, Command::All, arg)?,
            "inspect" => set_command(&mut command, Command::Inspect, arg)?,
            "report" => set_command(&mut command, Command::Report, arg)?,
            "enrol" | "enroll" => set_command(&mut command, Command::Enrol, arg)?,
            "devices" => set_command(&mut command, Command::Devices, arg)?,
            "revoke" => set_command(&mut command, Command::Revoke, arg)?,
            "help" | "--help" | "-h" => set_command(&mut command, Command::Help, arg)?,
            "version" | "--version" | "-V" => set_command(&mut command, Command::Version, arg)?,
            "--supply-dir" => {
                supply_dir = value_after(argv, &mut i, "--supply-dir")?;
            }
            "--bind" => {
                bind = value_after(argv, &mut i, "--bind")?;
            }
            "--dir" => {
                dir = Some(value_after(argv, &mut i, "--dir")?);
            }
            "--site-dir" => {
                site_only = Some(value_after(argv, &mut i, "--site-dir")?);
            }
            "--vault-dir" => {
                vault_only = Some(value_after(argv, &mut i, "--vault-dir")?);
            }
            "--onion-dir" => {
                onion_dir = Some(value_after(argv, &mut i, "--onion-dir")?);
            }
            "--store" => {
                store = value_after(argv, &mut i, "--store")?;
            }
            "--halt-record" => {
                halt_record = value_after(argv, &mut i, "--halt-record")?;
            }
            // Two flags rather than `--inspection <WORD>`, because the two
            // answers are not interchangeable values of one thing: one resumes
            // a phone and the other retires it, and a typo between two words
            // should not be able to pick the wrong one.
            "--lies-flat" => set_inspection(&mut inspection, Inspection::LiesFlat)?,
            "--deformed" => set_inspection(&mut inspection, Inspection::Deformed)?,
            "--device" => {
                device = Some(value_after(argv, &mut i, "--device")?);
            }
            "--quota" => {
                let raw = value_after(argv, &mut i, "--quota")?;
                quota = raw
                    .parse::<u64>()
                    .map_err(|_| ArgError(format!("--quota is a number of bytes, not {raw:?}")))?;
            }
            "--ceiling" => {
                let raw = value_after(argv, &mut i, "--ceiling")?;
                ceiling = raw
                    .parse::<u8>()
                    .ok()
                    .filter(|c| *c <= 100)
                    .ok_or_else(|| {
                        // Not clamped. A ceiling of 200 is somebody who meant
                        // something, and silently holding 100 would be this program
                        // deciding what, on the one setting that governs a lithium
                        // cell in their home.
                        ArgError(format!(
                            "--ceiling must be a whole number from 0 to 100, not {raw:?}"
                        ))
                    })?;
            }
            "--ticks" => {
                let raw = value_after(argv, &mut i, "--ticks")?;
                ticks = Some(raw.parse::<u32>().map_err(|_| {
                    ArgError(format!("--ticks must be a whole number, not {raw:?}"))
                })?);
            }
            "--assume-outage" => {
                let raw = value_after(argv, &mut i, "--assume-outage")?;
                let secs = raw.parse::<u64>().map_err(|_| {
                    ArgError(format!(
                        "--assume-outage is a number of seconds, not {raw:?}"
                    ))
                })?;
                assume_outage = Some(Duration::from_secs(secs));
            }
            other => {
                return Err(ArgError(format!(
                    "unrecognised argument {other:?}; run `vayucell help`"
                )))
            }
        }
        i += 1;
    }

    let command = match command
        .ok_or_else(|| ArgError("no command given; try `vayucell status`".to_owned()))?
    {
        Command::Run { .. } => Command::Run { ticks },
        other => other,
    };

    // `--dir` names whichever directory the command in hand is about; the
    // specific flags exist because `all` is about two of them at once, and one
    // flag cannot mean two folders. The specific one wins where both are given,
    // rather than being silently ignored under a general one.
    let site_dir = site_only.or_else(|| dir.clone());
    let vault_dir = vault_only.or(dir);

    // Refused here rather than defaulted to the working directory. A `site`
    // with no --dir that quietly published whatever folder the operator happened
    // to be standing in is the single worst thing this command could do.
    if command == Command::Vault && vault_dir.is_none() {
        return Err(ArgError(
            "vault needs --dir <DIR>, the folder to keep files in; there is no \
             default, because a default would store somebody's files wherever you \
             happened to be standing"
                .to_owned(),
        ));
    }
    if command == Command::Revoke && device.is_none() {
        return Err(ArgError(
            "revoke needs --device <NAME>; run `vayucell devices` to see which \
             names are enrolled"
                .to_owned(),
        ));
    }
    if command == Command::Enrol && device.is_none() {
        return Err(ArgError(
            "enrol needs --device <NAME>, something you will recognise later when \
             you come to revoke it"
                .to_owned(),
        ));
    }
    if command == Command::Site && site_dir.is_none() {
        return Err(ArgError(
            "site needs --dir <DIR>, the folder to publish; there is no default, \
             because a default would publish whatever directory you were standing in"
                .to_owned(),
        ));
    }

    if command == Command::All && site_dir.is_none() && vault_dir.is_none() {
        return Err(ArgError(
            "all needs --site-dir <DIR>, --vault-dir <DIR>, or both; with neither \
             it would be `vayucell serve` under a longer name"
                .to_owned(),
        ));
    }

    // Publishing through Tor is high-thermal load the governor sheds first
    // (ADR-0003 §5), and only `all` runs a governor that can do that. `site`
    // and `vault` consult one per request but none *runs* one — so accepting
    // the flag there would publish heat the operator was told is governed,
    // under a command where nothing enforces it.
    if onion_dir.is_some() && command != Command::All {
        return Err(ArgError(
            "only `all` accepts --onion-dir: publishing needs the supervisor \
             running beside it, because onion ingress is sustained load the \
             governor sheds first and `site` or `vault` run no governor at all"
                .to_owned(),
        ));
    }

    if command == Command::Inspect && inspection.is_none() {
        return Err(ArgError(
            "inspect needs --lies-flat or --deformed. Put the phone face-down on \
             a flat table first: --lies-flat if it lies flat and no edge is \
             lifting, --deformed if it rocks or any edge is. There is no default, \
             because the default would be this program deciding what you saw"
                .to_owned(),
        ));
    }

    Ok(Args {
        command,
        supply_dir,
        ceiling,
        assume_outage,
        bind,
        vault_dir,
        site_dir,
        onion_dir,
        store,
        device,
        quota,
        halt_record,
        inspection,
    })
}

fn set_inspection(slot: &mut Option<Inspection>, seen: Inspection) -> Result<(), ArgError> {
    match *slot {
        // Refused rather than last-one-wins. Somebody who typed both does not
        // know what they are asking for, and one of the two answers ends with a
        // phone going in the bin.
        Some(already) if already != seen => Err(ArgError(
            "--lies-flat and --deformed say opposite things; pass whichever one \
             you actually saw"
                .to_owned(),
        )),
        _ => {
            *slot = Some(seen);
            Ok(())
        }
    }
}

fn set_command(slot: &mut Option<Command>, cmd: Command, name: &str) -> Result<(), ArgError> {
    if slot.is_some() {
        return Err(ArgError(format!(
            "only one command at a time; {name:?} came after another"
        )));
    }
    *slot = Some(cmd);
    Ok(())
}

/// The value following a flag.
///
/// The failure this exists for: `--supply-dir` as the last argument. Treating a
/// missing value as "use the default" would run the governor against
/// `/sys/class/power_supply/battery` on a machine where the operator had just
/// said, in the same breath, that it is somewhere else.
fn value_after(argv: &[String], i: &mut usize, flag: &str) -> Result<String, ArgError> {
    *i += 1;
    argv.get(*i)
        .cloned()
        .filter(|v| !v.starts_with("--"))
        .ok_or_else(|| ArgError(format!("{flag} needs a value after it")))
}

#[cfg(test)]
mod tests {
    use super::{parse, ArgError, Args, Command, DEFAULT_CEILING, DEFAULT_SUPPLY};
    use core::time::Duration;

    use vayucell_core::governor::Inspection;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn a_bare_command_takes_the_documented_defaults() {
        let a = parse(&argv(&["status"])).expect("status parses");
        assert_eq!(
            a,
            Args {
                command: Command::Status,
                supply_dir: DEFAULT_SUPPLY.to_owned(),
                ceiling: DEFAULT_CEILING,
                assume_outage: None,
                bind: super::DEFAULT_BIND.to_owned(),
                site_dir: None,
                vault_dir: None,
                onion_dir: None,
                store: super::default_store(),
                device: None,
                quota: super::DEFAULT_QUOTA,
                halt_record: crate::halted::default_path(),
                inspection: None,
            }
        );
        assert_eq!(DEFAULT_CEILING, 60, "ADR-0002's recommended long-term hold");
    }

    #[test]
    fn a_ceiling_outside_the_range_is_refused_rather_than_clamped() {
        // The single most consequential argument this binary takes: it governs
        // how a lithium cell in somebody's home is charged. Clamping 200 to 100
        // would be this program deciding what they meant, and 100 is the value
        // that holds no ceiling at all — the silent outcome would be the unsafe
        // one.
        for bad in ["101", "200", "-5", "sixty", "", "60.5"] {
            let e = parse(&argv(&["run", "--ceiling", bad]))
                .expect_err(&format!("--ceiling {bad} must be refused"));
            assert!(e.to_string().contains("0 to 100"), "for {bad}: {e}");
        }
        assert_eq!(parse(&argv(&["run", "--ceiling", "0"])).unwrap().ceiling, 0);
        assert_eq!(
            parse(&argv(&["run", "--ceiling", "100"])).unwrap().ceiling,
            100
        );
    }

    #[test]
    fn a_flag_with_no_value_is_refused_rather_than_falling_back_to_the_default() {
        // `--supply-dir` as the last argument. Treating the missing value as
        // "use the default" would point the governor at the standard path on a
        // machine where the operator had just said, in the same breath, that it
        // is somewhere else.
        let e = parse(&argv(&["status", "--supply-dir"])).expect_err("must be refused");
        assert_eq!(
            e,
            ArgError("--supply-dir needs a value after it".to_owned())
        );

        // And a following flag is not a value either.
        let e = parse(&argv(&["status", "--supply-dir", "--ceiling", "60"]))
            .expect_err("a flag is not a value");
        assert!(e.to_string().contains("needs a value"));
    }

    #[test]
    fn two_commands_are_refused_rather_than_last_one_winning() {
        // `vayucell status run` silently running the loop is the kind of thing
        // somebody discovers after leaving it on overnight.
        let e = parse(&argv(&["status", "run"])).expect_err("must be refused");
        assert!(e.to_string().contains("only one command"));
    }

    #[test]
    fn no_command_is_refused_rather_than_defaulting_to_one() {
        let e = parse(&argv(&[])).expect_err("must be refused");
        assert!(e.to_string().contains("no command given"));
        let e = parse(&argv(&["--ceiling", "60"])).expect_err("options alone are not a command");
        assert!(e.to_string().contains("no command given"));
    }

    #[test]
    fn an_unrecognised_argument_is_named_rather_than_ignored() {
        let e = parse(&argv(&["status", "--force"])).expect_err("must be refused");
        assert!(e.to_string().contains("--force"), "{e}");
    }

    #[test]
    fn ticks_attaches_to_run_wherever_it_appears() {
        assert_eq!(
            parse(&argv(&["run", "--ticks", "5"])).unwrap().command,
            Command::Run { ticks: Some(5) }
        );
        assert_eq!(
            parse(&argv(&["--ticks", "5", "run"])).unwrap().command,
            Command::Run { ticks: Some(5) }
        );
        assert_eq!(
            parse(&argv(&["run"])).unwrap().command,
            Command::Run { ticks: None }
        );
    }

    #[test]
    fn an_outage_can_be_assumed_so_the_ladder_is_reachable_without_unplugging() {
        let a = parse(&argv(&["run", "--assume-outage", "200"])).unwrap();
        assert_eq!(a.assume_outage, Some(Duration::from_secs(200)));
        assert_eq!(parse(&argv(&["run"])).unwrap().assume_outage, None);
    }

    #[test]
    fn the_usage_text_states_what_every_exit_code_means() {
        // A monitor reads the exit code, and a person reads this to know which
        // code to alert on. A usage text that documented only 0 would make every
        // other outcome look like a crash.
        for expected in [
            "0   the panel reads PROTECTED",
            "1   the panel reads NOT FULLY VERIFIED",
            "2   the panel reads UNSAFE",
            "64  the arguments were not usable",
            "not a crash",
        ] {
            assert!(
                super::USAGE.contains(expected),
                "usage must say: {expected}"
            );
        }
    }

    #[test]
    fn all_counts_three_consecutive_ports_from_the_bind_address() {
        let [panel, site, vault] = super::adjacent_ports("0.0.0.0:8080").expect("numeric");
        assert_eq!(panel, "0.0.0.0:8080");
        assert_eq!(site, "0.0.0.0:8081");
        assert_eq!(vault, "0.0.0.0:8082");
    }

    #[test]
    fn the_three_ports_are_distinct_because_the_same_origin_policy_counts_ports() {
        // Not decoration. The panel says whether the battery is safe and the
        // site serves whatever somebody put in a folder; a shared origin would
        // let one read the other.
        let ports = super::adjacent_ports("127.0.0.1:9000").expect("numeric");
        let unique: std::collections::BTreeSet<&String> = ports.iter().collect();
        assert_eq!(unique.len(), 3, "{ports:?}");
    }

    #[test]
    fn an_address_with_no_countable_port_is_refused_rather_than_guessed() {
        // `serve` takes a hostname happily because it binds one thing. Counting
        // to the next port needs a number, and resolving a name to find one
        // would make the bound address depend on DNS at parse time.
        for bad in ["localhost:8080", "0.0.0.0", "8080", ""] {
            let e = super::adjacent_ports(bad).expect_err("{bad} was accepted");
            assert!(e.0.contains("0.0.0.0:8080"), "{bad:?}: {}", e.0);
        }
    }

    #[test]
    fn port_zero_is_refused_because_there_is_no_next_one_along() {
        // Port 0 asks the system to pick. "The one after whichever it picked" is
        // not a thing, and binding two more would hand the operator surfaces on
        // ports nobody chose.
        let e = super::adjacent_ports("0.0.0.0:0").expect_err("port 0 is not countable");
        assert!(e.0.contains("choose"), "{}", e.0);
    }

    #[test]
    fn a_port_too_near_the_top_is_refused_rather_than_wrapped() {
        // 65535 + 1 is not port 0. Wrapping would bind the vault to whatever the
        // system had spare, which is the one thing the operator cannot check.
        assert!(super::adjacent_ports("0.0.0.0:65533").is_ok());
        for top in ["0.0.0.0:65534", "0.0.0.0:65535"] {
            let e = super::adjacent_ports(top).expect_err("{top} left no room");
            assert!(e.0.contains("65533"), "{top}: {}", e.0);
        }
    }

    #[test]
    fn an_ipv6_bind_keeps_its_brackets_so_the_listener_can_parse_it_back() {
        let [panel, _, vault] = super::adjacent_ports("[::1]:8080").expect("numeric");
        assert_eq!(panel, "[::1]:8080");
        assert_eq!(vault, "[::1]:8082");
    }

    #[test]
    fn all_takes_the_two_directories_separately_because_it_serves_both() {
        let a = parse(&argv(&[
            "all",
            "--site-dir",
            "/srv/site",
            "--vault-dir",
            "/srv/files",
        ]))
        .expect("parses");
        assert_eq!(a.command, super::Command::All);
        assert_eq!(a.site_dir.as_deref(), Some("/srv/site"));
        assert_eq!(a.vault_dir.as_deref(), Some("/srv/files"));
    }

    #[test]
    fn all_may_serve_only_one_of_the_two() {
        let a = parse(&argv(&["all", "--site-dir", "/srv/site"])).expect("parses");
        assert_eq!(a.site_dir.as_deref(), Some("/srv/site"));
        assert_eq!(a.vault_dir, None);

        let b = parse(&argv(&["all", "--vault-dir", "/srv/files"])).expect("parses");
        assert_eq!(b.site_dir, None);
        assert_eq!(b.vault_dir.as_deref(), Some("/srv/files"));
    }

    #[test]
    fn all_with_neither_directory_is_refused_rather_than_serving_a_lone_panel() {
        // With neither it is `serve` under a longer name, and somebody who typed
        // `all` expecting their files to be reachable should be told, not left
        // to discover it.
        let e = parse(&argv(&["all"])).expect_err("all needs something to serve");
        assert!(e.0.contains("--site-dir"), "{}", e.0);
        assert!(e.0.contains("--vault-dir"), "{}", e.0);
    }

    #[test]
    fn the_specific_directory_flag_wins_over_the_general_one() {
        // Both given is somebody being explicit about one of them. Letting the
        // general flag win would ignore the more specific instruction silently.
        let a = parse(&argv(&["all", "--dir", "/both", "--vault-dir", "/files"])).expect("parses");
        assert_eq!(a.site_dir.as_deref(), Some("/both"));
        assert_eq!(a.vault_dir.as_deref(), Some("/files"));
    }

    #[test]
    fn vault_still_reads_its_directory_from_the_general_flag() {
        // The commands that serve one surface keep taking --dir, so nothing an
        // operator already types stops working.
        let a = parse(&argv(&["vault", "--dir", "/srv/files"])).expect("parses");
        assert_eq!(a.vault_dir.as_deref(), Some("/srv/files"));
    }

    #[test]
    fn inspect_needs_to_be_told_what_the_person_saw() {
        // No default, because the default would be this program deciding what
        // somebody saw when they looked at their phone.
        let e = parse(&argv(&["inspect"])).expect_err("inspect needs an observation");
        assert!(e.0.contains("--lies-flat"), "{}", e.0);
        assert!(e.0.contains("--deformed"), "{}", e.0);
        assert!(e.0.contains("flat table"), "{}", e.0);
    }

    #[test]
    fn the_two_observations_are_carried_separately() {
        let flat = parse(&argv(&["inspect", "--lies-flat"])).expect("parses");
        assert_eq!(flat.command, Command::Inspect);
        assert_eq!(flat.inspection, Some(Inspection::LiesFlat));

        let bad = parse(&argv(&["inspect", "--deformed"])).expect("parses");
        assert_eq!(bad.inspection, Some(Inspection::Deformed));
    }

    #[test]
    fn contradicting_observations_are_refused_rather_than_last_one_winning() {
        // One of these two answers ends with a phone going to hazardous waste.
        // Somebody who typed both does not know which they mean, and silently
        // taking the last is this program choosing on their behalf.
        let e = parse(&argv(&["inspect", "--lies-flat", "--deformed"]))
            .expect_err("both cannot be true");
        assert!(e.0.contains("opposite"), "{}", e.0);

        // The same flag twice is somebody being emphatic, not contradictory.
        let ok = parse(&argv(&["inspect", "--lies-flat", "--lies-flat"])).expect("parses");
        assert_eq!(ok.inspection, Some(Inspection::LiesFlat));
    }

    #[test]
    fn every_command_that_serves_traffic_is_named_as_one() {
        // The defect this predicate replaced: `run` and `all` each carried their
        // own halt check and the two commands added later did not get one, so a
        // halted phone served a website and accepted uploads as soon as somebody
        // restarted it into a different subcommand.
        //
        // Asserted by listing rather than by calling the function on itself,
        // which would pass whatever the function said.
        for c in [
            Command::Site,
            Command::Vault,
            Command::All,
            Command::Run { ticks: None },
        ] {
            assert!(c.serves_traffic(), "{c:?} serves traffic and is not marked");
        }
    }

    #[test]
    fn the_panel_is_the_one_surface_a_halt_does_not_take_away() {
        // Deliberate, and the only interesting entry. `serve` renders the panel,
        // which is what a person needs to read at exactly the moment the device
        // has halted — and it already reports the halt rather than serving
        // through it, because the panel floors its level at the standing.
        //
        // Taking it away would be the shed ladder's mistake made at the terminal.
        assert!(!Command::Serve.serves_traffic());
    }

    #[test]
    fn nothing_that_only_reads_or_prints_is_gated_by_a_halt() {
        // A halted phone must still be able to tell somebody what is wrong with
        // it, and must still let them record that they looked.
        for c in [
            Command::Status,
            Command::Report,
            Command::Inspect,
            Command::Devices,
            Command::Enrol,
            Command::Revoke,
            Command::Help,
            Command::Version,
        ] {
            assert!(!c.serves_traffic(), "{c:?} is gated and should not be");
        }
    }

    #[test]
    fn the_halt_record_has_a_default_and_can_be_moved() {
        let d = parse(&argv(&["status"])).expect("parses");
        assert!(d.halt_record.ends_with("halted"), "{}", d.halt_record);

        let moved = parse(&argv(&["status", "--halt-record", "/tmp/h"])).expect("parses");
        assert_eq!(moved.halt_record, "/tmp/h");
    }

    #[test]
    fn the_usage_text_says_this_has_never_run_on_a_phone() {
        // Charter Article IV reaches the one screen somebody sees before they
        // decide to trust this with a device.
        assert!(super::USAGE.contains("never been run against a phone"));
    }

    #[test]
    fn site_without_a_directory_is_refused_rather_than_defaulted() {
        // The worst thing this command could do is publish whatever folder the
        // operator happened to be standing in, so there is no default and the
        // refusal says why.
        let e = parse(&argv(&["site"])).expect_err("site needs --dir");
        assert!(e.0.contains("--dir"), "{}", e.0);
        assert!(e.0.contains("standing in"), "{}", e.0);
    }

    #[test]
    fn site_takes_a_directory_and_the_shared_bind_flag() {
        let a = parse(&argv(&[
            "site",
            "--dir",
            "/srv/www",
            "--bind",
            "0.0.0.0:8080",
        ]))
        .expect("site parses");
        assert_eq!(a.command, Command::Site);
        assert_eq!(a.site_dir.as_deref(), Some("/srv/www"));
        assert_eq!(a.bind, "0.0.0.0:8080");
    }

    #[test]
    fn site_still_binds_loopback_unless_told_otherwise() {
        // ADR-0003 §3. Reaching the rest of the network is something the
        // operator types; a website command is exactly where a helpful default
        // of 0.0.0.0 would feel natural and would be making their disclosure
        // decision for them.
        let a = parse(&argv(&["site", "--dir", "/srv/www"])).expect("site parses");
        assert_eq!(a.bind, super::DEFAULT_BIND);
        assert!(a.bind.starts_with("127.0.0.1"));
    }

    #[test]
    fn dir_without_a_value_is_refused() {
        let e = parse(&argv(&["site", "--dir"])).expect_err("--dir needs a value");
        assert!(e.0.contains("--dir"), "{}", e.0);
    }

    #[test]
    fn vault_without_a_directory_is_refused_rather_than_defaulted() {
        // The same rule as `site`, for a stronger reason: this one writes.
        let e = parse(&argv(&["vault"])).expect_err("vault needs --dir");
        assert!(e.0.contains("--dir"), "{}", e.0);
        assert!(e.0.contains("standing"), "{}", e.0);
    }

    #[test]
    fn enrol_without_a_device_name_is_refused() {
        let e = parse(&argv(&["enrol"])).expect_err("enrol needs --device");
        assert!(e.0.contains("--device"), "{}", e.0);
        assert!(e.0.contains("revoke"), "{}", e.0);
    }

    #[test]
    fn enrol_accepts_the_american_spelling_too() {
        // Nobody should have to guess which spelling a program wanted.
        for spelling in ["enrol", "enroll"] {
            let a = parse(&argv(&[spelling, "--device", "laptop"])).expect("parses");
            assert_eq!(a.command, Command::Enrol, "{spelling}");
            assert_eq!(a.device.as_deref(), Some("laptop"));
        }
    }

    #[test]
    fn the_vault_takes_a_directory_a_store_and_a_quota() {
        let a = parse(&argv(&[
            "vault",
            "--dir",
            "/srv/files",
            "--store",
            "/tmp/devices",
            "--quota",
            "500",
        ]))
        .expect("parses");
        assert_eq!(a.command, Command::Vault);
        assert_eq!(a.vault_dir.as_deref(), Some("/srv/files"));
        assert_eq!(a.store, "/tmp/devices");
        assert_eq!(a.quota, 500);
    }

    #[test]
    fn a_quota_that_is_not_a_number_is_refused_rather_than_defaulted() {
        let e =
            parse(&argv(&["vault", "--dir", "/d", "--quota", "lots"])).expect_err("not a number");
        assert!(e.0.contains("--quota"), "{}", e.0);
    }

    #[test]
    fn revoke_without_a_device_name_is_refused_and_says_how_to_find_one() {
        let e = parse(&argv(&["revoke"])).expect_err("revoke needs --device");
        assert!(e.0.contains("--device"), "{}", e.0);
        assert!(e.0.contains("vayucell devices"), "{}", e.0);
    }

    #[test]
    fn devices_needs_nothing_but_the_store() {
        let a = parse(&argv(&["devices", "--store", "/tmp/d"])).expect("parses");
        assert_eq!(a.command, Command::Devices);
        assert_eq!(a.store, "/tmp/d");
    }

    #[test]
    fn the_vault_binds_loopback_unless_told_otherwise() {
        // A command that accepts files is the last place a helpful default of
        // 0.0.0.0 belongs.
        let a = parse(&argv(&["vault", "--dir", "/d"])).expect("parses");
        assert_eq!(a.bind, super::DEFAULT_BIND);
    }

    #[test]
    fn all_takes_an_onion_directory_to_publish_through_the_system_daemon() {
        let a = parse(&argv(&[
            "all",
            "--site-dir",
            "/srv/site",
            "--onion-dir",
            "/var/lib/vayucell/onion",
        ]))
        .expect("parses");
        assert_eq!(
            a.onion_dir.as_deref(),
            Some("/var/lib/vayucell/onion"),
            "{a:?}"
        );
    }

    #[test]
    fn a_cell_without_onion_dir_publishes_nothing_by_default() {
        // ADR-0003 §3: the default is local-only, because publishing is an
        // irreversible disclosure and a default that publishes makes it on
        // the operator's behalf.
        for cmd in [vec!["status"], vec!["run"], vec!["all", "--site-dir", "/s"]] {
            let a = parse(&argv(&cmd)).expect("parses");
            assert_eq!(a.onion_dir, None, "{cmd:?}");
        }
    }

    #[test]
    fn onion_dir_is_refused_wherever_no_governor_runs() {
        // The flag promises governed publishing — shed first under thermal
        // load, stopped outright at PROTECT. `site` and `vault` consult a
        // governor but never run one, so the promise would be unenforced the
        // moment somebody restarted the cell into them; the refusal names
        // that instead of accepting and hoping.
        for cmd in [
            vec!["site", "--dir", "/srv", "--onion-dir", "/x"],
            vec!["vault", "--dir", "/srv", "--onion-dir", "/x"],
            vec!["serve", "--onion-dir", "/x"],
            vec!["run", "--onion-dir", "/x"],
        ] {
            let e = parse(&argv(&cmd)).expect_err(&format!("{cmd:?} must be refused"));
            assert!(e.0.contains("only `all`"), "{cmd:?}: {e}");
        }
    }
}
