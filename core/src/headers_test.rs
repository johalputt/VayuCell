// SPDX-License-Identifier: Apache-2.0

//! Security-header tests, in the attacker's voice.
//!
//! Note what is NOT here: there is no test for "a leaky referrer policy is
//! refused", because `Referrer::UnsafeUrl` cannot be written down. That proof is
//! a `compile_fail` doctest on the `headers` module, where rustdoc collects and
//! runs it — on a private item it would run zero tests and report success.

use crate::csp::{control_surface, Nonce};
use crate::headers::{Hsts, HstsError, Mode, Referrer, SecurityHeaders};

fn nonce() -> Nonce {
    Nonce::new("r4nd0mBase64urlValue00").expect("fixture nonce must be valid")
}

fn produced() -> Vec<(&'static str, String)> {
    SecurityHeaders::production(control_surface()).render(nonce())
}

fn value_of(name: &str) -> String {
    produced()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("{name} was not emitted"))
        .1
}

#[test]
fn the_production_set_enforces_rather_than_reports() {
    // The attack this forecloses is not an attack at all, which is why it is
    // dangerous: somebody debugging a policy switches to report-only, and the
    // header still appears in every log and every audit that greps for the
    // name, enforcing nothing.
    let h = SecurityHeaders::production(control_surface());
    assert!(h.enforces());
    assert!(
        produced()
            .iter()
            .any(|(n, _)| *n == "Content-Security-Policy"),
        "the production set must send the enforcing header"
    );
    assert!(
        !produced()
            .iter()
            .any(|(n, _)| *n == "Content-Security-Policy-Report-Only"),
        "the production set must never send the report-only header"
    );
}

#[test]
fn report_only_has_to_state_why_it_was_chosen() {
    // Not a formality. A value this dangerous should cost a sentence, and the
    // sentence lives at the call site rather than in a config file nobody reads.
    let h = SecurityHeaders::developing(control_surface(), "tightening img-src");
    assert!(!h.enforces(), "report-only must not claim to enforce");
    assert_eq!(
        h.render(nonce())[0].0,
        "Content-Security-Policy-Report-Only",
        "report-only must be delivered under its own header name"
    );
    assert!(matches!(
        Mode::ReportOnly("x".to_owned()),
        Mode::ReportOnly(_)
    ));
}

#[test]
fn every_header_in_the_set_is_emitted_together() {
    // The failure this catches is the ordinary one: seven independent lines in a
    // handler, six of which get copied to the next handler.
    let got: Vec<&str> = produced().iter().map(|(n, _)| *n).collect();
    for required in [
        "Content-Security-Policy",
        "X-Content-Type-Options",
        "X-Frame-Options",
        "Referrer-Policy",
        "Permissions-Policy",
        "Cross-Origin-Opener-Policy",
        "Cross-Origin-Resource-Policy",
        "Cross-Origin-Embedder-Policy",
        "Strict-Transport-Security",
    ] {
        assert!(
            got.contains(&required),
            "{required} is missing from {got:?}"
        );
    }
}

#[test]
fn content_sniffing_is_never_permitted() {
    // Deliberately not configurable. There is no case in this project where
    // letting the browser guess a content type is wanted, and a knob for it
    // would only ever be turned the wrong way.
    assert_eq!(value_of("X-Content-Type-Options"), "nosniff");
}

#[test]
fn the_page_is_refused_to_framers_by_two_independent_mechanisms() {
    // frame-ancestors is the modern control, but the WebView on an abandoned
    // vendor Android build may predate it. Both are sent because the browser is
    // the one thing here nobody gets to choose.
    assert_eq!(value_of("X-Frame-Options"), "DENY");
    assert!(
        value_of("Content-Security-Policy").contains("frame-ancestors 'none'"),
        "the CSP must also refuse framing"
    );
}

#[test]
fn the_referrer_never_leaks_a_path_to_another_origin() {
    assert_eq!(value_of("Referrer-Policy"), "no-referrer");
    for r in [
        Referrer::None_,
        Referrer::SameOrigin,
        Referrer::StrictOriginWhenCrossOrigin,
    ] {
        // Every representable value is one that does not send a full URL
        // cross-origin. That is the whole point of the enum being closed.
        assert!(
            !r.token().contains("unsafe") && r.token() != "no-referrer-when-downgrade",
            "{} leaks cross-origin",
            r.token()
        );
    }
}

