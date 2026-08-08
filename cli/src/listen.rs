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
    Method, Request, Response, Surface, VaultContext, MAX_HEADERS, MAX_REQUEST_LINE,
};
use vayucell_core::site::{Availability, SiteRoot};

/// How long a client may take to send its request line.
///
/// A connection that opens and never speaks otherwise holds a thread for as long
/// as it likes, on a phone whose battery this project exists to protect.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Serves until the process is stopped.
///
/// # Errors
///
/// Returns the reason the listener could not be established.
pub fn serve(addr: &str, panel: &dyn Fn() -> String) -> Result<(), String> {
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
    availability: &dyn Fn() -> Availability,
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
/// # Errors
///
/// Returns the reason the listener could not be established.
pub fn serve_vault(
    addr: &str,
    root: &vayucell_core::vault::VaultRoot,
    credentials: &vayucell_core::auth::Credentials,
    context: &dyn Fn() -> (vayucell_core::governor::Level, vayucell_core::shed::Stage),
    quota: vayucell_core::vault::Quota,
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

    accept_loop(&listener, Surface::Site, &|request, headers, body| {
        let (level, stage) = context();
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
            &|path| read_contained(root.dir(), path),
            &|plan, bytes| write_durably(plan, bytes),
        )
    })
}

/// Performs the ordering in a [`vayucell_core::vault::WritePlan`], in that order.
///
/// The plan is data and this is the only place that acts on it. Every step is
/// here, including the directory flush, which is the one that is invisible until
/// a real power cut.
fn write_durably(plan: &vayucell_core::vault::WritePlan, bytes: &[u8]) -> Result<(), String> {
    use std::fs::{File, OpenOptions};

    // 1. Write every byte to a temporary beside the destination.
    let mut temporary = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(plan.temporary())
        .map_err(|e| format!("{}: {e}", plan.temporary()))?;
    temporary
        .write_all(bytes)
        .map_err(|e| format!("{}: {e}", plan.temporary()))?;

    // 2. Ask the device to put the file's own bytes on the medium.
    temporary
        .sync_all()
        .map_err(|e| format!("{}: {e}", plan.temporary()))?;
    drop(temporary);

    // 3. The one atomic step.
    std::fs::rename(plan.temporary(), plan.destination()).map_err(|e| {
        // Leaving the temporary behind would be debris nobody recognises.
        let _ = std::fs::remove_file(plan.temporary());
        format!("{} -> {}: {e}", plan.temporary(), plan.destination())
    })?;

    // 4. The step everyone forgets: without it the rename is what is lost.
    File::open(plan.directory())
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{}: {e}", plan.directory()))
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
type Responder<'a> = dyn Fn(&Request, &Headers, &[u8]) -> Response + 'a;

fn accept_loop(
    listener: &TcpListener,
    surface: Surface,
    respond: &Responder<'_>,
) -> Result<(), String> {
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
    Ok(())
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
fn read_headers_and_body(
    reader: &mut BufReader<TcpStream>,
) -> Result<(Headers, Vec<u8>), BadRequest> {
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
