// SPDX-License-Identifier: Apache-2.0
//! The cell, seen from here: three requests and a listing parser.
//!
//! Plain `std` over TCP, one request per connection, `Content-Length`
//! everywhere. There is no TLS and no chunked encoding, and both absences are
//! stated where they bite rather than hidden behind an option.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// One stored file, as the vault's listing reports it.
///
/// Re-exported from the core so there is one shape of this truth, not two.
pub use vayucell_core::serve::StoredListing;

/// Why talking to the cell failed.
#[derive(Debug, PartialEq, Eq)]
pub enum CellError {
    /// The network itself: refused, reset, timed out.
    Wire(String),
    /// The answer could not be parsed as the HTTP this client speaks.
    Unspeaking(String),
    /// The vault refused, and said why in its body.
    Refused {
        /// The status code.
        status: u16,
        /// What the vault told the caller.
        body: String,
    },
}

impl core::fmt::Display for CellError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Wire(w) => write!(f, "the cell could not be reached: {w}"),
            Self::Unspeaking(w) => write!(f, "the answer was not the HTTP this client speaks: {w}"),
            Self::Refused { status, body } => write!(f, "the vault answered {status}: {body}"),
        }
    }
}

/// The address of a vault surface, as `host:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    host: String,
}

impl Cell {
    /// Wraps an already-validated `host:port` string.
    #[must_use]
    pub fn new(host: String) -> Self {
        Self { host }
    }

    fn connect(&self) -> Result<TcpStream, CellError> {
        let stream = TcpStream::connect(&self.host).map_err(|e| CellError::Wire(e.to_string()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .and_then(|()| stream.set_write_timeout(Some(Duration::from_secs(30))))
            .map_err(|e| CellError::Wire(e.to_string()))?;
        Ok(stream)
    }

    /// Fetches `GET /` — what is stored, sorted.
    ///
    /// # Errors
    ///
    /// Wire, unspeakable answers and refusals are all distinct variants.
    pub fn listing(&self, token: &str) -> Result<Vec<StoredListing>, CellError> {
        let mut stream = self.connect()?;
        write!(
            stream,
            "GET / HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\n\
             Accept: application/json\r\nConnection: close\r\n\r\n",
            self.host
        )
        .map_err(|e| CellError::Wire(e.to_string()))?;
        let (status, body) = read_response(&mut stream)?;
        if status == 200 {
            parse_listing(&body)
        } else {
            Err(CellError::Refused {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            })
        }
    }

    /// Stores one file with `PUT /<name>`.
    ///
    /// # Errors
    ///
    /// Anything that is not a `2xx`, including the governor's refusals and a
    /// full disk; the vault's own wording comes back with the status.
    pub fn put(&self, name: &str, bytes: &[u8], token: &str) -> Result<(), CellError> {
        let mut stream = self.connect()?;
        let head = format!(
            "PUT {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\n\
             Content-Type: application/octet-stream\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n",
            encode_path(name),
            self.host,
            bytes.len()
        );
        stream.write_all(head.as_bytes()).map_err(wire)?;
        stream.write_all(bytes).map_err(wire)?;
        let (status, body) = read_response(&mut stream)?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(CellError::Refused {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            })
        }
    }

    /// Fetches one file's bytes with `GET /<name>`.
    ///
    /// # Errors
    ///
    /// Anything that is not a `2xx`, with the vault's own wording; wire and
    /// unspeakable answers as everywhere else.
    pub fn get(&self, name: &str, token: &str) -> Result<Vec<u8>, CellError> {
        let mut stream = self.connect()?;
        let head = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\n\
             Connection: close\r\n\r\n",
            encode_path(name),
            self.host
        );
        stream.write_all(head.as_bytes()).map_err(wire)?;
        let (status, body) = read_response(&mut stream)?;
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(CellError::Refused {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            })
        }
    }

    /// Removes one file with `DELETE /<name>`.
    ///
    /// A 404 counts as success: deleting something already gone is the outcome
    /// the caller wanted, per ADR-0009 §7.
    ///
    /// # Errors
    ///
    /// Wire, unspeakable answers, and refusals other than a 404.
    pub fn delete(&self, name: &str, token: &str) -> Result<(), CellError> {
        let mut stream = self.connect()?;
        let head = format!(
            "DELETE {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\n\
             Connection: close\r\n\r\n",
            encode_path(name),
            self.host
        );
        stream.write_all(head.as_bytes()).map_err(wire)?;
        let (status, body) = read_response(&mut stream)?;
        if (200..300).contains(&status) || status == 404 {
            Ok(())
        } else {
            Err(CellError::Refused {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            })
        }
    }
}

fn wire(e: std::io::Error) -> CellError {
    CellError::Wire(e.to_string())
}

