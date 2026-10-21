// SPDX-License-Identifier: Apache-2.0

//! The catalogue: what a cell can offer, and what it cannot.
//!
//! PLAN §8 makes one click mean a catalogue rather than a shell prompt, and
//! PLAN §9 says the picker is filtered to the tier. The filtering has to say
//! *why*: a phone in a drawer is the whole premise of the project, and
//! installing the wrong thing on one is how the project earns a bad name.
//!
//! # The rule this module exists to keep
//!
//! **Every catalogue entry appears in every selection.** An entry this device
//! cannot run is greyed out and carries the requirement that failed; it is
//! never dropped. A catalogue that quietly hides what it will not run teaches
//! its reader that the software knows best, and the reader has no way to
//! tell "not offered here" from "not possible at all".
//!
//! Three answers, kept apart on purpose (CHARTER Article IV):
//!
//! - **it fits** — every requirement holds, and every requirement was
//!   measured;
//! - **it provably does not fit** — a named requirement failed, and the
//!   shortfall says which one;
//! - **it cannot be told** — the device's tier was never established, or a
//!   requirement was never measured. Never "it fits", and never a guess.
//!
//! The third answer is the one a picker is tempted to skip, and it is the
//! one that matters most here: a device whose storage nobody has measured
//! is not a device with no storage, and a catalogue that answered "provably
//! too big" would be inventing the failure.
//!
//! # The floor is a tier, and the tier is established
//!
//! The floors are the [`Tier`] of [`crate::capability`], not a second ladder
//! invented here: T0 stock Android can do almost nothing, T1 rooted can, T2
//! is a guest whose resources are the host's to give, and T3 mainline Linux
//! has the most. A floor of T1 usually means one thing — CHARTER Article
//! III.1, that no capability which serves traffic ships before the battery
//! governor, and the governor needs a charge ceiling that only a rooted
//! device can hold.
//!
//! # The heavy class is what heats the cell
//!
//! [`ThermalClass`] orders the work from idle to heavy. The heavy class is
//! where private model inference sits, and it is the class that warms a
//! lithium cell until the governor has to intervene. A catalogue that listed
//! a heavy service without saying so would be a catalogue that got somebody
//! to run it on a phone in a drawer without ever mentioning the battery.
//!
//! The class itself, and the policy that turns an observed cell into one, live
//! in [`crate::thermal`]: it is a property of the device, and this module is
//! about the services.
//!
//! # What this module does not claim
//!
//! Nothing here has been run on a handset. PLAN §11 keeps the P9 gate open —
//! "runs within the declared thermal envelope" — and this module declares
//! envelopes; it does not report runs. That gate needs silicon, and code
//! cannot substitute for it.

use core::fmt;

use crate::capability::Tier;
use crate::thermal::ThermalClass;

/// The resources a service wants.
///
/// "Wants", not "needs": a device with less than this cannot carry the
/// service, and the panel says so rather than letting the user find out by
/// watching a phone thrash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// Megabytes of memory the service wants to keep.
    pub memory_mb: u32,
    /// Megabytes of storage the service wants to keep.
    pub storage_mb: u64,
    /// The thermal class the service runs in.
    pub thermal: ThermalClass,
}

impl Envelope {
    /// An envelope asking for what it asks for.
    #[must_use]
    pub const fn new(memory_mb: u32, storage_mb: u64, thermal: ThermalClass) -> Self {
        Self {
            memory_mb,
            storage_mb,
            thermal,
        }
    }

