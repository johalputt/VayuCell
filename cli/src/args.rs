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
    /// Print usage.
    Help,
    /// Print the version.
    Version,
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

/// The default ceiling. ADR-0002: 60% is the recommended long-term hold.
pub const DEFAULT_CEILING: u8 = 60;

/// What `serve` binds unless told otherwise.
///
/// Loopback. ADR-0003 §3 makes local-only the default because publishing is an
/// irreversible disclosure, and binding every interface by default would make a
/// weaker version of that decision on the operator's behalf — reachable by
/// anything on their network, including whatever else is on the guest Wi-Fi.
pub const DEFAULT_BIND: &str = "127.0.0.1:8080";

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
    help                Print this
    version             Print the version

OPTIONS:
    --supply-dir <DIR>  Power-supply directory
                        [default: /sys/class/power_supply/battery]
    --ceiling <PCT>     Charge ceiling to hold, 0-100 [default: 60]
    --ticks <N>         Stop `run` after N passes instead of running on
    --assume-outage <S> Treat mains as lost this many seconds ago, so the shed
                        ladder can be exercised without unplugging anything
    --bind <ADDR>       Address for `serve` and `site` [default:
                        127.0.0.1:8080]. The default is loopback: reaching the
                        rest of your network is something you type, not
                        something you get
    --dir <DIR>         The directory `site` publishes. Required by `site`.
                        Hidden names are never served, no directory listing is
                        ever generated, and a symbolic link pointing outside
                        this directory is refused

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
    let mut site_dir: Option<String> = None;

    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        match arg {
            "status" => set_command(&mut command, Command::Status, arg)?,
            "run" => set_command(&mut command, Command::Run { ticks: None }, arg)?,
            "serve" => set_command(&mut command, Command::Serve, arg)?,
            "site" => set_command(&mut command, Command::Site, arg)?,
            "help" | "--help" | "-h" => set_command(&mut command, Command::Help, arg)?,
            "version" | "--version" | "-V" => set_command(&mut command, Command::Version, arg)?,
            "--supply-dir" => {
                supply_dir = value_after(argv, &mut i, "--supply-dir")?;
            }
            "--bind" => {
                bind = value_after(argv, &mut i, "--bind")?;
            }
            "--dir" => {
                site_dir = Some(value_after(argv, &mut i, "--dir")?);
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

    // Refused here rather than defaulted to the working directory. A `site`
    // with no --dir that quietly published whatever folder the operator happened
    // to be standing in is the single worst thing this command could do.
    if command == Command::Site && site_dir.is_none() {
        return Err(ArgError(
            "site needs --dir <DIR>, the folder to publish; there is no default, \
             because a default would publish whatever directory you were standing in"
                .to_owned(),
        ));
    }

    Ok(Args {
        command,
        supply_dir,
        ceiling,
        assume_outage,
        bind,
        site_dir,
    })
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
}
