// SPDX-License-Identifier: Apache-2.0

//! Tier 2: what a guest may claim about the handset that hosts it.
//!
//! # The problem this module exists for
//!
//! PLAN §3 gives tier 2 a shape no other tier has. A tier-2 device is a Linux
//! guest under the Android hypervisor: its own kernel, its own root, its own
//! userspace, running the same VayuCell code as a rooted handset. What it does
//! not have is *the phone*.
//!
//! Three of tier 2's properties belong to the host rather than to the guest:
//!
//! - **Reachability** is host-mediated. Whether a port inside the guest is
//!   reachable from the network is the host's decision.
//! - **Charge control** is host-mediated. The node that limits the real cell
//!   lives in the *host's* kernel. The guest cannot write it and cannot read it
//!   back.
//! - **Doze** is the host's. Whether the guest will be put to sleep is a
//!   property of the Android power manager, and from inside the guest it is
//!   not merely unreadable — it is unknowable, because it is a statement about
//!   the future.
//!
//! PLAN §11 puts the gate on this phase as *"guest survives host reboot;
//! escapes Doze"*. Neither half of that sentence is checkable from inside the
//! guest, and the honest way to write a contract about them is to say so.
//!
//! # What a guest can hold
//!
//! A guest may hold **one kind of fact**: something the host told it, in a
//! particular boot of the host. It cannot hold a fact it read from the cell,
//! because the cell's nodes are not in its namespace; and it cannot hold a
//! fact about the host's future, because the host's future is not observable
//! from inside it.
//!
//! So every fact here carries the boot of the host it was learned in, and
//! [`Ledger::observe`] exists to notice when the host has rebooted — at which
//! point every fact learned in the earlier boot stops holding, because a host
//! reboot resets the host's kernel: the ceiling it was holding is gone, and
//! the Doze exemption it had granted is gone with it.
//!
//! # Why a boot counter and not a timestamp
//!
//! A timestamp says when a fact was made. It does not say whether the thing
//! that made it is still running, and a guest cannot ask: the host is a
//! different kernel in a different address space, and the only thing the guest
//! can do is ask the host. A monotonically increasing boot counter answers
//! exactly the question that matters — *is this the same boot?* — and it
//! answers it without trusting a clock.
//!
//! # The honest limit, stated once
//!
//! A fact here is the host's word. It is not a measurement this software made,
//! and no report may present it as one. [`Standing`] has no variant that says
//! "this guest verified it", and there is nothing to add one to: the cell's
//! nodes are not in the guest's namespace.
//!
//! ```compile_fail
//! use vayucell_core::guest::Standing;
//! // There is no variant that says this guest checked the hardware itself. At
//! // tier 2 the cell's charge node lives in the host's kernel, so there is
//! // nothing here this guest could have read.
//! let s = Standing::Verified;
//! ```
//!
//! And the contract in one page, for a reader who wants to see the whole thing
//! work:
//!
//! ```
//! use vayucell_core::guest::{Attestation, Ledger, Mediated, Standing};
//!
//! let mut ledger = Ledger::new();
//!
//! // The host says which boot of itself this is, and what it has done.
//! let statement = Attestation::parse("epoch=8;ceiling=60;doze=exempt")
//!     .expect("a well-formed statement");
//! ledger.establish(&statement).expect("a current statement");
//!
//! assert!(matches!(
//!     ledger.standing(Mediated::Ceiling),
//!     Standing::Attested { .. }
//! ));
//!
//! // The host reboots. The ceiling it was holding went with it, and the guest
//! // learns that only because the boot counter moved.
//! let after = Attestation::parse("epoch=9").expect("a well-formed statement");
//! ledger.establish(&after).expect("a current statement");
//!
//! assert!(matches!(
//!     ledger.standing(Mediated::Ceiling),
//!     Standing::Void { .. }
//! ));
//! ```

use core::fmt;
use core::fmt::Write as _;
use core::num::NonZeroU64;

use crate::battery::Percent;
use crate::host::Host;

/// The environment variable through which the Android host tells this guest
/// which boot of itself the guest is running under, and what the host has done
/// on the guest's behalf.
///
/// The channel is deliberately local and severable: the host writes it into the
/// guest's environment, the guest reads it, and nothing dials out. The real
/// channel — a signed statement from a host agent over the hypervisor's
/// virtio console — is not built yet, and PLAN §11 keeps that gate open. What
/// is here is the *contract* that channel will have to satisfy, which is the
/// part that can be written down and tested without a handset.
pub const HOST_ATTESTATION_ENV: &str = "VAYUCELL_HOST_ATTESTATION";

