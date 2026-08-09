// SPDX-License-Identifier: Apache-2.0

//! Serving tests, in the attacker's voice.
//!
//! Here the attacker is a person. Everything below is something a client can
//! send that a browser never would.

use crate::csp::Nonce;
use crate::governor::Level;
use crate::host::FakeHost;
use crate::serve::VaultIo as _;
use crate::serve::{
    parse_request_line, refuse, route, route_site, BadRequest, Method, Request, Response, Surface,
    MAX_REQUEST_LINE,
};
use crate::shed::Stage;
use crate::site::{Availability, SiteRoot};

fn nonce() -> Nonce {
    Nonce::new("r4nd0mBase64urlValue00").expect("a strong enough nonce")
}

/// The body as text, for the assertions that are about words.
///
/// Bodies are bytes now because a site serves images. Everything below is a
/// text response, and `from_utf8_lossy` keeps the assertion about the sentence
/// rather than about the encoding.
fn text(r: &Response) -> String {
    String::from_utf8_lossy(&r.body).into_owned()
}

fn get(path: &str) -> Request {
    Request {
        method: Method::Get,
        path: path.to_owned(),
    }
}

// ── Parsing ───────────────────────────────────────────────────────────────────

#[test]
fn an_ordinary_request_parses() {
    let r = parse_request_line("GET /panel HTTP/1.1\r\n").expect("parses");
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.path, "/panel");
}

#[test]
fn a_query_string_is_discarded_rather_than_parsed() {
    // Nothing here takes a parameter, and a parser for arguments no route reads
    // is an attack surface maintained for nobody.
    let r = parse_request_line("GET /panel?a=1&b=2 HTTP/1.1").expect("parses");
    assert_eq!(r.path, "/panel");
}

#[test]
fn traversal_is_refused_rather_than_normalised_away() {
    // Stripping the segments and serving what is left means a request for
    // /../../etc/passwd quietly becomes a request for /etc/passwd — and the log
    // records the second one, so nobody ever learns the first was sent.
    for hostile in [
        "GET /../etc/passwd HTTP/1.1",
        "GET /panel/../../secret HTTP/1.1",
        "GET /./panel HTTP/1.1",
    ] {
        assert_eq!(
            parse_request_line(hostile),
            Err(BadRequest::Traversal),
            "{hostile}"
        );
    }
}

#[test]
fn percent_encoding_is_refused_rather_than_decoded() {
    // %2e%2e is the oldest bypass there is, and it works precisely when the check
    // runs against a different string from the one that arrived. Refusing the
    // encoding entirely means there is only ever one string.
    for encoded in [
        "GET /%2e%2e/etc/passwd HTTP/1.1",
        "GET /%2E%2E%2Fsecret HTTP/1.1",
        "GET /panel%00.txt HTTP/1.1",
        "GET /panel\\..\\secret HTTP/1.1",
    ] {
        assert_eq!(
            parse_request_line(encoded),
            Err(BadRequest::Traversal),
            "{encoded}"
        );
    }
}

#[test]
fn a_malformed_request_line_is_refused_with_a_reason() {
    for junk in ["", "GET", "GET /panel", "   ", "GET  HTTP/1.1"] {
        let e = parse_request_line(junk).expect_err(&format!("{junk:?} must be refused"));
        assert!(
            matches!(e, BadRequest::Malformed | BadRequest::BadPath),
            "{junk:?} -> {e:?}"
        );
    }
}

#[test]
fn a_request_line_longer_than_the_bound_is_refused_before_it_is_parsed() {
    // A bound rather than a read_to_end. The listener is on a home network and
    // the device is a phone with a governed battery; a client that never stops
    // sending should cost a bounded amount of memory rather than whatever it
    // decides.
    let huge = format!("GET /{} HTTP/1.1", "a".repeat(MAX_REQUEST_LINE));
    assert_eq!(parse_request_line(&huge), Err(BadRequest::Malformed));
}

#[test]
fn only_the_four_implemented_verbs_are_accepted() {
    // This test used to require that PUT be refused, and it was the guard the
    // module documentation pointed at: a route that mutated something could not
    // be written without first widening the enum "in a diff somebody has to
    // approve". ADR-0010 is that diff, and this is where it shows.
    //
    // The claim is narrower now rather than gone. PUT names one file and
    // replaces it, which is idempotent — a retry after a dropped connection is
    // safe. DELETE destroys somebody's data and deserves its own decision, and
    // POST has no meaning where nothing is being appended to.
    assert_eq!(
        parse_request_line("HEAD / HTTP/1.1").unwrap().method,
        Method::Head
    );
    assert_eq!(
        parse_request_line("PUT /a.txt HTTP/1.1").unwrap().method,
        Method::Put
    );
    assert_eq!(
        parse_request_line("DELETE /a.txt HTTP/1.1").unwrap().method,
        Method::Delete
    );
    for verb in ["POST", "PATCH", "TRACE", "CONNECT"] {
        let e = parse_request_line(&format!("{verb} / HTTP/1.1"))
            .expect_err(&format!("{verb} must be refused"));
        assert_eq!(e, BadRequest::UnsupportedMethod(verb.to_owned()));
        assert_eq!(refuse(&e).status, 405);
    }
}