    /// The first requirement this envelope does not meet, if any.
    ///
    /// Named rather than summarised: "too little memory" and "below the tier
    /// floor" are different answers, and an operator told only that
    /// something was too small has nothing to act on.
    ///
    /// A requirement that was never measured cannot fail. `None` here means
    /// "nothing measured failed", not "it fits" — the difference between the
    /// two is [`Reason::ResourcesUnknown`], and the caller is the only place
    /// that knows which one it is looking at.
    #[must_use]
    pub const fn shortfall(&self, budget: &Budget) -> Option<Shortfall> {
        if let Some(have_mb) = budget.memory_mb {
            if have_mb < self.memory_mb {
                return Some(Shortfall::Memory {
                    wanted_mb: self.memory_mb,
                    have_mb,
                });
            }
        }
        if let Some(have_mb) = budget.storage_mb {
            if have_mb < self.storage_mb {
                return Some(Shortfall::Storage {
                    wanted_mb: self.storage_mb,
                    have_mb,
                });
            }
        }
        if let Some(allowed) = budget.thermal {
            if self.thermal.worse_than(allowed) {
                return Some(Shortfall::Thermal {
                    wanted: self.thermal,
                    allowed,
                });
            }
        }
        None
    }
}

/// The requirement an entry failed.
///
/// Typed rather than a sentence, because the sentence is a rendering of this
/// and not the other way round: a caller can match on the failing
/// requirement without parsing English.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortfall {
    /// The device's tier is below the floor the service needs.
    Tier {
        /// The lowest tier that can provide the service.
        floor: Tier,
        /// The tier this device was established at.
        detected: Tier,
    },
    /// The device does not have the memory the service wants.
    Memory {
        /// Megabytes of memory the service wants.
        wanted_mb: u32,
        /// Megabytes of memory this device has.
        have_mb: u32,
    },
    /// The device does not have the storage the service wants.
    Storage {
        /// Megabytes of storage the service wants.
        wanted_mb: u64,
        /// Megabytes of storage this device has.
        have_mb: u64,
    },
    /// The device is not willing to run work this warm.
    Thermal {
        /// The thermal class the service runs in.
        wanted: ThermalClass,
        /// The thermal class this device is allowed to run.
        allowed: ThermalClass,
    },
}

impl Shortfall {
    /// The name of the requirement that failed.
    #[must_use]
    pub const fn requirement(self) -> &'static str {
        match self {
            Self::Tier { .. } => "tier",
            Self::Memory { .. } => "memory",
            Self::Storage { .. } => "storage",
            Self::Thermal { .. } => "thermal class",
        }
    }
}

/// What was never measured.
///
/// Three booleans rather than a list, because a list would allocate for a
/// thing that is almost always one of two or three answers, and because the
/// point of the type is to be printed: "storage and the thermal class" is
/// an answer an operator can act on, and "some resources" is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unmeasured {
    /// The device's memory was never measured.
    pub memory: bool,
    /// The device's storage was never measured.
    pub storage: bool,
    /// The device's thermal class was never established.
    pub thermal: bool,
}

impl Unmeasured {
    /// What is missing, in the order the requirements are checked.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.memory {
            out.push("memory");
        }
        if self.storage {
            out.push("storage");
        }
        if self.thermal {
            out.push("the thermal class");
        }
        out
    }
}

impl fmt::Display for Unmeasured {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = self.names();
        match names.len() {
            0 => f.write_str("nothing"),
            1 => f.write_str(names[0]),
            2 => write!(f, "{} and {}", names[0], names[1]),
            _ => write!(f, "{}, {} and {}", names[0], names[1], names[2]),
        }
    }
}