/// The first line of a guest record on disk, and the version it is written in.
const LEDGER_HEADER: &str = "vayucell guest ledger 1";

/// A boot of the Android host, counted by the host.
///
/// The counter is the whole point. A guest is destroyed and restarted by every
/// host reboot, so nothing a guest remembers in memory survives one — and
/// everything the *host* was doing for the guest is destroyed by the reboot
/// along with the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HostEpoch(NonZeroU64);

impl HostEpoch {
    /// The first boot of a host.
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    /// Wraps a boot counter, refusing zero.
    ///
    /// Zero is refused because a counter that has never been set reads as zero,
    /// and a zero epoch compares as older than every real boot — so it would
    /// silently void every fact the guest holds, and then read as a statement
    /// about the host rather than as the absence of one.
    #[must_use]
    pub const fn new(counter: u64) -> Option<Self> {
        match NonZeroU64::new(counter) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// The counter.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for HostEpoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.get())
    }
}

/// A control the Android host owns, and a guest can only ask about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mediated {
    /// The charge ceiling on the real cell. The node lives in the host's
    /// kernel; the guest cannot write it and cannot read it back.
    Ceiling,
    /// The host's Doze policy towards this guest.
    Doze,
    /// Whether the host publishes this guest's ingress.
    Reachability,
}

impl Mediated {
    /// Every control, in the order a report lists them.
    pub const ALL: [Self; 3] = [Self::Ceiling, Self::Doze, Self::Reachability];

    /// The name a report prints.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ceiling => "charge ceiling",
            Self::Doze => "doze policy",
            Self::Reachability => "ingress",
        }
    }

    /// The caveat that always travels with a claim about this control.
    ///
    /// Every one of these is a statement about the host, and every one of them
    /// is beyond the guest's reach. Saying so beside the claim — rather than in
    /// a document somewhere — is the difference between an honest report and
    /// one that a reader has to know the architecture to distrust.
    #[must_use]
    pub const fn caveat(self) -> &'static str {
        match self {
            Self::Ceiling => {
                "the cell's charge node lives in the host's kernel, so this guest \
                 cannot read it back"
            }
            Self::Doze => {
                "a guest cannot observe the host's Doze policy from inside, so this \
                 is a promise and not a measurement"
            }
            Self::Reachability => {
                "whether the host publishes this guest's ports is the host's \
                 decision, not something the guest can check"
            }
        }
    }
}

/// A statement the host has made about a control it owns.
///
/// The control is *inside* the claim rather than beside it. The first version
/// of this had a `control` field and a `value` field, and a caller could pair
/// the reachability value with the ceiling control: a compiling, passing,
/// wrong claim — the same defect class as the two transposed booleans in
/// [`crate::tier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// The host holds the real cell at this ceiling.
    Holding(Percent),
    /// The host holds no ceiling on the real cell.
    HoldingNothing,
    /// The host exempts this guest from Doze.
    DozeExempt,
    /// The host does not exempt this guest from Doze.
    DozeApplies,
    /// The host publishes this guest's ingress.
    Published,
    /// The host does not publish this guest's ingress.
    Withheld,
}

impl Claim {
    /// The control this claim is about.
    #[must_use]
    pub const fn control(self) -> Mediated {
        match self {
            Self::Holding(_) | Self::HoldingNothing => Mediated::Ceiling,
            Self::DozeExempt | Self::DozeApplies => Mediated::Doze,
            Self::Published | Self::Withheld => Mediated::Reachability,
        }
    }

