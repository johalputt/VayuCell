// SPDX-License-Identifier: Apache-2.0

//! The socket. ADR-0003 §3, local-only.
//!
//! Everything that decides anything lives in [`vayucell_core::serve`], which
//! owns no socket and is testable without one. This file is the part that cannot
//! be: it accepts connections and moves bytes.
//!
//! # It binds the loopback interface unless told otherwise, and says which
//!
//! ADR-0003 §3: publishing is an irreversible disclosure and the default must
//! not make it on the owner's behalf. So the default bind address is
//! `127.0.0.1`, reachable only from the device itself; serving the rest of the
//! home network is a flag somebody types, and the address actually bound is
//! printed rather than assumed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use vayucell_core::csp::Nonce;
use vayucell_core::host::RealHost;
use vayucell_core::serve::{
    parse_headers, parse_request_line, refuse, route, route_site, route_vault, BadRequest, Headers,
    Method, Request, Response, StorageFailure, Surface, VaultContext, MAX_HEADERS,
    MAX_REQUEST_LINE,
};
use vayucell_core::site::{Availability, SiteRoot};
use vayucell_core::vault::Quota;

/// How long a connection may go silent before it is dropped.
///
/// Applied to the socket, so it bounds each read rather than the whole
/// exchange: an upload that keeps sending is never cut off, and one that stops
/// dead for this long is.
///
/// # It is half of the arithmetic that decides whether a surface survives
///
/// A stalled connection occupies one worker for at most this long, so a caller
/// opening silent connections at `r` per second keeps roughly `r × timeout` of
/// them stalled at once. The surface keeps answering while that stays under
/// [`WORKERS`], and stops when it does not.
///
/// At ten seconds and eight workers, one silent connection per second was
/// enough to saturate it — measured, not reasoned: four requests in thirty
/// seconds still timed out. At five seconds the same attack leaves half the
/// pool free. Neither number makes it immune, and [`WORKERS`] says why.
///
/// Five is generous for the legitimate case it must not break. A request line
/// and its headers cross a home network in milliseconds, and this is an idle
/// timeout, so a healthy upload never approaches it.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Serves until the process is stopped.
///
/// # Errors
///
/// Returns the reason the listener could not be established.
pub fn serve(addr: &str, panel: &(dyn Fn() -> String + Sync)) -> Result<(), String> {
    let listener = TcpListener::bind(addr).map_err(|e| format!("{addr}: {e}"))?;
    let bound = listener
        .local_addr()
        .map_err(|e| format!("the socket would not say what it bound: {e}"))?;

    // The bound address is reported rather than the one that was asked for: a
    // port of 0 resolves to something else entirely, and an operator checking
    // that this is not on the open network needs the real answer.
    println!("vayucell: serving the panel on http://{bound}/ (local only)");

    accept_loop(&listener, Surface::Control, &|r, _headers, _body| {
        route(r, &panel())
    })
}

/// Serves a directory of files, under the governor.
///
/// `availability` is consulted **per request** rather than once at startup. That
/// costs a handful of small sysfs reads per request, which on a home network is
/// nothing, and it buys the property the whole project turns on: a site that
/// stops being served the moment the cell is in trouble, rather than one that
/// keeps serving because the process started while everything was fine. A cached
/// verdict is a verdict that goes stale, and the stale direction is always the
/// reassuring one.
///
/// # Errors
///
/// Returns the reason the listener could not be established.
pub fn serve_site(
    addr: &str,
    root: &SiteRoot,
    availability: &(dyn Fn() -> Availability + Sync),
) -> Result<(), String> {
    let listener = TcpListener::bind(addr).map_err(|e| format!("{addr}: {e}"))?;
    let bound = listener
        .local_addr()
        .map_err(|e| format!("the socket would not say what it bound: {e}"))?;

    println!("vayucell: serving {} on http://{bound}/", root.dir());

    accept_loop(&listener, Surface::Site, &|r, _headers, _body| {
        route_site(r, root, &RealHost, availability(), &|path| {
            read_contained(root.dir(), path)
        })
    })
}

/// Serves the vault: authenticated reads and writes of stored files.
///
/// `context` is called per request for the same reason the site's availability
/// is — the governor's answer goes stale, and stale always fails in the
/// reassuring direction.
///
/// `limit` is the ceiling; how much is already used is measured per request by
/// [`used_bytes`]. Taking a usage figure once at startup would make the limit
/// hold for exactly one upload and then stop meaning anything.
///
/// # Errors
///
/// Returns the reason the listener could not be established.
pub fn serve_vault(
    addr: &str,
    root: &vayucell_core::vault::VaultRoot,
    credentials: &vayucell_core::auth::Credentials,
    context: &(dyn Fn() -> (vayucell_core::governor::Level, vayucell_core::shed::Stage) + Sync),
    limit: u64,
) -> Result<(), String> {
    let listener = TcpListener::bind(addr).map_err(|e| format!("{addr}: {e}"))?;
    let bound = listener
        .local_addr()
        .map_err(|e| format!("the socket would not say what it bound: {e}"))?;

    println!(
        "vayucell: vault at {} on http://{bound}/ — {} device(s) enrolled",
        root.dir(),
        credentials.len()
    );
    if credentials.is_empty() {
        // Not a warning to scroll past: in this state the vault answers 401 to
        // everything, which is correct and is also not what the operator wanted.
        println!(
            "vayucell: no device is enrolled, so every request will be refused.\n\
             \x20         Enrol one with: vayucell enrol --device <name>"
        );
    }

    // Said here because this is the moment somebody starts keeping files on a
    // phone. ADR-0004's opening sentence is that a phone is a replica and never
    // the only copy, and until this printed, nothing anywhere told an operator
    // that it is the only copy — the honesty types existed and had no caller.
    println!(
        "vayucell: {}\n\
         \x20         Nothing here replicates yet, so keep a copy elsewhere of \
         anything you would mind losing.",
        vayucell_core::durability::RecoveryPoint::NoReplica.describe(Duration::ZERO)
    );

    accept_loop(&listener, Surface::Site, &|request, headers, body| {
        let (level, stage) = context();
        let quota = used_bytes(root.dir()).map(|used| Quota::new(used, limit));
        if quota.is_none() {
            // The caller is told the vault will not take a write; only the
            // operator is told which directory would not answer.
            eprintln!(
                "vayucell: {} could not be measured, so no file can be admitted",
                root.dir()
            );
        }
        let ctx = VaultContext {
            credentials,
            root,
            quota,
            level,
            stage,
        };
        route_vault(
            request,
            headers,
            &ctx,
            body,
            &RealVaultIo { root: root.dir() },
        )
    })
}