#[test]
fn exactly_the_two_changing_verbs_write() {
    // Asked of the type rather than of a list, so a verb added later does not
    // quietly acquire a permission nobody granted it.
    let writers: Vec<Method> = [Method::Get, Method::Head, Method::Put, Method::Delete]
        .into_iter()
        .filter(|m| m.writes())
        .collect();
    assert_eq!(writers, [Method::Put, Method::Delete]);
}

#[test]
fn a_path_that_does_not_start_at_the_root_is_refused() {
    assert_eq!(
        parse_request_line("GET panel HTTP/1.1"),
        Err(BadRequest::BadPath)
    );
    // An absolute-form target is a proxy request, and this surface is not one.
    assert_eq!(
        parse_request_line("GET http://elsewhere/panel HTTP/1.1"),
        Err(BadRequest::BadPath)
    );
}

// ── Routing ───────────────────────────────────────────────────────────────────

#[test]
fn the_root_and_the_panel_path_both_serve_the_panel() {
    for p in ["/", "/panel"] {
        let r = route(&get(p), "BATTERY SAFETY: PROTECTED\n");
        assert_eq!(r.status, 200);
        assert!(text(&r).contains("BATTERY SAFETY"), "{p}");
    }
}

#[test]
fn the_health_path_does_not_restate_the_devices_condition() {
    // A health endpoint that reported a level would be a second place the
    // device's state is described, and the second place is the one that goes
    // stale — then disagrees with the panel, in the reassuring direction.
    let r = route(&get("/health"), "BATTERY SAFETY: UNSAFE\n");
    assert_eq!(r.status, 200);
    assert!(!text(&r).contains("UNSAFE"), "{}", text(&r));
    assert!(text(&r).contains("read /panel"), "{}", text(&r));
}

#[test]
fn an_unknown_path_is_a_404_and_not_a_redirect_to_the_panel() {
    let r = route(&get("/wp-admin"), "panel");
    assert_eq!(r.status, 404);
    assert!(!text(&r).contains("panel\n"));
}

// ── Every response carries the posture ────────────────────────────────────────

#[test]
fn even_a_404_carries_the_full_security_posture() {
    // A 404 served without a CSP is still a page a browser will execute script
    // in, and error paths are where headers get dropped because the happy path
    // is the one anybody checks.
    let out =
        String::from_utf8(route(&get("/nope"), "p").render(Surface::Control, nonce(), Method::Get))
            .unwrap();
    assert!(out.starts_with("HTTP/1.1 404 Not Found"));
    for required in [
        "Content-Security-Policy:",
        "X-Content-Type-Options: nosniff",
        "X-Frame-Options: DENY",
        "Referrer-Policy: no-referrer",
        "Permissions-Policy:",
        "Cross-Origin-Opener-Policy: same-origin",
        "Strict-Transport-Security:",
    ] {
        assert!(out.contains(required), "a 404 must still carry {required}");
    }
}

#[test]
fn a_refusal_carries_the_posture_too() {
    let bad = parse_request_line("POST / HTTP/1.1").unwrap_err();
    let out =
        String::from_utf8(refuse(&bad).render(Surface::Control, nonce(), Method::Get)).unwrap();
    assert!(out.starts_with("HTTP/1.1 405"));
    assert!(out.contains("Content-Security-Policy:"));
}

#[test]
fn the_policy_served_permits_no_inline_script() {
    // The whole point of csp.rs reaching a socket. Until now these headers had
    // never been sent to anything.
    let out =
        String::from_utf8(route(&get("/"), "panel").render(Surface::Control, nonce(), Method::Get))
            .unwrap();
    assert!(!out.contains("unsafe-inline"), "{out}");
    assert!(!out.contains("unsafe-eval"), "{out}");
    assert!(out.contains("default-src 'none'"), "{out}");
    assert!(out.contains("nonce-r4nd0mBase64urlValue00"), "{out}");
}

#[test]
fn a_head_request_omits_the_body_but_still_states_its_length() {
    let r = Response::text("0123456789".to_owned());
    let head = String::from_utf8(r.render(Surface::Control, nonce(), Method::Head)).unwrap();
    let body = String::from_utf8(r.render(Surface::Control, nonce(), Method::Get)).unwrap();

    assert!(head.contains("Content-Length: 10"), "{head}");
    assert!(head.ends_with("\r\n\r\n"), "a HEAD carries no body");
    assert!(body.ends_with("0123456789"));
}

#[test]
fn the_body_length_is_counted_in_bytes_rather_than_characters() {
    // A panel carries °C and an em dash. Counting characters would understate the
    // length and truncate the last bytes on the wire, on exactly the responses
    // that say something about temperature.
    let r = Response::text("45 °C — warm\n".to_owned());
    let out = String::from_utf8(r.render(Surface::Control, nonce(), Method::Get)).unwrap();
    assert!(
        out.contains(&format!("Content-Length: {}", "45 °C — warm\n".len())),
        "{out}"
    );
    assert!(out.contains("Content-Length: 16"), "{out}");
}