    /// The claim as a person reads it.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Holding(percent) => {
                format!("the Android host reports the cell is held at {percent}")
            }
            Self::HoldingNothing => {
                "the Android host reports that it holds no ceiling on the cell".to_owned()
            }
            Self::DozeExempt => {
                "the Android host reports that it exempts this guest from Doze".to_owned()
            }
            Self::DozeApplies => {
                "the Android host reports that Doze applies to this guest".to_owned()
            }
            Self::Published => {
                "the Android host reports that it publishes this guest's ingress".to_owned()
            }
            Self::Withheld => {
                "the Android host reports that it withholds this guest's ingress".to_owned()
            }
        }
    }

    /// The wire form, for the guest's own record on disk.
    #[must_use]
    pub fn wire(self) -> String {
        match self {
            Self::Holding(percent) => format!("holding {}", percent.get()),
            Self::HoldingNothing => "holding nothing".to_owned(),
            Self::DozeExempt => "doze-exempt".to_owned(),
            Self::DozeApplies => "doze-applies".to_owned(),
            Self::Published => "ingress-published".to_owned(),
            Self::Withheld => "ingress-withheld".to_owned(),
        }
    }

    /// Reads the wire form back.
    ///
    /// # Errors
    ///
    /// Returns why the text is not a claim this build knows. A record that
    /// cannot be understood is reported as such rather than skipped: a claim
    /// this build does not understand is a claim nobody may report.
    pub fn from_wire(text: &str) -> Result<Self, String> {
        match text {
            "holding nothing" => Ok(Self::HoldingNothing),
            "doze-exempt" => Ok(Self::DozeExempt),
            "doze-applies" => Ok(Self::DozeApplies),
            "ingress-published" => Ok(Self::Published),
            "ingress-withheld" => Ok(Self::Withheld),
            _ => match text.strip_prefix("holding ") {
                Some(rest) => {
                    let percent: u8 = rest
                        .parse()
                        .map_err(|_| format!("{rest:?} is not a percentage"))?;
                    if percent > 100 {
                        return Err(format!("{percent} is not a percentage"));
                    }
                    Ok(Self::Holding(Percent::clamped(i64::from(percent))))
                }
                None => Err(format!("{text:?} is not a claim this build knows")),
            },
        }
    }
}

/// Why something the guest was told could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The host said nothing at all.
    Absent,
    /// What was said was not in a form this build understands.
    Malformed(String),
    /// What was said made two claims about one control.
    Contradictory(String),
    /// What was said came from an earlier boot of the host.
    Replayed(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => f.write_str("the host has said nothing"),
            Self::Malformed(why) => write!(f, "what the host said could not be read: {why}"),
            Self::Contradictory(why) => {
                write!(f, "what the host said contradicts itself: {why}")
            }
            Self::Replayed(why) => {
                write!(f, "what the host said came from an earlier boot: {why}")
            }
        }
    }
}

/// What learning the host's boot counter did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    /// The guest had not seen the counter before.
    FirstSight,
    /// The host is in the boot the guest already knew about.
    SameBoot,
    /// The host has rebooted since the guest last looked.
    Rebooted,
    /// The counter went backwards, which no host does. Refused.
    Replayed,
}

/// What may be reported about a control the host owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// The host said this in that boot, and the guest read the statement.
    Attested {
        /// What the host said.
        claim: Claim,
        /// The boot of the host it said it in.
        epoch: HostEpoch,
    },
    /// The host has rebooted since. The claim no longer holds.
    Void {
        /// What the host said, in an earlier boot.
        claim: Claim,
        /// The boot it was said in.
        established_in: HostEpoch,
        /// The boot the host is in now.
        host_now: HostEpoch,
    },
    /// Nothing establishes this.
    Unknown,
}

impl Standing {
    /// The label a report prints.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Attested { .. } => "ATTESTED",
            Self::Void { .. } => "VOID",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// The sentence a report prints.
    #[must_use]
    pub fn evidence(&self) -> String {
        match self {
            Self::Attested { claim, epoch } => format!(
                "{} (host boot {epoch}); {}",
                claim.describe(),
                claim.control().caveat()
            ),
            Self::Void {
                claim,
                established_in,
                host_now,
            } => format!(
                "{} in host boot {established_in}, but the host has since rebooted \
                 and is now in boot {host_now}, so it no longer holds",
                claim.describe()
            ),
            Self::Unknown => "the host has said nothing about this".to_owned(),
        }
    }
}

/// What the host has said, in one statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    epoch: HostEpoch,
    claims: Vec<Claim>,
}

impl Attestation {
    /// A statement from the host.
    ///
    /// # Errors
    ///
    /// Two claims about one control are refused: a host that says both "the
    /// cell is held at 60%" and "the host holds no ceiling" has said nothing,
    /// and picking one of them would be this program deciding which.
    pub fn new(epoch: HostEpoch, claims: Vec<Claim>) -> Result<Self, Fault> {
        let mut held: Vec<Claim> = Vec::new();
        for claim in claims {
            push_claim(&mut held, claim)?;
        }
        Ok(Self {
            epoch,
            claims: held,
        })
    }

