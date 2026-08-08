// SPDX-License-Identifier: Apache-2.0

//! Turning a request into a response. ADR-0003 §3, local-only.
//!
//! # No sockets here
//!
//! This module parses bytes and returns bytes. The listener lives in the binary,
//! because a module that owned a socket could only be tested by opening one, and
//! the cases worth testing are a malformed request line, a path that walks out of
//! the document root, and a body that never arrives — none of which a well-behaved
//! client will ever send.
//!
//! # Everything this serves carries the full posture
//!
//! [`Response::render`] attaches [`crate::headers::SecurityHeaders`] to every
//! response including the errors, because a 404 served without a CSP is still a
//! page a browser will execute script in. The nonce is minted per response and
//! consumed by the render, so it cannot be reused — the type will not allow it.
//!
//! # This is local-only, and that is the whole design
//!
//! ADR-0003 §3 makes local-only the default because publishing is an
//! irreversible disclosure. Nothing here reaches the network beyond the socket
//! the caller hands it, there is no route that mutates anything, and there is no
//! authentication — because there is nothing to authorise. A panel is what the
//! owner of the device can already see by picking it up.

use crate::csp::{control_surface, published_site, Nonce, Policy};
use crate::headers::SecurityHeaders;
use crate::host::Host;
use crate::site::{resolve, status_for, Availability, Refusal, Resolved, SiteRoot};

/// What was asked for.
///
/// Only the two verbs a read-only surface needs. There is no `Post`, no `Put`
/// and no `Delete` variant, so a route that mutated something could not be
/// written without first widening this enum in a diff somebody has to approve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A read.
    Get,
    /// A read with the body suppressed.
    Head,
}

/// A parsed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The verb.
    pub method: Method,
    /// The path, already normalised and known not to escape the root.
    pub path: String,
}

/// Why a request was refused before it reached a route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadRequest {
    /// The request line was not `VERB SP PATH SP VERSION`.
    Malformed,
    /// A verb this surface does not implement.
    UnsupportedMethod(String),
    /// The path tried to leave the document root.
    ///
    /// Refused here rather than resolved and checked later. A traversal that is
    /// normalised first and validated second is one where the validation reads
    /// a path the filesystem already agreed to.
    Traversal,
    /// The path was not a valid target.
    BadPath,
}

impl core::fmt::Display for BadRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BadRequest::Malformed => f.write_str("the request line was not understood"),
            BadRequest::UnsupportedMethod(m) => {
                write!(f, "{m} is not a method this surface implements")
            }
            BadRequest::Traversal => f.write_str("the path tried to leave the document root"),
            BadRequest::BadPath => f.write_str("the path was not a valid target"),
        }
    }
}

/// The largest request line this will consider.
///
/// A bound rather than a `read_to_end`. The listener is on a home network and the
/// device is a phone with a governed battery; a client that never stops sending
/// should cost a bounded amount of memory rather than whatever it decides.
pub const MAX_REQUEST_LINE: usize = 8 * 1024;

/// Parses a request line.
///
/// # Errors
///
/// Returns why it was refused. Every refusal is a named reason rather than a
/// bare `None`, because the reason ends up in a log an operator reads.
pub fn parse_request_line(line: &str) -> Result<Request, BadRequest> {
    if line.len() > MAX_REQUEST_LINE {
        return Err(BadRequest::Malformed);
    }

    let mut parts = line.trim_end_matches(['\r', '\n']).split(' ');
    let (verb, target) = match (parts.next(), parts.next(), parts.next()) {
        (Some(v), Some(t), Some(_)) if !v.is_empty() && !t.is_empty() => (v, t),
        _ => return Err(BadRequest::Malformed),
    };

    let method = match verb {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        other => return Err(BadRequest::UnsupportedMethod(other.to_owned())),
    };

    // The query string is discarded rather than parsed: nothing here takes a
    // parameter, and a parser for arguments no route reads is an attack surface
    // maintained for no one.
    let path = target.split('?').next().unwrap_or(target);
    Ok(Request {
        method,
        path: normalise(path)?,
    })
}

/// Rejects anything that could leave the document root.
///
/// Deliberately a refusal rather than a resolution. Stripping `..` segments and
/// serving what is left means a request for `/../../etc/passwd` quietly becomes a
/// request for `/etc/passwd`, and the log records the second one.
fn normalise(path: &str) -> Result<String, BadRequest> {
    if !path.starts_with('/') {
        return Err(BadRequest::BadPath);
    }
    // Percent-encoding is refused rather than decoded, for the same reason: a
    // decoder here means the check runs against a different string from the one
    // that arrived, and %2e%2e is the oldest bypass there is.
    if path.contains('%') || path.contains('\\') || path.contains('\0') {
        return Err(BadRequest::Traversal);
    }
    if path.split('/').any(|seg| seg == ".." || seg == ".") {
        return Err(BadRequest::Traversal);
    }
    Ok(path.to_owned())
}

/// Which surface a response belongs to, and therefore which policy it carries.
///
/// Passed at every call rather than defaulted. Two surfaces exist with two
/// different policies — see [`crate::csp::published_site`] — and a default would
/// mean the weaker one could be attached to a control-surface response by
/// somebody who simply did not think about it. Naming it at the call site makes
/// "which policy is this carrying" a decision rather than an inheritance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The device's own panel. Script runs only with the per-response nonce.
    Control,
    /// A site the operator publishes. Their own script files run; inline does not.
    Site,
}

impl Surface {
    /// The policy this surface serves under.
    #[must_use]
    pub fn policy(self) -> Policy {
        match self {
            Surface::Control => control_surface(),
            Surface::Site => published_site(),
        }
    }
}

