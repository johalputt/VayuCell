// SPDX-License-Identifier: Apache-2.0
//! `vayucell-sync` — the companion that keeps one folder in step with one
//! cell's vault.
//!
//! The cell never syncs on its own: storage there is a request somebody
//! makes, never a folder that mirrors itself. This command is the somebody.
//! It dials exactly one address, only while it runs, does what was asked,
//! prints what it did, and exits.

mod args;
mod cell;
mod plan;

use args::{ArgError, Command};
use cell::{Cell, CellError};
use plan::Action;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&argv) {
        eprintln!("vayucell-sync: {e}");
        std::process::exit(report::EXIT_USAGE);
    }
}

fn run(argv: &[String]) -> Result<(), ArgError> {
    let a = args::parse(argv)?;
    let token = args::token_from(&|k| std::env::var(k).ok(), &a.token_env)?;
    let host = vayucell_core::host::RealHost;

    if !vayucell_core::host::Host::exists(&host, &a.dir) {
        return Err(ArgError(format!(
            "`{}` does not exist on this machine",
            a.dir
        )));
    }

    let cell = Cell::new(a.cell.clone());
    let remote = cell.listing(&token).map_err(cell_error)?;
    let (locals, skipped) =
        plan::walk(&a.dir).map_err(|e| ArgError(format!("{} could not be read: {e}", a.dir)))?;
    for s in &skipped {
        println!("skipped  {s}");
    }
    let actions = plan::diff(&locals, &remote);

    match a.command {
        Command::Plan => {
            print_plan(&actions);
            println!("plan only — nothing was sent, nothing was deleted");
        }
        Command::Push => apply(&cell, &token, &a.dir, &actions, a.prune)?,
    }
    Ok(())
}

/// Prints every action the diff wants, and what would be left alone.
fn print_plan(actions: &[Action]) {
    for action in actions {
        match action {
            Action::Upload { name, why } => println!("upload   {name} ({why})"),
            Action::Prune { name } => {
                println!("prune    {name} — pass --prune to act on this")
            }
        }
    }
    if actions.is_empty() {
        println!("up to date; nothing to do");
    }
}

/// Applies the uploads in name order; prunes only when `prune` says so, and
/// only after every upload succeeded — clearing remote copies of files that
/// failed to re-upload is how data is lost twice in one afternoon.
fn apply(
    cell: &Cell,
    token: &str,
    dir: &str,
    actions: &[Action],
    prune: bool,
) -> Result<(), ArgError> {
    for action in actions {
        if let Action::Upload { name, why } = action {
            let bytes = std::fs::read(std::path::Path::new(dir).join(name))
                .map_err(|e| ArgError(format!("{name} could not be read: {e}")))?;
            // Sized from what was actually read rather than what the walk
            // saw: the file may have changed between the two moments.
            cell.put(name, &bytes, token).map_err(cell_error)?;
            println!("uploaded {} ({} bytes, {why})", name, bytes.len());
        }
    }
    let mut pruned = 0;
    if prune {
        for action in actions {
            if let Action::Prune { name } = action {
                cell.delete(name, token).map_err(cell_error)?;
                println!("deleted  {name} — gone locally, so gone remotely");
                pruned += 1;
            }
        }
    }
    if pruned > 0 && !prune {
        unreachable!("prunes only count when pruning ran");
    }
    let prunable = actions
        .iter()
        .filter(|a| matches!(a, Action::Prune { .. }))
        .count();
    if prunable > 0 && !prune {
        println!(
            "{prunable} remote file(s) no longer exist locally; run again with \
             --prune to delete them"
        );
    }
    Ok(())
}

fn cell_error(e: CellError) -> ArgError {
    ArgError(e.to_string())
}