/// The real filesystem, containment-checked.
///
/// Every path this touches goes through [`read_contained`] or through a
/// [`vayucell_core::vault::WritePlan`], both of which are anchored to the vault
/// directory. Nothing here joins a caller's string to a root.
struct RealVaultIo<'a> {
    root: &'a str,
}

impl vayucell_core::serve::VaultIo for RealVaultIo<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        read_contained(self.root, path)
    }

    fn list(&self) -> Result<Vec<vayucell_core::serve::StoredListing>, StorageFailure> {
        // All or nothing, per the trait's contract: a directory that cannot be
        // read fails the listing rather than answering with whatever happened
        // to enumerate, because a client pruning against a short one would call
        // the files it never heard about remote extras.
        let mut out = Vec::new();
        let dir =
            std::fs::read_dir(self.root).map_err(|e| logged_failure("listing", self.root, &e))?;
        for entry in dir {
            let entry = entry.map_err(|e| logged_failure("listing", self.root, &e))?;
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue, // not expressible as a vault name, so not addressable
            };
            if name.starts_with('.') {
                // Unreachable through this API — Name refuses them as a class —
                // so an operator-made dotfile is a foreign object here exactly
                // as it is on the site: present, but not listed.
                continue;
            }
            // DirEntry::metadata does not follow links: a link sitting in the
            // vault directory is skipped like any other thing this API cannot
            // have stored, rather than walked through.
            let meta = entry
                .metadata()
                .map_err(|e| logged_failure("listing", &name, &e))?;
            if !meta.is_file() {
                continue;
            }
            let modified = meta
                .modified()
                .map_err(|e| logged_failure("listing", &name, &e))?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            out.push(vayucell_core::serve::StoredListing {
                name,
                bytes: meta.len(),
                modified,
            });
        }
        Ok(out)
    }

    fn write(
        &self,
        plan: &vayucell_core::vault::WritePlan,
        bytes: &[u8],
    ) -> Result<(), StorageFailure> {
        write_durably(plan, bytes)
    }

    fn remove(&self, path: &str) -> Result<bool, StorageFailure> {
        // Canonicalised first, for the same reason a read is: a symbolic link
        // inside the vault pointing at something outside it would otherwise let
        // a delete reach a file that was never stored here.
        let Ok(real_root) = std::fs::canonicalize(self.root) else {
            eprintln!("vayucell: {}: the vault directory went away", self.root);
            return Err(StorageFailure::Failed(
                "the vault directory is not there, so nothing can be removed from it; \
                 the reason is in this cell's log"
                    .to_owned(),
            ));
        };
        let Ok(real_file) = std::fs::canonicalize(path) else {
            // Absent, which is not an error — the caller wanted it gone.
            return Ok(false);
        };
        if !real_file.starts_with(&real_root) {
            eprintln!(
                "vayucell: refusing to delete {path} — it resolves to {}, outside {}",
                real_file.display(),
                real_root.display()
            );
            return Ok(false);
        }
        match std::fs::remove_file(&real_file) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => {
                eprintln!("vayucell: removing {path}: {e}");
                Err(StorageFailure::Failed(
                    "the delete did not complete; the reason is in this cell's log".to_owned(),
                ))
            }
        }
    }
}

/// How many bytes the vault directory is holding, or `None` if that is not
/// knowable right now.
///
/// `None` rather than a partial sum, and never `0` as a fallback. An
/// under-count is the one error that matters here: it is indistinguishable from
/// free space, so it admits writes a full vault should refuse, and it does so
/// silently.
///
/// The vault is flat, so this is one directory read rather than a walk. A
/// subdirectory somebody created by hand is skipped and its contents are not
/// counted — the quota governs what the vault stores, not what else was put in
/// the folder. `symlink_metadata` is used deliberately: a link counts as the
/// link, so a link to a huge file elsewhere cannot inflate the usage figure and
/// lock the vault, and cannot deflate it either.
fn used_bytes(dir: &str) -> Option<u64> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut total: u64 = 0;
    for entry in entries {
        let metadata = entry.ok()?.path().symlink_metadata().ok()?;
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Some(total)
}

/// Performs the ordering in a [`vayucell_core::vault::WritePlan`], in that order.
///
/// The plan is data and this is the only place that acts on it. Every step is
/// here, including the directory flush, which is the one that is invisible until
/// a real power cut.
fn write_durably(
    plan: &vayucell_core::vault::WritePlan,
    bytes: &[u8],
) -> Result<(), StorageFailure> {
    use std::fs::{File, OpenOptions};

    // 0. Neither path may already be a symbolic link.
    //
    // A read canonicalises, and a delete canonicalises "for the same reason a
    // read is: a symbolic link inside the vault pointing at something outside it
    // would otherwise let a delete reach a file that was never stored here". The
    // same sentence is true of a write, and this was the one operation of the
    // three with no such check.
    //
    // The temporary is the dangerous one. `OpenOptions::open` follows links, so
    // a link sitting at the `.partial` path is opened, truncated and filled with
    // the uploaded bytes wherever it points, and the rename afterwards moves the
    // link — not the content — into place. The destination is the quieter one:
    // `rename` replaces a link rather than following it, so nothing escapes, but
    // an operator's link is destroyed without a word by a vault that would have
    // refused to *read* through it. Refusing both is what makes the three
    // operations agree.
    //
    // Checked with `symlink_metadata`, which reports the link rather than
    // following it — the same reason `used_bytes` uses it. This is a check
    // before an open, so a link created in the gap between them is not closed by
    // it; that requires write access to the vault directory, which is the same
    // user this process runs as, and the credential store already states that
    // the same user is not an adversary this design can hold off. Said here
    // rather than left for somebody to assume the stronger thing.
    for path in [plan.temporary(), plan.destination()] {
        if is_symlink(path) {
            // The path goes to the operator's log, on the operator's device.
            // The caller gets the class of problem and no map of the vault.
            eprintln!(
                "vayucell: refusing to write {path} — it is a symbolic link, and the \
                 vault stores files rather than writing through links to somewhere else"
            );
            return Err(StorageFailure::Conflict(
                "a symbolic link is stored under that name, and this vault will not \
                 write through one; retrying will not clear it, so tell whoever runs \
                 this cell"
                    .to_owned(),
            ));
        }
    }

    // 1. Write every byte to a temporary beside the destination.
    let mut temporary = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(plan.temporary())
        .map_err(|e| logged_failure("opening the temporary", plan.temporary(), &e))?;
    temporary
        .write_all(bytes)
        .map_err(|e| logged_failure("writing the temporary", plan.temporary(), &e))?;

    // 2. Ask the device to put the file's own bytes on the medium.
    temporary
        .sync_all()
        .map_err(|e| logged_failure("flushing the temporary", plan.temporary(), &e))?;
    drop(temporary);

    // 3. The one atomic step.
    std::fs::rename(plan.temporary(), plan.destination()).map_err(|e| {
        // Leaving the temporary behind would be debris nobody recognises.
        let _ = std::fs::remove_file(plan.temporary());
        eprintln!(
            "vayucell: renaming {} -> {}: {e}",
            plan.temporary(),
            plan.destination()
        );
        not_completed()
    })?;

    // 4. The step everyone forgets: without it the rename is what is lost.
    File::open(plan.directory())
        .and_then(|d| d.sync_all())
        .map_err(|e| logged_failure("flushing the directory", plan.directory(), &e))
}

