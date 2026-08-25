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
mod mirror;
mod plan;

use args::{ArgError, Command};
use cell::{Cell, CellError};
use plan::Action;
use std::path::PathBuf;

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

    match a.command {
        Command::Plan | Command::Push => {
            let remote = cell.listing(&token).map_err(cell_error)?;
            let (locals, skipped) = plan::walk(&a.dir)
                .map_err(|e| ArgError(format!("{} could not be read: {e}", a.dir)))?;
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
                _ => unreachable!("the outer match owns these"),
            }
        }
        Command::Replicate => {
            let Some(receipt_path) = a.receipt.as_deref() else {
                return Err(ArgError("replicate needs --receipt".to_owned()));
            };
            let (cycle, skipped) =
                mirror::replicate(&cell, &token, &a.dir, a.prune).map_err(mirror_error)?;
            for s in &skipped {
                println!("skipped  {s}");
            }
            write_receipt(
                receipt_path,
                vayucell_core::replica::Receipt::Replication {
                    completed_unix: now_unix(),
                    files: cycle.files,
                    bytes: cycle.bytes,
                    covered_mtime: cycle.covered_mtime,
                },
            )?;
            println!(
                "mirrored {} file(s), {} bytes; receipt written to {receipt_path}",
                cycle.files, cycle.bytes
            );
        }
        Command::Drill => {
            let Some(receipt_path) = a.receipt.as_deref() else {
                return Err(ArgError("drill needs --receipt".to_owned()));
            };
            let cycle = mirror::drill(&cell, &token, &a.dir).map_err(mirror_error)?;
            write_receipt(
                receipt_path,
                vayucell_core::replica::Receipt::RestoreDrill {
                    completed_unix: now_unix(),
                    files: cycle.files,
                    bytes: cycle.bytes,
                },
            )?;
            println!(
                "drilled {} file(s), {} bytes — every fresh download matched \
                 the mirror byte for byte; receipt written to {receipt_path}",
                cycle.files, cycle.bytes
            );
        }
    }
    Ok(())
}

