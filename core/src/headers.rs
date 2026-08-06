// SPDX-License-Identifier: Apache-2.0

//! Response security headers, per ADR-0006 §3.
//!
//! # Why a set and not a header
//!
//! A Content Security Policy is the loudest of these and the least complete. It
//! does nothing about a MIME type the browser decides to sniff, a referrer
//! leaking a session path to another origin, a page kept in a shared browsing
//! context group with an attacker's, or a device permission the page never
//! needed and was granted anyway. Each of those has its own header, each is one
//! line, and each is forgotten separately.
//!
//! So they are not separate lines here. [`SecurityHeaders`] emits the whole set
//! or none of it, and the ones with no legitimate weaker value are **not
//! configurable at all** — `X-Content-Type-Options` is always `nosniff` because
//! there is no case in this project where content sniffing is wanted, and a knob
//! for it would only ever be turned the wrong way.
//!
//! # What is unrepresentable
//!
//! The same technique the registry and the policy builder use.
//!
//! [`Referrer`] has no `unsafe-url` variant and no `no-referrer-when-downgrade`
//! variant. Both leak a full URL cross-origin, both are still common defaults,
//! and neither can be written down here:
//!
//! ```compile_fail
//! use vayucell_core::headers::Referrer;
//! let r = Referrer::UnsafeUrl;
//! ```
//!
//! A positive control, so the proof above is known to fail for the reason
//! claimed rather than because the import is wrong:
//!
//! ```
//! use vayucell_core::headers::Referrer;
//! assert_eq!(Referrer::SameOrigin.token(), "same-origin");
//! ```
//!
//! # Report-only
//!
//! `Content-Security-Policy-Report-Only` is genuinely useful while a policy is
//! being tightened, and it is the single most dangerous thing in this module: a
//! report-only header on a production build enforces **nothing** while looking
//! identical to an enforcing one in every log, every screenshot and every audit
//! that greps for the header name.
//!
//! So [`Mode::ReportOnly`] carries a reason, and [`SecurityHeaders::production`]
//! refuses it outright. Choosing it is possible; choosing it by accident is not.

use crate::csp::{Nonce, Policy};

/// How the Content Security Policy is delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// The browser blocks violations. The only mode a release may ship.
    Enforce,
    /// The browser reports violations and blocks nothing. Carries the reason it
    /// was chosen, because a value this dangerous should cost a sentence.
    ReportOnly(String),
}

impl Mode {
    /// The header name this mode is delivered under.
    #[must_use]
    pub fn header_name(&self) -> &'static str {
        match self {
            Mode::Enforce => "Content-Security-Policy",
            Mode::ReportOnly(_) => "Content-Security-Policy-Report-Only",
        }
    }

    /// Whether this mode actually blocks anything.
    #[must_use]
    pub fn enforces(&self) -> bool {
        matches!(self, Mode::Enforce)
    }
}

/// Referrer policies this project will send.
///
/// The leaky values are absent by construction — see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Referrer {
    /// Send nothing, ever.
    None_,
    /// Send the full URL to the same origin, nothing cross-origin.
    SameOrigin,
    /// Same origin gets the full URL; cross-origin gets the origin only, and
    /// only when the target is at least as secure.
    StrictOriginWhenCrossOrigin,
}

impl Referrer {
    /// The header value.
    #[must_use]
    pub fn token(&self) -> &'static str {
        match self {
            Referrer::None_ => "no-referrer",
            Referrer::SameOrigin => "same-origin",
            Referrer::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
        }
    }
}

/// HTTP Strict Transport Security.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hsts {
    max_age_secs: u32,
    include_subdomains: bool,
}

/// Why an HSTS policy was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HstsError {
    /// Below [`Hsts::MIN_MAX_AGE`].
    MaxAgeTooShort,
}

impl core::fmt::Display for HstsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("HSTS max-age is below 180 days, which is not a commitment")
    }
}

impl Hsts {
    /// 180 days, the shortest max-age this project will send.
    ///
    /// A short max-age is worse than useless: it reads as an HSTS deployment in
    /// every scan while leaving a window in which a downgrade still works. The
    /// header is a promise about the future, and a promise measured in hours is
    /// not one.
    pub const MIN_MAX_AGE: u32 = 60 * 60 * 24 * 180;

    /// Builds a policy.
    ///
    /// # Errors
    ///
    /// Returns [`HstsError::MaxAgeTooShort`] below [`Hsts::MIN_MAX_AGE`].
    pub fn new(max_age_secs: u32, include_subdomains: bool) -> Result<Self, HstsError> {
        if max_age_secs < Self::MIN_MAX_AGE {
            return Err(HstsError::MaxAgeTooShort);
        }
        Ok(Self {
            max_age_secs,
            include_subdomains,
        })
    }

