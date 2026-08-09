// SPDX-License-Identifier: Apache-2.0

//! Whether a person still has to look at this phone.
//!
//! # The claim this module exists to make true
//!
//! When the governor halts, the binary says:
//!
//! > the governor has halted. This requires a person who has looked at the
//! > phone; no restart clears it.
//!
//! It was not true. [`crate::runtime::Supervisor::new`] takes a governor rather
//! than building one *"so a device that was halted before a restart comes back
//! halted"*, and [`crate::governor::Governor::after_inspection`] is the way back
//! down the ladder — both were written, and every caller passed a fresh
//! `Governor::new`, at `Level::Normal`. So a phone that halted on temperature
//! came back serving the moment anything restarted it: the operator, Android
//! reclaiming memory, a power cut, a boot script.
//!
//! A hard stop that any restart clears is a log line. This module is the record
//! that makes it a state.
//!
//! # It decides; it does not read or write
//!
//! Rendering and parsing live here, and the file does not. That keeps the one
//! interesting rule — below — reachable in a test with no filesystem.
//!
//! # An unreadable record is a halted device
//!
//! Three outcomes, not two: no record, a record, and *a record nobody could
//! read*. The third is the one that decides whether this module is worth having.
//!
//! A missing record means no halt was ever written. A record that exists and
//! cannot be read means **something is there and its contents are unknown** —
//! and the thing it would say, if it could be read, is that a lithium cell in
//! somebody's home crossed a hard threshold. Treating that as "probably fine"
//! is the one mistake this project exists to refuse, so [`Standing::Unreadable`]
//! is not clear and never becomes clear by itself.

use core::fmt;

use crate::governor::Level;

/// A halt that was recorded, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Halt {
    reason: String,
}

/// Why a recorded halt could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HaltError {
    /// The record was empty, so it names no reason.
    ///
    /// Refused rather than accepted as a halt with no explanation: an operator
    /// told to look at their phone deserves to be told what the device saw.
    Empty,
    /// The record carried a control character.
    ///
    /// The reason is printed to a terminal. A newline in it rewrites the
    /// following line, which is how a record becomes a way to forge output.
    Control,
}

impl fmt::Display for HaltError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str(
                "the halt record is empty, so it does not say what the device saw; \
                 it names no reason and cannot be trusted to be one",
            ),
            Self::Control => f.write_str(
                "the halt record contains a control character, and it is printed \
                 to a terminal; a record that can rewrite the screen is not a record",
            ),
        }
    }
}

impl Halt {
    /// A halt carrying the reason the governor gave.
    ///
    /// # Errors
    ///
    /// Returns why the reason is not usable as one.
    pub fn new(reason: &str) -> Result<Self, HaltError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(HaltError::Empty);
        }
        if reason.chars().any(char::is_control) {
            return Err(HaltError::Control);
        }
        Ok(Self {
            reason: reason.to_owned(),
        })
    }

    /// What the device saw.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The record's whole contents.
    ///
    /// One line, so a person can read it with `cat` and a partially written one
    /// is obviously partial rather than plausibly complete.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}\n", self.reason)
    }

    /// Reads a record back.
    ///
    /// # Errors
    ///
    /// Returns why the record could not be understood. That is **not** the same
    /// as no record — see [`Standing`].
    pub fn parse(raw: &str) -> Result<Self, HaltError> {
        Self::new(raw)
    }
}

/// Whether this device may start serving.
///
/// Three states. There is deliberately no fourth, and no way to express
/// "halted but probably fine now":
///
/// ```compile_fail
/// use vayucell_core::halt::Standing;
/// // The type annotation is load-bearing: a bare variant name is a constructor
/// // and would compile as a function value without it.
/// let s: Standing = Standing::ProbablyRecovered;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// No halt has been recorded. The governor starts where the cell puts it.
    Clear,
    /// A halt was recorded, and no inspection has cleared it.
    Halted(Halt),
    /// A record exists and could not be read, so nothing is known about it.
    Unreadable(String),
}

impl Standing {
    /// Whether the device may begin serving without a person looking at it.
    ///
    /// Only [`Standing::Clear`] may. Written as a match rather than
    /// `matches!(self, Self::Clear)` so a variant added later fails to compile
    /// here instead of quietly falling on whichever side the author of that
    /// variant did not think about.
    #[must_use]
    pub const fn may_start_serving(&self) -> bool {
        match self {
            Self::Clear => true,
            Self::Halted(_) | Self::Unreadable(_) => false,
        }
    }

    /// The level no report about this device may sit below.
    ///
    /// A standing halt is a fact about the device that a fresh reading cannot
    /// see. The cell may be cool now — it usually is by the time anybody looks —
    /// and the panel must still not say the governor is at `NORMAL` with no
    /// threshold crossed, because a record on disk says one was crossed and
    /// nobody has been to look at the phone.
    ///
    /// Callers take the worse of this and whatever they measured. That is the
    /// same rule [`crate::runtime::Supervisor`] users follow for a latched
    /// level, applied to a latch that outlives the process.
    #[must_use]
    pub const fn floor(&self) -> Level {
        match self {
            Self::Clear => Level::Normal,
            Self::Halted(_) | Self::Unreadable(_) => Level::Halt,
        }
    }

    /// What to tell the operator.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Clear => "no halt is recorded on this device".to_owned(),
            Self::Halted(h) => format!(
                "this device halted and no one has recorded looking at it since: {}",
                h.reason()
            ),
            Self::Unreadable(why) => format!(
                "this device has a halt record that could not be read, so it is \
                 treated as halted: {why}"
            ),
        }
    }
}
