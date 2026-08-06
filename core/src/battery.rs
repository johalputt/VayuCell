// SPDX-License-Identifier: Apache-2.0

//! Battery telemetry, per ADR-0002 §3.
//!
//! # Why the units are types
//!
//! `/sys/class/power_supply/battery/temp` reports **decidegrees**. `voltage_now`
//! reports **microvolts**. `current_now` reports **microamps**. Every one of
//! those is a bare integer in the kernel interface, and every one of them is a
//! factor of a thousand or ten away from the unit a person reasons in.
//!
//! A hard-stop threshold of 60 compared against a raw reading of 601 does not
//! crash, does not warn, and does not fire. It reads as a cell at 60.1 °C being
//! nowhere near the limit, on a device sitting in somebody's home. So the unit
//! lives in the type: [`DeciCelsius`] is what the kernel said, [`Celsius`] is
//! what a threshold is written in, and the conversion between them is a named
//! function rather than a multiplication somebody has to remember.
//!
//! ```compile_fail
//! use vayucell_core::battery::{Celsius, DeciCelsius};
//! // The kernel's reading is not a temperature until it is converted.
//! let hard_stop: Celsius = DeciCelsius::from_sysfs(601);
//! ```
//!
//! A positive control, so the proof above is known to fail for the reason
//! claimed rather than because the import is wrong:
//!
//! ```
//! use vayucell_core::battery::DeciCelsius;
//! assert_eq!(DeciCelsius::from_sysfs(601).to_celsius().whole_degrees(), 60);
//! ```
//!
//! # The vendor's opinion is recorded, never acted on
//!
//! [`VendorHealth`] carries whatever the kernel's `health` node says. ADR-0002
//! §Open-decisions-5 is blunt about why it is only carried: **vendors report
//! `Good` on visibly swollen cells.** It is kept because a disagreement between
//! the vendor's opinion and the measured trend is itself worth recording, and it
//! is kept out of every decision path.

use core::fmt;

/// Temperature as the kernel reports it: tenths of a degree Celsius.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeciCelsius(i32);

/// Temperature in whole degrees, which is how thresholds are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Celsius(i32);

impl DeciCelsius {
    /// Wraps a raw `temp` reading.
    #[must_use]
    pub const fn from_sysfs(deci: i32) -> Self {
        Self(deci)
    }

    /// The raw value, for the evidence line in a log.
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Converts to whole degrees, truncating toward zero.
    ///
    /// Truncation is deliberate and it is the safe direction: 60.9 °C becomes
    /// 60, so a threshold at 60 fires. Rounding to nearest would let 60.4 °C
    /// read as 60 and 60.5 as 61, which makes the boundary depend on which side
    /// of half a degree the cell happens to be — a distinction no thermal design
    /// should rest on.
    #[must_use]
    pub const fn to_celsius(self) -> Celsius {
        Celsius(self.0 / 10)
    }
}

impl Celsius {
    /// A threshold, written the way a person writes one.
    #[must_use]
    pub const fn new(degrees: i32) -> Self {
        Self(degrees)
    }

    /// The value in whole degrees.
    #[must_use]
    pub const fn whole_degrees(self) -> i32 {
        self.0
    }
}

impl fmt::Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} °C", self.0)
    }
}

impl fmt::Display for DeciCelsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{} °C", self.0 / 10, (self.0 % 10).abs())
    }
}

/// A percentage of a whole, clamped to 0–100 at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Percent(u8);

impl Percent {
    /// Clamps into range.
    ///
    /// A vendor kernel that reports 127% capacity is not a reason to panic on a
    /// device somebody is asleep next to; it is a reason to treat the cell as
    /// full and let the health trend carry the signal.
    #[must_use]
    pub fn clamped(value: i64) -> Self {
        // u8::try_from cannot fail after the clamp, and saying so with a
        // fallback rather than a cast keeps the crate free of the lint that
        // exists because casts like this one usually are wrong.
        Self(u8::try_from(value.clamp(0, 100)).unwrap_or(0))
    }

    /// The value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// The vendor kernel's own opinion of the cell.
///
/// Recorded, never acted on. See the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorHealth {
    /// The node said something recognised.
    Reported(&'static str),
    /// The node was absent or unreadable. Not "good".
    Unavailable,
}

/// One sample of the cell.
///
/// Every field is required. A reading assembled from whatever happened to be
/// readable, with the rest defaulted, is a reading whose gaps are invisible by
/// the time a threshold is compared against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    /// Reported state of charge.
    pub capacity: Percent,
    /// Cell voltage, microvolts. The honest state-of-charge signal.
    pub voltage_now_uv: i64,
    /// Current, microamps. Sign indicates charge or discharge.
    pub current_now_ua: i64,
    /// Pack temperature, as the kernel reported it.
    pub temp: DeciCelsius,
    /// Lifetime equivalent full cycles.
    pub cycle_count: u32,
    /// Present capacity when full, microamp-hours.
    pub charge_full_uah: i64,
    /// Original design capacity, microamp-hours.
    pub charge_full_design_uah: i64,
    /// The vendor's opinion. Carried, not consulted.
    pub vendor_health: VendorHealth,
}

/// State of health, as a fraction of design capacity.
///
/// [`StateOfHealth::Unknown`] is a distinct answer, not a zero. A device whose
/// `charge_full_design` node is missing has not lost all its capacity — nothing
/// measured it, and a governor that treats the two alike would retire a healthy
/// cell or, far worse, keep serving on a dead one because the arithmetic
/// happened to land somewhere harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateOfHealth {
    /// Measured, as a percentage of design capacity.
    Measured(Percent),
    /// Not measurable on this device.
    Unknown,
}

impl fmt::Display for StateOfHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateOfHealth::Measured(p) => write!(f, "{p}"),
            StateOfHealth::Unknown => f.write_str("unknown"),
        }
    }
}

impl Reading {
    /// State of health, or [`StateOfHealth::Unknown`] where it cannot be
    /// computed.
    #[must_use]
    pub fn state_of_health(&self) -> StateOfHealth {
        if self.charge_full_design_uah <= 0 || self.charge_full_uah < 0 {
            return StateOfHealth::Unknown;
        }
        StateOfHealth::Measured(Percent::clamped(
            self.charge_full_uah * 100 / self.charge_full_design_uah,
        ))
    }

    /// Whether the cell is taking charge.
    #[must_use]
    pub const fn is_charging(&self) -> bool {
        self.current_now_ua > 0
    }

    /// A one-line evidence string for a transition log.
    ///
    /// Every state change names the reading that caused it. "Halted" is useless;
    /// "halted: pack temperature 61.2 °C exceeded hard stop 60 °C" is
    /// actionable, and this is the half of that sentence the reading owns.
    #[must_use]
    pub fn evidence(&self) -> String {
        format!(
            "temp {}, capacity {}, {} µV, {} µA, {} cycles, health {}",
            self.temp,
            self.capacity,
            self.voltage_now_uv,
            self.current_now_ua,
            self.cycle_count,
            self.state_of_health(),
        )
    }
}
