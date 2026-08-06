// SPDX-License-Identifier: Apache-2.0

//! The Observation-and-Control Contract registry described in ADR-0001.
//!
//! Every ability VayuCell might have on a device is a [`Capability`] whose
//! obligations are expressed as types. ADR-0005 §2.1 is the reason this is
//! Rust: two of the rules that needed runtime guards in the first
//! implementation are now facts about the type system.
//!
//! * [`Capability::verify`] is not an [`Option`]. **A capability with no
//!   read-back does not compile.** That is the single most important rule in
//!   the project — a control that cannot be read back after being set is
//!   indistinguishable from one that silently stopped working, and reporting it
//!   would be CHARTER Article IV violated by the reporting mechanism itself.
//!
//! * [`Tier`] has no `Unset` variant. An undetected tier is `None`, which
//!   cannot be compared against a floor at all, so there is no zero value to
//!   defend against.
//!
//! Two rules genuinely need checking and are enforced by [`Capability::check`]:
//! a safety-class capability may not degrade quietly, and only an observe-class
//! capability may omit `apply`.
//!
//! # What cannot be written down
//!
//! These are proofs, not tests: they are compiled by `cargo test` and fail the
//! build if they ever start compiling.
//!
//! A capability with no read-back does not exist:
//!
//! ```compile_fail
//! use vayucell_core::capability::{Capability, Class, Disposition, Observation, ProbeError, Tier};
//! fn d() -> Result<Observation, ProbeError> { unimplemented!() }
//! let _ = Capability {
//!     id: "x", floor: Tier::T0, class: Class::Observe,
//!     detect: d, apply: None,
//!     on_absent: Disposition::Refuse, rationale: "x",
//! };  // error: missing field `verify`
//! ```
//!
//! A read-back cannot be declared absent, which is the invariant itself
//! rather than the generic "no field may be omitted" rule:
//!
//! ```compile_fail
//! use vayucell_core::capability::{Capability, Class, Disposition, Observation, ProbeError, Result_, Tier};
//! fn d() -> Result<Observation, ProbeError> { Ok(Observation::new(Result_::Present, "e")) }
//! let _ = Capability {
//!     id: "x", floor: Tier::T0, class: Class::Observe,
//!     detect: d, apply: None,
//!     verify: None,  // error: expected fn pointer, found `Option<_>`
//!     on_absent: Disposition::Refuse, rationale: "x",
//! };
//! ```
//!
//! There is no unset tier to reach for:
//!
//! ```compile_fail
//! use vayucell_core::capability::Tier;
//! let _t = Tier::Unset;  // error: no variant named `Unset`
//! ```
//!
//! And for contrast, the same capability WITH a read-back compiles:
//!
//! ```
//! use vayucell_core::capability::{Capability, Class, Disposition, Observation, ProbeError, Result_, Tier};
//! fn d() -> Result<Observation, ProbeError> { Ok(Observation::new(Result_::Present, "e")) }
//! let c = Capability {
//!     id: "x", floor: Tier::T0, class: Class::Observe,
//!     detect: d, apply: None, verify: d,
//!     on_absent: Disposition::Refuse, rationale: "x",
//! };
//! assert!(c.check().is_ok());
//! ```

use std::collections::BTreeMap;
use std::fmt;

/// The environment a device provides, per ADR-0001 §1.
///
/// A tier is **detected from positive evidence**, never inferred from a model
/// name or a version number. There is deliberately no `Unset` variant: a device
/// whose tier could not be established is represented as `None` and satisfies
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Stock Android, unprivileged userspace.
    T0,
    /// Stock Android with root available.
    T1,
    /// Virtualised Linux guest (protected KVM).
    T2,
    /// Mainline Linux (postmarketOS-class).
    T3,
}

impl Tier {
    /// Reports whether this tier satisfies a capability whose floor is `min`.
    #[must_use]
    pub fn at_least(self, min: Tier) -> bool {
        self >= min
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Tier::T0 => "T0",
            Tier::T1 => "T1",
            Tier::T2 => "T2",
            Tier::T3 => "T3",
        };
        f.write_str(s)
    }
}