// ── The published site ────────────────────────────────────────────────────────

const SITE: &str = "/srv/site";

fn site_host() -> FakeHost {
    FakeHost::new()
        .with_dir(SITE)
        .with_file(&format!("{SITE}/index.html"), "<h1>hello</h1>")
        .with_file(&format!("{SITE}/.env"), "TOKEN=shouldnotleak")
}

fn site_root(host: &FakeHost) -> SiteRoot {
    SiteRoot::open(host, SITE).expect("the fixture creates it")
}

/// Reads whatever the fake host holds, as bytes.
fn reader(host: &FakeHost) -> impl Fn(&str) -> Option<Vec<u8>> + '_ {
    move |p: &str| crate::host::Host::read(host, p).map(String::into_bytes)
}

#[test]
fn a_published_file_is_served_with_its_declared_type() {
    let host = site_host();
    let r = route_site(
        &get("/"),
        &site_root(&host),
        &host,
        Availability::Serving,
        &reader(&host),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.content_type, "text/html; charset=utf-8");
    assert_eq!(text(&r), "<h1>hello</h1>");
}

#[test]
fn a_site_response_carries_the_site_policy_and_not_the_panels() {
    // The two surfaces differ in exactly one directive. If they ever stop
    // differing, either the panel has been weakened or the operator's own
    // scripts have stopped running, and both are worth failing a build over.
    let host = site_host();
    let r = route_site(
        &get("/"),
        &site_root(&host),
        &host,
        Availability::Serving,
        &reader(&host),
    );
    let out = String::from_utf8(r.render(Surface::Site, nonce(), Method::Get)).unwrap();
    assert!(out.contains("script-src 'self'"), "{out}");
    assert!(
        !out.contains("nonce-"),
        "a site does not get the panel's nonce: {out}"
    );
    assert!(!out.contains("unsafe-inline"), "{out}");
    assert!(out.contains("default-src 'none'"), "{out}");
    assert!(out.contains("frame-ancestors 'none'"), "{out}");

    let panel =
        String::from_utf8(route(&get("/"), "panel").render(Surface::Control, nonce(), Method::Get))
            .unwrap();
    assert!(panel.contains("script-src 'nonce-"), "{panel}");
    assert!(!panel.contains("script-src 'self'"), "{panel}");
}

#[test]
fn a_withheld_site_refuses_before_it_resolves_anything() {
    // The status must not depend on whether the path exists. A 404 for a missing
    // file and a 503 for a real one is a directory map, served by a device that
    // is refusing to serve.
    let host = site_host();
    for path in ["/", "/index.html", "/definitely-not-here"] {
        let r = route_site(
            &get(path),
            &site_root(&host),
            &host,
            Availability::of(Level::Protect, Stage::Serving),
            &reader(&host),
        );
        assert_eq!(r.status, 503, "{path}");
        assert!(text(&r).contains("PROTECT"), "{path}: {}", text(&r));
    }
}

#[test]
fn a_site_withheld_by_the_outage_ladder_says_which_rung() {
    let host = site_host();
    let r = route_site(
        &get("/"),
        &site_root(&host),
        &host,
        Availability::of(Level::Normal, Stage::Shed),
        &reader(&host),
    );
    assert_eq!(r.status, 503);
    assert!(
        text(&r).contains("stopped non-essential services"),
        "{}",
        text(&r)
    );
}

#[test]
fn a_hidden_file_is_never_served_and_its_contents_never_appear() {
    let host = site_host();
    let r = route_site(
        &get("/.env"),
        &site_root(&host),
        &host,
        Availability::Serving,
        &reader(&host),
    );
    assert_eq!(r.status, 404);
    assert!(!text(&r).contains("shouldnotleak"), "{}", text(&r));
}

#[test]
fn a_file_that_resolved_but_cannot_be_read_answers_exactly_like_a_typo() {
    // This started as a 500, which read as the helpful answer and was a leak: a
    // stranger could tell "this path exists but I cannot have it" from "this
    // path does not exist", one probe at a time. The operator gets the reason in
    // the log on the device they own; the wire gets the same 404 either way.
    let host = site_host();
    let unreadable = route_site(
        &get("/"),
        &site_root(&host),
        &host,
        Availability::Serving,
        &|_| None,
    );
    let missing = route_site(
        &get("/definitely-not-here"),
        &site_root(&host),
        &host,
        Availability::Serving,
        &reader(&host),
    );
    assert_eq!(unreadable.status, 404);
    assert_eq!(missing.status, 404);
    assert_eq!(
        unreadable.status, missing.status,
        "the two must be indistinguishable to a stranger"
    );
}

