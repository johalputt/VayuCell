// SPDX-License-Identifier: Apache-2.0

//! CSP tests, in the attacker's voice.
//!
//! Note what is NOT here: there is no test for "a policy with `'unsafe-inline'`
//! is refused", because such a policy cannot be written down. That proof is a
//! `compile_fail` doctest on the `csp` module, where rustdoc collects and runs
//! it — on a private item it would run zero tests and report success.

use crate::csp::{allowed_origin, control_surface, Nonce, NonceError, Policy, PolicyError, Source};

fn nonce() -> Nonce {
    Nonce::new("r4nd0mBase64urlValue00").expect("fixture nonce must be valid")
}

#[test]
fn the_baseline_denies_everything_it_was_not_asked_about() {
    // The attack: rely on default-src 'self' and forget a directive. Whatever
    // was forgotten inherits same-origin permission, and the policy's coverage
    // becomes a question of what the author remembered on the day.
    let rendered = Policy::locked_down().render(nonce());
    assert!(
        rendered.contains("default-src 'none'"),
        "the baseline must deny by default, got {rendered}"
    );
    assert!(rendered.contains("object-src 'none'"));
}

#[test]
fn the_control_surface_never_permits_inline_or_eval() {
    // The rule the whole module exists for. It cannot be violated through the
    // API, so this asserts the rendered output as a second, independent check —
    // a belt on top of the type system's braces.
    let rendered = control_surface().render(nonce());
    for forbidden in ["unsafe-inline", "unsafe-eval", "unsafe-hashes"] {
        assert!(
            !rendered.contains(forbidden),
            "{forbidden} appeared in the rendered policy: {rendered}"
        );
    }
}

#[test]
fn script_may_run_only_with_the_per_response_nonce() {
    let rendered = control_surface().render(nonce());
    assert!(
        rendered.contains("script-src 'nonce-r4nd0mBase64urlValue00'"),
        "script-src must be nonce-locked, got {rendered}"
    );
    // 'self' on script-src would let any file the attacker can write to the
    // device's own origin execute, which is the entire value of the nonce.
    assert!(
        !rendered.contains("script-src 'self'"),
        "script-src must not fall back to 'self': {rendered}"
    );
}

#[test]
fn a_passive_source_cannot_be_smuggled_onto_an_executable_directive() {
    // The attack: script-src data:. It reads like a small concession for an
    // inline image and it permits any script an injection can spell.
    for directive in ["script-src", "default-src", "worker-src", "object-src"] {
        for source in [Source::Data, Source::Https] {
            let err = Policy::locked_down()
                .allow(directive, &[source])
                .expect_err("a passive source on an executable directive must be refused");
            assert!(matches!(
                err,
                PolicyError::PassiveSourceOnExecutableDirective { .. }
            ));
        }
    }
}

#[test]
fn the_same_passive_source_is_allowed_where_it_cannot_execute() {
    // The corollary, so the rule above is strict rather than simply blocking
    // everything — a rule that refuses all uses would be replaced by the first
    // person who needs an inline image.
    let p = Policy::locked_down()
        .allow("img-src", &[Source::Own, Source::Data])
        .expect("data: is passive and img-src cannot execute");
    assert!(p.render(nonce()).contains("img-src 'self' data:"));
}

#[test]
fn an_origin_outside_the_closed_allowlist_is_refused() {
    // The attack: a config file, a theme, or a bug widening the policy to an
    // origin nobody reviewed.
    let err = Policy::locked_down()
        .allow("img-src", &[Source::Origin("https://example.invalid")])
        .expect_err("an unlisted origin must be refused");
    assert!(matches!(err, PolicyError::OriginNotAllowed(_)));
    assert!(
        !allowed_origin("https://example.invalid"),
        "the allowlist must not admit an arbitrary origin"
    );
}

#[test]
fn violation_reports_never_leave_the_device() {
    // CHARTER.md Article V.2 and V.5. A report endpoint on someone else's host
    // is telemetry arriving through a side door: it describes what the
    // operator's own pages tried to do, on a schedule the operator never chose.
    for remote in [
        "https://collector.example",
        "//collector.example/report",
        "http://192.0.2.1/csp",
    ] {
        let err = Policy::locked_down()
            .report_to(remote)
            .expect_err("a remote report endpoint must be refused");
        assert!(
            matches!(err, PolicyError::ReportEndpointNotLocal(_)),
            "{remote}"
        );
    }
    assert!(control_surface()
        .render(nonce())
        .contains("report-uri /csp-report"));
}

