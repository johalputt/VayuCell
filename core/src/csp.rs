// SPDX-License-Identifier: Apache-2.0

//! Content Security Policy, per ADR-0006.
//!
//! # Why this is a type and not a string
//!
//! A CSP is usually a string constant, and string constants are edited under
//! deadline. `'unsafe-inline'` is one word, it makes the immediate problem go
//! away, and the page keeps working — so nothing about the change announces that
//! the policy's central guarantee has just been removed. Every review of that
//! diff has to catch it by reading carefully, forever.
//!
//! So the policy is built from [`Source`], and **`Source` has no variant for
//! `'unsafe-inline'` or `'unsafe-eval'`**. Restoring them is not a one-word edit
//! to a literal; it is an addition to a public enum in this file, which is a
//! diff nobody merges by accident. The registry in [`crate::capability`] uses the
//! same technique for the same reason: the guarantee lives in the type, where
//! forgetting it does not compile.
//!
//! ```compile_fail
//! use vayucell_core::csp::Source;
//! // There is no such variant, and adding one is the reviewable act.
//! let s = Source::UnsafeInline;
//! ```
//!
//! ```compile_fail
//! use vayucell_core::csp::Source;
//! let s = Source::UnsafeEval;
//! ```
//!
//! A positive control, so the two proofs above are known to be failing for the
//! reason claimed and not because the import is wrong:
//!
//! ```
//! use vayucell_core::csp::Source;
//! let s = Source::Own;
//! assert_eq!(s.token(), "'self'");
//! ```
//!
//! # Where the reporting endpoint may point
//!
//! CHARTER.md Article V.5: no dependency on a service the project controls. A
//! violation report is a description of what the operator's own pages tried to
//! do, and shipping that to a collector run by this project would be exactly the
//! telemetry Article V.2 forbids, arriving through a side door. The report
//! endpoint is therefore a same-origin path and [`Policy::report_to`] will not
//! accept anything else.

use core::fmt::Write as _;

/// A source expression permitted in a directive.
///
/// The unsafe keywords are absent by construction. See the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// `'none'` — nothing at all. The default for every directive.
    Nothing,
    /// `'self'` — the document's own origin.
    Own,
    /// `'nonce-…'` — the per-response nonce. The only way script may execute.
    Nonce,
    /// `data:` — inline data URIs. Passive image content only; the builder
    /// refuses it on any directive that can execute.
    Data,
    /// `https:` — any origin, provided it is encrypted. Passive content only,
    /// on the same rule as [`Source::Data`].
    Https,
    /// A specific origin from the closed allowlist in [`allowed_origin`].
    Origin(&'static str),
}

impl Source {
    /// The token as it appears in the header.
    #[must_use]
    pub fn token(&self) -> &'static str {
        match self {
            Source::Nothing => "'none'",
            Source::Own => "'self'",
            // Rendered with the nonce substituted; this is the placeholder form.
            Source::Nonce => "'nonce-'",
            Source::Data => "data:",
            Source::Https => "https:",
            Source::Origin(o) => o,
        }
    }

    /// Whether this source can introduce executable code.
    ///
    /// Used to refuse `data:` and `https:` on executable directives, where they
    /// would defeat the policy entirely — `script-src data:` permits any script
    /// an injection can spell.
    fn is_passive_only(&self) -> bool {
        matches!(self, Source::Data | Source::Https)
    }
}

/// The origins this project will ever admit into a policy, and why.
///
/// A closed list rather than an open parameter: a caller that can pass an
/// arbitrary origin is a caller that can widen the policy from a config file, a
/// hosted theme, or a bug. Nothing here is reachable from user input.
///
/// It is currently empty. VayuCell serves its own control surface from the
/// device and has no third-party origin to admit; the function exists so that
/// the first proposal to add one has to arrive as a change to this file.
#[must_use]
pub fn allowed_origin(origin: &str) -> bool {
    const ALLOWED: &[&str] = &[];
    ALLOWED.contains(&origin)
}

/// A per-response nonce.
///
/// Constructed through [`Nonce::new`], which enforces the length and alphabet.
/// The value must come from a cryptographically secure generator and must never
/// be reused across responses — a predictable or repeated nonce makes
/// `script-src 'nonce-…'` no stronger than `'unsafe-inline'`, while still
/// reading like a strict policy in every audit that only looks at the header.
/// Deliberately **not** `Clone`. A nonce is valid for exactly one response, and
/// a type that can be duplicated makes reuse the path of least resistance.
#[derive(Debug, PartialEq, Eq)]
pub struct Nonce(String);