#[test]
fn a_site_serves_bytes_that_are_not_text() {
    // The reason Response::body is Vec<u8>. A PNG is not valid UTF-8 and a
    // String body would have made the type system demand that it were.
    let host = site_host().with_file(&format!("{SITE}/logo.png"), "placeholder");
    let bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe];
    let r = route_site(
        &get("/logo.png"),
        &site_root(&host),
        &host,
        Availability::Serving,
        &|_| Some(bytes.clone()),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.content_type, "image/png");
    assert_eq!(r.body, bytes);

    let out = r.render(Surface::Site, nonce(), Method::Get);
    assert!(
        out.ends_with(&bytes[..]),
        "the bytes must survive rendering"
    );
    assert!(String::from_utf8(out).is_err(), "this response is not text");
}

// ── The vault route ───────────────────────────────────────────────────────────

const VDIR: &str = "/data/vault";

fn vault_host() -> FakeHost {
    FakeHost::new().with_dir(VDIR)
}

fn a_secret() -> String {
    "A".repeat(vayucell_core_secret_chars())
}
fn vayucell_core_secret_chars() -> usize {
    crate::auth::SECRET_CHARS
}

fn enrolled() -> crate::auth::Credentials {
    crate::auth::Credentials::new(vec![crate::auth::Credential {
        device: crate::auth::DeviceName::new("laptop").expect("plain"),
        secret: crate::auth::Secret::new(&a_secret()).expect("minted"),
    }])
}

fn bearer(secret: &str) -> crate::serve::Headers {
    crate::serve::parse_headers(&[&format!("Authorization: Bearer {secret}")]).expect("valid")
}

fn put(path: &str) -> Request {
    Request {
        method: Method::Put,
        path: path.to_owned(),
    }
}

struct Ctx {
    creds: crate::auth::Credentials,
    root: crate::vault::VaultRoot,
}

fn ctx(host: &FakeHost) -> Ctx {
    Ctx {
        creds: enrolled(),
        root: crate::vault::VaultRoot::open(host, VDIR).expect("the fixture creates it"),
    }
}

fn context(c: &Ctx, level: Level, stage: Stage) -> crate::serve::VaultContext<'_> {
    crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: Some(crate::vault::Quota::new(0, 1_000_000)),
        level,
        stage,
    }
}

/// A filesystem the tests describe rather than inhabit.
struct FakeIo {
    stored: std::cell::RefCell<std::collections::BTreeMap<String, Vec<u8>>>,
    write_fails: Option<crate::serve::StorageFailure>,
    seen_plan: std::cell::RefCell<Option<(String, String)>>,
}

impl FakeIo {
    fn new() -> Self {
        Self {
            stored: std::cell::RefCell::new(std::collections::BTreeMap::new()),
            write_fails: None,
            seen_plan: std::cell::RefCell::new(None),
        }
    }
    fn with(self, path: &str, bytes: &[u8]) -> Self {
        self.stored
            .borrow_mut()
            .insert(path.to_owned(), bytes.to_vec());
        self
    }
    fn failing(mut self, why: &str) -> Self {
        self.write_fails = Some(crate::serve::StorageFailure::Failed(why.to_owned()));
        self
    }

    fn conflicting(mut self, why: &str) -> Self {
        self.write_fails = Some(crate::serve::StorageFailure::Conflict(why.to_owned()));
        self
    }
}

impl crate::serve::VaultIo for FakeIo {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.stored.borrow().get(path).cloned()
    }
    fn write(
        &self,
        plan: &crate::vault::WritePlan,
        bytes: &[u8],
    ) -> Result<(), crate::serve::StorageFailure> {
        *self.seen_plan.borrow_mut() =
            Some((plan.temporary().to_owned(), plan.destination().to_owned()));
        if let Some(failure) = &self.write_fails {
            return Err(failure.clone());
        }
        self.stored
            .borrow_mut()
            .insert(plan.destination().to_owned(), bytes.to_vec());
        Ok(())
    }
    fn remove(&self, path: &str) -> Result<bool, crate::serve::StorageFailure> {
        Ok(self.stored.borrow_mut().remove(path).is_some())
    }
}

/// An io that panics on every call, for the paths that must not reach it.
struct NeverIo;
impl crate::serve::VaultIo for NeverIo {
    fn read(&self, _: &str) -> Option<Vec<u8>> {
        panic!("the filesystem must not be touched")
    }
    fn write(
        &self,
        _: &crate::vault::WritePlan,
        _: &[u8],
    ) -> Result<(), crate::serve::StorageFailure> {
        panic!("the filesystem must not be touched")
    }
    fn remove(&self, _: &str) -> Result<bool, crate::serve::StorageFailure> {
        panic!("the filesystem must not be touched")
    }
}

