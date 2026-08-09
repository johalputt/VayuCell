// SPDX-License-Identifier: Apache-2.0

//! Sovereign ingress, per ADR-0003.
//!
//! # Every mode declares seven properties, and none of them is optional
//!
//! ADR-0003 §2 replaced a four-field description with seven, because three of the
//! fields it lacked each changed a decision. [`Mode::profile`] returns a
//! [`Profile`] whose every field is required — no `Option`, no default — so a
//! mode added later cannot leave the awkward ones blank. The awkward ones are
//! exactly the fields that matter: who can evict you, whether ordinary browsers
//! can reach you at all, and what happens after a compromise.
//!
//! # The governor wins, by construction
//!
//! ADR-0003 §5 exists because its absence was the design's worst defect: the
//! default ingress mode maximised sustained CPU, sustained CPU is heat, and heat
//! is the ageing this project's flagship subsystem exists to suppress — and
//! neither document mentioned the other. [`shed_for`] is the repair. It takes a
//! [`crate::governor::Level`] and answers what runs, and there is no argument by
//! which a mode outranks the governor.
//!
//! # Verified means a request came from outside and was served
//!
//! Not "the tunnel process is running", not "the address was published", not
//! "the daemon returned success". [`Reachability`] has no variant for any of
//! those, because a loopback test proves nothing about a path whose entire
//! difficulty is external. See [`Reachability::Verified`].
//!
//! # And it stops meaning it
//!
//! ADR-0003 §4 requires the round trip to *re-run on a schedule*, "because the
//! failure that matters is the path that worked for six weeks and then
//! stopped". [`Reachability::Unverified`] has said, in its own documentation,
//! that it is also the state a mode returns to when its check is overdue — and
//! nothing computed overdue. `Verified` carried a free-form string that no code
//! ever compared to a clock, so the type was, exactly and literally, *a
//! verification that never expires*: the thing its own comment named as unable
//! to notice the failure that matters.
//!
//! The repair is [`FRESH_FOR`] and a mandatory argument. There is no way to ask
//! whether a path is verified without saying **when** you are asking —
//! [`Reachability::is_verified`] and [`Reachability::describe`] both take the
//! clock's reading, so a caller cannot leave time out the way every caller of
//! the old signature did. A round trip older than [`FRESH_FOR`] is unverified,
//! and so is one stamped ahead of the clock, because an age that cannot be
//! established is not an age.

use core::time::Duration;

use crate::governor::Level;

/// How a request reaches the device. ADR-0003 §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A Tor v3 onion service.
    Onion,
    /// A tunnel to a rented host that forwards inbound connections.
    Relay,
    /// A port the ISP actually forwards.
    Direct,
    /// The owner's own network, and nothing else. The default.
    LocalOnly,
}

/// Who can take this ingress away from you.
///
/// The distinction the draft of ADR-0003 flattened. It ranked an onion service as
/// having *no* dependency, which is a ruler chosen to flatter the default: an
/// onion depends on reaching the Tor network. That is a different dependency from
/// a rented host, and arguably a much better one — a commons cannot evict you the
/// way a supplier can — but it is not nothing, and calling it nothing is how a
/// comparison table becomes marketing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dependency {
    /// Nothing outside the building.
    None,
    /// A public commons. No single party can evict you.
    Commons,
    /// A supplier, who may stop at will.
    Supplier,
    /// Your ISP continuing to permit inbound connections.
    Carrier,
}

impl Dependency {
    /// Who can end it.
    #[must_use]
    pub const fn who_can_evict_you(self) -> &'static str {
        match self {
            Dependency::None => "nobody, because nothing outside the building is involved",
            Dependency::Commons => "no single party; it is a commons, not a supplier",
            Dependency::Supplier => "the provider, at will and without notice",
            Dependency::Carrier => "your ISP, by changing a policy you were never party to",
        }
    }
}

/// How much sustained power a mode draws — which is heat. ADR-0003 §5.
///
/// Ordered, and the ordering is load-bearing: [`shed_for`] sheds the highest
/// class first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThermalClass {
    /// Negligible sustained work.
    Low,
    /// Some sustained work.
    Moderate,
    /// Continuous cryptographic work: circuit builds, relay crypto.
    High,
}