/// The wall clock, as seconds since the epoch. The companion is allowed
/// std's clock; what it is not allowed is guessing when the clock will not say.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Folds one finished cycle into the evidence file and moves it into place.
///
/// The previous text is parsed before anything is written, exactly as the
/// cell side refuses to guess around a file it cannot read: overwriting
/// unreadable evidence with fresh-looking evidence would be destroying the
/// record rather than adding to it.
fn write_receipt(path: &str, receipt: vayucell_core::replica::Receipt) -> Result<(), ArgError> {
    let existing = std::fs::read_to_string(path).ok();
    let next = vayucell_core::replica::upsert(existing.as_deref(), &receipt)
        .map_err(|e| ArgError(format!("{path} holds evidence this tool cannot parse: {e}")))?;
    let tmp = PathBuf::from(path).with_extension("vayutmp");
    std::fs::write(&tmp, next)
        .map_err(|e| ArgError(format!("could not write the receipt beside {path}: {e}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| ArgError(format!("could not put the receipt in place at {path}: {e}")))?;
    Ok(())
}

fn mirror_error(e: mirror::MirrorError) -> ArgError {
    ArgError(e.to_string())
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

        /// The same server, plus a set of files it serves for `GET /<name>`:
        /// the listing and other scripted replies still come from `script`,
        /// in order, while each file request is answered from `files` — so
        /// replicate/drill tests get real per-file bodies on the wire.
        fn start_with_files(
            script: Vec<(u16, String, String)>,
            files: Vec<(&'static str, &'static [u8])>,
        ) -> (Self, std::thread::JoinHandle<Vec<String>>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr").to_string();
            let script = std::cell::RefCell::new(script);
            let handle = std::thread::spawn(move || {
                let script = script.into_inner();
                let mut seen = Vec::new();
                let mut next_file = 0usize;
                loop {
                    if seen.len() >= script.len() + files.len() {
                        return seen;
                    }
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
                    let request = String::from_utf8_lossy(&raw).into_owned();
                    seen.push(request.clone());

                    // A GET for one of the known files is answered from the
                    // file table; everything else consumes the script.
                    let served_from_files = request.starts_with("GET /")
                        && !request.starts_with("GET / HTTP/1.1")
                        && files
                            .iter()
                            .any(|(n, _)| request.starts_with(&format!("GET /{n} ")));
                    let reply = if served_from_files {
                        let (_, body) = files
                            .iter()
                            .find(|(n, _)| request.starts_with(&format!("GET /{n} ")))
                            .expect("looked up");
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len())
                            .into_bytes()
                            .into_iter()
                            .chain(body.iter().copied())
                            .collect::<Vec<u8>>()
                    } else {
                        let Some((status, reason, body)) = script.get(next_file) else {
                            return seen;
                        };
                        let (status, reason) = (*status, reason.clone());
                        let body = body.clone();
                        next_file += 1;
                        format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .into_bytes()
                    };
                    sock.write_all(&reply).expect("reply");
                }
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

    #[test]
    fn replicate_pulls_the_whole_listing_into_an_empty_mirror_and_writes_its_receipt() {
        use vayucell_core::replica::{parse as parse_receipts, Receipt};
        let s = Scratch::new("mirror");
        let r = Scratch::new("mirror-receipt");
        let receipt_path = r.0.join("evidence.json");

        let listing =
            "[{\"name\":\"a.bin\",\"bytes\":4,\"modified\":100},{\"name\":\"b.txt\",\"bytes\":6,\"modified\":200}]";
        let (vault, server) = FakeVault::start_with_files(
            vec![(200, "OK".to_owned(), listing.to_owned())],
            vec![
                ("a.bin", b"AAAA".as_slice()),
                ("b.txt", b"b.b.b.".as_slice()),
            ],
        );
        let cell = Cell::new(vault.addr.clone());
        let (cycle, skipped) = mirror::replicate(&cell, "t", &s.dir(), false).expect("mirrors");
        assert_eq!(skipped.len(), 0);
        assert_eq!(
            cycle,
            super::mirror::Cycle {
                files: 2,
                bytes: 10,
                covered_mtime: 200
            }
        );

        // The mirror holds exactly what the vault listed.
        assert_eq!(
            std::fs::read(s.0.join("a.bin")).expect("a"),
            b"AAAA".as_slice()
        );
        assert_eq!(
            std::fs::read(s.0.join("b.txt")).expect("b"),
            b"b.b.b.".as_slice()
        );

        // And the receipt the run leaves behind parses into what happened.
        write_receipt(
            receipt_path.to_str().expect("utf-8 path"),
            Receipt::Replication {
                completed_unix: now_unix(),
                files: cycle.files,
                bytes: cycle.bytes,
                covered_mtime: cycle.covered_mtime,
            },
        )
        .expect("receipt written");
        let text = std::fs::read_to_string(&receipt_path).expect("read back");
        let receipts = parse_receipts(&text).expect("parses");
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].kind(), "replication");

        let seen = server.join().expect("server thread");
        assert_eq!(seen.len(), 3);
        assert!(seen[0].starts_with("GET / HTTP/1.1"), "{}", seen[0]);
        assert!(seen[1].starts_with("GET /a.bin"), "{}", seen[1]);
        assert!(seen[2].starts_with("GET /b.txt"), "{}", seen[2]);
    }

    #[test]
    fn replicate_leaves_matching_files_alone_and_prunes_only_when_told() {
        let s = Scratch::new("mirror2");
        // One file already correct (size and mtime), one stale by size, one
        // local extra the vault no longer has.
        s.put("same.bin", b"SAME");
        let same_mtime = mirror_mtime_of(&s, "same.bin");
        s.put("stale.bin", b"OLDOLDOLDOLD");
        s.put("ghost.txt", b"gone from the vault");

        let listing = format!(
            "[{{\"name\":\"same.bin\",\"bytes\":4,\"modified\":{same_mtime}}},{{\"name\":\"stale.bin\",\"bytes\":5,\"modified\":9}}]"
        );
        let (vault, _server) = FakeVault::start_with_files(
            vec![(200, "OK".to_owned(), listing)],
            vec![("stale.bin", b"FRESH".as_slice())],
        );
        let cell = Cell::new(vault.addr.clone());

        // Without --prune: stale refreshed, ghost untouched.
        mirror::replicate(&cell, "t", &s.dir(), false).expect("mirrors");
        assert_eq!(
            std::fs::read(s.0.join("stale.bin")).expect("refreshed"),
            b"FRESH".as_slice()
        );
        assert!(
            s.0.join("ghost.txt").exists(),
            "no deletion without the flag"
        );

        // With it: the ghost goes.
        let (vault2, _) = FakeVault::start_with_files(
            vec![(
                200,
                "OK".to_owned(),
                format!(
                    "[{{\"name\":\"same.bin\",\"bytes\":4,\"modified\":{same_mtime}}},\
                     {{\"name\":\"stale.bin\",\"bytes\":5,\"modified\":9}}]"
                ),
            )],
            // The prune pass still refreshes the stale file, so its body has
            // to be servable here too.
            vec![("stale.bin", b"FRESH".as_slice())],
        );
        let cell2 = Cell::new(vault2.addr.clone());
        mirror::replicate(&cell2, "t", &s.dir(), true).expect("mirrors with prune");
        assert!(!s.0.join("ghost.txt").exists(), "pruned on the flag");
    }

    #[test]
    fn a_size_lie_from_the_wire_stops_replicate_before_anything_claims_success() {
        // The listing says 99 bytes; the body is 4. Trusting the body would
        // store a file the vault's own listing cannot describe.
        let s = Scratch::new("liar");
        let listing = "[{\"name\":\"lie.bin\",\"bytes\":99,\"modified\":1}]".to_owned();
        let (vault, _) = FakeVault::start_with_files(
            vec![(200, "OK".to_owned(), listing)],
            vec![("lie.bin", b"tiny".as_slice())],
        );
        let cell = Cell::new(vault.addr.clone());
        let e = mirror::replicate(&cell, "t", &s.dir(), false).expect_err("refuses");
        match e {
            super::mirror::MirrorError::File { name, why } => {
                assert_eq!(name, "lie.bin");
                assert!(why.contains("says 99 bytes but sent 4"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        // And nothing was left wearing the real name.
        assert!(
            !s.0.join("lie.bin").exists() || {
                let bytes = std::fs::read(s.0.join("lie.bin")).unwrap_or_default();
                bytes == b"tiny"
            }
        );
    }

    #[test]
    fn drill_compares_fresh_downloads_against_the_mirror_byte_for_byte() {
        let s = Scratch::new("drill");
        s.put("good.bin", b"identical");
        s.put("bad.bin", b"on-disk-copy");
        let listing =
            "[{\"name\":\"good.bin\",\"bytes\":9,\"modified\":1},{\"name\":\"bad.bin\",\"bytes\":12,\"modified\":2}]";
        let (vault, server) = FakeVault::start_with_files(
            vec![(200, "OK".to_owned(), listing.to_owned())],
            vec![
                ("good.bin", b"identical".as_slice()),
                ("bad.bin", b"WIRE-COPY!!!!".as_slice()),
            ],
        );
        let cell = Cell::new(vault.addr.clone());
        let e = mirror::drill(&cell, "t", &s.dir()).expect_err("mismatch named");
        match e {
            super::mirror::MirrorError::File { name, why } => {
                assert_eq!(name, "bad.bin");
                assert!(why.contains("byte for byte"), "{why}");
            }
            other => panic!("{other:?}"),
        }

        // A clean drill walks every file and returns its cycle.
        let s2 = Scratch::new("drill-ok");
        s2.put("only.bin", b"match!");
        let listing_ok = "[{\"name\":\"only.bin\",\"bytes\":6,\"modified\":77}]".to_owned();
        let (vault2, server2) = FakeVault::start_with_files(
            vec![(200, "OK".to_owned(), listing_ok)],
            vec![("only.bin", b"match!".as_slice())],
        );
        let cell2 = Cell::new(vault2.addr.clone());
        let cycle = mirror::drill(&cell2, "t", &s2.dir()).expect("clean drill");
        assert_eq!(cycle.files, 1);
        assert_eq!(cycle.bytes, 6);
        assert_eq!(cycle.covered_mtime, 77);
        let _ = server.join();
        let _ = server2.join().expect("server2");
    }

    #[test]
    fn drill_names_a_mirror_copy_that_has_gone_missing_rather_than_passing_it() {
        let s = Scratch::new("drill-missing");
        // Nothing in the mirror at all.
        let listing = "[{\"name\":\"lost.bin\",\"bytes\":2,\"modified\":1}]".to_owned();
        let (vault, _) = FakeVault::start_with_files(
            vec![(200, "OK".to_owned(), listing)],
            vec![("lost.bin", b"hi".as_slice())],
        );
        let cell = Cell::new(vault.addr.clone());
        let e = mirror::drill(&cell, "t", &s.dir()).expect_err("nothing to compare");
        assert!(
            e.to_string().contains("lost.bin") && e.to_string().contains("no readable copy"),
            "{e}"
        );
    }

    /// The mtime a just-written fixture actually carries, for listings that
    /// must describe it accurately.
    fn mirror_mtime_of(s: &Scratch, name: &str) -> u64 {
        std::fs::metadata(s.0.join(name))
            .expect("metadata")
            .modified()
            .expect("mtime")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after epoch")
            .as_secs()
    }
}