/// Why an entry is greyed out.
///
/// Two shapes, and the difference between them is the difference between
/// "provably does not fit" and "cannot be told" (CHARTER Article IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A named requirement did not hold.
    TooSmall(Shortfall),
    /// The device's tier was never established, so the floor cannot be checked.
    TierUnknown {
        /// The lowest tier that can provide the service.
        floor: Tier,
    },
    /// The device's resources were never measured.
    ///
    /// The floor was met and nothing measured failed, but a requirement was
    /// never measured either — so whether the service fits is not a question
    /// anybody can answer. This is the answer a picker must give rather than
    /// the one it would prefer: passing zero for an unmeasured resource would
    /// make every service look too big, and that failure is invented by the
    /// question rather than found in the device.
    ResourcesUnknown {
        /// The lowest tier that can provide the service.
        floor: Tier,
        /// The requirements that were never measured.
        missing: Unmeasured,
    },
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall(Shortfall::Tier { floor, detected }) => {
                write!(
                    f,
                    "this device is {detected} and the service needs at least {floor}"
                )
            }
            Self::TooSmall(Shortfall::Memory { wanted_mb, have_mb }) => {
                write!(
                    f,
                    "this device has {have_mb} MB of memory and the service wants {wanted_mb} MB"
                )
            }
            Self::TooSmall(Shortfall::Storage { wanted_mb, have_mb }) => {
                write!(
                    f,
                    "this device has {have_mb} MB of storage and the service wants {wanted_mb} MB"
                )
            }
            Self::TooSmall(Shortfall::Thermal { wanted, allowed }) => {
                write!(
                    f,
                    "this device is allowed {allowed} work and the service runs {wanted}"
                )
            }
            Self::TierUnknown { floor } => write!(
                f,
                "this device's tier was never established, so whether it meets the {floor} floor \
                 cannot be told"
            ),
            Self::ResourcesUnknown { missing, .. } => write!(
                f,
                "no measurement of this device's {missing} exists, so whether this service fits \
                 cannot be told"
            ),
        }
    }
}

/// What the device has to give.
///
/// Every figure is optional, because none of them is a fact until somebody
/// measures it. `None` is **not** zero: a device whose storage was never
/// measured is not a device with no storage, and a picker that passed zero
/// would report every service as provably too big — a failure invented by
/// the question rather than found in the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// The tier established on this device, if one ever was.
    ///
    /// `None` is not a zero tier. It means the profiler never named this
    /// device's tier, which is a different thing from naming it and finding
    /// it low, and it is answered with "cannot be told" rather than with a
    /// failure.
    pub tier: Option<Tier>,
    /// Megabytes of memory the device can give a service, if it was measured.
    pub memory_mb: Option<u32>,
    /// Megabytes of storage the device can give a service, if it was measured.
    pub storage_mb: Option<u64>,
    /// The hottest class of work this device is willing to run, if it was
    /// established.
    pub thermal: Option<ThermalClass>,
}

impl Budget {
    /// What the device has, as far as anybody has measured.
    ///
    /// Every figure is `None` where nobody has measured it. The convenience
    /// constructor [`Budget::measured`] is for a device that has been profiled
    /// completely.
    #[must_use]
    pub const fn new(
        tier: Option<Tier>,
        memory_mb: Option<u32>,
        storage_mb: Option<u64>,
        thermal: Option<ThermalClass>,
    ) -> Self {
        Self {
            tier,
            memory_mb,
            storage_mb,
            thermal,
        }
    }

    /// A device that has been profiled all the way through.
    #[must_use]
    pub const fn measured(
        tier: Option<Tier>,
        memory_mb: u32,
        storage_mb: u64,
        thermal: ThermalClass,
    ) -> Self {
        Self {
            tier,
            memory_mb: Some(memory_mb),
            storage_mb: Some(storage_mb),
            thermal: Some(thermal),
        }
    }

    /// What was never measured.
    #[must_use]
    pub const fn unmeasured(&self) -> Unmeasured {
        Unmeasured {
            memory: self.memory_mb.is_none(),
            storage: self.storage_mb.is_none(),
            thermal: self.thermal.is_none(),
        }
    }
}

/// One service the cell can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Stable identifier, used in reports and in the installer's manifest.
    pub id: &'static str,
    /// The name a person reads in the picker.
    pub name: &'static str,
    /// One sentence on what the service is for.
    pub purpose: &'static str,
    /// The lowest tier that can provide this service.
    pub floor: Tier,
    /// The resources this service wants.
    pub envelope: Envelope,
}