    /// The boot of the host this statement belongs to.
    #[must_use]
    pub const fn epoch(&self) -> HostEpoch {
        self.epoch
    }

    /// The claims it makes.
    #[must_use]
    pub fn claims(&self) -> &[Claim] {
        &self.claims
    }

    /// The claim about one control, if it made one.
    ///
    /// `None` means the host said nothing about that control, which is not the
    /// same as the host saying something negative about it.
    #[must_use]
    pub fn claim(&self, control: Mediated) -> Option<Claim> {
        self.claims.iter().copied().find(|c| c.control() == control)
    }

    /// Parses a host statement.
    ///
    /// The form is `epoch=8;ceiling=60;doze=exempt;ingress=published`. The boot
    /// counter is required; the three claims are not, because a host that has
    /// said nothing about its Doze policy has not said the guest will be
    /// dozed.
    ///
    /// # Errors
    ///
    /// Returns why the statement could not be read. A statement this build
    /// cannot understand is refused outright rather than partly believed.
    pub fn parse(text: &str) -> Result<Self, Fault> {
        let mut epoch: Option<HostEpoch> = None;
        let mut claims: Vec<Claim> = Vec::new();

        for part in text.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((key, value)) = part.split_once('=') else {
                return Err(Fault::Malformed(format!(
                    "{part:?} is not a key and a value"
                )));
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "epoch" => {
                    if epoch.is_some() {
                        return Err(Fault::Contradictory(
                            "the host stated its boot twice".to_owned(),
                        ));
                    }
                    let counter: u64 = value.parse().map_err(|_| {
                        Fault::Malformed(format!("{value:?} is not a boot counter"))
                    })?;
                    let Some(found) = HostEpoch::new(counter) else {
                        return Err(Fault::Malformed(format!(
                            "boot counter {counter} is not a boot"
                        )));
                    };
                    epoch = Some(found);
                }
                "ceiling" => {
                    let claim = if value == "none" {
                        Claim::HoldingNothing
                    } else {
                        let percent: u8 = value.parse().map_err(|_| {
                            Fault::Malformed(format!("{value:?} is not a percentage"))
                        })?;
                        if percent > 100 {
                            return Err(Fault::Malformed(format!(
                                "ceiling {percent} is not a percentage"
                            )));
                        }
                        Claim::Holding(Percent::clamped(i64::from(percent)))
                    };
                    push_claim(&mut claims, claim)?;
                }
                "doze" => {
                    let claim = match value {
                        "exempt" => Claim::DozeExempt,
                        "applies" => Claim::DozeApplies,
                        other => {
                            return Err(Fault::Malformed(format!(
                                "doze {other:?} is not a policy this build knows"
                            )))
                        }
                    };
                    push_claim(&mut claims, claim)?;
                }
                "ingress" => {
                    let claim = match value {
                        "published" => Claim::Published,
                        "withheld" => Claim::Withheld,
                        other => {
                            return Err(Fault::Malformed(format!(
                                "ingress {other:?} is not a policy this build knows"
                            )))
                        }
                    };
                    push_claim(&mut claims, claim)?;
                }
                other => {
                    return Err(Fault::Malformed(format!(
                        "{other:?} is not a statement this build knows"
                    )))
                }
            }
        }

        let Some(epoch) = epoch else {
            return Err(Fault::Malformed(
                "the host did not say which boot of itself this is".to_owned(),
            ));
        };
        Ok(Self { epoch, claims })
    }
}

/// Adds a claim, refusing a second claim about the same control.
fn push_claim(claims: &mut Vec<Claim>, claim: Claim) -> Result<(), Fault> {
    if claims.iter().any(|held| held.control() == claim.control()) {
        return Err(Fault::Contradictory(format!(
            "two statements about the {}",
            claim.control().name()
        )));
    }
    claims.push(claim);
    Ok(())
}

/// What the guest has established, and the boot of the host each fact belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    observed: Option<HostEpoch>,
    facts: Vec<(Claim, HostEpoch)>,
}

