// SPDX-License-Identifier: Apache-2.0

//! The power-supply sysfs interface, per ADR-0002 §2 and §3.
//!
//! This is where the governor stops being a state machine and starts touching a
//! device. Everything here reads or writes `/sys/class/power_supply/…`.
//!
//! # A reading is never assembled from what happened to be readable
//!
//! [`read_battery`] returns `Result`, and the error names the node that was
//! missing. The alternative — fill in what can be read, default the rest —
//! produces a [`crate::battery::Reading`] whose gaps are invisible by the time a
//! threshold is compared against it. A pack temperature that defaulted to zero
//! is a cell that never gets warm, on a device nobody is watching.
//!
//! # Which node answered is part of the answer
//!
//! ADR-0002 §2 requires the probe to record **which** mechanism responded,
//! because the four differ in reliability and the panel has to say which one it
//! is using. [`Kind::node`] carries it, and it goes into the device profile
//! and the hardware database.
//!
//! # T0 is the common case and it has no mechanism at all
//!
//! [`detect_mechanism`] returning `None` is not an error and not a bug. On an
//! unrooted stock phone there is no supported way to stop the charger, and the
//! honest response is a safety row that stays red forever — not a mechanism that
//! reports success while doing nothing.

use crate::battery::{DeciCelsius, Percent, Reading, VendorHealth};
use crate::governor::ChargeMechanism;
use crate::host::{Host, Writer};

/// The directory the kernel exposes the pack under.
pub const SUPPLY: &str = "/sys/class/power_supply/battery";

/// Why a reading could not be taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// A required node was absent or unreadable.
    Missing {
        /// The node.
        node: &'static str,
    },
    /// A node was present and did not contain a number.
    Unparseable {
        /// The node.
        node: &'static str,
        /// What it contained instead.
        found: String,
    },
}

impl core::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReadError::Missing { node } => write!(
                f,
                "{node} could not be read, so no reading was taken; a partial \
                 reading with the rest defaulted is worse than none"
            ),
            ReadError::Unparseable { node, found } => {
                write!(f, "{node} contained {found:?}, which is not a number")
            }
        }
    }
}

fn read_i64(host: &dyn Host, dir: &str, node: &'static str) -> Result<i64, ReadError> {
    let raw = host
        .read(&format!("{dir}/{node}"))
        .ok_or(ReadError::Missing { node })?;
    raw.trim()
        .parse::<i64>()
        .map_err(|_| ReadError::Unparseable {
            node,
            found: raw.trim().to_owned(),
        })
}

/// Every node under the supply directory that [`read_battery`] consults.
///
/// Published so a device report can say which of them a handset actually has
/// without keeping its own list. Two lists would drift, and the one that drifts
/// is the report — which is the only thing anybody would have to go on about a
/// device nobody here is holding.
///
/// `health` is included and is the one that may be absent without failing a
/// read: vendors report "Good" on visibly swollen cells, so nothing acts on it.
pub const NODES: [&str; 8] = [
    "capacity",
    "voltage_now",
    "current_now",
    "temp",
    "cycle_count",
    "charge_full",
    "charge_full_design",
    "health",
];

/// Takes one sample of the cell.
///
/// # Errors
///
/// Returns the node that prevented it. Every required field must be present:
/// see the module documentation for why a partial reading is refused rather
/// than filled in.
pub fn read_battery(host: &dyn Host, dir: &str) -> Result<Reading, ReadError> {
    // `health` is the one node that may be absent without failing the read, and
    // it is the one node nothing acts on. Vendors report "Good" on visibly
    // swollen cells, so its absence costs nothing and its presence is recorded
    // only so a disagreement with the measured trend is visible.
    let vendor_health = match host.read(&format!("{dir}/health")) {
        Some(_) => VendorHealth::Reported("reported"),
        None => VendorHealth::Unavailable,
    };

    Ok(Reading {
        capacity: Percent::clamped(read_i64(host, dir, "capacity")?),
        voltage_now_uv: read_i64(host, dir, "voltage_now")?,
        current_now_ua: read_i64(host, dir, "current_now")?,
        temp: DeciCelsius::from_sysfs(i32::try_from(read_i64(host, dir, "temp")?).map_err(
            |_| ReadError::Unparseable {
                node: "temp",
                found: "out of range for a temperature".to_owned(),
            },
        )?),
        cycle_count: u32::try_from(read_i64(host, dir, "cycle_count")?).unwrap_or(0),
        charge_full_uah: read_i64(host, dir, "charge_full")?,
        charge_full_design_uah: read_i64(host, dir, "charge_full_design")?,
        vendor_health,
    })
}