impl Entry {
    /// Why this entry would be greyed out on this device, if it would be.
    ///
    /// `None` means it fits. The tier is checked first and the comparison
    /// stops there if the tier was never established: the tier says what
    /// kind of device this is, and a device whose kind is unknown is not a
    /// device whose resources can be judged either.
    ///
    /// Then the measured requirements, and then the ones that were never
    /// measured. A requirement that was measured and did not hold is a
    /// definite answer even when the others are unknown — "this phone has
    /// 2 GB and the service wants 4" is a fact whatever else nobody measured.
    /// But nothing failing is not the same as everything holding: if a
    /// requirement was never measured, the answer is that this service
    /// cannot be told to fit.
    #[must_use]
    pub fn judge(&self, budget: &Budget) -> Option<Reason> {
        match budget.tier {
            None => Some(Reason::TierUnknown { floor: self.floor }),
            Some(detected) if !detected.at_least(self.floor) => {
                Some(Reason::TooSmall(Shortfall::Tier {
                    floor: self.floor,
                    detected,
                }))
            }
            Some(_) => {
                if let Some(shortfall) = self.envelope.shortfall(budget) {
                    return Some(Reason::TooSmall(shortfall));
                }
                let missing = budget.unmeasured();
                if missing.names().is_empty() {
                    None
                } else {
                    Some(Reason::ResourcesUnknown {
                        floor: self.floor,
                        missing,
                    })
                }
            }
        }
    }
}

/// One catalogue entry that will not be offered, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Greyed<'a> {
    /// The entry that will not be offered.
    pub entry: &'a Entry,
    /// Why it will not be offered.
    pub reason: Reason,
}

impl fmt::Display for Greyed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.entry.id, self.reason)
    }
}

/// What this device can run, and what it cannot.
///
/// The two lists together account for the whole catalogue. There is no third
/// place an entry can go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection<'a> {
    /// The entries that fit, in catalogue order.
    pub fitting: Vec<&'a Entry>,
    /// The entries that do not, in catalogue order, each with its reason.
    pub greyed: Vec<Greyed<'a>>,
}

impl Selection<'_> {
    /// How many catalogue entries are accounted for, fitting and greyed together.
    #[must_use]
    pub fn total(&self) -> usize {
        self.fitting.len() + self.greyed.len()
    }

    /// The reason an entry is greyed out, if it is.
    #[must_use]
    pub fn reason_for(&self, id: &str) -> Option<&Reason> {
        self.greyed
            .iter()
            .find(|greyed| greyed.entry.id == id)
            .map(|greyed| &greyed.reason)
    }

    /// Whether an entry is offered on this device.
    #[must_use]
    pub fn fits(&self, id: &str) -> bool {
        self.fitting.iter().any(|entry| entry.id == id)
    }
}

/// The safety and posture report.
///
/// T0, because the report is the one thing offered at every tier: CHARTER
/// Article III.2 requires a device which cannot hold a charge ceiling to be
/// told so plainly, and a report that needs a daemon is a report that Doze
/// will kill.
pub const VAYU_PANEL: Entry = Entry {
    id: "vayu.panel",
    name: "Safety and posture report",
    purpose: "What this cell can verify about itself, and what it cannot.",
    floor: Tier::T0,
    envelope: Envelope::new(64, 32, ThermalClass::Idle),
};

/// The full VayuPress stack.
///
/// T1, because CHARTER Article III.1 puts the battery governor before every
/// capability that serves traffic, and the governor needs a charge ceiling
/// to hold — which stock Android has no way to write.
pub const VAYU_PRESS: Entry = Entry {
    id: "vayu.press",
    name: "The full VayuPress stack",
    purpose: "Website, blog, private mail, encrypted chat, cookieless analytics, scoped API, bot shield and onion, in one binary.",
    floor: Tier::T1,
    envelope: Envelope::new(512, 2048, ThermalClass::Light),
};