/// Why a nonce was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceError {
    /// Fewer than [`Nonce::MIN_LEN`] characters.
    TooShort,
    /// Contains a character outside the base64url alphabet. A nonce carrying a
    /// quote or a semicolon could terminate the directive and rewrite the rest
    /// of the policy.
    IllegalCharacter,
}

impl core::fmt::Display for NonceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            NonceError::TooShort => "nonce is shorter than 22 characters (128 bits)",
            NonceError::IllegalCharacter => "nonce contains a character outside base64url",
        })
    }
}

impl Nonce {
    /// The shortest accepted nonce: 22 base64url characters, which is 128 bits.
    ///
    /// The CSP specification asks for at least 128 bits of entropy. Length is
    /// the only part of that this type can check — it cannot tell a random value
    /// from a counter — so the documentation carries the rest of the obligation
    /// and [`Nonce::new`] enforces the half that is checkable rather than
    /// pretending to enforce both.
    pub const MIN_LEN: usize = 22;

    /// Validates and wraps a nonce.
    ///
    /// # Errors
    ///
    /// Returns [`NonceError`] if the value is too short or contains a character
    /// that could break out of the directive.
    pub fn new(value: impl Into<String>) -> Result<Self, NonceError> {
        let value = value.into();
        if value.len() < Self::MIN_LEN {
            return Err(NonceError::TooShort);
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(NonceError::IllegalCharacter);
        }
        Ok(Self(value))
    }

    /// The nonce value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A directive and the sources permitted for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Directive {
    name: &'static str,
    sources: Vec<Source>,
    executable: bool,
}

/// Why a policy was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// A passive-only source was placed on a directive that can execute code.
    PassiveSourceOnExecutableDirective {
        /// The directive that was being built.
        directive: &'static str,
        /// The source that was refused.
        source: Source,
    },
    /// An origin outside the closed allowlist.
    OriginNotAllowed(String),
    /// The reporting endpoint was not a same-origin absolute path.
    ReportEndpointNotLocal(String),
}

impl core::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PolicyError::PassiveSourceOnExecutableDirective { directive, source } => write!(
                f,
                "{} may not be used on {directive}, which can execute code",
                source.token()
            ),
            PolicyError::OriginNotAllowed(o) => {
                write!(f, "origin {o} is not in the closed allowlist")
            }
            PolicyError::ReportEndpointNotLocal(e) => write!(
                f,
                "report endpoint {e} is not a same-origin absolute path; violation \
                 reports describe the operator's own pages and never leave the device"
            ),
        }
    }
}

/// A Content Security Policy under construction.
///
/// Starts from a deny-everything baseline. Every permission is an explicit
/// addition, so a directive nobody thought about is `'none'` rather than
/// whatever the browser defaults to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    directives: Vec<Directive>,
    report_to: Option<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self::locked_down()
    }
}

impl Policy {
    /// The baseline: everything denied.
    ///
    /// `default-src 'none'` rather than `'self'`. With `'self'` a directive
    /// nobody enumerated silently inherits same-origin permission, and the
    /// policy's coverage becomes a question of what the author remembered. With
    /// `'none'` a forgotten directive fails closed and shows up as a broken page
    /// during development rather than as an open door in production.
    #[must_use]
    pub fn locked_down() -> Self {
        Self {
            directives: vec![
                Directive {
                    name: "default-src",
                    sources: vec![Source::Nothing],
                    executable: true,
                },
                // A page that cannot be framed cannot be clickjacked. The device
                // control surface is never legitimately embedded.
                Directive {
                    name: "frame-ancestors",
                    sources: vec![Source::Nothing],
                    executable: false,
                },
                // Without this, an injected <base> rewrites every relative URL on
                // the page — including the ones a nonce'd script is loaded from.
                Directive {
                    name: "base-uri",
                    sources: vec![Source::Nothing],
                    executable: false,
                },
                // An injected form that posts the operator's session somewhere
                // else is not blocked by any script directive.
                Directive {
                    name: "form-action",
                    sources: vec![Source::Own],
                    executable: false,
                },
                Directive {
                    name: "object-src",
                    sources: vec![Source::Nothing],
                    executable: true,
                },
            ],
            report_to: None,
        }
    }