#[test]
fn an_unauthenticated_put_is_refused_before_anything_else_is_looked_at() {
    // The order of the checks is the security property. Checking the name first
    // tells a stranger which filenames are acceptable; checking the device first
    // tells them the battery level of a phone that is none of their business.
    let host = vault_host();
    let c = ctx(&host);
    let no_creds = crate::serve::parse_headers(&[]).expect("valid");

    // A halted device, a full disk and an unacceptable name, all at once. The
    // answer must still be 401 and nothing else.
    let hostile = crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: Some(crate::vault::Quota::new(1000, 1000)),
        level: Level::Halt,
        stage: Stage::ShuttingDown,
    };
    let r = crate::serve::route_vault(
        &put("/../etc/passwd"),
        &no_creds,
        &hostile,
        b"x",
        &FakeIo::new(),
    );
    assert_eq!(r.status, 401);
    let body = text(&r);
    assert!(!body.contains("HALT"), "the device state leaked: {body}");
    assert!(!body.contains("passwd"), "the path was echoed: {body}");
    assert!(
        !body.to_lowercase().contains("full"),
        "the disk leaked: {body}"
    );
}

#[test]
fn a_wrong_secret_is_refused_and_says_only_that() {
    let host = vault_host();
    let c = ctx(&host);
    let wrong = "B".repeat(crate::auth::SECRET_CHARS);
    let r = crate::serve::route_vault(
        &put("/a.txt"),
        &bearer(&wrong),
        &context(&c, Level::Normal, Stage::Serving),
        b"x",
        &FakeIo::new(),
    );
    assert_eq!(r.status, 401);
    assert!(!text(&r).contains(&wrong), "the offered secret was echoed");
}

#[test]
fn an_enrolled_device_may_store_a_file_and_gets_an_honest_receipt() {
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &put("/report.pdf"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"hello",
        &FakeIo::new(),
    );
    assert_eq!(r.status, 200);
    let body = text(&r);
    assert!(body.contains("report.pdf"), "{body}");
    assert!(body.contains('5'), "{body}");
    for forbidden in ["saved", "safe", "durable"] {
        assert!(!body.to_lowercase().contains(forbidden), "{body}");
    }
}

#[test]
fn the_write_is_handed_the_plan_rather_than_a_path_it_invented() {
    let host = vault_host();
    let c = ctx(&host);
    let io = FakeIo::new();
    let r = crate::serve::route_vault(
        &put("/notes.md"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"abc",
        &io,
    );
    assert_eq!(r.status, 200);
    assert_eq!(
        io.read("/data/vault/notes.md").as_deref(),
        Some(&b"abc"[..]),
        "the bytes reached the writer unchanged"
    );
    let (temporary, destination) = io.seen_plan.into_inner().expect("the writer ran");
    assert_eq!(destination, "/data/vault/notes.md");
    assert_eq!(temporary, "/data/vault/.notes.md.partial");
}

#[test]
fn an_authenticated_put_still_obeys_the_governor() {
    // Authentication is not permission to overheat somebody's phone.
    let host = vault_host();
    let c = ctx(&host);
    for (level, stage) in [
        (Level::Derated, Stage::Serving),
        (Level::Halt, Stage::Serving),
        (Level::Normal, Stage::Announced),
        (Level::Normal, Stage::Shed),
    ] {
        let r = crate::serve::route_vault(
            &put("/a.txt"),
            &bearer(&a_secret()),
            &context(&c, level, stage),
            b"x",
            &NeverIo,
        );
        assert_eq!(r.status, 503, "{level} {stage:?}");
    }
}

#[test]
fn a_file_that_does_not_fit_is_told_apart_from_a_device_that_will_not_take_it() {
    // Both are the operator's problem rather than the caller's mistake, and the
    // caller can tell which from the status without reading prose.
    let host = vault_host();
    let c = ctx(&host);
    let full = crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: Some(crate::vault::Quota::new(1000, 1000)),
        level: Level::Normal,
        stage: Stage::Serving,
    };
    let r = crate::serve::route_vault(
        &put("/a.txt"),
        &bearer(&a_secret()),
        &full,
        b"xxxxx",
        &NeverIo,
    );
    assert_eq!(
        r.status, 507,
        "a full disk is not the same as a halted phone"
    );
}

#[test]
fn a_name_that_is_really_a_path_is_refused_after_authentication() {
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &put("/../../etc/passwd"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"x",
        &NeverIo,
    );
    assert_eq!(r.status, 400);
}

#[test]
fn a_failed_write_is_reported_rather_than_reported_as_stored() {
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &put("/a.txt"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"x",
        &FakeIo::new().failing("the disk went away"),
    );
    assert_eq!(r.status, 500);
    assert!(text(&r).contains("disk went away"), "{}", text(&r));
}

#[test]
fn something_stored_in_the_way_answers_conflict_rather_than_server_error() {
    // 500 said the server had broken. It had not: the request was well formed,
    // the device is fine, and something in the vault needs a person. A caller
    // told 500 retries; a caller told 409 stops and tells somebody, which is the
    // only thing that will ever clear it.
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &put("/a.txt"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"x",
        &FakeIo::new().conflicting("a symbolic link is stored under that name"),
    );
    assert_eq!(r.status, 409);
    assert!(text(&r).contains("symbolic link"), "{}", text(&r));
}