/// What is left after a compromise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompromiseStory {
    /// Rotate a credential; the address survives.
    Recoverable,
    /// The identity *is* the address. There is no revocation.
    ///
    /// The mode ADR-0003's draft ranked most sovereign has the worst recovery,
    /// and the draft did not mention it. Stealing the ed25519 key means
    /// impersonating the service permanently — no revocation, no authority to
    /// appeal to, and no way for a visitor to notice.
    Permanent,
    /// Nothing is exposed to compromise from outside.
    NotApplicable,
}

/// The seven properties every mode must declare. ADR-0003 §2.
///
/// Every field is required. A `Profile` with `Option` fields would let a mode
/// added later leave the uncomfortable ones blank, and the uncomfortable ones are
/// the three the draft was missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// What it depends on.
    pub dependency: Dependency,
    /// Whether a browser with no special software can reach it.
    pub ordinary_browsers: bool,
    /// Sustained power, and therefore heat.
    pub thermal: ThermalClass,
    /// Whether it survives the device's address changing.
    pub survives_roaming: bool,
    /// What a party in the middle can read.
    pub middle_sees: &'static str,
    /// What recovery looks like.
    pub compromise: CompromiseStory,
    /// Whether reaching it costs money.
    pub costs_money: bool,
}

impl Mode {
    /// This mode's declared properties.
    #[must_use]
    pub const fn profile(self) -> Profile {
        match self {
            Mode::Onion => Profile {
                dependency: Dependency::Commons,
                // RFC 7686: .onion is a reserved special-use name. It is not in
                // DNS and never resolves. "Serve a real site from a drawer with
                // no rented infrastructure" is true about the transport and
                // overstated about the audience, and this field is where that
                // correction lives so no copy can restate the claim.
                ordinary_browsers: false,
                thermal: ThermalClass::High,
                survives_roaming: true,
                middle_sees: "relays carry ciphertext; the rendezvous point learns no identity",
                compromise: CompromiseStory::Permanent,
                costs_money: false,
            },
            Mode::Relay => Profile {
                dependency: Dependency::Supplier,
                ordinary_browsers: true,
                thermal: ThermalClass::Low,
                survives_roaming: true,
                middle_sees: "a relay that terminates TLS can read everything passing through it",
                compromise: CompromiseStory::Recoverable,
                costs_money: true,
            },
            Mode::Direct => Profile {
                dependency: Dependency::Carrier,
                ordinary_browsers: true,
                thermal: ThermalClass::Low,
                survives_roaming: false,
                middle_sees: "nobody in the middle",
                compromise: CompromiseStory::Recoverable,
                costs_money: false,
            },
            Mode::LocalOnly => Profile {
                dependency: Dependency::None,
                ordinary_browsers: true,
                thermal: ThermalClass::Low,
                survives_roaming: true,
                middle_sees: "nothing leaves your network",
                compromise: CompromiseStory::NotApplicable,
                costs_money: false,
            },
        }
    }

    /// Whether choosing this mode publishes the device to the world.
    #[must_use]
    pub const fn publishes(self) -> bool {
        !matches!(self, Mode::LocalOnly)
    }
}

/// The mode a newly installed cell runs. ADR-0003 §3.
///
/// [`Mode::LocalOnly`], and this is a retreat from the ADR's draft rather than an
/// omission. A default that publishes makes an irreversible disclosure decision
/// on the owner's behalf, which Charter Article VIII.5 forbids without explicit
/// confirmation — and it is the only default executable on T0, the tier most
/// retired phones are permanently stuck at.
pub const DEFAULT: Mode = Mode::LocalOnly;