/// Reads one response, requiring `Content-Length` and refusing chunked.
fn read_response(stream: &mut TcpStream) -> Result<(u16, Vec<u8>), CellError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find_double_crlf(&buf) {
            break pos;
        }
        let n = stream.read(&mut chunk).map_err(wire)?;
        if n == 0 {
            return Err(CellError::Unspeaking(
                "the connection closed before the headers ended".to_owned(),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 {
            return Err(CellError::Unspeaking("header block over 64 KiB".to_owned()));
        }
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| CellError::Unspeaking("empty response".to_owned()))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| CellError::Unspeaking(format!("unreadable status line: {status_line:?}")))?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => {
                content_length = value.parse().ok();
            }
            "transfer-encoding" => {
                chunked = value.eq_ignore_ascii_case("chunked");
            }
            _ => {}
        }
    }
    if chunked {
        return Err(CellError::Unspeaking(
            "chunked responses are not spoken here; the vault always sends \
             Content-Length, so whatever answered is not it"
                .to_owned(),
        ));
    }
    let Some(len) = content_length else {
        return Err(CellError::Unspeaking(
            "no Content-Length; the vault always sends one".to_owned(),
        ));
    };

    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < len {
        let n = stream.read(&mut chunk).map_err(wire)?;
        if n == 0 {
            return Err(CellError::Unspeaking(
                "connection closed part-way through the body".to_owned(),
            ));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    Ok((status, body))
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Percent-encodes every byte that is not an URL-safe character.
///
/// The vault reads the path as the name directly, so today only names that
/// need no encoding at all are addressable; [`super::plan`] skips the rest
/// and says so. This encoder exists so that the day the vault learns to
/// decode, the client side is already correct.
#[must_use]
pub fn encode_path(name: &str) -> String {
    let mut out = String::from("/");
    for byte in name.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Parses the listing the vault serves at `GET /`.
///
/// Tolerant about key order, strict about shape: an array of objects each
/// carrying `name` (string), `bytes` and `modified` (unsigned integers).
///
/// # Errors
///
/// Names what was wrong rather than returning a partial listing, because a
/// partial listing is the lie the whole contract exists to prevent.
pub fn parse_listing(body: &[u8]) -> Result<Vec<StoredListing>, CellError> {
    let text = core::str::from_utf8(body)
        .map_err(|_| CellError::Unspeaking("listing is not UTF-8".to_owned()))?;
    let mut p = Parser { s: text, i: 0 };
    p.skip_ws();
    p.expect('[')?;
    let mut out = Vec::new();
    p.skip_ws();
    if p.peek() == Some(']') {
        p.bump();
        p.skip_ws();
        p.end()?;
        return Ok(out);
    }
    loop {
        p.skip_ws();
        p.expect('{')?;
        let mut name = None;
        let mut bytes = None;
        let mut modified = None;
        loop {
            p.skip_ws();
            let key = p.string()?;
            p.skip_ws();
            p.expect(':')?;
            p.skip_ws();
            match key.as_str() {
                "name" => name = Some(p.string()?),
                "bytes" => bytes = Some(p.uint()?),
                "modified" => modified = Some(p.uint()?),
                other => {
                    return Err(unreadable(format!(
                        "unknown key {other:?} in a listing entry"
                    )))
                }
            }
            p.skip_ws();
            match p.next_char() {
                Some(',') => continue,
                Some('}') => break,
                _ => return Err(unreadable("a listing entry did not end with }".to_owned())),
            }
        }
        let name = name.ok_or_else(|| unreadable("an entry had no name".to_owned()))?;
        let bytes = bytes.ok_or_else(|| unreadable("an entry had no bytes".to_owned()))?;
        let modified = modified.ok_or_else(|| unreadable("an entry had no modified".to_owned()))?;
        out.push(StoredListing {
            name,
            bytes,
            modified,
        });
        p.skip_ws();
        match p.next_char() {
            Some(',') => continue,
            Some(']') => break,
            _ => return Err(unreadable("the listing did not end with ]".to_owned())),
        }
    }
    p.skip_ws();
    p.end()?;
    Ok(out)
}

fn unreadable(why: String) -> CellError {
    CellError::Unspeaking(format!("the listing was not understood: {why}"))
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }
    fn next_char(&mut self) -> Option<char> {
        self.bump()
    }
    fn skip_ws(&mut self) {
        while matches!(
            self.peek(),
            Some(' ') | Some('\t') | Some('\n') | Some('\r')
        ) {
            self.i += self.peek().unwrap().len_utf8();
        }
    }
    fn expect(&mut self, c: char) -> Result<(), CellError> {
        if self.bump() == Some(c) {
            Ok(())
        } else {
            Err(unreadable(format!("expected {c:?}")))
        }
    }
    fn end(&self) -> Result<(), CellError> {
        if self.i == self.s.len() {
            Ok(())
        } else {
            Err(unreadable("trailing characters after the JSON".to_owned()))
        }
    }
    fn string(&mut self) -> Result<String, CellError> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('u') => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            let d = self
                                .bump()
                                .ok_or_else(|| unreadable("cut \\u".to_owned()))?;
                            let v = d
                                .to_digit(16)
                                .ok_or_else(|| unreadable("bad \\u digit".to_owned()))?;
                            code = code * 16 + v;
                        }
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                    }
                    Some(other) => {
                        return Err(unreadable(format!("unknown escape \\{other}")));
                    }
                    None => return Err(unreadable("string ended mid-escape".to_owned())),
                },
                Some(c) => out.push(c),
                None => return Err(unreadable("unterminated string".to_owned())),
            }
        }
    }
    fn uint(&mut self) -> Result<u64, CellError> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        self.s[start..self.i]
            .parse::<u64>()
            .map_err(|_| unreadable("not an unsigned integer".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_shaped_listing_parses_into_entries() {
        let body = br#"[{"name":"a.pdf","bytes":4096,"modified":222},{"name":"z.txt","bytes":1,"modified":111}]"#;
        let got = parse_listing(body).expect("parses");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].name, "a.pdf");
        assert_eq!(got[0].bytes, 4096);
        assert_eq!(got[0].modified, 222);
        assert_eq!(got[1].name, "z.txt");
    }

    #[test]
    fn keys_may_arrive_in_any_order_and_whitespace_is_not_a_problem() {
        let body = b"[ { \"bytes\" : 3 , \"modified\" : 9 , \"name\" : \"b.txt\" } ]";
        let got = parse_listing(body).expect("parses");
        assert_eq!(got[0].name, "b.txt");
        assert_eq!(got[0].bytes, 3);
        assert_eq!(got[0].modified, 9);
    }

    #[test]
    fn an_empty_vault_parses_to_no_entries_and_nothing_else_is_tolerated() {
        assert!(parse_listing(b"[]").expect("empty").is_empty());
        assert!(parse_listing(b"[{\"name\":\"x\"").is_err());
        assert!(parse_listing(b"not json").is_err());
        assert!(parse_listing(b"[] trailing").is_err());
        assert!(parse_listing(b"[{\"name\":true}]").is_err());
        assert!(parse_listing(b"[{\"nope\":1}]").is_err());
    }

    #[test]
    fn an_escaped_quote_survives_the_parser_the_way_it_survived_the_server() {
        let body = br#"[{"name":"say \"hi\".txt","bytes":2,"modified":9}]"#;
        let got = parse_listing(body).expect("parses");
        assert_eq!(got[0].name, r#"say "hi".txt"#);
    }

    #[test]
    fn paths_are_encoded_byte_for_byte_so_the_day_decoding_arrives_both_sides_agree() {
        assert_eq!(encode_path("plain.txt"), "/plain.txt");
        assert_eq!(encode_path("with space.txt"), "/with%20space.txt");
        assert_eq!(encode_path("héllo"), "/h%C3%A9llo");
        assert_eq!(encode_path("a+b"), "/a%2Bb");
    }

    #[test]
    fn a_chunked_answer_is_named_as_unspeakable_rather_than_half_parsed() {
        // The fake below feeds a real socket, because the reader works on
        // streams and a stream is what it should meet in a test.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            // Read until the end of the headers only: the client is still
            // waiting for its answer, so waiting for EOF here would deadlock
            // the pair.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = sock.read(&mut buf).expect("read");
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
            }
            let _ = sock.write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ndata\r\n0\r\n\r\n",
            );
        });
        let cell = Cell::new(addr.to_string());
        let e = cell.listing("token").expect_err("chunked");
        assert!(matches!(e, CellError::Unspeaking(_)), "{e}");
        assert!(e.to_string().contains("chunked"), "{e}");
        server.join().expect("server thread");
    }

    #[test]
    fn a_refusal_comes_back_with_its_status_and_the_vaults_own_words() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            // Read until the end of the headers only: the client is still
            // waiting for its answer, so waiting for EOF here would deadlock
            // the pair.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = sock.read(&mut buf).expect("read");
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
            }
            let body = b"the device says no.\n";
            let reply = format!(
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(reply.as_bytes());
            let _ = sock.write_all(body);
        });
        let cell = Cell::new(addr.to_string());
        let e = cell.listing("token").expect_err("refused");
        assert!(matches!(e, CellError::Refused { status: 503, .. }), "{e}");
        assert!(e.to_string().contains("the device says no"), "{e}");
        server.join().expect("server thread");
    }
}