#[test]
fn the_two_storage_failures_answer_with_different_statuses() {
    // Written against the type rather than a route, because the distinction is
    // the type's whole reason for existing and a route test would pass with both
    // arms returning the same number.
    use crate::serve::StorageFailure;
    assert_eq!(
        StorageFailure::Conflict("x".to_owned()).status(),
        (409, "Conflict")
    );
    assert_eq!(
        StorageFailure::Failed("x".to_owned()).status(),
        (500, "Internal Server Error")
    );
    assert_eq!(StorageFailure::Failed("why".to_owned()).told(), "why");
}

#[test]
fn an_enrolled_device_may_read_back_what_it_stored() {
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &get("/a.bin"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"",
        &FakeIo::new().with("/data/vault/a.bin", &[1, 2, 3]),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.body, vec![1, 2, 3]);
}

#[test]
fn a_read_of_something_not_stored_is_a_404() {
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &get("/nope"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"",
        &FakeIo::new(),
    );
    assert_eq!(r.status, 404);
}

#[test]
fn an_empty_store_refuses_every_device_including_a_well_formed_one() {
    // The state every installation begins in, reaching the route.
    let host = vault_host();
    let root = crate::vault::VaultRoot::open(&host, VDIR).expect("fixture");
    let empty = crate::auth::Credentials::empty();
    let c = crate::serve::VaultContext {
        credentials: &empty,
        root: &root,
        quota: Some(crate::vault::Quota::new(0, 1_000_000)),
        level: Level::Normal,
        stage: Stage::Serving,
    };
    let r = crate::serve::route_vault(&put("/a.txt"), &bearer(&a_secret()), &c, b"x", &NeverIo);
    assert_eq!(r.status, 401);
    assert!(text(&r).contains("no device is enrolled"), "{}", text(&r));
}

// ── Headers ───────────────────────────────────────────────────────────────────

#[test]
fn a_bearer_credential_is_read_whatever_the_case_of_the_field_name() {
    // A client sending `authorization:` in lower case is using a library, not
    // mounting an attack.
    for line in [
        "Authorization: Bearer abc",
        "authorization: Bearer abc",
        "AUTHORIZATION: bearer abc",
    ] {
        let h = crate::serve::parse_headers(&[line]).expect("valid");
        assert_eq!(h.bearer(), Some("abc"), "{line}");
    }
}

#[test]
fn a_scheme_this_does_not_implement_reads_as_nothing_presented() {
    // Distinguishing "wrong scheme" from "no header" in the response would tell
    // an unauthenticated stranger which schemes exist.
    for line in [
        "Authorization: Basic dXNlcjpwYXNz",
        "Authorization: Bearer",
        "Authorization: Bearer ",
        "Authorization: ",
    ] {
        let h = crate::serve::parse_headers(&[line]).expect("valid");
        assert_eq!(h.bearer(), None, "{line}");
    }
}

#[test]
fn a_body_larger_than_the_limit_is_refused_before_a_byte_of_it_is_read() {
    // Refusing after reading it is the exhaustion the limit exists to prevent.
    let too_big = crate::serve::MAX_BODY + 1;
    let e = crate::serve::parse_headers(&[&format!("Content-Length: {too_big}")])
        .expect_err("over the limit");
    assert_eq!(e, BadRequest::BodyTooLarge(too_big));
    assert_eq!(refuse(&e).status, 413);

    let ok = crate::serve::parse_headers(&[&format!("Content-Length: {}", crate::serve::MAX_BODY)])
        .expect("exactly the limit");
    assert_eq!(ok.content_length(), Some(crate::serve::MAX_BODY));
}

#[test]
fn a_content_length_that_is_not_a_number_is_refused() {
    for value in ["abc", "-1", "1.5", ""] {
        let e = crate::serve::parse_headers(&[&format!("Content-Length: {value}")])
            .expect_err("{value}");
        assert_eq!(e, BadRequest::MalformedHeader, "{value}");
    }
}

#[test]
fn more_headers_than_the_limit_are_refused() {
    let lines: Vec<String> = (0..=crate::serve::MAX_HEADERS)
        .map(|i| format!("X-Filler-{i}: x"))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(
        crate::serve::parse_headers(&refs).unwrap_err(),
        BadRequest::TooManyHeaders
    );
}

#[test]
fn a_header_line_with_no_colon_is_refused() {
    assert_eq!(
        crate::serve::parse_headers(&["not a header"]).unwrap_err(),
        BadRequest::MalformedHeader
    );
}

#[test]
fn headers_no_route_reads_are_discarded_rather_than_stored() {
    // A map of arbitrary headers is a thing that ends up being logged.
    let h = crate::serve::parse_headers(&[
        "User-Agent: something/1.0",
        "Cookie: session=secret",
        "Authorization: Bearer abc",
    ])
    .expect("valid");
    assert_eq!(h.bearer(), Some("abc"));
    assert_eq!(h.content_length(), None);
    assert!(!format!("{h:?}").contains("session"), "{h:?}");
}