impl Ledger {
    /// An empty record: the guest has established nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records which boot of the host this guest is running under.
    ///
    /// This is where a reboot is noticed. The guest is restarted by every host
    /// reboot and comes back with its memory empty, so the boot counter on disk
    /// is the only thing that can tell it the world has moved on.
    pub fn observe(&mut self, epoch: HostEpoch) -> Observation {
        match self.observed {
            None => {
                self.observed = Some(epoch);
                Observation::FirstSight
            }
            Some(known) if epoch < known => Observation::Replayed,
            Some(known) if epoch == known => Observation::SameBoot,
            Some(_) => {
                self.observed = Some(epoch);
                Observation::Rebooted
            }
        }
    }

    /// The boot the guest last observed.
    #[must_use]
    pub const fn observed_epoch(&self) -> Option<HostEpoch> {
        self.observed
    }

    /// Records what the host has just said.
    ///
    /// A statement from an earlier boot of the host is refused. Believing one
    /// would put back a ceiling the host released when it rebooted — which is
    /// the whole failure this module is built to prevent, and it is cheaper to
    /// prevent than to detect afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`Fault::Replayed`] for a statement from an earlier boot.
    pub fn establish(&mut self, attestation: &Attestation) -> Result<(), Fault> {
        if let Some(known) = self.observed.filter(|known| attestation.epoch < *known) {
            return Err(Fault::Replayed(format!(
                "the host stated boot {} but this guest is in boot {known}",
                attestation.epoch
            )));
        }
        if self.observed != Some(attestation.epoch) {
            self.observed = Some(attestation.epoch);
        }
        for claim in attestation.claims() {
            // The newer statement replaces the older one about the same
            // control. The older one is not forgotten: it is still in the
            // record, and it is void rather than current, which is a different
            // and more truthful answer than "there was never any".
            self.facts
                .retain(|(held, _)| held.control() != claim.control());
            self.facts.push((*claim, attestation.epoch));
        }
        Ok(())
    }

    /// What may be reported about a control the host owns.
    #[must_use]
    pub fn standing(&self, control: Mediated) -> Standing {
        let Some((claim, epoch)) = self
            .facts
            .iter()
            .find(|(held, _)| held.control() == control)
        else {
            return Standing::Unknown;
        };
        if let Some(now) = self.observed.filter(|now| *epoch < *now) {
            return Standing::Void {
                claim: *claim,
                established_in: *epoch,
                host_now: now,
            };
        }
        Standing::Attested {
            claim: *claim,
            epoch: *epoch,
        }
    }

    /// Every control and what may be reported about it, in report order.
    #[must_use]
    pub fn standings(&self) -> Vec<(Mediated, Standing)> {
        Mediated::ALL
            .iter()
            .map(|control| (*control, self.standing(*control)))
            .collect()
    }

    /// The record as it is written to disk.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::from(LEDGER_HEADER);
        out.push('\n');
        if let Some(epoch) = self.observed {
            // Writing into a String cannot fail, and a record is not a place to
            // start propagating an error that cannot happen.
            let _ = writeln!(out, "observed {epoch}");
        } else {
            let _ = writeln!(out, "observed unknown");
        }
        for (claim, epoch) in &self.facts {
            let _ = writeln!(out, "fact {epoch} {}", claim.wire());
        }
        out
    }

    /// Reads the record back.
    ///
    /// # Errors
    ///
    /// Returns why the record could not be read. A record that exists and
    /// cannot be understood is **not** read as an empty record: something
    /// wrote it, and treating it as "the guest has established nothing" would
    /// turn a lost file into a clean bill of health.
    pub fn parse(text: &str) -> Result<Self, Fault> {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default();
        if header.trim() != LEDGER_HEADER {
            return Err(Fault::Malformed(format!(
                "the guest record does not begin with {LEDGER_HEADER:?}"
            )));
        }

        let mut ledger = Ledger::new();
        let mut observed_seen = false;

        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.first().copied() {
                Some("observed") => {
                    if observed_seen {
                        return Err(Fault::Contradictory(
                            "the guest record states the host's boot twice".to_owned(),
                        ));
                    }
                    observed_seen = true;
                    let value = words.get(1).copied().unwrap_or_default();
                    if value != "unknown" {
                        let counter: u64 = value.parse().map_err(|_| {
                            Fault::Malformed(format!("{value:?} is not a boot counter"))
                        })?;
                        let Some(epoch) = HostEpoch::new(counter) else {
                            return Err(Fault::Malformed(format!(
                                "boot counter {counter} is not a boot"
                            )));
                        };
                        ledger.observed = Some(epoch);
                    }
                    if words.len() > 2 {
                        return Err(Fault::Malformed(format!(
                            "{line:?} has words this build does not know"
                        )));
                    }
                }
                Some("fact") => {
                    let value = words.get(1).copied().unwrap_or_default();
                    let counter: u64 = value.parse().map_err(|_| {
                        Fault::Malformed(format!("{value:?} is not a boot counter"))
                    })?;
                    let Some(epoch) = HostEpoch::new(counter) else {
                        return Err(Fault::Malformed(format!(
                            "boot counter {counter} is not a boot"
                        )));
                    };
                    let claim_text = words[2..].join(" ");
                    let claim = Claim::from_wire(&claim_text)
                        .map_err(|why| Fault::Malformed(why.clone()))?;
                    if ledger
                        .facts
                        .iter()
                        .any(|(held, _)| held.control() == claim.control())
                    {
                        return Err(Fault::Contradictory(format!(
                            "the guest record holds two facts about the {}",
                            claim.control().name()
                        )));
                    }
                    ledger.facts.push((claim, epoch));
                }
                Some(other) => {
                    return Err(Fault::Malformed(format!(
                        "{other:?} is not a line this build knows"
                    )))
                }
                None => {}
            }
        }

        // A fact from a boot the guest never observed cannot have been written
        // by this program, and is not a record worth believing.
        if let Some(now) = ledger.observed {
            if let Some((claim, _)) = ledger.facts.iter().find(|(_, epoch)| *epoch > now) {
                return Err(Fault::Malformed(format!(
                    "the guest record holds a fact about the {} from a boot it never observed",
                    claim.control().name()
                )));
            }
        }

        Ok(ledger)
    }
}

