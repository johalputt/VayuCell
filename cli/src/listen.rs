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
use vayucell_core::serve::{parse_request_line, refuse, route, Method, MAX_REQUEST_LINE};

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

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle(s, panel) {
                    eprintln!("vayucell: {e}");
                }
            }
            Err(e) => eprintln!("vayucell: a connection failed before it began: {e}"),
        }
    }
    Ok(())
}

fn handle(stream: TcpStream, panel: &dyn Fn() -> String) -> Result<(), String> {
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
            Ok(r) => (route(&r, &panel()), r.method),
            Err(bad) => (refuse(&bad), Method::Get),
        }
    };

    let (body, method) = response;
    let mut out = stream;
    out.write_all(&body.render(nonce()?, method))
        .map_err(|e| format!("could not write the response: {e}"))?;
    out.flush().map_err(|e| format!("could not flush: {e}"))
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
fn base64url(bytes: &[u8]) -> String {
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