#[test]
fn put_is_parsed_and_is_the_only_verb_that_writes() {
    let r = parse_request_line("PUT /a.txt HTTP/1.1").expect("PUT parses");
    assert_eq!(r.method, Method::Put);
    assert!(Method::Put.writes());
    assert!(!Method::Get.writes());
    assert!(!Method::Head.writes());
}

#[test]
fn post_has_no_meaning_here_and_is_not_implemented() {
    // DELETE arrived with its own decision, which is what this test used to
    // require. POST did not, and will not while nothing is appended to.
    for verb in ["POST", "PATCH"] {
        assert!(matches!(
            parse_request_line(&format!("{verb} /a.txt HTTP/1.1")),
            Err(BadRequest::UnsupportedMethod(_))
        ));
    }
}

// ── Deleting ──────────────────────────────────────────────────────────────────

fn delete(path: &str) -> Request {
    Request {
        method: Method::Delete,
        path: path.to_owned(),
    }
}

#[test]
fn an_enrolled_device_may_remove_what_it_stored() {
    let host = vault_host();
    let c = ctx(&host);
    let io = FakeIo::new().with("/data/vault/a.txt", b"gone soon");
    let r = crate::serve::route_vault(
        &delete("/a.txt"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"",
        &io,
    );
    assert_eq!(r.status, 200);
    assert!(io.read("/data/vault/a.txt").is_none(), "it is still there");
}

#[test]
fn deleting_something_already_gone_is_a_404_rather_than_an_error() {
    // A retry after a dropped connection lands exactly here, and it is the
    // outcome the caller wanted either way.
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &delete("/never.txt"),
        &bearer(&a_secret()),
        &context(&c, Level::Normal, Stage::Serving),
        b"",
        &FakeIo::new(),
    );
    assert_eq!(r.status, 404);
}

#[test]
fn an_unauthenticated_delete_never_reaches_the_filesystem() {
    let host = vault_host();
    let c = ctx(&host);
    let r = crate::serve::route_vault(
        &delete("/a.txt"),
        &crate::serve::parse_headers(&[]).expect("valid"),
        &context(&c, Level::Normal, Stage::Serving),
        b"",
        &NeverIo,
    );
    assert_eq!(r.status, 401);
}

#[test]
fn a_delete_obeys_the_governor_exactly_as_a_write_does() {
    // Removing a file is a change, and a device in trouble is not the place to
    // be making changes to somebody's data.
    let host = vault_host();
    let c = ctx(&host);
    for (level, stage) in [
        (Level::Derated, Stage::Serving),
        (Level::Halt, Stage::Serving),
        (Level::Normal, Stage::Announced),
    ] {
        let r = crate::serve::route_vault(
            &delete("/a.txt"),
            &bearer(&a_secret()),
            &context(&c, level, stage),
            b"",
            &NeverIo,
        );
        assert_eq!(r.status, 503, "{level} {stage:?}");
    }
}

#[test]
fn a_full_disk_never_refuses_the_request_that_would_free_some() {
    // The perverse case: refusing a delete because there is no room is refusing
    // the one thing that would make room. It falls out of offering zero bytes
    // rather than being special-cased, and this is the test that says so.
    let host = vault_host();
    let c = ctx(&host);
    let full = crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: Some(crate::vault::Quota::new(1000, 1000)),
        level: Level::Normal,
        stage: Stage::Serving,
    };
    let io = FakeIo::new().with("/data/vault/big.bin", b"takes up room");
    let r = crate::serve::route_vault(&delete("/big.bin"), &bearer(&a_secret()), &full, b"", &io);
    assert_eq!(
        r.status, 200,
        "a full disk refused the delete that would free it"
    );
    assert!(io.read("/data/vault/big.bin").is_none());
}

#[test]
fn a_read_is_withheld_at_protect_and_below_exactly_as_the_site_is() {
    // ADR-0009 §2's table gives the vault the site's read column, and for a
    // while the code gave it no column at all: a PROTECT device handed files
    // out while refusing to serve a web page from the same cell.
    let host = vault_host();
    let c = ctx(&host);
    let io = FakeIo::new().with("/data/vault/a.txt", b"stored");

    for (level, stage) in [
        (Level::Protect, Stage::Serving),
        (Level::Halt, Stage::Serving),
        (Level::Normal, Stage::Shed),
        (Level::Normal, Stage::ShuttingDown),
    ] {
        let r = crate::serve::route_vault(
            &get("/a.txt"),
            &bearer(&a_secret()),
            &context(&c, level, stage),
            b"",
            &io,
        );
        assert_eq!(r.status, 503, "{level} {stage:?} handed the file out");
        assert!(
            !text(&r).contains("this site"),
            "somebody asking for their file was told about a website: {}",
            text(&r)
        );
    }
}