    /// One year, covering subdomains — what a release ships.
    ///
    /// A `const` rather than a checked call, so the production set has no
    /// runtime failure path at all. The assertion below proves it satisfies
    /// [`Hsts::MIN_MAX_AGE`] at compile time: if someone lowers this value below
    /// the floor, the crate stops building rather than shipping a token
    /// promise. The guarantee lives where forgetting it does not compile.
    pub const ONE_YEAR: Self = Self {
        max_age_secs: 60 * 60 * 24 * 365,
        include_subdomains: true,
    };

    /// The header value.
    #[must_use]
    pub fn value(&self) -> String {
        let mut v = format!("max-age={}", self.max_age_secs);
        if self.include_subdomains {
            v.push_str("; includeSubDomains");
        }
        v
    }
}

const _: () = assert!(
    Hsts::ONE_YEAR.max_age_secs >= Hsts::MIN_MAX_AGE,
    "Hsts::ONE_YEAR must satisfy the minimum max-age it is shipped under"
);

/// The full set of response security headers.
///
/// Emitted together or not at all. A caller cannot take the CSP and leave the
/// rest, which is what happens whenever these are seven independent lines in a
/// handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityHeaders {
    policy: Policy,
    mode: Mode,
    referrer: Referrer,
    hsts: Option<Hsts>,
}

impl SecurityHeaders {
    /// The set a release ships: enforcing, no referrer, HSTS for a year with
    /// subdomains.
    ///
    /// HSTS is `Some` here rather than optional-by-default because the device
    /// serves over TLS and a downgrade is the cheapest attack available on a
    /// home network.
    #[must_use]
    pub fn production(policy: Policy) -> Self {
        Self {
            policy,
            mode: Mode::Enforce,
            referrer: Referrer::None_,
            hsts: Some(Hsts::ONE_YEAR),
        }
    }

    /// A set for local development, where the policy is being tightened and TLS
    /// may not be present.
    ///
    /// Takes the reason for report-only as a parameter so it appears in the
    /// call, not in a config file nobody reads.
    #[must_use]
    pub fn developing(policy: Policy, why: impl Into<String>) -> Self {
        Self {
            policy,
            mode: Mode::ReportOnly(why.into()),
            referrer: Referrer::SameOrigin,
            hsts: None,
        }
    }

    /// Overrides the referrer policy.
    #[must_use]
    pub fn with_referrer(mut self, referrer: Referrer) -> Self {
        self.referrer = referrer;
        self
    }

    /// Whether this set actually blocks policy violations.
    #[must_use]
    pub fn enforces(&self) -> bool {
        self.mode.enforces()
    }

    /// The headers, as ordered name/value pairs.
    ///
    /// Consumes a [`Nonce`], because rendering the policy consumes one and a
    /// nonce is valid for exactly one response.
    #[must_use]
    pub fn render(&self, nonce: Nonce) -> Vec<(&'static str, String)> {
        let mut out = vec![(self.mode.header_name(), self.policy.render(nonce))];

        // Not configurable. There is no case in this project where letting the
        // browser guess a content type is wanted, and a knob for it would only
        // ever be turned the wrong way.
        out.push(("X-Content-Type-Options", "nosniff".to_owned()));

        // frame-ancestors in the CSP is the modern control and supersedes this,
        // but the WebView on an abandoned vendor Android build may predate it.
        // Sending both costs one line and covers the browser we cannot choose.
        out.push(("X-Frame-Options", "DENY".to_owned()));

        out.push(("Referrer-Policy", self.referrer.token().to_owned()));

        // Deny by enumeration rather than by omission: an unlisted feature is
        // governed by the browser's default, and defaults change in our absence.
        out.push((
            "Permissions-Policy",
            [
                "accelerometer",
                "autoplay",
                "camera",
                "display-capture",
                "encrypted-media",
                "geolocation",
                "gyroscope",
                "magnetometer",
                "microphone",
                "midi",
                "payment",
                "usb",
                "xr-spatial-tracking",
            ]
            .map(|f| format!("{f}=()"))
            .join(", "),
        ));

        // Isolate the browsing context so a cross-origin opener cannot reach
        // this document, and so embedded content has to opt in.
        out.push(("Cross-Origin-Opener-Policy", "same-origin".to_owned()));
        out.push(("Cross-Origin-Resource-Policy", "same-origin".to_owned()));
        out.push(("Cross-Origin-Embedder-Policy", "require-corp".to_owned()));

        if let Some(h) = &self.hsts {
            out.push(("Strict-Transport-Security", h.value()));
        }

        out
    }
}