/// The personal cloud.
///
/// T1, for the same reason as every service that serves traffic — the
/// governor needs a charge ceiling — and because the sync daemon has to
/// come back after a reboot with an encrypted store on disk, which stock
/// Android's Doze will not allow.
pub const VAYU_CLOUD: Entry = Entry {
    id: "vayu.cloud",
    name: "Personal cloud storage",
    purpose: "File sync, photo backup, versioned and encrypted, on hardware you own.",
    floor: Tier::T1,
    envelope: Envelope::new(512, 8192, ThermalClass::Light),
};

/// The backup target.
///
/// T1: it serves traffic to somebody else's house, so the ceiling argument
/// applies, and a cell that is off-site backup is a cell that stays plugged
/// in for years.
pub const VAYU_BACKUP: Entry = Entry {
    id: "vayu.backup",
    name: "Backup target",
    purpose: "Off-site backup for a relative's house, at nothing per month.",
    floor: Tier::T1,
    envelope: Envelope::new(256, 8192, ThermalClass::Idle),
};

/// Media and household services.
///
/// T1, and the thermal class is sustained rather than light: a library that
/// transcodes and a hub that never sleeps are the first services here that
/// work for hours rather than answer a request.
pub const VAYU_HOUSEHOLD: Entry = Entry {
    id: "vayu.household",
    name: "Media and household services",
    purpose: "Library serving, ad-blocking DNS and a home automation hub, on one box.",
    floor: Tier::T1,
    envelope: Envelope::new(256, 4096, ThermalClass::Sustained),
};

/// Private model inference.
///
/// T1, and deliberately not T2: the heavy class is what warms a lithium
/// cell, and the governor is the only thing that can shed the workload
/// before the cell suffers for it. A tier-2 guest cannot read the cell at
/// all, so a heavy workload in a guest is ungoverned by definition.
pub const VAYU_INFERENCE: Entry = Entry {
    id: "vayu.inference",
    name: "Private model inference",
    purpose: "Local, private assistance from a quantised model on the phone's own accelerator.",
    floor: Tier::T1,
    envelope: Envelope::new(4096, 8192, ThermalClass::Heavy),
};

/// The built-in catalogue, in the order the picker shows it.
///
/// Every entry declares the tier floor it needs and the envelope it wants,
/// so the panel can grey out what this device cannot run and say why. The
/// order is fixed, because a picker whose rows move between launches is a
/// picker nobody can screenshot.
///
/// Not a `const fn`: reading a static is not a constant expression on the
/// minimum supported Rust, and a catalogue nobody can read at compile time
/// is not worth an unstable feature.
#[must_use]
pub fn catalogue() -> &'static [Entry] {
    &CATALOGUE
}

static CATALOGUE: [Entry; 6] = [
    VAYU_PANEL,
    VAYU_PRESS,
    VAYU_CLOUD,
    VAYU_BACKUP,
    VAYU_HOUSEHOLD,
    VAYU_INFERENCE,
];

/// Filters the built-in catalogue against a device's budget.
///
/// Every entry ends up in one list or the other: the ones that fit, and the
/// ones greyed out with the requirement that failed. Nothing is dropped,
/// because a service that is not offered is a service the reader cannot ask
/// about.
#[must_use]
pub fn select(budget: &Budget) -> Selection<'static> {
    select_from(catalogue(), budget)
}

/// Filters a given catalogue against a device's budget.
///
/// The same filter the picker uses, over a catalogue the caller supplies.
#[must_use]
pub fn select_from<'a>(entries: &'a [Entry], budget: &Budget) -> Selection<'a> {
    let mut fitting = Vec::new();
    let mut greyed = Vec::new();
    for entry in entries {
        match entry.judge(budget) {
            Some(reason) => greyed.push(Greyed { entry, reason }),
            None => fitting.push(entry),
        }
    }
    Selection { fitting, greyed }
}