#[test]
fn device_permissions_are_denied_by_enumeration_not_by_omission() {
    // An unlisted feature is governed by the browser's default, and defaults
    // change without asking us. On a device that has a camera, a microphone and
    // a location, that is not a theoretical difference.
    let p = value_of("Permissions-Policy");
    for feature in ["camera", "microphone", "geolocation", "usb", "payment"] {
        assert!(
            p.contains(&format!("{feature}=()")),
            "{feature} is not denied in {p}"
        );
    }
}

#[test]
fn the_browsing_context_is_isolated() {
    assert_eq!(value_of("Cross-Origin-Opener-Policy"), "same-origin");
    assert_eq!(value_of("Cross-Origin-Resource-Policy"), "same-origin");
    assert_eq!(value_of("Cross-Origin-Embedder-Policy"), "require-corp");
}

#[test]
fn a_token_hsts_max_age_is_refused_rather_than_sent() {
    // A short max-age reads as an HSTS deployment in every scan while leaving a
    // window in which a downgrade still works. The header is a promise about the
    // future, and a promise measured in hours is not one.
    assert_eq!(Hsts::new(0, true).unwrap_err(), HstsError::MaxAgeTooShort);
    assert_eq!(
        Hsts::new(3600, true).unwrap_err(),
        HstsError::MaxAgeTooShort
    );
    assert_eq!(
        Hsts::new(Hsts::MIN_MAX_AGE - 1, true).unwrap_err(),
        HstsError::MaxAgeTooShort
    );
    Hsts::new(Hsts::MIN_MAX_AGE, true).expect("the minimum itself must be accepted");
}

#[test]
fn production_hsts_covers_subdomains_and_lasts_a_year() {
    let v = value_of("Strict-Transport-Security");
    assert!(v.contains("includeSubDomains"), "{v}");
    assert!(v.contains("max-age=31536000"), "{v}");
}

#[test]
fn development_sends_no_hsts_because_it_cannot_honour_it() {
    // Pinning HTTPS from a machine that is serving plain HTTP locks the
    // developer out of their own device, and the lockout outlives the mistake.
    let h = SecurityHeaders::developing(control_surface(), "local, no TLS");
    assert!(
        !h.render(nonce())
            .iter()
            .any(|(n, _)| *n == "Strict-Transport-Security"),
        "development must not pin HTTPS"
    );
}

#[test]
fn the_set_is_deterministic() {
    // Two identical sets must produce byte-identical headers, or nothing built
    // on top of this can be diffed or reproduced.
    let a = SecurityHeaders::production(control_surface()).render(nonce());
    let b = SecurityHeaders::production(control_surface()).render(nonce());
    assert_eq!(a, b);
}

#[test]
fn the_production_posture_matches_the_committed_snapshot() {
    // docs/security-posture.txt is the security posture, written down.
    //
    // Every guard in this module and in `csp` tests one property in isolation,
    // which is right and is not enough: a change that weakens the posture while
    // keeping each individual assertion true reads, in a diff, as a small edit
    // to a Rust file. Nobody reviewing it sees the header set change.
    //
    // With the snapshot committed, weakening anything produces a diff in a plain
    // text file that states, in order, exactly what every response will carry.
    // That is a thing a reviewer notices without knowing the codebase.
    //
    // On failure: look at what changed before regenerating. The regeneration
    // command is the easy part and it is not the point.
    //   cargo test --workspace -- --ignored regenerate_the_security_posture
    let expected = include_str!("../../docs/security-posture.txt");
    assert_eq!(
        render_posture(),
        expected,
        "\nThe response security posture has changed.\n\
         Read the diff above and satisfy yourself the change is intended, then \
         regenerate docs/security-posture.txt.\n"
    );
}

/// The posture as a stable, human-readable block.
fn render_posture() -> String {
    let mut out = String::from(
        "# VayuCell response security posture\n\
         #\n\
         # Generated from core/src/headers.rs. Do not edit by hand.\n\
         # Regenerate: cargo test --workspace -- --ignored regenerate_the_security_posture\n\
         #\n\
         # The nonce is a fixed placeholder here; in a response it is a fresh\n\
         # 128-bit value, used once.\n\n",
    );
    for (name, value) in SecurityHeaders::production(control_surface()).render(nonce()) {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(&value);
        out.push('\n');
    }
    out
}

#[test]
#[ignore = "writes docs/security-posture.txt; run deliberately after reviewing a posture change"]
fn regenerate_the_security_posture() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/security-posture.txt");
    std::fs::write(path, render_posture()).expect("could not write the snapshot");
    println!("wrote {path}");
}