/// Whether a detected tier satisfies a floor.
///
/// An undetected tier (`None`) satisfies nothing. This is a free function
/// rather than a method so the absent case cannot be forgotten at a call site.
#[must_use]
pub fn tier_satisfies(detected: Option<Tier>, floor: Tier) -> bool {
    matches!(detected, Some(t) if t.at_least(floor))
}

/// What kind of thing a capability governs. Decides how strictly absence is
/// treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Battery and thermal controls. Never degrades quietly (CHARTER Art. III).
    Safety,
    /// Ingress and site serving.
    Serving,
    /// Durability and wear.
    Storage,
    /// Reachability and egress.
    Network,
    /// Read-only measurement. The only class that may omit `apply`.
    Observe,
}

impl fmt::Display for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Class::Safety => "safety",
            Class::Serving => "serving",
            Class::Storage => "storage",
            Class::Network => "network",
            Class::Observe => "observe",
        };
        f.write_str(s)
    }
}

/// What happens when a capability is absent on this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Continue with reduced function.
    Degrade,
    /// Refuse the dependent operation.
    Refuse,
}

/// The outcome of detecting or verifying a capability.
///
/// There is deliberately no `Ok` or `Pass` member. [`Result_::Present`] means
/// the mechanism exists; whether it *works* is a separate question answered by
/// verification. [`Result_::Unverified`] is never treated as
/// [`Result_::Absent`], and absence is never treated as protection
/// (CHARTER Article IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::module_name_repetitions)]
pub enum Result_ {
    /// The mechanism was found on this device.
    Present,
    /// The mechanism is genuinely not present.
    Absent,
    /// Could not be established. **Not** the same as `Absent`.
    Unverified,
}

impl fmt::Display for Result_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Result_::Present => "present",
            Result_::Absent => "absent",
            Result_::Unverified => "unverified",
        };
        f.write_str(s)
    }
}

/// A [`Result_`] with the evidence that produced it.
///
/// The evidence is mandatory: a bare verdict cannot be audited, and an operator
/// reading "absent" deserves to know what was looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// What was concluded.
    pub result: Result_,
    /// Why — the node that answered, or what was looked for and not found.
    pub evidence: String,
}

impl Observation {
    /// Builds an observation.
    #[must_use]
    pub fn new(result: Result_, evidence: impl Into<String>) -> Self {
        Self {
            result,
            evidence: evidence.into(),
        }
    }
}

/// Errors a probe may return. Distinct from an [`Observation`]: a failure to
/// *run* the probe is not a finding about the device.
///
/// The string is the reason, for the operator.
#[derive(Debug)]
pub struct ProbeError(pub String);

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "probe failed: {}", self.0)
    }
}

impl std::error::Error for ProbeError {}

/// Establishes whether the mechanism exists on this device.
pub type DetectFn = fn() -> Result<Observation, ProbeError>;

/// Sets the capability to a target value.
pub type ApplyFn = fn(&str) -> Result<(), ProbeError>;

/// Reads the state back from the hardware or kernel.
///
/// Never optional. See the module documentation.
pub type VerifyFn = fn() -> Result<Observation, ProbeError>;

/// One contract. Every field is an obligation, and the struct cannot be built
/// without answering all of them.
#[derive(Debug, Clone)]
pub struct Capability {
    /// Stable identifier, used in reports and the hardware database.
    pub id: &'static str,
    /// Lowest tier that can provide this.
    pub floor: Tier,
    /// What kind of thing this governs.
    pub class: Class,
    /// Is the mechanism here?
    pub detect: DetectFn,
    /// Set it. `None` is permitted **only** for [`Class::Observe`].
    pub apply: Option<ApplyFn>,
    /// Read it back. Not an `Option` — see the module documentation.
    pub verify: VerifyFn,
    /// Degrade, or refuse?
    pub on_absent: Disposition,
    /// Why these answers, in prose, shown to the operator.
    pub rationale: &'static str,
}