#[test]
fn a_read_still_answers_where_a_write_would_not() {
    // The asymmetry is the point, and a read that was refused everywhere a write
    // is would make the two columns one. DERATED is heat, and handing back a
    // file somebody already stored is not what is producing it.
    let host = vault_host();
    let c = ctx(&host);
    let io = FakeIo::new().with("/data/vault/a.txt", b"stored");

    for (level, stage) in [
        (Level::Derated, Stage::Serving),
        (Level::Normal, Stage::Announced),
    ] {
        let read = crate::serve::route_vault(
            &get("/a.txt"),
            &bearer(&a_secret()),
            &context(&c, level, stage),
            b"",
            &io,
        );
        assert_eq!(read.status, 200, "{level} {stage:?} withheld a read");

        let write = crate::serve::route_vault(
            &put("/b.txt"),
            &bearer(&a_secret()),
            &context(&c, level, stage),
            b"x",
            &io,
        );
        assert_eq!(write.status, 503, "{level} {stage:?} took a write");
    }
}

#[test]
fn a_withheld_read_never_reaches_the_disk() {
    // Not a filter over an answer that was already fetched. On a device in
    // trouble the point is that the storage is not spun up at all.
    let host = vault_host();
    let c = ctx(&host);
    let io = FakeIo::new().with("/data/vault/a.txt", b"stored");
    let r = crate::serve::route_vault(
        &get("/a.txt"),
        &bearer(&a_secret()),
        &context(&c, Level::Halt, Stage::Serving),
        b"",
        &io,
    );
    assert_eq!(r.status, 503);
    assert!(
        !r.body.windows(6).any(|w| w == b"stored"),
        "the file was read and then withheld"
    );
}

#[test]
fn a_vault_that_could_not_be_measured_refuses_the_upload_with_503_not_507() {
    // 507 asserts that the storage is insufficient, which is a measurement. The
    // whole content of this refusal is that no measurement exists, so it must
    // not borrow the status that claims one.
    let host = vault_host();
    let c = ctx(&host);
    let unmeasured = crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: None,
        level: Level::Normal,
        stage: Stage::Serving,
    };
    let io = FakeIo::new();
    let r = crate::serve::route_vault(&put("/a.txt"), &bearer(&a_secret()), &unmeasured, b"x", &io);
    assert_eq!(r.status, 503, "an unreadable vault admitted a write");
    assert!(
        io.read("/data/vault/a.txt").is_none(),
        "the refusal did not stop the write"
    );
    let said = text(&r);
    assert!(said.contains("could not be read"), "{said}");
}

#[test]
fn a_vault_that_could_not_be_measured_still_allows_a_delete() {
    // The same reasoning as the full one, and worse: a directory whose usage
    // cannot be read is a directory somebody needs to be able to empty.
    let host = vault_host();
    let c = ctx(&host);
    let unmeasured = crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: None,
        level: Level::Normal,
        stage: Stage::Serving,
    };
    let io = FakeIo::new().with("/data/vault/big.bin", b"takes up room");
    let r = crate::serve::route_vault(
        &delete("/big.bin"),
        &bearer(&a_secret()),
        &unmeasured,
        b"",
        &io,
    );
    assert_eq!(r.status, 200, "an unreadable vault refused a delete");
    assert!(io.read("/data/vault/big.bin").is_none());
}

#[test]
fn a_full_vault_says_insufficient_storage_rather_than_service_unavailable() {
    // The status line is read by machines that never see the prose. 507 with a
    // reason phrase of "Service Unavailable" is two different answers in one
    // line, and the wrong one is the one most clients parse.
    let host = vault_host();
    let c = ctx(&host);
    let full = crate::serve::VaultContext {
        credentials: &c.creds,
        root: &c.root,
        quota: Some(crate::vault::Quota::new(1000, 1000)),
        level: Level::Normal,
        stage: Stage::Serving,
    };
    let r = crate::serve::route_vault(
        &put("/a.txt"),
        &bearer(&a_secret()),
        &full,
        b"x",
        &FakeIo::new(),
    );
    assert_eq!(r.status, 507);
    let line = String::from_utf8(r.render(Surface::Site, nonce(), Method::Put)).unwrap();
    assert!(
        line.starts_with("HTTP/1.1 507 Insufficient Storage\r\n"),
        "{}",
        line.lines().next().unwrap_or_default()
    );
}

#[test]
fn only_head_omits_the_body_and_every_other_verb_carries_it() {
    // Written as `method == Method::Get` when Get and Head were the only verbs,
    // this silently swallowed the body of every PUT the moment PUT existed: an
    // upload confirmed nothing and a 400 explained nothing. The question that
    // survives a new verb is which one *omits* a body, not which one carries it.
    let r = Response::text("the receipt\n".to_owned());
    for method in [Method::Get, Method::Put] {
        let out = String::from_utf8(r.render(Surface::Site, nonce(), method)).unwrap();
        assert!(out.ends_with("the receipt\n"), "{method:?} lost its body");
    }
    let head = String::from_utf8(r.render(Surface::Site, nonce(), Method::Head)).unwrap();
    assert!(head.ends_with("\r\n\r\n"), "HEAD must carry no body");
    assert!(head.contains("Content-Length: 12"), "{head}");
}
