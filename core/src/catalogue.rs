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
//! - **it fits** — every requirement holds;
//! - **it provably does not fit** — a named requirement failed, and the
//!   shortfall says which one;
//! - **it cannot be told** — the device's tier was never established, so the
//!   floor cannot be checked. Never "it fits", and never a guess.
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
//! # What this module does not claim
//!
//! Nothing here has been run on a handset. PLAN §11 keeps the P9 gate open —
//! "runs within the declared thermal envelope" — and this module declares
//! envelopes; it does not report runs. That gate needs silicon, and code
//! cannot substitute for it.

use core::fmt;

use crate::capability::Tier;

/// How hard a workload leans on the cell.
///
/// The order is the point: the governor compares what a service wants against
/// what the device is willing to give, and "worse" means hotter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThermalClass {
    /// The cell does essentially nothing: a report, a wake-up, a ping.
    Idle,
    /// Steady background work, such as a sync daemon answering a request a minute.
    Light,
    /// Continuous work that will run for hours, such as transcoding.
    Sustained,
    /// A flat-out multi-core workload with the accelerator running.
    Heavy,
}

impl ThermalClass {
    /// The class's place in the order, lowest first.
    const fn rank(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Light => 1,
            Self::Sustained => 2,
            Self::Heavy => 3,
        }
    }

    /// Whether this class is worse — hotter — than `other`.
    ///
    /// The comparison the governor wants: a device willing to run only
    /// [`ThermalClass::Light`] work cannot carry a [`ThermalClass::Heavy`]
    /// service, and a device willing to run heavy work can carry anything.
    #[must_use]
    pub const fn worse_than(self, other: Self) -> bool {
        self.rank() > other.rank()
    }
}

impl fmt::Display for ThermalClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Idle => "idle",
            Self::Light => "light",
            Self::Sustained => "sustained",
            Self::Heavy => "heavy",
        })
    }
}

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
    #[must_use]
    pub const fn shortfall(&self, budget: &Budget) -> Option<Shortfall> {
        if budget.memory_mb < self.memory_mb {
            Some(Shortfall::Memory {
                wanted_mb: self.memory_mb,
                have_mb: budget.memory_mb,
            })
        } else if budget.storage_mb < self.storage_mb {
            Some(Shortfall::Storage {
                wanted_mb: self.storage_mb,
                have_mb: budget.storage_mb,
            })
        } else if self.thermal.worse_than(budget.thermal) {
            Some(Shortfall::Thermal {
                wanted: self.thermal,
                allowed: budget.thermal,
            })
        } else {
            None
        }
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
        }
    }
}

/// What the device has to give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// The tier established on this device, if one ever was.
    ///
    /// `None` is not a zero tier. It means the profiler never named this
    /// device's tier, which is a different thing from naming it and finding
    /// it low, and it is answered with "cannot be told" rather than with a
    /// failure.
    pub tier: Option<Tier>,
    /// Megabytes of memory the device can give a service.
    pub memory_mb: u32,
    /// Megabytes of storage the device can give a service.
    pub storage_mb: u64,
    /// The hottest class of work this device is willing to run.
    pub thermal: ThermalClass,
}

impl Budget {
    /// What the device has, as the profiler found it.
    #[must_use]
    pub const fn new(
        tier: Option<Tier>,
        memory_mb: u32,
        storage_mb: u64,
        thermal: ThermalClass,
    ) -> Self {
        Self {
            tier,
            memory_mb,
            storage_mb,
            thermal,
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
    /// stops there if the tier was never established: the tier and the
    /// resource figures come from the same profiling pass (PLAN §9.2), so a
    /// device whose tier is unknown has not been profiled, and half an
    /// answer about memory would be no more trustworthy than the tier.
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
            Some(_) => self.envelope.shortfall(budget).map(Reason::TooSmall),
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
