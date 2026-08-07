// SPDX-License-Identifier: Apache-2.0

//! Serving tests, in the attacker's voice.
//!
//! Here the attacker is a person. Everything below is something a client can
//! send that a browser never would.

use crate::csp::Nonce;
use crate::serve::{
    parse_request_line, refuse, route, BadRequest, Method, Request, Response, MAX_REQUEST_LINE,
};

fn nonce() -> Nonce {
    Nonce::new("r4nd0mBase64urlValue00").expect("a strong enough nonce")
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
fn only_the_two_read_verbs_are_implemented() {
    // Method has no Post, Put or Delete variant, so a route that mutated
    // something could not be written without first widening the enum in a diff
    // somebody has to approve.
    assert_eq!(
        parse_request_line("HEAD / HTTP/1.1").unwrap().method,
        Method::Head
    );
    for verb in ["POST", "PUT", "DELETE", "PATCH", "TRACE", "CONNECT"] {
        let e = parse_request_line(&format!("{verb} / HTTP/1.1"))
            .expect_err(&format!("{verb} must be refused"));
        assert_eq!(e, BadRequest::UnsupportedMethod(verb.to_owned()));
        assert_eq!(refuse(&e).status, 405);
    }
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
        assert!(r.body.contains("BATTERY SAFETY"), "{p}");
    }
}

#[test]
fn the_health_path_does_not_restate_the_devices_condition() {
    // A health endpoint that reported a level would be a second place the
    // device's state is described, and the second place is the one that goes
    // stale — then disagrees with the panel, in the reassuring direction.
    let r = route(&get("/health"), "BATTERY SAFETY: UNSAFE\n");
    assert_eq!(r.status, 200);
    assert!(!r.body.contains("UNSAFE"), "{}", r.body);
    assert!(r.body.contains("read /panel"), "{}", r.body);
}

#[test]
fn an_unknown_path_is_a_404_and_not_a_redirect_to_the_panel() {
    let r = route(&get("/wp-admin"), "panel");
    assert_eq!(r.status, 404);
    assert!(!r.body.contains("panel\n"));
}

// ── Every response carries the posture ────────────────────────────────────────

#[test]
fn even_a_404_carries_the_full_security_posture() {
    // A 404 served without a CSP is still a page a browser will execute script
    // in, and error paths are where headers get dropped because the happy path
    // is the one anybody checks.
    let out = String::from_utf8(route(&get("/nope"), "p").render(nonce(), Method::Get)).unwrap();
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
    let out = String::from_utf8(refuse(&bad).render(nonce(), Method::Get)).unwrap();
    assert!(out.starts_with("HTTP/1.1 405"));
    assert!(out.contains("Content-Security-Policy:"));
}

#[test]
fn the_policy_served_permits_no_inline_script() {
    // The whole point of csp.rs reaching a socket. Until now these headers had
    // never been sent to anything.
    let out = String::from_utf8(route(&get("/"), "panel").render(nonce(), Method::Get)).unwrap();
    assert!(!out.contains("unsafe-inline"), "{out}");
    assert!(!out.contains("unsafe-eval"), "{out}");
    assert!(out.contains("default-src 'none'"), "{out}");
    assert!(out.contains("nonce-r4nd0mBase64urlValue00"), "{out}");
}

#[test]
fn a_head_request_omits_the_body_but_still_states_its_length() {
    let r = Response::text("0123456789".to_owned());
    let head = String::from_utf8(r.render(nonce(), Method::Head)).unwrap();
    let body = String::from_utf8(r.render(nonce(), Method::Get)).unwrap();

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
    let out = String::from_utf8(r.render(nonce(), Method::Get)).unwrap();
    assert!(
        out.contains(&format!("Content-Length: {}", "45 °C — warm\n".len())),
        "{out}"
    );
    assert!(out.contains("Content-Length: 16"), "{out}");
}