/// Exit codes, mirroring the cell's own vocabulary.
mod report {
    /// A refused invocation, an unreadable folder, or a vault that said no.
    pub const EXIT_USAGE: i32 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-sync-main-{tag}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn dir(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
        fn put(&self, rel: &str, bytes: &[u8]) {
            std::fs::write(self.0.join(rel), bytes).expect("written");
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// One request answered per accepted connection, then the socket closes.
    /// The requests themselves come back so assertions can hold the client to
    /// the wire it promised.
    struct FakeVault {
        addr: String,
    }

    impl FakeVault {
        /// Reads exactly one HTTP exchange's worth of request: headers, plus the
        /// body Content-Length names if there is one.
        fn read_request(sock: &mut std::net::TcpStream) -> Vec<u8> {
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                if let Some(head_end) = raw.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
                {
                    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
                    let len: usize = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse().ok())
                        })
                        .unwrap_or(0);
                    if raw.len() >= head_end + len {
                        return raw;
                    }
                }
                let n = sock.read(&mut buf).expect("read");
                if n == 0 {
                    return raw;
                }
                raw.extend_from_slice(&buf[..n]);
            }
        }

        fn start(
            script: Vec<(u16, String, String)>,
        ) -> (Self, std::thread::JoinHandle<Vec<String>>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr").to_string();
            let script = std::cell::RefCell::new(script);
            let handle = std::thread::spawn(move || {
                let script = script.into_inner();
                let mut seen = Vec::new();
                for (status, reason, body) in script {
                    // A bounded wait, because the assertions below count
                    // requests: when the client is done, this server must be
                    // too, rather than blocking the test on an accept nobody
                    // is coming to make.
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    listener.set_nonblocking(true).expect("nonblocking");
                    let (mut sock, _) = loop {
                        match listener.accept() {
                            Ok(pair) => break pair,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(e) => panic!("accept: {e}"),
                        }
                        if std::time::Instant::now() > deadline {
                            return seen;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    };
                    sock.set_nonblocking(false).expect("blocking again");
                    let raw = Self::read_request(&mut sock);
                    seen.push(String::from_utf8_lossy(&raw).into_owned());
                    let reply = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    sock.write_all(reply.as_bytes()).expect("reply");
                }
                seen
            });
            (Self { addr }, handle)
        }
    }

    #[test]
    fn push_uploads_what_diffs_and_leaves_up_to_date_files_alone() {
        let s = Scratch::new("push");
        s.put("changed.txt", b"NEWBYTES");
        s.put("same.txt", b"same");

        // The up-to-date file's mtime must be the file's real one, or the
        // plan is right to call it changed and this test stops testing what
        // its name says.
        let same_mtime = std::fs::metadata(s.0.join("same.txt"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (vault, server) = FakeVault::start(vec![
            (
                200,
                "OK".to_owned(),
                format!(
                    r#"[{{"name":"same.txt","bytes":4,"modified":{same_mtime}}},{{"name":"gone.txt","bytes":9,"modified":9}}]"#
                ),
            ),
            (200, "OK".to_owned(), "stored\n".to_owned()),
        ]);
        let cell = Cell::new(vault.addr.clone());
        let remote = cell.listing("tok").expect("listing");
        let (locals, _) = plan::walk(&s.dir()).expect("walk");
        let actions = plan::diff(&locals, &remote);

        apply(&cell, "tok", &s.dir(), &actions, false).expect("applies");
        // Two actions: the new upload, and the remote extra that only --prune
        // would ever act on.
        assert_eq!(actions.len(), 2, "{actions:?}");
        assert_eq!(
            actions[0],
            Action::Upload {
                name: "changed.txt".to_owned(),
                why: plan::Difference::New
            }
        );
        assert_eq!(
            actions[1],
            Action::Prune {
                name: "gone.txt".to_owned()
            }
        );

        let seen = server.join().expect("server thread");
        assert_eq!(seen.len(), 2, "listing + one upload");
        assert!(seen[0].starts_with("GET / HTTP/1.1"), "{}", seen[0]);
        assert!(seen[1].starts_with("PUT /changed.txt"), "{}", seen[1]);
        assert!(
            seen[1].contains("Authorization: Bearer tok"),
            "the token rode along: {}",
            seen[1]
        );
        assert!(seen[1].ends_with("NEWBYTES"), "{}", seen[1]);
    }

    #[test]
    fn prune_only_deletes_when_the_flag_says_so_and_only_after_uploads_win() {
        let s = Scratch::new("prune");
        s.put("fresh.txt", b"x");

        let listing = r#"[{"name":"stale.txt","bytes":1,"modified":1}]"#.to_owned();
        let stored = "stored\n".to_owned();
        let gone = "no longer stored here\n".to_owned();
        let (vault, server) = FakeVault::start(vec![
            (200, "OK".to_owned(), listing.clone()),
            (200, "OK".to_owned(), stored),
            (200, "OK".to_owned(), gone),
        ]);
        let cell = Cell::new(vault.addr.clone());
        let remote = cell.listing("t").expect("listing");
        let (locals, _) = plan::walk(&s.dir()).expect("walk");
        let actions = plan::diff(&locals, &remote);
        assert_eq!(actions.len(), 2, "{actions:?}");

        // Without the flag: upload happens, delete does not.
        apply(&cell, "t", &s.dir(), &actions, false).expect("applies");
        let seen = server.join().expect("server");
        assert_eq!(seen.len(), 2, "no DELETE without --prune: {seen:?}");

        // With it: the same plan applied under --prune. No fresh listing is
        // taken — this phase proves what apply does with the actions it is
        // handed, so the wire shows exactly PUT then DELETE.
        let (vault2, server2) = FakeVault::start(vec![
            (200, "OK".to_owned(), "stored\n".to_owned()),
            (200, "OK".to_owned(), "no longer stored here\n".to_owned()),
        ]);
        let cell2 = Cell::new(vault2.addr.clone());
        apply(&cell2, "t", &s.dir(), &actions, true).expect("applies with prune");
        let seen2 = server2.join().expect("server");
        assert_eq!(seen2.len(), 2);
        assert!(seen2[0].starts_with("PUT /fresh.txt"), "{}", seen2[0]);
        assert!(seen2[1].starts_with("DELETE /stale.txt"), "{}", seen2[1]);
    }

    #[test]
    fn a_refused_upload_stops_the_run_and_never_reaches_the_prunes() {
        let s = Scratch::new("refused");
        s.put("big.txt", b"DATA");
        s.put("small.txt", b"tiny");

        let (vault, server) = FakeVault::start(vec![
            (200, "OK".to_owned(), "[]".to_owned()),
            (
                503,
                "Service Unavailable".to_owned(),
                "the device says no.\n".to_owned(),
            ),
        ]);
        let cell = Cell::new(vault.addr.clone());
        let remote = cell.listing("t").expect("listing");
        let (locals, _) = plan::walk(&s.dir()).expect("walk");
        let actions = plan::diff(&locals, &remote);

        let e = apply(&cell, "t", &s.dir(), &actions, false).expect_err("refused");
        assert!(e.0.contains("503"), "{e}");
        let seen = server.join().expect("server");
        assert_eq!(seen.len(), 2, "stopped at first refusal: {seen:?}");
    }
}