/// What to send back.
///
/// The body is bytes, not text. A site serves images and fonts, and a `String`
/// body would have made the type system demand that a PNG be valid UTF-8 —
/// which would have been discovered by whoever first put a photograph on their
/// site, rather than here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The reason phrase.
    pub reason: &'static str,
    /// The content type.
    pub content_type: &'static str,
    /// The body.
    pub body: Vec<u8>,
}

impl Response {
    /// A plain-text 200.
    #[must_use]
    pub fn text(body: String) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type: "text/plain; charset=utf-8",
            body: body.into_bytes(),
        }
    }

    /// A 200 carrying bytes of a declared type.
    ///
    /// The type is `&'static str` so it can only come from the allowlist in
    /// [`crate::site::content_type`]. A caller cannot compute one from the
    /// request, which is how a content type ends up being whatever an uploader
    /// named their file.
    #[must_use]
    pub fn bytes(body: Vec<u8>, content_type: &'static str) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type,
            body,
        }
    }

    /// A refusal, phrased for whoever reads it.
    #[must_use]
    pub fn refused(status: u16, reason: &'static str, why: &str) -> Self {
        Self {
            status,
            reason,
            content_type: "text/plain; charset=utf-8",
            body: format!("{reason}: {why}\n").into_bytes(),
        }
    }

    /// The complete response, headers and all.
    ///
    /// `nonce` is consumed, so it cannot be attached to a second response — the
    /// type enforces what a review comment would only ask for. Every response
    /// carries the full posture including the errors, because a 404 without a
    /// CSP is still a page a browser will execute script in.
    #[must_use]
    pub fn render(&self, surface: Surface, nonce: Nonce, method: Method) -> Vec<u8> {
        use core::fmt::Write as _;

        let mut head = format!("HTTP/1.1 {} {}\r\n", self.status, self.reason);
        for (name, value) in SecurityHeaders::production(surface.policy()).render(nonce) {
            // Writing into a String cannot fail, and a response builder is not a
            // place to start propagating an error that cannot happen.
            let _ = write!(head, "{name}: {value}\r\n");
        }
        let _ = write!(head, "Content-Type: {}\r\n", self.content_type);
        // Always the real length, even for HEAD, where the body is omitted but
        // the length still describes what a GET would return. Counted in BYTES:
        // a panel carries °C and an em dash, and counting characters would
        // truncate the last bytes on exactly the responses about temperature.
        let _ = write!(head, "Content-Length: {}\r\n", self.body.len());
        head.push_str("Connection: close\r\n\r\n");

        let mut out = head.into_bytes();
        if method == Method::Get {
            out.extend_from_slice(&self.body);
        }
        out
    }
}

/// Routes a request to a response.
///
/// `panel` is the rendered safety panel, passed in rather than assembled here so
/// this stays a pure function of its inputs.
#[must_use]
pub fn route(request: &Request, panel: &str) -> Response {
    match request.path.as_str() {
        "/" | "/panel" => Response::text(panel.to_owned()),
        // Deliberately present and deliberately empty of detail: a health
        // endpoint that reported a level would be a second place the device's
        // state is described, and the second place is the one that goes stale.
        "/health" => {
            Response::text("this process is answering; read /panel for what it found\n".to_owned())
        }
        _ => Response::refused(404, "Not Found", "no such path on this surface"),
    }
}

/// Routes a request against a published site.
///
/// Separate from [`route`] rather than a branch inside it, because the two
/// surfaces answer different questions. The panel answers "what did this device
/// find"; the site answers "what did the operator publish", and it answers that
/// only while the governor and the outage ladder both permit it.
///
/// `read` is passed in rather than performed here, so the resolution, the
/// availability check and every refusal are decided before a byte is read and
/// none of it needs a filesystem to test. It returns `None` for a file that
/// resolved but could not be read, which is a real case: one deleted between the
/// existence check and the read, or one whose permissions exclude this process.
#[must_use]
pub fn route_site(
    request: &Request,
    root: &SiteRoot,
    host: &dyn Host,
    availability: Availability,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Response {
    // Checked before resolution, not after. A withheld site that still resolved
    // paths would answer one status for a missing file and another for one that
    // exists, and the difference between those two answers is a map of the
    // operator's directory, served while the device is refusing to serve.
    if !availability.is_serving() {
        return Response::refused(503, "Service Unavailable", &availability.describe());
    }

    match resolve(root, host, &request.path) {
        Resolved::File { path, content_type } => match read(&path) {
            Some(body) => Response::bytes(body, content_type),
            // The same 404 a missing path gets, deliberately. It was there when
            // it resolved and is not now, or this process cannot read it, or the
            // containment check in the binary refused it — and a status that
            // varied between those and a typo would tell a stranger which paths
            // exist. The operator gets the real reason where it belongs, in the
            // log on the device they own.
            None => Response::refused(
                404,
                "Not Found",
                &Refusal::NotFound(request.path.clone()).to_string(),
            ),
        },
        Resolved::Refused(why) => {
            Response::refused(status_for(&why), "Not Found", &why.to_string())
        }
    }
}

/// The response for a request that could not be parsed.
#[must_use]
pub fn refuse(bad: &BadRequest) -> Response {
    match bad {
        BadRequest::UnsupportedMethod(_) => {
            Response::refused(405, "Method Not Allowed", &bad.to_string())
        }
        BadRequest::Traversal => Response::refused(403, "Forbidden", &bad.to_string()),
        BadRequest::Malformed | BadRequest::BadPath => {
            Response::refused(400, "Bad Request", &bad.to_string())
        }
    }
}