/// The guest's own record, as it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// The guest has no record yet.
    Absent,
    /// The record exists and could not be opened.
    Unreadable(String),
    /// The record exists and could not be understood.
    Corrupt(String),
    /// The record was read.
    Read(Ledger),
}

/// What this guest knows about the host that owns it.
///
/// The type exists so that the panel, the device report and the supervisor all
/// ask the same question of the same place: what has the host said, what has
/// this guest established, and what has gone wrong with either.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guest {
    ledger: Ledger,
    host_fault: Option<Fault>,
    record_fault: Option<Fault>,
}

impl Guest {
    /// A guest that has established nothing and been told nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Folds the host's statement into the guest's own record.
    ///
    /// The order matters. The record is read first, because it is what
    /// remembers the boot the facts were learned in; then the host's statement
    /// is applied to it, which is what notices a reboot.
    #[must_use]
    pub fn reconcile(host: &dyn Host, record: Record) -> Self {
        let (mut ledger, record_fault) = match record {
            Record::Read(ledger) => (ledger, None),
            Record::Absent => (Ledger::new(), None),
            Record::Unreadable(why) | Record::Corrupt(why) => {
                (Ledger::new(), Some(Fault::Malformed(why)))
            }
        };

        let host_fault = match host.env(HOST_ATTESTATION_ENV) {
            None => Some(Fault::Absent),
            Some(text) => match Attestation::parse(&text) {
                Ok(statement) => {
                    ledger.observe(statement.epoch());
                    ledger.establish(&statement).err()
                }
                Err(fault) => Some(fault),
            },
        };

        Self {
            ledger,
            host_fault,
            record_fault,
        }
    }

    /// What may be reported about a control the host owns.
    #[must_use]
    pub fn standing(&self, control: Mediated) -> Standing {
        self.ledger.standing(control)
    }

    /// Every control and what may be reported about it, in report order.
    #[must_use]
    pub fn standings(&self) -> Vec<(Mediated, Standing)> {
        self.ledger.standings()
    }

    /// The boot of the host this guest last observed.
    #[must_use]
    pub const fn boot(&self) -> Option<HostEpoch> {
        self.ledger.observed_epoch()
    }

    /// Why the host's statement could not be used, if it could not.
    #[must_use]
    pub fn host_fault(&self) -> Option<&Fault> {
        self.host_fault.as_ref()
    }

    /// Why the guest's own record could not be used, if it could not.
    #[must_use]
    pub fn record_fault(&self) -> Option<&Fault> {
        self.record_fault.as_ref()
    }

    /// The record itself.
    #[must_use]
    pub const fn ledger(&self) -> &Ledger {
        &self.ledger
    }
}
