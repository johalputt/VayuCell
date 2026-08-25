// SPDX-License-Identifier: Apache-2.0
//! Argument parsing for `vayucell-sync`, in the same spirit as the cell's
//! own parser: thirty lines of std, no dependency, and every refusal names
//! what to do differently.

/// What was wrong with an invocation.
#[derive(Debug, PartialEq, Eq)]
pub struct ArgError(pub String);

impl core::fmt::Display for ArgError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which way the folder moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Print what would happen; touch nothing.
    Plan,
    /// Apply the uploads, and prune only when told.
    Push,
}

/// A parsed invocation.
#[derive(Debug, Clone)]
pub struct Args {
    /// Plan or push.
    pub command: Command,
    /// The folder on this machine. Its top level mirrors the vault's flat
    /// store; nothing nested is considered.
    pub dir: String,
    /// The cell to talk to, as `host:port`.
    pub cell: String,
    /// Remove remote files that no longer exist locally. Push-only, and
    /// never implied.
    pub prune: bool,
    /// Environment variable holding the device token.
    pub token_env: String,
}

const USAGE: &str = "usage:
  vayucell-sync plan <LOCAL_DIR> <HOST:PORT>
  vayucell-sync push <LOCAL_DIR> <HOST:PORT> [--prune]

options:
  --token-env <VAR>   environment variable holding the device token
                      (default: VAYUCELL_TOKEN)

plan prints what would move and touches nothing. push applies it;
--prune additionally deletes remote files that no longer exist locally,
which is why it is a flag you have to type.";

/// Parses an argument vector.
///
/// # Errors
///
/// Returns a message that names the fix, for every refusal.
pub fn parse(argv: &[String]) -> Result<Args, ArgError> {
    let mut iter = argv.iter();
    let word = iter.next().ok_or_else(|| ArgError(USAGE.to_owned()))?;
    let command = match word.as_str() {
        "plan" => Command::Plan,
        "push" => Command::Push,
        other => {
            return Err(ArgError(format!(
                "first argument is `{other}`, which is not plan or push.\n\n{USAGE}"
            )))
        }
    };

    let dir = iter
        .next()
        .ok_or_else(|| ArgError(format!("no folder given.\n\n{USAGE}")))?
        .clone();
    let cell = iter
        .next()
        .ok_or_else(|| ArgError(format!("no cell given.\n\n{USAGE}")))?
        .clone();

    if !cell.contains(':') {
        return Err(ArgError(format!(
            "`{cell}` has no port. Name the vault port the cell printed at startup, \
             like {cell}:8080 — guessing ports is how files land on the wrong service."
        )));
    }
    if let Some(rest) = cell.strip_prefix("http://") {
        return Err(ArgError(format!(
            "drop the `http://` — give the address alone, like {rest}"
        )));
    }
    if cell.starts_with("https://") || cell.contains("://") {
        return Err(ArgError(
            "this client speaks plain HTTP only. Over your own network that is \
             the transport; through Tor, the onion path is already encrypted end \
             to end. There is no TLS here to configure, because there are no \
             dependencies here to provide it."
                .to_owned(),
        ));
    }

    let mut prune = false;
    let mut token_env = "VAYUCELL_TOKEN".to_owned();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--prune" if command == Command::Push => prune = true,
            "--prune" => {
                return Err(ArgError(
                    "--prune belongs to push; plan never deletes anything.".to_owned(),
                ))
            }
            "--token-env" => {
                token_env = iter
                    .next()
                    .ok_or_else(|| ArgError("--token-env needs a variable name".to_owned()))?
                    .clone();
            }
            other => {
                return Err(ArgError(format!(
                    "`{other}` is not a flag this command takes.\n\n{USAGE}"
                )))
            }
        }
    }

    Ok(Args {
        command,
        dir,
        cell,
        prune,
        token_env,
    })
}