/// What the operator must be shown before choosing a mode. ADR-0003 §5.4.
///
/// Before, not after. A disclosure that arrives once the choice is made is a
/// changelog entry, and the two combinations worth naming — heat on a governed
/// cell, and heat on a cell that cannot hold a ceiling at all — are exactly the
/// ones somebody would otherwise discover months later.
#[must_use]
pub fn disclosures(mode: Mode, can_hold_ceiling: bool) -> Vec<String> {
    let p = mode.profile();
    let mut out = Vec::new();

    if p.thermal == ThermalClass::High {
        out.push(
            "this mode raises sustained temperature on this device, and the \
             battery governor may shed it under thermal load"
                .to_owned(),
        );
        if !can_hold_ceiling {
            // ADR-0003 §5.5: the combination with no mitigation available. Said
            // at the moment of choosing, because afterwards there is nothing to
            // be done about it.
            out.push(
                "this device cannot hold a charge ceiling, so sustained heat from \
                 this mode has no mitigation available on it at all"
                    .to_owned(),
            );
        }
    }
    if !p.ordinary_browsers {
        out.push(
            "a visitor needs a Tor-capable client; this address is not in DNS and \
             will not resolve in an ordinary browser"
                .to_owned(),
        );
    }
    if p.compromise == CompromiseStory::Permanent {
        out.push(
            "the identity key is the address: if it is stolen the impersonation is \
             permanent, there is no revocation, and a visitor cannot tell"
                .to_owned(),
        );
    }
    if p.dependency == Dependency::Supplier {
        out.push(format!(
            "this mode can be ended by {}, and {}",
            p.dependency.who_can_evict_you(),
            p.middle_sees
        ));
    }
    if p.costs_money {
        out.push("this mode costs money to keep running".to_owned());
    }
    out
}

/// Which modes still run at a given governor level. ADR-0003 §5.
///
/// The repair for the defect §0 records: the flagship safety subsystem and the
/// default ingress mode were in direct conflict and nothing in the design
/// noticed. There is no argument by which a mode outranks the governor, so this
/// takes the level and there is no parameter that overrides it.
///
/// - `NORMAL` — everything runs.
/// - `DERATED` — high-thermal ingress is shed **first**, before storage or
///   serving work, because it is the load that is making the device hot.
/// - `PROTECT` and `HALT` — nothing outward-facing runs. Local-only survives,
///   because it is not what is heating the device and stopping it would take the
///   panel away from the person who needs to read it.
#[must_use]
pub fn shed_for(level: Level, running: &[Mode]) -> Vec<Mode> {
    running
        .iter()
        .copied()
        .filter(|m| match level {
            Level::Normal => true,
            Level::Derated => m.profile().thermal < ThermalClass::High,
            Level::Protect | Level::Halt => !m.publishes(),
        })
        .collect()
}

/// How long a completed round trip stands before it must be repeated.
///
/// ADR-0003 §4 requires the check to re-run on a schedule and does not say how
/// often. Short enough that a path which stops working is noticed within one
/// sitting rather than one season, and long enough that the check is not itself
/// the sustained load the governor exists to shed.
///
/// The tests that pin the expiry use literal durations rather than this
/// constant. A test written against the constant it is pinning stays green when
/// the constant is widened to a century, which is the exact change that
/// reintroduces the defect.
pub const FRESH_FOR: Duration = Duration::from_secs(15 * 60);

/// Whether an ingress path has actually been shown to work. ADR-0003 §4.
///
/// There is deliberately no variant for a running process:
///
/// ```compile_fail
/// use vayucell_core::ingress::Reachability;
/// // "The tunnel is up" is not a state this type can express, because it is not
/// // evidence that anything can reach the device.
/// let r: Reachability = Reachability::ProcessRunning;
/// ```
///
/// And there is no way to ask whether a path is verified without saying when
/// you are asking:
///
/// ```compile_fail
/// use core::time::Duration;
/// use vayucell_core::ingress::Reachability;
/// let r = Reachability::Verified { at: Duration::from_secs(0) };
/// let _: bool = r.is_verified();
/// ```
///
/// ADR-0001 makes read-back unwaivable, and for ingress the read-back is a real
/// round trip. A loopback test proves nothing about a path whose entire
/// difficulty is external.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reachability {
    /// A request originating **outside** the device traversed the path and was
    /// served, and the cell observed itself serving it.
    Verified {
        /// The clock's reading when the round trip completed.
        ///
        /// Monotonic, taken from [`crate::runtime::Clock::elapsed`], and
        /// therefore measured from the start of *this* process. That is the
        /// property wanted: a wall-clock stamp survives a restart the observing
        /// process did not, and the cell would go on reporting a round trip
        /// nothing in it ever watched. A restarted cell has verified nothing.
        at: Duration,
    },
    /// A round trip was attempted and did not complete.
    Failed(String),
    /// No round trip stands for this path. The state every mode starts in.
    ///
    /// Also the state a mode returns to when its check is overdue: the failure
    /// that matters is the path that worked for six weeks and then stopped, and
    /// a verification that never expires cannot notice it. [`Reachability::as_of`]
    /// is what makes that sentence true rather than aspirational.
    Unverified(String),
}