#[test]
fn a_weak_nonce_is_refused_rather_than_rendered() {
    // A nonce shorter than 128 bits is guessable, and a guessed nonce makes
    // script-src 'nonce-…' exactly as strong as 'unsafe-inline' while still
    // reading as a strict policy in every audit that only looks at the header.
    assert_eq!(Nonce::new("short").unwrap_err(), NonceError::TooShort);
    assert_eq!(
        Nonce::new("a".repeat(21)).unwrap_err(),
        NonceError::TooShort
    );
    Nonce::new("a".repeat(22)).expect("22 characters is 128 bits and must be accepted");
}

#[test]
fn a_nonce_cannot_carry_a_character_that_escapes_the_directive() {
    // The attack: a nonce containing a quote or a semicolon closes the directive
    // and appends attacker-chosen policy to the rest of the header.
    for hostile in [
        "aaaaaaaaaaaaaaaaaaaaaa'; script-src *",
        "aaaaaaaaaaaaaaaaaaaaaa;",
        "aaaaaaaaaaaaaaaaaaaaaa 'self'",
        "aaaaaaaaaaaaaaaaaaaaaa\"",
    ] {
        assert_eq!(
            Nonce::new(hostile).unwrap_err(),
            NonceError::IllegalCharacter,
            "{hostile:?} must not be accepted as a nonce"
        );
    }
}

#[test]
fn allowing_a_source_clears_the_none_that_was_there() {
    // A browser treats "img-src 'none' 'self'" as unparseable and drops the
    // whole directive — a policy that reads strict and enforces nothing. This is
    // the failure mode with no symptom until someone tests it.
    //
    // Note the two sources. An earlier version of this test passed only
    // Source::Own, which never reaches the branch that strips 'none' — so it
    // asserted a property that was true for the wrong reason and would have gone
    // on passing with the guard deleted. The mutation gate is what found that.
    let rendered = Policy::locked_down()
        .allow("img-src", &[Source::Nothing, Source::Own])
        .expect("a passive directive accepts both")
        .render(nonce());

    assert!(
        rendered.contains("img-src 'self'"),
        "the real source must survive: {rendered}"
    );
    assert!(
        !rendered.contains("'none' 'self'") && !rendered.contains("'self' 'none'"),
        "'none' must not survive beside a real source: {rendered}"
    );

    // And the directive that was NOT touched keeps its 'none', or the rule above
    // could be satisfied by stripping 'none' everywhere.
    assert!(
        rendered.contains("object-src 'none'"),
        "an untouched directive must keep denying: {rendered}"
    );
}

#[test]
fn a_page_cannot_be_framed_or_have_its_base_rewritten() {
    // Neither is blocked by any script directive, and both turn a read-only
    // injection into a working attack: framing enables clickjacking, and an
    // injected <base> rewrites the URL every nonce'd script is fetched from.
    let rendered = control_surface().render(nonce());
    assert!(rendered.contains("frame-ancestors 'none'"), "{rendered}");
    assert!(rendered.contains("base-uri 'none'"), "{rendered}");
    assert!(rendered.contains("form-action 'self'"), "{rendered}");
}

#[test]
fn the_rendered_policy_is_deterministic() {
    // Two identical policies must produce byte-identical headers, or the
    // reproducible-build and diff-review arguments do not hold for anything
    // built on top of this.
    let a = control_surface().render(nonce());
    let b = control_surface().render(nonce());
    assert_eq!(a, b);
}

#[test]
fn every_directive_the_control_surface_needs_is_stated_explicitly() {
    // With default-src 'none' a forgotten directive fails closed, which is
    // right — and means the set of directives actually named is the policy's
    // real surface. This pins it so a deletion is visible.
    let rendered = control_surface().render(nonce());
    for directive in [
        "default-src",
        "script-src",
        "style-src",
        "img-src",
        "font-src",
        "connect-src",
        "frame-ancestors",
        "base-uri",
        "form-action",
        "object-src",
    ] {
        assert!(
            rendered.contains(directive),
            "{directive} is missing from the control surface policy: {rendered}"
        );
    }
}