/// Reads the token from the environment, refusing to guess.
///
/// # Errors
///
/// Names the variable when it is unset or empty.
pub fn token_from(env: &dyn Fn(&str) -> Option<String>, var: &str) -> Result<String, ArgError> {
    match env(var) {
        Some(t) if !t.trim().is_empty() => Ok(t),
        Some(_) => Err(ArgError(format!(
            "{var} is set but empty; a blank token authenticates nobody"
        ))),
        None => Err(ArgError(format!(
            "{var} is not set. Enrol this machine with `vayucell enrol --device <name>` \
             on the cell, put the token it prints into {var}, and run this again."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn plan_takes_a_folder_and_a_cell_and_nothing_else_is_needed() {
        let a = parse(&argv(&["plan", "/home/me/site", "192.168.1.20:8080"])).expect("valid");
        assert_eq!(a.command, Command::Plan);
        assert_eq!(a.dir, "/home/me/site");
        assert_eq!(a.cell, "192.168.1.20:8080");
        assert!(!a.prune);
        assert_eq!(a.token_env, "VAYUCELL_TOKEN");
    }

    #[test]
    fn push_without_prune_never_deletes_and_with_it_says_so() {
        let plain = parse(&argv(&["push", "/d", "h:1"])).expect("valid");
        assert!(!plain.prune);
        let pruned = parse(&argv(&["push", "/d", "h:1", "--prune"])).expect("valid");
        assert!(pruned.prune);
    }

    #[test]
    fn prune_on_plan_is_refused_because_plan_never_deletes_anything() {
        let e = parse(&argv(&["plan", "/d", "h:1", "--prune"])).expect_err("misplaced flag");
        assert!(e.0.contains("plan never deletes"), "{}", e.0);
    }

    #[test]
    fn a_portless_host_is_refused_rather_than_guessed() {
        let e = parse(&argv(&["plan", "/d", "myphone.local"])).expect_err("no port");
        assert!(e.0.contains("no port"), "{}", e.0);
        assert!(
            e.0.contains("8080"),
            "the refusal shows an example: {}",
            e.0
        );
    }

    #[test]
    fn an_http_prefix_is_stripped_by_refusing_and_showing_the_form() {
        let e = parse(&argv(&["plan", "/d", "http://h:1"])).expect_err("scheme given");
        assert!(e.0.contains("drop the"), "{}", e.0);
        assert!(e.0.contains("h:1"), "{}", e.0);
    }

    #[test]
    fn https_is_refused_in_its_own_words_not_as_a_generic_error() {
        let e = parse(&argv(&["plan", "/d", "https://h:1"])).expect_err("tls requested");
        assert!(e.0.contains("plain HTTP"), "{}", e.0);
        assert!(e.0.contains("onion path is already encrypted"), "{}", e.0);
    }

    #[test]
    fn an_unknown_word_is_answered_with_the_usage() {
        let e = parse(&argv(&["sync", "/d", "h:1"])).expect_err("unknown command");
        assert!(e.0.contains("usage:"), "{}", e.0);
    }

    #[test]
    fn the_token_env_flag_renames_where_the_token_lives() {
        let a = parse(&argv(&[
            "push",
            "/d",
            "h:1",
            "--token-env",
            "LAPTOP_VAULT_TOKEN",
        ]))
        .expect("valid");
        assert_eq!(a.token_env, "LAPTOP_VAULT_TOKEN");
    }

    #[test]
    fn a_dangling_token_env_flag_is_an_error_not_a_panic() {
        let e = parse(&argv(&["push", "/d", "h:1", "--token-env"])).expect_err("dangling");
        assert!(e.0.contains("--token-env"), "{}", e.0);
    }

    #[test]
    fn the_token_is_read_from_the_named_variable_and_blank_means_unset() {
        let env = |k: &str| (k == "T").then(|| "secret".to_owned());
        assert_eq!(token_from(&env, "T").expect("present"), "secret");

        let blank = |k: &str| (k == "T").then(|| "   ".to_owned());
        let e = token_from(&blank, "T").expect_err("blank");
        assert!(e.0.contains("set but empty"), "{}", e.0);

        let absent = |_: &str| None;
        let e = token_from(&absent, "MISSING").expect_err("absent");
        assert!(
            e.0.contains("MISSING") && e.0.contains("vayucell enrol"),
            "{}",
            e.0
        );
    }
}