impl Reachability {
    /// Whether this path has been shown to work from outside, **as of `now`**.
    ///
    /// `now` is the clock's reading, and it is required. The predicate cannot be
    /// asked without it, which is the whole repair: the previous signature let
    /// every caller treat a round trip from six weeks ago as current, and every
    /// caller did.
    ///
    /// A stamp the clock cannot account for — one ahead of `now` — is not
    /// evidence. It cannot be aged, and something that cannot be checked is
    /// never reported clean.
    ///
    /// Written as a match rather than `matches!` so a variant added later fails
    /// to compile here instead of quietly falling on whichever side its author
    /// did not think about.
    #[must_use]
    pub const fn is_verified(&self, now: Duration) -> bool {
        match self {
            Self::Verified { at } => match now.checked_sub(*at) {
                // `age >= FRESH_FOR` without a const comparison operator: the
                // subtraction saturates to `None` exactly while age is short.
                Some(age) => age.checked_sub(FRESH_FOR).is_none(),
                None => false,
            },
            Self::Failed(_) | Self::Unverified(_) => false,
        }
    }

    /// This standing as it must be reported at `now`.
    ///
    /// A round trip that has aged past [`FRESH_FOR`] becomes
    /// [`Reachability::Unverified`] naming how old it is — not
    /// [`Reachability::Failed`], because nothing failed. Nobody looked.
    #[must_use]
    pub fn as_of(&self, now: Duration) -> Self {
        let Self::Verified { at } = self else {
            return self.clone();
        };
        if self.is_verified(now) {
            return self.clone();
        }
        Self::Unverified(match now.checked_sub(*at) {
            Some(age) => format!(
                "the last round trip completed {} ago and one stands for {}",
                spell(age),
                spell(FRESH_FOR)
            ),
            None => "the last round trip is stamped ahead of this cell's clock, \
                     so its age cannot be established"
                .to_owned(),
        })
    }

    /// How long this standing has left before a check is due.
    ///
    /// `None` when a check is due now — which includes every path that has never
    /// had one. A scheduler asking "when next?" and a panel asking "is it
    /// current?" are then answering out of the same arithmetic rather than each
    /// keeping its own idea of overdue.
    #[must_use]
    pub fn due_in(&self, now: Duration) -> Option<Duration> {
        match self {
            Self::Verified { at } if self.is_verified(now) => {
                FRESH_FOR.checked_sub(now.checked_sub(*at)?)
            }
            _ => None,
        }
    }

    /// What to render, as of `now`. Never "up".
    #[must_use]
    pub fn describe(&self, now: Duration) -> String {
        match self.as_of(now) {
            Self::Verified { at } => format!(
                "a request from outside this device was served {} ago and this \
                 stands for another {}; that is what verified means here",
                spell(now.saturating_sub(at)),
                spell(FRESH_FOR.saturating_sub(now.saturating_sub(at)))
            ),
            Self::Failed(why) => format!("the last round trip did not complete: {why}"),
            Self::Unverified(why) => format!(
                "nothing currently stands as a request from outside ({why}), so \
                 this path is unverified — which is not the same as down, and not \
                 the same as up"
            ),
        }
    }
}

/// A duration in the words an operator reads, rather than in seconds they have
/// to divide.
fn spell(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        return format!("{secs} seconds");
    }
    format!("{} minutes {} seconds", secs / 60, secs % 60)
}