/// Logs what happened, where, and returns what the caller may be told.
///
/// The split exists because the two audiences need different things. The
/// operator is at the device and needs the path and the errno; the caller is on
/// somebody's phone, can act on neither, and must not be handed the vault's
/// location — `route_site` has answered this way for reads since it was written.
fn logged_failure(doing: &str, path: &str, e: &std::io::Error) -> StorageFailure {
    eprintln!("vayucell: {doing} {path}: {e}");
    not_completed()
}

/// The one sentence a caller is told about a write that did not complete.
///
/// Deliberately the same for every step. Which of the four failed is the
/// operator's diagnostic, and telling a caller apart-by-step would leak the
/// shape of the write ordering to anything that can send traffic.
fn not_completed() -> StorageFailure {
    StorageFailure::Failed(
        "the write did not complete, and nothing was stored under that name; \
         the reason is in this cell's log"
            .to_owned(),
    )
}

/// Whether this path is a symbolic link, without following it.
///
/// `symlink_metadata` reports the link itself. `metadata` would report whatever
/// it points at, which for an absent target is an error and for a present one is
/// indistinguishable from an ordinary file — either way the answer would be
/// about the wrong object.
fn is_symlink(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Reads a file, having first confirmed against the real filesystem that it is
/// under the root.
///
/// [`vayucell_core::site::resolve`] makes traversal impossible through the
/// *request*, and says plainly that it cannot see symbolic links, because the
/// host interface it is written against cannot. This is where that gap is
/// closed: both paths are canonicalised, which resolves every link, and the file
/// is read only if the real path is genuinely inside the real root. A link
/// inside the site directory pointing at `/etc/shadow` resolves to `/etc/shadow`
/// and is refused here.
fn read_contained(root: &str, path: &str) -> Option<Vec<u8>> {
    let real_root = std::fs::canonicalize(root).ok()?;
    let real_file = std::fs::canonicalize(path).ok()?;
    if !real_file.starts_with(&real_root) {
        eprintln!(
            "vayucell: refusing {} — it resolves to {}, which is outside {}",
            path,
            real_file.display(),
            real_root.display()
        );
        return None;
    }
    match std::fs::read(&real_file) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            // The visitor gets the same 404 a typo gets, so the status cannot be
            // used to map the directory. The operator gets the reason, here, on
            // the device they own.
            eprintln!("vayucell: {} resolved but could not be read: {e}", path);
            None
        }
    }
}

/// Answers one parsed request. The body is empty for every read surface.
///
/// `Sync`, because several workers hold this at once — see [`WORKERS`].
type Responder<'a> = dyn Fn(&Request, &Headers, &[u8]) -> Response + Sync + 'a;

/// How many connections one surface will work on at a time.
///
/// # This was one, and one was a denial of service
///
/// A single accept loop reads each connection to completion before looking at
/// the next. A client that opens a socket and sends nothing therefore holds the
/// whole surface until [`READ_TIMEOUT`] expires — and one new silent connection
/// per second holds it permanently. Measured against the running binary: **zero
/// successful panel reads in thirty seconds**, from a caller sending no bytes,
/// presenting no credential and needing nothing but the ability to open a TCP
/// connection.
///
/// The panel is the surface that answers whether the battery in somebody's house
/// is safe. Silencing it is the worst available outcome of this project's own
/// design, and it took one socket.
///
/// # What this fixes, and what it does not
///
/// Eight workers means eight concurrent stalls are needed rather than one, and
/// [`READ_TIMEOUT`] bounds each. That is a real change in cost and it is **not**
/// immunity: blocking I/O with a fixed number of workers cannot be made immune
/// to a caller who opens connections faster than they time out. Fixing that
/// properly means an event loop, and an event loop without dependencies is a
/// large amount of subtle code this project would then have to be right about.
///
/// So the limit is stated rather than implied away. VayuCell binds loopback
/// unless told otherwise, and reaching the home network is a flag somebody
/// types — see ADR-0003 §3. On that network, a determined caller can still
/// exhaust this pool, and no amount of tuning the number changes that.
///
/// Eight rather than more: these are threads on a phone whose battery is the
/// entire point, and they exist to absorb stalls, not to serve load.
const WORKERS: usize = 8;

fn accept_loop(
    listener: &TcpListener,
    surface: Surface,
    respond: &Responder<'_>,
) -> Result<(), String> {
    // Every worker accepts from the same listener. The kernel hands each
    // connection to one of them, so there is no queue to size, nothing to
    // hand off between threads, and no state that two workers could disagree
    // about — which is the only reason a pool is affordable under a rule that
    // forbids reaching for a runtime.
    std::thread::scope(|scope| {
        for _ in 1..WORKERS {
            scope.spawn(|| accept_serially(listener, surface, respond));
        }
        accept_serially(listener, surface, respond);
    });
    Ok(())
}

/// One worker: accept, answer, repeat. Never returns while the socket is open.
fn accept_serially(listener: &TcpListener, surface: Surface, respond: &Responder<'_>) {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle(s, surface, respond) {
                    eprintln!("vayucell: {e}");
                }
            }
            Err(e) => eprintln!("vayucell: a connection failed before it began: {e}"),
        }
    }
}