/// Why a capability was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    /// The identifier was empty or blank.
    NoId,
    /// The rationale was empty or blank.
    NoRationale,
    /// A controlling capability omitted `apply`.
    NoApply,
    /// A safety capability tried to degrade quietly.
    SafetyDegrades,
    /// The identifier was already registered.
    Duplicate,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Invalid::NoId => "id is empty",
            Invalid::NoRationale => "rationale is empty",
            Invalid::NoApply => {
                "apply is None on a controlling capability (only Class::Observe may omit it)"
            }
            Invalid::SafetyDegrades => {
                "a Class::Safety capability may not use Disposition::Degrade — safety controls \
                 refuse or are reported permanently failing (CHARTER Art. III)"
            }
            Invalid::Duplicate => "already registered",
        };
        f.write_str(s)
    }
}

impl std::error::Error for Invalid {}

impl Capability {
    /// Checks the obligations the compiler cannot.
    ///
    /// The two rules that *did* need runtime guards in the first implementation
    /// — a missing `verify`, and a tier zero value — are gone: the first does
    /// not compile, and the second does not exist.
    ///
    /// # Errors
    /// Returns every way in which the capability is incomplete.
    pub fn check(&self) -> Result<(), Vec<Invalid>> {
        let mut errs = Vec::new();
        if self.id.trim().is_empty() {
            errs.push(Invalid::NoId);
        }
        if self.rationale.trim().is_empty() {
            errs.push(Invalid::NoRationale);
        }
        if self.apply.is_none() && self.class != Class::Observe {
            errs.push(Invalid::NoApply);
        }
        if self.class == Class::Safety && self.on_absent == Disposition::Degrade {
            errs.push(Invalid::SafetyDegrades);
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}

/// Every declared capability.
///
/// A `BTreeMap` so iteration is ordered and reports are deterministic without
/// an explicit sort.
#[derive(Debug, Default)]
pub struct Registry {
    caps: BTreeMap<&'static str, Capability>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            caps: BTreeMap::new(),
        }
    }

    /// Validates and adds a capability.
    ///
    /// # Errors
    /// Returns the reasons the capability was refused.
    pub fn register(&mut self, c: Capability) -> Result<(), Vec<Invalid>> {
        c.check()?;
        if self.caps.contains_key(c.id) {
            return Err(vec![Invalid::Duplicate]);
        }
        self.caps.insert(c.id, c);
        Ok(())
    }

    /// Looks a capability up.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Capability> {
        self.caps.get(id)
    }

    /// How many capabilities are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.caps.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }

    /// Every registered identifier, in sorted order.
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.caps.keys().copied().collect()
    }

    /// Re-checks every registered capability, so one mutated after
    /// registration still fails the build.
    ///
    /// # Errors
    /// Returns each offending identifier with its reasons.
    pub fn validate(&self) -> Result<(), Vec<(&'static str, Vec<Invalid>)>> {
        let mut errs = Vec::new();
        for (id, c) in &self.caps {
            if let Err(e) = c.check() {
                errs.push((*id, e));
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Capabilities this device can provide, given its detected tier.
    ///
    /// An undetected tier yields nothing, per [`tier_satisfies`].
    #[must_use]
    pub fn available_at(&self, detected: Option<Tier>) -> Vec<&'static str> {
        self.caps
            .values()
            .filter(|c| tier_satisfies(detected, c.floor))
            .map(|c| c.id)
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unnecessary_wraps)]
mod internal_tests {
    use super::*;

    fn d() -> Result<Observation, ProbeError> {
        Ok(Observation::new(Result_::Present, "e"))
    }

    #[test]
    fn validate_catches_a_capability_mutated_after_registration() {
        // register() refuses a degrading safety capability, so the only way to
        // get one into the map is to bypass registration -- which a careless
        // refactor could do. validate() is the backstop, and this test has the
        // module-private access needed to prove it works.
        let mut r = Registry::new();
        r.caps.insert(
            "forced.in",
            Capability {
                id: "forced.in",
                floor: Tier::T0,
                class: Class::Safety,
                detect: d,
                apply: None,
                verify: d,
                on_absent: Disposition::Degrade, // never survives register()
                rationale: "inserted without validation",
            },
        );

        let errs = r
            .validate()
            .expect_err("validate must catch a bypassed capability");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].0, "forced.in");
        assert!(
            errs[0].1.contains(&Invalid::SafetyDegrades),
            "got {:?}",
            errs[0].1
        );
    }
}
