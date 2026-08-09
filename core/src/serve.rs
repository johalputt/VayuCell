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

use crate::auth::{Credentials, Verdict};
use crate::csp::{control_surface, published_site, Nonce, Policy};
use crate::governor::Level;
use crate::headers::SecurityHeaders;
use crate::host::Host;
use crate::shed::Stage;
use crate::site::{resolve, status_for, Availability, Refusal, Resolved, SiteRoot};
use crate::vault::{Admission, Name, Quota, Receipt, Refused, VaultRoot, WritePlan};

/// What was asked for.
///
/// This enum is the project's write surface, expressed as a type. It carried
/// only `Get` and `Head` until the vault existed, and the comment then said that
/// a route which mutated something could not be written without first widening
/// it in a diff somebody has to approve. This is that diff.
///
/// `Put` and `Delete` are here; `Post` is not. `Put` names one file and replaces
/// it, and `Delete` names one file and removes it — both idempotent, so a retry
/// after a dropped connection is safe. `Post` has no meaning where nothing is
/// appended to, and a verb with no meaning is a verb whose route nobody can
/// reason about.
///
/// `Delete` arrived after `Put` and on purpose: removing somebody's file is the
/// one action here with no undo, and it deserved its own decision rather than
/// riding along with the one that stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A read.
    Get,
    /// A read with the body suppressed.
    Head,
    /// Store the body under the named file, replacing what was there.
    Put,
    /// Remove the named file.
    Delete,
}

impl Method {
    /// Whether this verb changes anything.
    ///
    /// Asked of the type so a route can refuse a write without enumerating
    /// verbs, which is how a verb added later quietly acquires a permission
    /// nobody granted it.
    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Method::Put | Method::Delete)
    }
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
    /// More header lines than [`MAX_HEADERS`].
    TooManyHeaders,
    /// A header line was not `Name: value`.
    MalformedHeader,
    /// A declared body larger than [`MAX_BODY`].
    BodyTooLarge(u64),
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
            BadRequest::TooManyHeaders => {
                write!(f, "more than {MAX_HEADERS} header lines were sent")
            }
            BadRequest::MalformedHeader => f.write_str("a header line was not understood"),
            BadRequest::BodyTooLarge(n) => write!(
                f,
                "the body is {n} bytes and the limit is {MAX_BODY}; refused before any \
                 of it was read"
            ),
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
        "PUT" => Method::Put,
        "DELETE" => Method::Delete,
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

/// The most header lines this will read before giving up.
///
/// A client that sends headers forever otherwise holds a connection and a
/// growing allocation, on a phone whose battery this project exists to protect.
pub const MAX_HEADERS: usize = 64;

/// The largest body this will accept, in bytes.
///
/// A bound the vault's quota does not replace: the quota describes the disk, and
/// this describes what may be held while deciding whether it fits. Sixty-four
/// mebibytes is comfortably more than a document and comfortably less than a
/// phone's memory.
pub const MAX_BODY: u64 = 64 * 1024 * 1024;

/// The two headers any route here actually reads.
///
/// Everything else is discarded rather than stored, for the same reason the
/// query string is: a parser for values no route consults is an attack surface
/// maintained for nobody, and a map of arbitrary headers is a thing that ends up
/// being logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    bearer: Option<String>,
    content_length: Option<u64>,
}

impl Headers {
    /// The bearer credential presented, if any.
    ///
    /// `None` covers both "no `Authorization` header" and "an `Authorization`
    /// header this does not understand". Both mean the same thing to a caller —
    /// nothing was presented that could be checked — and distinguishing them in
    /// the response would tell an unauthenticated stranger which schemes exist.
    #[must_use]
    pub fn bearer(&self) -> Option<&str> {
        self.bearer.as_deref()
    }

    /// The declared body length, if one was declared.
    #[must_use]
    pub const fn content_length(&self) -> Option<u64> {
        self.content_length
    }
}

/// Parses the header block.
///
/// # Errors
///
/// Returns why it was refused. A body longer than [`MAX_BODY`] is refused here,
/// before a single byte of it is read — refusing after reading it is the
/// resource exhaustion the limit exists to prevent.
pub fn parse_headers(lines: &[&str]) -> Result<Headers, BadRequest> {
    if lines.len() > MAX_HEADERS {
        return Err(BadRequest::TooManyHeaders);
    }
    let mut headers = Headers::default();
    for line in lines {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(BadRequest::MalformedHeader);
        };
        let value = value.trim();
        // Field names are case-insensitive per RFC 9110, and a client that sends
        // `authorization:` in lower case is not making an attack — it is using a
        // library.
        match name.to_ascii_lowercase().as_str() {
            "authorization" => {
                // Only Bearer. A scheme this does not implement is treated as
                // nothing presented, rather than as a different kind of refusal.
                if let Some(rest) = strip_bearer(value) {
                    headers.bearer = Some(rest.to_owned());
                }
            }
            "content-length" => {
                let n: u64 = value.parse().map_err(|_| BadRequest::MalformedHeader)?;
                if n > MAX_BODY {
                    return Err(BadRequest::BodyTooLarge(n));
                }
                headers.content_length = Some(n);
            }
            _ => {}
        }
    }
    Ok(headers)
}