    /// Permits `sources` for `directive`.
    ///
    /// # Errors
    ///
    /// Refuses a passive-only source on an executable directive, and refuses an
    /// origin outside the closed allowlist.
    pub fn allow(
        mut self,
        directive: &'static str,
        sources: &[Source],
    ) -> Result<Self, PolicyError> {
        let executable = matches!(
            directive,
            "default-src" | "script-src" | "script-src-elem" | "worker-src" | "object-src"
        );

        for s in sources {
            if executable && s.is_passive_only() {
                return Err(PolicyError::PassiveSourceOnExecutableDirective {
                    directive,
                    source: *s,
                });
            }
            if let Source::Origin(o) = s {
                if !allowed_origin(o) {
                    return Err(PolicyError::OriginNotAllowed((*o).to_owned()));
                }
            }
        }

        let mut sources = sources.to_vec();
        sources.sort_unstable();
        sources.dedup();

        // A directive that permits something is no longer denying everything.
        // Leaving 'none' beside a real source is not merely untidy: browsers
        // treat the whole directive as unparseable, which is a policy that reads
        // strict and enforces nothing.
        if sources.len() > 1 {
            sources.retain(|s| *s != Source::Nothing);
        }

        self.directives.retain(|d| d.name != directive);
        self.directives.push(Directive {
            name: directive,
            sources,
            executable,
        });
        self.directives.sort_by(|a, b| a.name.cmp(b.name));
        Ok(self)
    }

    /// Sets the violation-reporting endpoint.
    ///
    /// # Errors
    ///
    /// Refuses anything that is not a same-origin absolute path. CHARTER.md
    /// Article V.2 and V.5: a violation report describes what the operator's own
    /// pages tried to do, and it does not leave the device.
    pub fn report_to(mut self, endpoint: &str) -> Result<Self, PolicyError> {
        if !endpoint.starts_with('/') || endpoint.starts_with("//") {
            return Err(PolicyError::ReportEndpointNotLocal(endpoint.to_owned()));
        }
        self.report_to = Some(endpoint.to_owned());
        Ok(self)
    }

    /// Renders the header value for one response.
    ///
    /// Consumes the nonce, because a nonce is good for exactly one response.
    /// [`Nonce`] is not `Clone`, so this is not a stylistic preference: reusing
    /// one requires generating another, and a reused nonce is a strict-looking
    /// policy that has stopped being strict.
    ///
    /// ```compile_fail
    /// use vayucell_core::csp::{control_surface, Nonce};
    /// let n = Nonce::new("r4nd0mBase64urlValue00").unwrap();
    /// let policy = control_surface();
    /// let _first = policy.render(n);
    /// // The nonce was moved into the first response and cannot serve a second.
    /// let _second = policy.render(n);
    /// ```
    #[must_use]
    pub fn render(&self, nonce: Nonce) -> String {
        // Moved out rather than borrowed, so the nonce is genuinely consumed.
        let nonce = nonce.0;
        let mut out = String::new();
        for (i, d) in self.directives.iter().enumerate() {
            if i > 0 {
                out.push_str("; ");
            }
            out.push_str(d.name);
            for s in &d.sources {
                out.push(' ');
                match s {
                    Source::Nonce => {
                        // Infallible: writing to a String cannot fail.
                        let _ = write!(out, "'nonce-{nonce}'");
                    }
                    other => out.push_str(other.token()),
                }
            }
        }
        if let Some(r) = &self.report_to {
            let _ = write!(out, "; report-uri {r}");
        }
        out
    }
}

/// The policy for the device's own control surface.
///
/// Scripts run only with the per-response nonce. Styles and fonts come from the
/// device. Images additionally admit `data:` so a locally generated chart or QR
/// image does not require a second request — passive content on a non-executable
/// directive, which the builder enforces rather than trusting.
///
/// # Panics
///
/// Does not panic in practice: every source used here is accepted by
/// [`Policy::allow`], and the test suite asserts it. The `expect` calls make a
/// future edit that violates a rule fail loudly at startup rather than silently
/// shipping a weaker policy.
#[must_use]
pub fn control_surface() -> Policy {
    Policy::locked_down()
        .allow("script-src", &[Source::Nonce])
        .expect("nonce is valid on script-src")
        .allow("style-src", &[Source::Own])
        .expect("self is valid on style-src")
        .allow("img-src", &[Source::Own, Source::Data])
        .expect("img-src is not executable")
        .allow("font-src", &[Source::Own])
        .expect("self is valid on font-src")
        .allow("connect-src", &[Source::Own])
        .expect("self is valid on connect-src")
        .report_to("/csp-report")
        .expect("a same-origin path is a valid endpoint")
}