fn handle(stream: TcpStream, surface: Surface, respond: &Responder<'_>) -> Result<(), String> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|e| format!("could not set a read timeout: {e}"))?;

    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| format!("could not read the connection: {e}"))?,
    );

    // Bounded. A client that sends a request line and never a newline should
    // cost a known amount of memory rather than whatever it decides to spend.
    let mut line = String::new();
    let read = (&mut reader)
        .take(MAX_REQUEST_LINE as u64)
        .read_line(&mut line)
        .map_err(|e| format!("could not read the request line: {e}"))?;

    let response = if read == 0 {
        // A connection that opened and said nothing. Not an error worth logging
        // loudly — port scanners do it constantly — but not something to answer
        // with a panel either.
        return Ok(());
    } else {
        match parse_request_line(&line) {
            Ok(r) => match read_headers_and_body(&mut reader) {
                Ok((headers, body)) => (respond(&r, &headers, &body), r.method),
                Err(bad) => (refuse(&bad), Method::Get),
            },
            Err(bad) => (refuse(&bad), Method::Get),
        }
    };

    let (body, method) = response;

    // The other half of ADR-0008 §3. Every site refusal now says the same
    // sentence on the wire, which is only honest if the operator can still find
    // out which of six reasons it was — and that promise was made in the ADR and
    // never implemented. This is where it is kept: on the device the operator
    // owns, where a visitor cannot read it.
    if let Some(line) = body.log() {
        eprintln!("vayucell: {line}");
    }

    let mut out = stream;
    out.write_all(&body.render(surface, nonce()?, method))
        .map_err(|e| format!("could not write the response: {e}"))?;
    out.flush().map_err(|e| format!("could not flush: {e}"))
}

/// Reads the header block and, if one was declared, the body.
///
/// Bounded at every step. The header count is capped by
/// [`vayucell_core::serve::MAX_HEADERS`], each line by
/// [`vayucell_core::serve::MAX_REQUEST_LINE`], and the body by the
/// `Content-Length` the header parser has already refused if it was too large —
/// so nothing here allocates on a number a stranger chose without that number
/// having passed a check first.
///
/// The body is read to exactly the declared length rather than to end of
/// stream. A client that declares ten bytes and sends a hundred gets ten stored
/// and the rest ignored; a client that declares a hundred and sends ten hits the
/// read timeout rather than storing a truncated file as though it were whole.
fn read_headers_and_body(reader: &mut impl BufRead) -> Result<(Headers, Vec<u8>), BadRequest> {
    let mut lines: Vec<String> = Vec::new();
    loop {
        let mut line = String::new();
        let read = reader
            .take(MAX_REQUEST_LINE as u64)
            .read_line(&mut line)
            .map_err(|_| BadRequest::MalformedHeader)?;
        // End of stream, or the blank line that ends the header block.
        if read == 0 || line.trim_end_matches(['\r', '\n']).is_empty() {
            break;
        }
        if lines.len() >= MAX_HEADERS {
            return Err(BadRequest::TooManyHeaders);
        }
        lines.push(line);
    }

    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let headers = parse_headers(&refs)?;

    let body = match headers.content_length() {
        None | Some(0) => Vec::new(),
        Some(n) => {
            // usize on a 32-bit phone is 32 bits, and MAX_BODY fits. The
            // conversion is checked rather than cast, because a silent
            // truncation here would read the wrong number of bytes.
            let n = usize::try_from(n).map_err(|_| BadRequest::BodyTooLarge(n))?;
            let mut body = vec![0u8; n];
            reader
                .read_exact(&mut body)
                .map_err(|_| BadRequest::Malformed)?;
            body
        }
    };
    Ok((headers, body))
}

/// A fresh nonce for one response.
///
/// Read from the kernel rather than generated here. This crate has no dependency
/// to draw randomness from and hand-rolling a generator for a value whose whole
/// job is being unguessable would be the worst possible place to start.
fn nonce() -> Result<Nonce, String> {
    // read_exact into a fixed buffer, NOT std::fs::read. /dev/urandom has no
    // end, so a whole-file read never returns — it allocates until the process
    // is killed. The first version of this did exactly that and the listener
    // died on its first request, which is the good outcome: the same mistake in
    // a path that is merely slow rather than fatal would have shipped.
    use std::io::Read as _;
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .map_err(|e| format!("no randomness available for a nonce: {e}"))?;
    Nonce::new(base64url(&buf)).map_err(|e| format!("the minted nonce was refused: {e}"))
}