/// The credential out of `Bearer <credential>`, with the scheme matched
/// case-insensitively and the separator required.
fn strip_bearer(value: &str) -> Option<&str> {
    let (scheme, rest) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
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
    /// What the operator is told, on the device they own. **Never rendered.**
    ///
    /// ADR-0008 §3 unified every site refusal to one status because the
    /// differences between them are "a directory listing delivered one status
    /// code at a time", and promised the operator's diagnosis was "not lost — it
    /// goes to the log on the device they own".
    ///
    /// There was no such log. Nothing was written for a hidden name, a directory
    /// with no index, a traversal attempt or a plain miss, so the diagnosis
    /// survived only in the response body — the one place §3 says it must not
    /// be. That is why the bodies still discriminated: taking them away without
    /// this field would have left the operator with nothing.
    ///
    /// Private, and [`Response::render`] never reads it. A caller cannot put it
    /// on the wire by accident because a caller cannot reach it.
    log: Option<String>,
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
            log: None,
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
            log: None,
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
            log: None,
        }
    }

    /// The same response, carrying a line for the operator's log.
    ///
    /// The two audiences want different things and only one of them is a
    /// stranger. This is what lets a refusal say the same sentence to every
    /// visitor while the operator still learns which of six reasons it was.
    #[must_use]
    pub fn explaining(mut self, log: impl Into<String>) -> Self {
        self.log = Some(log.into());
        self
    }

    /// The line for the operator's log, if this response has one.
    ///
    /// Read by the binary, which prints it to its own stderr. Never rendered.
    #[must_use]
    pub fn log(&self) -> Option<&str> {
        self.log.as_deref()
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
        // Suppressed for HEAD and for nothing else. This was written as
        // `== Method::Get` when those were the only two verbs, and the day PUT
        // arrived it silently swallowed every receipt and every error message a
        // PUT could produce — a 400 with no body, an upload with no
        // confirmation. Asking which verb *omits* a body is the question that
        // stays correct when a verb is added.
        if method != Method::Head {
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
            None => not_published(&request.path, &Refusal::NotFound(request.path.clone()))
                .explaining(format!("{} resolved and could not be read", request.path)),
        },
        // Every refusal, one sentence. ADR-0008 §3 unified the *status* for this
        // reason and left the bodies discriminating, so "/folder is a directory
        // with no index.html in it" and "nothing is published at /nodir" told a
        // stranger apart exactly what the shared status was hiding — a directory
        // listing delivered one body at a time instead of one status at a time.
        //
        // The real reason is not lost; it is in the response's log, which is
        // printed on the device the operator owns and never rendered.
        Resolved::Refused(why) => not_published(&request.path, &why).explaining(why.to_string()),
    }
}

/// The one answer every site refusal gives.
///
/// The path is echoed because the visitor sent it and learns nothing from
/// reading it back. Nothing else is: not whether it exists, not whether it is a
/// directory, not whether it was refused on its shape rather than looked for.
///
/// [`status_for`] is the authority on the status and every caller goes through
/// it — including the traversal refusal in [`refuse`], which used to decide for
/// itself and decided differently. Two places answering one question is how they
/// came to disagree; this keeps the body uniform, which is the half that was
/// missing, and leaves the status where it was already documented to live.
///
/// The reason phrase is `Not Found` because every arm of [`status_for`] is 404,
/// and a test pins that. A variant added later that is not would fail there
/// before it could arrive here with the wrong phrase.
#[must_use]
fn not_published(path: &str, why: &Refusal) -> Response {
    Response::refused(
        status_for(why),
        "Not Found",
        &Refusal::NotFound(path.to_owned()).to_string(),
    )
}

/// Why a stored file could not be written or removed.
///
/// This replaced a bare `String`, and the `String` is the point. The trait
/// documented it as "what went wrong, **for the operator's log**" — and
/// [`route_vault`] put it straight into the response body, so the operator's log
/// went to the caller. On this project's own filesystem layout that meant a
/// `PUT` answered with an absolute path to the vault directory.
///
/// [`route_site`] already had the rule, in as many words: *"the operator gets
/// the reason in the log on the device they own; the wire gets the same answer
/// either way."* The write path did the opposite of its sibling.
///
/// Two answers rather than one, because 500 was also saying the wrong thing. A
/// symbolic link stored under the requested name is not this server having
/// broken — the request was well formed, the device is fine, and something in
/// the vault needs a person. That is a conflict, and it is the caller's cue to
/// stop retrying and tell somebody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageFailure {
    /// Something already stored under that name blocks the write, and no retry
    /// will clear it.
    Conflict(String),
    /// The operation was attempted against the filesystem and did not complete.
    Failed(String),
}