/// Which of the four mechanisms a device actually has.
///
/// ADR-0002 §2: these are not one feature. They are four mechanisms with four
/// different reliabilities, and the panel has to say which one it is using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `charge_control_end_threshold`, in percent. The mainline node, preferred.
    EndThreshold,
    /// `constant_charge_current_max`, in microamps. Slows charging rather than
    /// stopping it — real ageing benefit, but it is not a ceiling and the panel
    /// must not present it as one.
    CurrentLimit,
    /// `input_suspend`, 1 to stop. Common on Qualcomm platforms. A blunt switch:
    /// it is either charging or it is not.
    InputSuspend,
    /// `voltage_max`, in microvolts. Terminal voltage; the most durable runtime
    /// control where it is writable.
    VoltageMax,
}

impl Kind {
    /// The node this mechanism writes.
    #[must_use]
    pub const fn node(self) -> &'static str {
        match self {
            Kind::EndThreshold => "charge_control_end_threshold",
            Kind::CurrentLimit => "constant_charge_current_max",
            Kind::InputSuspend => "input_suspend",
            Kind::VoltageMax => "voltage_max",
        }
    }

    /// Whether this mechanism actually holds a **ceiling**, as opposed to
    /// influencing charging some other way.
    ///
    /// Only [`Kind::EndThreshold`] does. The others slow charging, stop it
    /// outright, or cap terminal voltage — all useful, none of them a percentage
    /// the governor can read back and compare. Presenting them as a ceiling
    /// would be a green row for something nobody verified.
    #[must_use]
    pub const fn is_ceiling(self) -> bool {
        matches!(self, Kind::EndThreshold)
    }
}

/// Probe order, most preferred first. ADR-0002 §2.
pub const PROBE_ORDER: [Kind; 4] = [
    Kind::EndThreshold,
    Kind::VoltageMax,
    Kind::CurrentLimit,
    Kind::InputSuspend,
];

/// Finds the best mechanism this device exposes.
///
/// `None` means the device has none — the T0 case, which is the most common
/// device and the worst one. It is not an error and it must not be reported as
/// a mechanism that quietly does nothing.
#[must_use]
pub fn detect_mechanism(host: &dyn Host, dir: &str) -> Option<Kind> {
    PROBE_ORDER
        .into_iter()
        .find(|k| host.exists(&format!("{dir}/{}", k.node())))
}

/// A charge ceiling backed by a real sysfs node.
///
/// Only constructed for [`Kind::is_ceiling`] mechanisms, because the governor's
/// contract is a percentage written and the same percentage read back. See
/// [`SysfsCeiling::new`].
pub struct SysfsCeiling<'w, W: Writer + Host> {
    host: &'w mut W,
    path: String,
    kind: Kind,
}

impl<'w, W: Writer + Host> SysfsCeiling<'w, W> {
    /// Binds to a mechanism that genuinely holds a percentage ceiling.
    ///
    /// Returns `None` for the mechanisms that do not. A `constant_charge_current_max`
    /// dressed up as a ceiling would let the governor write 60, read back 500000
    /// microamps, and call it reverted — or worse, compare the two numbers and
    /// call it held.
    #[must_use]
    pub fn new(host: &'w mut W, dir: &str, kind: Kind) -> Option<Self> {
        if !kind.is_ceiling() {
            return None;
        }
        Some(Self {
            path: format!("{dir}/{}", kind.node()),
            kind,
            host,
        })
    }

    /// Which mechanism this is, for the device profile and the panel.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// The node that answered, verbatim. Recorded in the hardware database.
    #[must_use]
    pub fn node_path(&self) -> &str {
        &self.path
    }
}

impl<W: Writer + Host> ChargeMechanism for SysfsCeiling<'_, W> {
    fn apply(&mut self, ceiling: Percent) -> Result<(), String> {
        let value = ceiling.get().to_string();
        self.host.write(&self.path, &value)
    }

    fn verify(&self) -> Result<Percent, String> {
        // Read from the hardware, never from a value this process remembers
        // writing. Remembering is what a configuration does; reading back is
        // what makes it a control.
        let raw = self
            .host
            .read(&self.path)
            .ok_or_else(|| format!("{}: could not be read back", self.path))?;
        let n: i64 = raw
            .trim()
            .parse()
            .map_err(|_| format!("{}: contains {:?}, not a number", self.path, raw.trim()))?;
        Ok(Percent::clamped(n))
    }
}