/// Base64url, no padding. Twenty lines rather than a dependency.
///
/// Public because [`crate::enrol`] mints secrets with it too, and two encoders
/// is one more than can be checked against the known vectors below.
pub fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let take = chunk.len() + 1;
        for i in 0..take {
            let idx = ((n >> (18 - 6 * i)) & 0x3f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64url;

    #[test]
    fn the_nonce_encoding_produces_only_characters_that_cannot_escape_a_directive() {
        // A nonce carrying a quote or a semicolon would rewrite the policy it was
        // meant to protect. csp.rs refuses such a nonce; this makes sure the
        // encoder never mints one in the first place.
        for len in 1..48usize {
            let bytes: Vec<u8> = (0..len)
                .map(|i| u8::try_from(i % 256).unwrap_or(0))
                .collect();
            let s = base64url(&bytes);
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{s} contains a character that could escape the directive"
            );
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn the_encoding_matches_known_vectors() {
        // Hand-rolled, so it is checked against values somebody else computed.
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url(&[0xff, 0xef]), "_-8");
    }
}

#[cfg(test)]
mod containment_tests {
    use super::read_contained;
    use std::io::Write as _;

    /// A scratch directory, removed when the test ends.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("vayucell-contain-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a scratch directory");
            Self(dir)
        }
        fn path(&self, rel: &str) -> std::path::PathBuf {
            self.0.join(rel)
        }
        fn write(&self, rel: &str, body: &str) -> std::path::PathBuf {
            let p = self.path(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("a parent directory");
            }
            let mut f = std::fs::File::create(&p).expect("a file");
            f.write_all(body.as_bytes()).expect("written");
            p
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_ordinary_file_inside_the_root_is_read() {
        let s = Scratch::new("ordinary");
        let root = s.path("site");
        std::fs::create_dir_all(&root).expect("the root");
        s.write("site/index.html", "<h1>hi</h1>");

        let got = read_contained(
            root.to_str().expect("utf-8"),
            s.path("site/index.html").to_str().expect("utf-8"),
        );
        assert_eq!(got.as_deref(), Some(&b"<h1>hi</h1>"[..]));
    }

    #[test]
    fn a_symlink_pointing_out_of_the_root_is_refused() {
        // The gap core::site names and says it cannot close, closed here against
        // the real filesystem. A link is transparent to `exists` and to `read`,
        // so nothing above this function can see it; canonicalising both paths
        // resolves it and the comparison catches it.
        let s = Scratch::new("symlink");
        let root = s.path("site");
        std::fs::create_dir_all(&root).expect("the root");
        let secret = s.write("secret.txt", "PRIVATE");

        let link = root.join("leak.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, &link).expect("a symlink");
        #[cfg(not(unix))]
        return;

        // The link really does resolve to the secret, so the test is not passing
        // because the setup silently failed.
        assert_eq!(
            std::fs::read_to_string(&link).expect("the link resolves"),
            "PRIVATE"
        );

        assert_eq!(
            read_contained(root.to_str().expect("utf-8"), link.to_str().expect("utf-8")),
            None,
            "a link out of the root must not be read"
        );
    }

    #[test]
    fn a_symlink_staying_inside_the_root_is_still_read() {
        // Refusing every link would break an ordinary arrangement — a shared
        // asset directory linked into two places — so the check is about where
        // the link lands, not about whether one exists.
        let s = Scratch::new("inside");
        let root = s.path("site");
        std::fs::create_dir_all(root.join("assets")).expect("the root");
        let real = s.write("site/assets/style.css", "body{}");

        let link = root.join("style.css");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).expect("a symlink");
        #[cfg(not(unix))]
        return;

        assert_eq!(
            read_contained(root.to_str().expect("utf-8"), link.to_str().expect("utf-8")).as_deref(),
            Some(&b"body{}"[..])
        );
    }

    #[test]
    fn a_delete_cannot_reach_through_a_symlink_out_of_the_vault() {
        // The same gap as a read, on the one operation with no undo. A link
        // inside the vault pointing at something outside it must not let a
        // DELETE remove a file that was never stored here.
        use vayucell_core::serve::VaultIo as _;
        let s = Scratch::new("delete-symlink");
        let root = s.path("vault");
        std::fs::create_dir_all(&root).expect("the root");
        let outside = s.write("precious.txt", "KEEP");

        let link = root.join("bait.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).expect("a symlink");
        #[cfg(not(unix))]
        return;

        let io = super::RealVaultIo {
            root: root.to_str().expect("utf-8"),
        };
        assert_eq!(
            io.remove(link.to_str().expect("utf-8")),
            Ok(false),
            "a link out of the vault was followed"
        );
        assert!(outside.exists(), "the file outside the vault was deleted");
    }

    #[test]
    fn a_delete_of_something_inside_the_vault_removes_it() {
        use vayucell_core::serve::VaultIo as _;
        let s = Scratch::new("delete-inside");
        let root = s.path("vault");
        std::fs::create_dir_all(&root).expect("the root");
        let file = s.write("vault/a.txt", "bytes");

        let io = super::RealVaultIo {
            root: root.to_str().expect("utf-8"),
        };
        assert_eq!(io.remove(file.to_str().expect("utf-8")), Ok(true));
        assert!(!file.exists());
        // And again, which must be Ok(false) rather than an error.
        assert_eq!(io.remove(file.to_str().expect("utf-8")), Ok(false));
    }

    #[test]
    fn a_path_that_does_not_exist_is_none_rather_than_a_panic() {
        let s = Scratch::new("missing");
        let root = s.path("site");
        std::fs::create_dir_all(&root).expect("the root");
        assert_eq!(
            read_contained(
                root.to_str().expect("utf-8"),
                s.path("site/nope").to_str().expect("utf-8")
            ),
            None
        );
    }

    #[test]
    fn a_sibling_directory_with_the_roots_name_as_a_prefix_is_not_inside_it() {
        // The bug a naive string comparison has: "/srv/site-backup" starts with
        // "/srv/site". Path::starts_with compares components rather than bytes,
        // and this is the test that says so out loud.
        let s = Scratch::new("prefix");
        let root = s.path("site");
        std::fs::create_dir_all(&root).expect("the root");
        let neighbour = s.write("site-backup/secrets.txt", "PRIVATE");

        assert_eq!(
            read_contained(
                root.to_str().expect("utf-8"),
                neighbour.to_str().expect("utf-8")
            ),
            None,
            "a sibling sharing a name prefix is not inside the root"
        );
    }
}

#[cfg(test)]
mod reading_tests {
    use super::read_headers_and_body;
    use std::io::BufReader;
    use vayucell_core::serve::{BadRequest, MAX_BODY, MAX_HEADERS};

    fn read(raw: &str) -> Result<(vayucell_core::serve::Headers, Vec<u8>), BadRequest> {
        read_headers_and_body(&mut BufReader::new(raw.as_bytes()))
    }

    #[test]
    fn the_header_block_ends_at_the_blank_line_and_the_body_follows() {
        let (headers, body) = read("Content-Length: 5\r\n\r\nhello").expect("valid");
        assert_eq!(headers.content_length(), Some(5));
        assert_eq!(body, b"hello");
    }

    #[test]
    fn a_request_with_no_headers_at_all_reads_as_no_headers_and_no_body() {
        let (headers, body) = read("\r\n").expect("valid");
        assert_eq!(headers.bearer(), None);
        assert!(body.is_empty());
    }

    #[test]
    fn the_body_is_read_to_the_declared_length_and_not_past_it() {
        // A client that declares ten bytes and sends a hundred gets ten stored.
        // Reading to end of stream instead would store whatever it felt like
        // sending, which is the bound the Content-Length check exists to set.
        let (_, body) = read("Content-Length: 3\r\n\r\nabcdefghij").expect("valid");
        assert_eq!(body, b"abc");
    }

    #[test]
    fn a_body_shorter_than_declared_is_refused_rather_than_stored_truncated() {
        // Storing a short read as though it were whole is how a file becomes
        // silently damaged, which is the thing the whole vault is built against.
        let e = read("Content-Length: 10\r\n\r\nshort").expect_err("short body");
        assert_eq!(e, BadRequest::Malformed);
    }

    #[test]
    fn a_declared_length_of_zero_produces_no_body_and_no_read() {
        let (_, body) = read("Content-Length: 0\r\n\r\n").expect("valid");
        assert!(body.is_empty());
    }

    #[test]
    fn an_oversized_declaration_is_refused_before_the_body_is_touched() {
        let e =
            read(&format!("Content-Length: {}\r\n\r\n", MAX_BODY + 1)).expect_err("over the limit");
        assert_eq!(e, BadRequest::BodyTooLarge(MAX_BODY + 1));
    }

    #[test]
    fn more_header_lines_than_the_limit_stop_the_read_rather_than_growing_it() {
        let mut raw = String::new();
        for i in 0..=MAX_HEADERS {
            raw.push_str(&format!("X-Filler-{i}: x\r\n"));
        }
        raw.push_str("\r\n");
        assert_eq!(read(&raw).unwrap_err(), BadRequest::TooManyHeaders);
    }

    #[test]
    fn the_bearer_credential_survives_the_read() {
        let (headers, _) = read("Authorization: Bearer abc123\r\n\r\n").expect("valid");
        assert_eq!(headers.bearer(), Some("abc123"));
    }

    #[test]
    fn a_stream_that_ends_without_a_blank_line_is_not_an_error() {
        // A client that closes after its headers has sent a complete request as
        // far as this is concerned; there is simply no body.
        let (headers, body) = read("Authorization: Bearer abc123\r\n").expect("valid");
        assert_eq!(headers.bearer(), Some("abc123"));
        assert!(body.is_empty());
    }
}

#[cfg(test)]
mod durable_write_tests {
    use super::{write_durably, StorageFailure};
    use vayucell_core::vault::{Admission, Name, Quota, VaultRoot};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-write-{name}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn dir(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn plan_for(dir: &str, name: &str) -> vayucell_core::vault::WritePlan {
        let host = vayucell_core::host::RealHost;
        let root = VaultRoot::open(&host, dir).expect("the scratch exists");
        Admission::of(
            vayucell_core::governor::Level::Normal,
            vayucell_core::shed::Stage::Serving,
            Some(Quota::new(0, 1_000_000)),
            10,
        )
        .plan(&root, &Name::new(name).expect("plain"))
        .expect("accepted")
    }

    #[test]
    fn a_write_cannot_reach_through_a_symlink_at_the_temporary_path() {
        // The one operation of the three that had no containment. A read
        // canonicalises and a delete canonicalises; a write opened its temporary
        // with OpenOptions, which follows links. A link sitting at the .partial
        // path is opened, truncated and filled with the uploaded bytes wherever
        // it points, and the rename afterwards moves the link rather than the
        // content — so the upload lands outside the vault and the vault looks
        // empty.
        let s = Scratch::new("write-symlink-temp");
        let outside = std::path::Path::new(&s.dir()).join("precious.txt");
        std::fs::write(&outside, "KEEP").expect("a file outside the vault");
        let root = std::path::Path::new(&s.dir()).join("vault");
        std::fs::create_dir_all(&root).expect("the root");

        let plan = plan_for(root.to_str().expect("utf-8"), "notes.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, plan.temporary()).expect("a symlink");
        #[cfg(not(unix))]
        return;

        let refused = write_durably(&plan, b"UPLOADED").expect_err("a link is not a temporary");
        assert!(
            matches!(refused, StorageFailure::Conflict(_)),
            "{refused:?}"
        );
        assert!(refused.told().contains("symbolic link"), "{refused:?}");

        assert_eq!(
            std::fs::read_to_string(&outside).expect("still there"),
            "KEEP",
            "the upload was written through the link, outside the vault"
        );
    }

    #[test]
    fn a_write_does_not_silently_replace_a_symlink_at_the_destination() {
        // The quieter half. `rename` replaces a link rather than following it,
        // so nothing escapes here — but the vault would have *refused to read*
        // through this link, and destroying it without a word is the two
        // operations disagreeing about the same file.
        let s = Scratch::new("write-symlink-dest");
        let outside = std::path::Path::new(&s.dir()).join("elsewhere.txt");
        std::fs::write(&outside, "THEIRS").expect("a file outside the vault");
        let root = std::path::Path::new(&s.dir()).join("vault");
        std::fs::create_dir_all(&root).expect("the root");

        let plan = plan_for(root.to_str().expect("utf-8"), "notes.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, plan.destination()).expect("a symlink");
        #[cfg(not(unix))]
        return;

        let refused = write_durably(&plan, b"UPLOADED").expect_err("a link is not a destination");
        assert!(
            matches!(refused, StorageFailure::Conflict(_)),
            "{refused:?}"
        );
        assert!(refused.told().contains("symbolic link"), "{refused:?}");
        assert!(
            std::fs::symlink_metadata(plan.destination())
                .expect("still there")
                .file_type()
                .is_symlink(),
            "the operator's link was destroyed without a word"
        );
    }

    #[test]
    fn an_ordinary_write_over_a_real_file_is_still_allowed() {
        // The check is about links, not about overwriting. A vault that refused
        // to replace a stored file would break the ordinary case the whole
        // surface exists for.
        let s = Scratch::new("write-overwrite");
        let plan = plan_for(&s.dir(), "notes.txt");
        std::fs::write(plan.destination(), b"OLD").expect("a real file");

        write_durably(&plan, b"NEW").expect("an ordinary overwrite");
        assert_eq!(
            std::fs::read_to_string(plan.destination()).expect("read back"),
            "NEW"
        );
    }

    #[test]
    fn a_write_lands_under_the_real_name_and_leaves_no_temporary() {
        let s = Scratch::new("lands");
        let plan = plan_for(&s.dir(), "report.txt");
        write_durably(&plan, b"the bytes").expect("writes");

        assert_eq!(
            std::fs::read(plan.destination()).expect("exists"),
            b"the bytes"
        );
        assert!(
            !std::path::Path::new(plan.temporary()).exists(),
            "the temporary survived the rename"
        );
    }

    #[test]
    fn a_second_write_replaces_the_first_rather_than_appending() {
        // PUT names one file and replaces it. Appending would make a retry after
        // a dropped connection double the file.
        let s = Scratch::new("replace");
        let plan = plan_for(&s.dir(), "a.txt");
        write_durably(&plan, b"first").expect("writes");
        write_durably(&plan, b"second").expect("writes");
        assert_eq!(
            std::fs::read(plan.destination()).expect("exists"),
            b"second"
        );
    }

    #[test]
    fn the_temporary_is_hidden_so_a_site_would_never_serve_a_half_written_file() {
        let s = Scratch::new("hidden");
        let plan = plan_for(&s.dir(), "photo.jpg");
        let leaf = plan
            .temporary()
            .rsplit_once('/')
            .expect("has a parent")
            .1
            .to_owned();
        assert!(leaf.starts_with('.'), "{leaf}");
        // And core refuses that leaf as a name, which is what stops the site
        // serving it — two modules, one property.
        assert!(Name::new(&leaf).is_err());
    }

    #[test]
    fn an_empty_body_is_a_real_file_rather_than_a_skipped_write() {
        // Storing nothing is a thing somebody can mean, and the result must be
        // an empty file rather than no file.
        let s = Scratch::new("empty");
        let plan = plan_for(&s.dir(), "empty.txt");
        write_durably(&plan, b"").expect("writes");
        assert_eq!(std::fs::read(plan.destination()).expect("exists"), b"");
    }

    #[test]
    fn a_write_into_a_directory_that_went_away_fails_rather_than_reporting_success() {
        let s = Scratch::new("gone");
        let plan = plan_for(&s.dir(), "a.txt");
        std::fs::remove_dir_all(&s.0).expect("removes the directory");
        let e = write_durably(&plan, b"x").expect_err("nowhere to write");
        assert!(
            matches!(e, StorageFailure::Failed(_)),
            "a vanished directory is the server failing, not a conflict: {e:?}"
        );
        assert!(!e.told().is_empty(), "the failure explained nothing");
    }

    #[test]
    fn nothing_the_caller_is_told_about_a_write_carries_a_filesystem_path() {
        // The trait's error was documented as being "for the operator's log" and
        // went into the response body, so a PUT was answered with an absolute
        // path to the vault directory. route_site has answered reads the other
        // way since it was written: the reason goes to the log on the device the
        // operator owns, and the wire gets the class of problem.
        //
        // A separator is the tell. Every path this code could name has one, and
        // no sentence meant for a caller needs one.
        let s = Scratch::new("no-paths");
        let plan = plan_for(&s.dir(), "notes.txt");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&s.0, plan.destination()).expect("a symlink");
        #[cfg(not(unix))]
        return;
        let conflict = write_durably(&plan, b"x").expect_err("a link is not a destination");

        std::fs::remove_file(plan.destination()).expect("clear the link");
        std::fs::remove_dir_all(&s.0).expect("removes the directory");
        let failed = write_durably(&plan, b"x").expect_err("nowhere to write");

        for e in [conflict, failed] {
            assert!(
                !e.told().contains('/'),
                "the caller was told where the vault lives: {}",
                e.told()
            );
        }
    }
}

#[cfg(test)]
mod usage_tests {
    use super::used_bytes;

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-usage-{name}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn dir(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
        fn write(&self, rel: &str, bytes: &[u8]) {
            std::fs::write(self.0.join(rel), bytes).expect("written");
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_empty_vault_holds_nothing_which_is_not_the_same_as_unknown() {
        let s = Scratch::new("empty");
        assert_eq!(used_bytes(&s.dir()), Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_counts_as_the_link_and_not_as_what_it_points_at() {
        // Following it would let anything that can create a link decide the
        // usage figure: a link to a large file elsewhere locks the vault against
        // its owner, and the target is not on this disk in the first place.
        let s = Scratch::new("link");
        s.write("a.txt", b"1234");
        std::fs::create_dir(s.0.join("elsewhere")).expect("a directory");
        std::fs::write(s.0.join("elsewhere/big.bin"), vec![0u8; 5000]).expect("written");
        std::os::unix::fs::symlink(s.0.join("elsewhere/big.bin"), s.0.join("link.bin"))
            .expect("a link");

        let used = used_bytes(&s.dir()).expect("readable");
        assert!(
            used < 5000,
            "the link was followed and counted its target: {used}"
        );
        assert!(used >= 4, "the ordinary file stopped being counted: {used}");
    }

    #[test]
    fn what_is_stored_is_added_up_including_debris_from_an_interrupted_write() {
        // The `.partial` temporary occupies the disk whether or not it is
        // anybody's file, so a quota that ignored it would be a quota that can
        // be exceeded by crashing.
        let s = Scratch::new("sum");
        s.write("a.txt", b"12345");
        s.write("b.txt", b"1234567890");
        s.write(".c.txt.partial", b"123");
        assert_eq!(used_bytes(&s.dir()), Some(18));
    }

    #[test]
    fn a_directory_that_does_not_exist_is_unknown_rather_than_empty() {
        // The distinction the whole Option exists for. Zero here would read as
        // "all the room is free" and admit every upload.
        let s = Scratch::new("gone");
        let dir = s.dir();
        std::fs::remove_dir_all(&s.0).expect("removes it");
        assert_eq!(used_bytes(&dir), None);
    }

    #[test]
    fn a_subdirectory_somebody_created_is_skipped_rather_than_walked() {
        // The vault is flat. What else is in the folder is not the vault's, and
        // a walk is a symlink loop waiting to happen.
        let s = Scratch::new("nested");
        s.write("a.txt", b"1234");
        std::fs::create_dir(s.0.join("sub")).expect("a subdirectory");
        std::fs::write(s.0.join("sub/big.bin"), vec![0u8; 5000]).expect("written");
        assert_eq!(used_bytes(&s.dir()), Some(4));
    }
}

#[cfg(test)]
mod listing_tests {
    use super::RealVaultIo;
    use vayucell_core::serve::{StorageFailure, VaultIo as _};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("vayucell-listing-{name}"));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("scratch");
            Self(d)
        }
        fn dir(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
        fn write(&self, rel: &str, bytes: &[u8]) {
            std::fs::write(self.0.join(rel), bytes).expect("written");
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn io_for(dir: &str) -> RealVaultIo<'_> {
        RealVaultIo { root: dir }
    }

    #[test]
    fn the_walk_reports_each_file_with_its_size_and_its_last_write() {
        let s = Scratch::new("files");
        s.write("b.txt", b"1234567890");
        s.write("a.txt", b"1234");
        let mut found = io_for(&s.dir()).list().expect("readable");
        // The route sorts; the walk reports what the directory yielded. Compare
        // as a set so this test pins the contents and not an accident of order.
        found.sort_by(|x, y| x.name.cmp(&y.name));
        let names: Vec<_> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert_eq!(found[0].bytes, 4);
        assert_eq!(found[1].bytes, 10);
        for entry in &found {
            assert!(entry.modified > 0, "a real write has a real mtime");
        }
    }

    #[test]
    fn an_empty_vault_walks_to_an_empty_listing_rather_than_an_error() {
        let s = Scratch::new("empty");
        assert!(io_for(&s.dir()).list().expect("readable").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn entries_whose_metadata_will_not_read_fail_the_listing_instead_of_vanishing() {
        // A directory with read but no execute lets the walk list names while
        // refusing every stat. That is precisely the state a short listing
        // would lie about, so the contract answers Failed and the client
        // retries. Permissions are restored before the scratch is removed so
        // cleanup can run.
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new("dark");
        s.write("a.txt", b"x");
        std::fs::set_permissions(&s.0, std::fs::Permissions::from_mode(0o444))
            .expect("chmod");
        let result = io_for(&s.dir()).list();
        let _ = std::fs::set_permissions(&s.0, std::fs::Permissions::from_mode(0o755));
        let e = result.expect_err("metadata unreadable");
        assert!(matches!(e, StorageFailure::Failed(_)), "{e:?}");
    }

    #[test]
    fn a_directory_that_cannot_be_read_fails_instead_of_listing_nothing() {
        // Listing nothing is the answer a pruning client would act on; failing
        // is the answer it retries against. The difference is somebody's files.
        let s = Scratch::new("gone");
        let dir = s.dir();
        std::fs::remove_dir_all(&s.0).expect("removes it");
        let e = io_for(&dir).list().expect_err("unreadable");
        assert!(matches!(e, StorageFailure::Failed(_)), "{e:?}");
    }

    #[test]
    fn things_this_api_could_never_have_stored_are_not_listed() {
        // A subdirectory and a dotfile are foreign objects here: Name refuses
        // trees and hidden names as classes, so nothing that arrived through
        // the API can have made them. They are skipped, not served as names a
        // later GET would 400 or 404 on.
        let s = Scratch::new("foreign");
        s.write("kept.txt", b"x");
        s.write(".hidden", b"x");
        std::fs::create_dir(s.0.join("sub")).expect("a subdirectory");

        let found = io_for(&s.dir()).list().expect("readable");
        let names: Vec<_> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["kept.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_skipped_rather_than_described_as_its_target() {
        // DirEntry::metadata does not follow links, which is the whole reason
        // it is used here: describing a link by its target would hand a sync
        // client a name and a size belonging to something outside the vault.
        let s = Scratch::new("link");
        s.write("real.txt", b"1234");
        std::fs::write(s.0.join("elsewhere.bin"), vec![0u8; 500]).expect("written");
        std::os::unix::fs::symlink(s.0.join("elsewhere.bin"), s.0.join("link.bin"))
            .expect("a link");

        let found = io_for(&s.dir()).list().expect("readable");
        let names: Vec<_> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["real.txt"]);
    }
}

#[cfg(test)]
mod pool_tests {
    use super::{accept_loop, READ_TIMEOUT, WORKERS};
    use std::io::{Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};
    use vayucell_core::serve::{Headers, Request, Response, Surface};

    /// A surface on an ephemeral port, answering everything the same way.
    fn surface() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let addr = listener
            .local_addr()
            .expect("the socket says what it bound");
        std::thread::spawn(move || {
            let respond = |_: &Request, _: &Headers, _: &[u8]| Response::text("ok\n".to_owned());
            let _ = accept_loop(&listener, Surface::Control, &respond);
        });
        addr
    }

    /// What "answering" means here.
    ///
    /// Not merely "faster than the timeout". A surface with one worker answers
    /// a queued request the instant the stall ahead of it expires, which lands
    /// just *under* `READ_TIMEOUT` and passed the first version of these tests —
    /// the mutation that reduced the pool to one worker survived them. A pool
    /// that is absorbing stalls answers in milliseconds, so the bound has to be
    /// nowhere near the thing it is distinguishing itself from.
    ///
    /// Arithmetic on whole milliseconds rather than `checked_div(4).expect(..)`:
    /// `Option::expect` is not const-stable at this crate's declared MSRV, and
    /// the MSRV job is the only thing that says so — the toolchain these gates
    /// run on is years newer and compiles it happily.
    const PROMPTLY: Duration = Duration::from_millis(READ_TIMEOUT.as_secs() * 1000 / 4);

    fn ask(addr: std::net::SocketAddr) -> Duration {
        let start = Instant::now();
        let mut s = TcpStream::connect(addr).expect("connects");
        s.set_read_timeout(Some(READ_TIMEOUT)).expect("timeout set");
        s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .expect("writes");
        let mut buf = [0u8; 64];
        let _ = s.read(&mut buf).expect("answers");
        start.elapsed()
    }

    #[test]
    fn a_connection_that_never_speaks_does_not_hold_the_surface() {
        // The defect this pool exists for. With one accept loop, a socket that
        // opened and sent nothing held the whole surface until the read timeout
        // expired — and one new silent connection per second held it for good.
        // Measured against the running binary at the time: zero successful
        // panel reads in thirty seconds.
        let addr = surface();
        assert!(ask(addr) < PROMPTLY, "the surface was not answering");

        let _silent = TcpStream::connect(addr).expect("connects and says nothing");
        std::thread::sleep(Duration::from_millis(200));

        let took = ask(addr);
        assert!(
            took < PROMPTLY,
            "a silent connection held the surface for {took:?}; the pool absorbs nothing"
        );
    }

    #[test]
    fn several_silent_connections_still_leave_the_surface_answering() {
        // One is the interesting case; a handful is the realistic one. Fewer
        // than WORKERS must never be enough to close a surface.
        let addr = surface();
        let silent: Vec<TcpStream> = (0..WORKERS - 1)
            .map(|_| TcpStream::connect(addr).expect("connects"))
            .collect();
        std::thread::sleep(Duration::from_millis(200));

        let took = ask(addr);
        assert!(
            took < PROMPTLY,
            "{} silent connections closed the surface ({took:?})",
            silent.len()
        );
    }

    #[test]
    fn the_pool_is_large_enough_for_the_timeout_it_is_paired_with() {
        // The arithmetic the two constants are chosen against, asserted so that
        // raising one without the other fails here rather than in somebody's
        // house. A caller opening silent connections at one per second keeps
        // roughly `timeout` of them stalled at once; the surface answers while
        // that stays under WORKERS.
        let stalled_at_one_per_second = usize::try_from(READ_TIMEOUT.as_secs()).expect("small");
        assert!(
            stalled_at_one_per_second < WORKERS,
            "a silent connection per second stalls {stalled_at_one_per_second} of {WORKERS} \
             workers, which saturates the surface"
        );
    }
}