impl StorageFailure {
    /// The status and reason this answers with.
    ///
    /// 409 for a conflict: the request was well formed and the server is not
    /// broken, the target is. 500 for a failure, which is what a server that
    /// could not do its job is.
    #[must_use]
    pub const fn status(&self) -> (u16, &'static str) {
        match self {
            Self::Conflict(_) => (409, "Conflict"),
            Self::Failed(_) => (500, "Internal Server Error"),
        }
    }

    /// What the caller is told.
    ///
    /// **Never a filesystem path.** The caller cannot act on where the vault
    /// lives, and on a device somebody else has already reached it is a map. The
    /// implementor puts the path in the log, which is on the operator's own
    /// device, and puts the *class* of problem here.
    #[must_use]
    pub fn told(&self) -> &str {
        match self {
            Self::Conflict(told) | Self::Failed(told) => told,
        }
    }
}

/// The filesystem, as the vault route needs it.
///
/// A trait rather than three closures. With two it was a pair of arguments a
/// caller could transpose; with three it is a shape somebody would get wrong
/// silently, and a reader and a remover swapped is not a mistake anybody should
/// be relying on review to catch.
///
/// Every method returns rather than performing anything the route decided
/// against: the route settles *whether*, and this settles *how*.
pub trait VaultIo {
    /// The bytes stored under `path`, or `None` for any reason it could not be
    /// read — absent, unreadable, or refused by a containment check.
    fn read(&self, path: &str) -> Option<Vec<u8>>;

    /// Performs the ordering in a [`WritePlan`], in that order.
    ///
    /// # Errors
    ///
    /// Returns what the caller may be told. The detail — which path, which
    /// errno — belongs in the implementor's log, not in the return value: this
    /// value reaches the wire.
    fn write(&self, plan: &WritePlan, bytes: &[u8]) -> Result<(), StorageFailure>;

    /// Removes `path`, answering whether anything was there.
    ///
    /// `Ok(false)` for a path that did not exist is deliberately not an error:
    /// deleting something already gone is the outcome the caller wanted, and a
    /// retry after a dropped connection lands there.
    ///
    /// # Errors
    ///
    /// Returns what the caller may be told, under the same rule as
    /// [`VaultIo::write`].
    fn remove(&self, path: &str) -> Result<bool, StorageFailure>;
}

/// Everything the vault route needs to know about the device and its owner.
///
/// Bundled rather than passed as seven arguments, so a caller that forgets one
/// fails to compile instead of getting a default.
#[derive(Debug, Clone, Copy)]
pub struct VaultContext<'a> {
    /// The devices enrolled on this cell.
    pub credentials: &'a Credentials,
    /// The directory files are kept in.
    pub root: &'a VaultRoot,
    /// How much room there is, or `None` when that could not be read.
    ///
    /// Measured per request rather than captured at startup, for the same reason
    /// the governor is consulted per request: a usage figure read once is wrong
    /// for every request after the first, and wrong in the admitting direction.
    pub quota: Option<Quota>,
    /// The governor's level.
    pub level: Level,
    /// The outage ladder's rung.
    pub stage: Stage,
}

/// Routes a request against the vault.
///
/// # The order of the checks is the security property
///
/// Authentication runs **first**, before the path is looked at, before the
/// governor is consulted and before the disk is measured. Every other order
/// leaks something to a stranger: checking the name first tells them which
/// filenames are acceptable, and checking the device state first tells them the
/// battery level of a phone they have no business knowing about. An
/// unauthenticated caller learns exactly one thing — that they are not enrolled.
///
/// `read` and `write` are passed in, so every decision here is testable without
/// a filesystem. `write` performs the ordering in [`WritePlan`]; this module
/// decides *whether*, never *how*.
#[must_use]
pub fn route_vault(
    request: &Request,
    headers: &Headers,
    ctx: &VaultContext<'_>,
    body: &[u8],
    io: &dyn VaultIo,
) -> Response {
    let verdict = ctx.credentials.verify(headers.bearer());
    let Verdict::Authenticated(_device) = verdict else {
        let Verdict::Refused(why) = verdict else {
            unreachable!("Verdict has exactly two variants")
        };
        return Response::refused(401, "Unauthorized", &why.to_string());
    };

    // The path is one name, never a tree. The vault is flat on purpose: a
    // directory structure is a second thing to validate and a second place for a
    // traversal to hide.
    let name = match Name::new(request.path.trim_start_matches('/')) {
        Ok(n) => n,
        Err(e) => return Response::refused(400, "Bad Request", &e.to_string()),
    };

    let stored = format!("{}/{}", ctx.root.dir(), name);

    match request.method {
        Method::Get | Method::Head => {
            // The device is asked before the disk is touched, on the site's
            // thresholds rather than the vault's stricter ones: reading a stored
            // file out is a read, and ADR-0009 §2 gives the vault the site's
            // read column exactly. `DERATED` still answers; `PROTECT` does not.
            let availability = Availability::of(ctx.level, ctx.stage);
            if !availability.is_serving() {
                return Response::refused(
                    503,
                    "Service Unavailable",
                    &availability.describe_stored_file(),
                );
            }
            match io.read(&stored) {
                Some(bytes) => Response::bytes(bytes, "application/octet-stream"),
                None => Response::refused(404, "Not Found", "nothing is stored under that name"),
            }
        }
        Method::Put => {
            let offered = body.len() as u64;
            let admission = Admission::of(ctx.level, ctx.stage, ctx.quota, offered);
            let Some(plan) = admission.plan(ctx.root, &name) else {
                return refused_admission(&admission);
            };
            match io.write(&plan, body) {
                Ok(()) => {
                    let receipt = Receipt::new(name, offered);
                    Response::text(format!("{}\n", receipt.describe()))
                }
                // The plan was sound and the device agreed; the write itself
                // did not happen. The caller learns which kind of not-happened
                // it was and nothing about where this vault lives.
                Err(failure) => {
                    let (status, reason) = failure.status();
                    Response::refused(status, reason, failure.told())
                }
            }
        }
        Method::Delete => {
            // The disk is not consulted at all. A full vault must never refuse
            // the one request that would free some, and neither must one whose
            // usage could not be read — a delete needs no room and does not
            // need to know how much there is.
            let admission = Admission::for_removal(ctx.level, ctx.stage);
            if !admission.is_accepting() {
                return refused_admission(&admission);
            }
            match io.remove(&stored) {
                Ok(true) => Response::text(format!("{name} is no longer stored here\n")),
                // Not an error. Deleting something already gone is the outcome
                // the caller wanted, and a retry after a dropped connection
                // lands exactly here.
                Ok(false) => {
                    Response::refused(404, "Not Found", "nothing is stored under that name")
                }
                Err(failure) => {
                    let (status, reason) = failure.status();
                    Response::refused(status, reason, failure.told())
                }
            }
        }
    }
}

/// The response for a device that will not take a write.
///
/// 503 for the device's condition, 507 for the disk. Both are the operator's
/// problem rather than the caller's mistake, and the caller can tell which from
/// the status without reading prose.
///
/// A vault whose usage could not be measured answers 503, not 507. 507 asserts
/// that the storage is insufficient, which is a measurement; the whole content
/// of that refusal is that no measurement exists.
fn refused_admission(admission: &Admission) -> Response {
    let (status, reason) = if matches!(admission, Admission::Refusing(Refused::Full(_))) {
        (507, "Insufficient Storage")
    } else {
        (503, "Service Unavailable")
    };
    Response::refused(status, reason, &admission.describe())
}

/// The response for a request that could not be parsed.
#[must_use]
pub fn refuse(bad: &BadRequest) -> Response {
    match bad {
        BadRequest::UnsupportedMethod(_) => {
            Response::refused(405, "Method Not Allowed", &bad.to_string())
        }
        // 404, not 403. ADR-0008 §3 names a 403 for a forbidden path as the
        // tempting design it rejected — "each is individually defensible;
        // together they are a directory listing delivered one status code at a
        // time" — and then this answered 403 anyway, so a prober could tell a
        // traversal-shaped request apart from an ordinary miss. The operator
        // still learns which it was, from the log.
        // Through `not_published` like every other refusal, and through
        // `status_for` with it. This decided its own status and decided 403 —
        // the one ADR-0008 §3 names as the tempting design it rejects — because
        // there were two authorities on one question. There is now one.
        //
        // "that path" rather than the requested one: the request line was
        // refused before a path existed, and echoing the raw bytes of a
        // traversal attempt back is its own small mistake.
        BadRequest::Traversal => {
            not_published("that path", &Refusal::Escape(String::new())).explaining(bad.to_string())
        }
        BadRequest::BodyTooLarge(_) => {
            Response::refused(413, "Content Too Large", &bad.to_string())
        }
        BadRequest::Malformed
        | BadRequest::BadPath
        | BadRequest::TooManyHeaders
        | BadRequest::MalformedHeader => Response::refused(400, "Bad Request", &bad.to_string()),
    }
}
