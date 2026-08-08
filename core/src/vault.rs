// SPDX-License-Identifier: Apache-2.0

//! The vault: files the device is asked to keep.
//!
//! # This is the first thing here that accepts rather than serves
//!
//! Everything before it reads. [`crate::site`] serves a directory and takes
//! nothing; [`crate::serve::Method`] still has no `Post`, `Put` or `Delete`.
//! Accepting a file is a different kind of act, and the two failures that
//! matter are not the ones a reader has: a write that half-lands, and a write
//! that lands when the device was in no condition to take it.
//!
//! This module is the part that decides. It performs no I/O — it validates the
//! name, checks the room, asks the governor, and hands back the *ordering* a
//! caller must follow. The bytes are somebody else's job, which is what makes
//! every decision here testable without a filesystem.
//!
//! # Durability is never claimed, because ADR-0004 says it cannot be
//!
//! ADR-0004 §0: a sealed phone cannot drop its own storage rail, so no on-device
//! test can tell an honest flash from one that acknowledges a flush it never
//! performed. The response is not a better test — it is a design that does not
//! need the answer.
//!
//! So [`WritePlan`] does the crash-safe sequence anyway, because it costs
//! nothing and it is correct on every device that is honest; and [`Receipt`]
//! **refuses to say the file is safe**. There is no `Durable` variant to return.
//! What it reports is where the bytes are and what is still unknown about them.
//!
//! # A write is refused earlier than a read
//!
//! [`crate::site::Availability`] keeps serving a website at `DERATED`, because a
//! file read is not what is heating the device. A *write* is refused there, and
//! the asymmetry is the point: a refused upload costs somebody one retry, and a
//! half-written file outlives the event that interrupted it.
//!
//! The outage ladder decides the rest of it, and it already said so in words.
//! [`Stage::Announced`]'s obligation is *"told the fleet and stopped accepting
//! new work"* — an upload is new work, so `Announced` refuses. Nothing here
//! invents a policy; it reads the one the ladder already carries.

use crate::durability::DurabilityClass;
use crate::governor::Level;
use crate::host::Host;
use crate::shed::Stage;

/// A directory the device keeps files in.
///
/// The field is private and [`VaultRoot::open`] is the only constructor, so a
/// vault nobody asked the host about cannot exist:
///
/// ```compile_fail
/// use vayucell_core::vault::VaultRoot;
/// // A directory that was assumed is a directory that was not checked.
/// let v = VaultRoot { dir: "/data/vault".to_owned() };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRoot {
    dir: String,
}

/// Why a directory cannot be used as a vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootError {
    /// No directory was named.
    Empty,
    /// The host cannot see it.
    Missing(String),
}

impl core::fmt::Display for RootError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RootError::Empty => f.write_str("no directory was given to keep files in"),
            RootError::Missing(d) => write!(
                f,
                "{d} cannot be seen by this process, so there is nowhere to put anything"
            ),
        }
    }
}

impl VaultRoot {
    /// Opens a directory to keep files in.
    ///
    /// # Errors
    ///
    /// Returns why the directory is not usable, in the words an operator reads.
    pub fn open(host: &dyn Host, dir: &str) -> Result<Self, RootError> {
        let dir = dir.trim_end_matches('/');
        if dir.is_empty() {
            return Err(RootError::Empty);
        }
        if !host.exists(dir) {
            return Err(RootError::Missing(dir.to_owned()));
        }
        Ok(Self {
            dir: dir.to_owned(),
        })
    }

    /// The directory being kept.
    #[must_use]
    pub fn dir(&self) -> &str {
        &self.dir
    }
}

/// Why a filename was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    /// Nothing was given.
    Empty,
    /// `.` or `..`, which name a directory rather than a file in it.
    Relative,
    /// Begins with a dot.
    Hidden,
    /// Contains a path separator, so it is a path and not a name.
    Separator,
    /// Contains a NUL or a control character.
    Control,
    /// Longer than [`Name::MAX_BYTES`] bytes.
    TooLong(usize),
    /// Ends in a space or a dot, which several filesystems silently strip.
    TrailingSpaceOrDot,
}

impl core::fmt::Display for NameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NameError::Empty => f.write_str("a file needs a name"),
            NameError::Relative => {
                f.write_str("\".\" and \"..\" name a directory, not a file in one")
            }
            NameError::Hidden => f.write_str(
                "names beginning with a dot are not accepted; they are how a .env or a \
                 .git arrives somewhere nobody looks",
            ),
            NameError::Separator => f.write_str(
                "a name may not contain a path separator — this stores files, not trees",
            ),
            NameError::Control => f.write_str(
                "a name may not contain control characters; they do not survive a terminal",
            ),
            NameError::TooLong(n) => write!(
                f,
                "the name is {n} bytes and the limit is {}; most filesystems refuse longer",
                Name::MAX_BYTES
            ),
            NameError::TrailingSpaceOrDot => f.write_str(
                "a name may not end in a space or a dot: several filesystems strip it \
                 silently, so the file you asked for and the file that exists differ",
            ),
        }
    }
}

/// A filename that has been checked.
///
/// The field is private, so a path cannot be smuggled in as a name:
///
/// ```compile_fail
/// use vayucell_core::vault::Name;
/// // Every refusal in NameError would be bypassed by this one line.
/// let n = Name("../../etc/passwd".to_owned());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Name(String);

impl Name {
    /// The longest name accepted, in bytes.
    ///
    /// Bytes rather than characters. The limit filesystems impose is on bytes,
    /// and a name of 255 emoji is roughly a kilobyte.
    pub const MAX_BYTES: usize = 255;

    /// Checks a filename.
    ///
    /// # Errors
    ///
    /// Returns which rule it broke. Every refusal is named, because "invalid
    /// filename" tells somebody holding a file nothing about what to change.
    pub fn new(raw: &str) -> Result<Self, NameError> {
        if raw.is_empty() {
            return Err(NameError::Empty);
        }
        if raw.len() > Self::MAX_BYTES {
            return Err(NameError::TooLong(raw.len()));
        }
        if raw == "." || raw == ".." {
            return Err(NameError::Relative);
        }
        // Separator before Hidden, deliberately. `../secrets` breaks both rules,
        // and "begins with a dot" is a true statement that sends somebody to fix
        // the wrong half. The structural problem is that they handed over a path
        // where a name was asked for, and that is what they are told.
        if raw.contains('/') || raw.contains('\\') {
            return Err(NameError::Separator);
        }
        if raw.starts_with('.') {
            return Err(NameError::Hidden);
        }
        if raw.chars().any(char::is_control) {
            return Err(NameError::Control);
        }
        if raw.ends_with(' ') || raw.ends_with('.') {
            return Err(NameError::TrailingSpaceOrDot);
        }
        Ok(Self(raw.to_owned()))
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Name {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How much room the vault has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    used: u64,
    limit: u64,
}

/// Why an incoming file does not fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    /// The size of the file that was offered.
    pub offered: u64,
    /// How many bytes are free.
    pub free: u64,
    /// How many bytes would have to be freed for it to fit.
    pub shortfall: u64,
}

impl core::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "this file is {} bytes and only {} are free; {} more would have to be freed first",
            self.offered, self.free, self.shortfall
        )
    }
}

impl Quota {
    /// A quota with a limit and a current usage.
    #[must_use]
    pub const fn new(used: u64, limit: u64) -> Self {
        Self { used, limit }
    }

    /// Bytes still available.
    #[must_use]
    pub const fn free(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    /// Whether a file of this size fits.
    ///
    /// Asked **before** the bytes are taken, never after. A vault that accepts
    /// a file and discovers halfway through that it does not fit has already
    /// spent the write it was trying to avoid, and has to delete a partial file
    /// to recover — which is the failure mode, not the recovery.
    ///
    /// # Errors
    ///
    /// Returns exactly how short it is, so the answer names an amount rather
    /// than saying "full".
    pub const fn admits(&self, offered: u64) -> Result<(), TooLarge> {
        let free = self.free();
        if offered <= free {
            Ok(())
        } else {
            Err(TooLarge {
                offered,
                free,
                shortfall: offered - free,
            })
        }
    }
}

/// One action in a durable write, in the order it must happen.
///
/// Ordered, and the ordering is the whole content of the type: every step is
/// worthless without the ones before it, and [`Step::RenameOverDestination`]
/// before [`Step::FlushFile`] would publish a name pointing at bytes that may
/// not be there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Write every byte to a temporary name beside the destination.
    ///
    /// Beside it, not in `/tmp`: a rename across filesystems is a copy, and a
    /// copy is not atomic.
    WriteTemporary,
    /// Ask the device to put the file's own bytes on the medium.
    FlushFile,
    /// Rename the temporary over the destination.
    ///
    /// The one atomic step. Either the old file is there or the new one is —
    /// a reader can never see a half-written file under the real name.
    RenameOverDestination,
    /// Ask the device to put the *directory entry* on the medium.
    ///
    /// The step everyone forgets. Without it the rename itself can be the thing
    /// that is lost, leaving the old contents under the new expectation.
    FlushDirectory,
}

impl Step {
    /// What this step is for, in one line an operator can read in a log.
    #[must_use]
    pub const fn why(self) -> &'static str {
        match self {
            Step::WriteTemporary => {
                "so a crash leaves a temporary file rather than a damaged one under the real name"
            }
            Step::FlushFile => {
                "so the rename does not publish a name whose bytes are still in cache"
            }
            Step::RenameOverDestination => {
                "the only atomic step: either the old file or the new one, never half of either"
            }
            Step::FlushDirectory => {
                "so the rename itself survives; without it the entry can be the thing that is lost"
            }
        }
    }
}

/// The ordered sequence a caller must perform to write a file.
///
/// Returned as data rather than performed here, so the *ordering* — which is
/// the entire correctness argument — can be asserted by a test that owns no
/// filesystem, and so no caller can invent its own order quietly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePlan {
    temporary: String,
    destination: String,
}

impl WritePlan {
    /// The path to write the bytes to first.
    #[must_use]
    pub fn temporary(&self) -> &str {
        &self.temporary
    }

    /// The path the temporary is renamed over.
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// The directory whose entry must be flushed after the rename.
    #[must_use]
    pub fn directory(&self) -> &str {
        self.destination
            .rsplit_once('/')
            .map_or(".", |(parent, _)| parent)
    }

    /// Every step, in the only order that is safe.
    #[must_use]
    pub const fn steps() -> [Step; 4] {
        [
            Step::WriteTemporary,
            Step::FlushFile,
            Step::RenameOverDestination,
            Step::FlushDirectory,
        ]
    }
}

/// Why the vault will not take a file right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The governor is protecting the cell.
    Governor(Level),
    /// The outage ladder is winding the node down.
    Outage(Stage),
    /// There is not enough room.
    Full(TooLarge),
}

/// Whether the vault is accepting files.
///
/// Two states. There is deliberately no third:
///
/// ```compile_fail
/// use vayucell_core::vault::Admission;
/// // The type annotation is load-bearing: a bare variant name is a constructor,
/// // and without it this would compile as a function value.
/// let a: Admission = Admission::ProbablyFine;
/// ```
///
/// "Degraded", "best effort" and "probably" are all ways of taking somebody's
/// file while reserving the right not to have kept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The file may be written.
    Accepting,
    /// The file is refused, and this is why.
    Refusing(Refused),
}

impl Admission {
    /// Whether the device is in a condition to take a file, and whether it fits.
    ///
    /// Stricter than [`crate::site::Availability`], deliberately. A website is
    /// still served at `DERATED` because reading a file is not what is heating
    /// the device. A write is refused there, because a refused upload costs one
    /// retry and a half-written file outlives the event that interrupted it.
    ///
    /// Only `NORMAL` on mains accepts. `Stage::Announced` refuses, and that is
    /// not a new policy — the ladder already describes that rung as *"stopped
    /// accepting new work"*, and an upload is new work.
    ///
    /// Room is checked last, so an operator whose device is halted is told
    /// about the halt rather than about their disk.
    #[must_use]
    pub const fn of(level: Level, stage: Stage, quota: Quota, offered: u64) -> Self {
        match level {
            Level::Derated | Level::Protect | Level::Halt => {
                return Self::Refusing(Refused::Governor(level))
            }
            Level::Normal => {}
        }
        match stage {
            Stage::Announced | Stage::Shed | Stage::Quiesced | Stage::ShuttingDown => {
                return Self::Refusing(Refused::Outage(stage))
            }
            Stage::Serving => {}
        }
        match quota.admits(offered) {
            Ok(()) => Self::Accepting,
            Err(too_large) => Self::Refusing(Refused::Full(too_large)),
        }
    }

    /// Whether a file may be written.
    #[must_use]
    pub const fn is_accepting(&self) -> bool {
        matches!(self, Self::Accepting)
    }

    /// The plan for writing `name` into `root`, if the vault will take it.
    ///
    /// Returns `None` when it will not, so a caller cannot obtain a plan for a
    /// write the device has refused — the refusal and the plan are the same
    /// decision, and splitting them is how a check gets skipped.
    #[must_use]
    pub fn plan(&self, root: &VaultRoot, name: &Name) -> Option<WritePlan> {
        if !self.is_accepting() {
            return None;
        }
        Some(WritePlan {
            // Beside the destination, and hidden, so a crash leaves something an
            // operator can recognise as debris rather than as their file.
            temporary: format!("{}/.{}.partial", root.dir(), name.as_str()),
            destination: format!("{}/{}", root.dir(), name.as_str()),
        })
    }

    /// What to tell whoever offered the file.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Accepting => "this device is accepting files".to_owned(),
            Self::Refusing(Refused::Governor(level)) => format!(
                "this file was not taken: the device is at {level}, and protecting \
                 its battery outranks storing anything"
            ),
            Self::Refusing(Refused::Outage(stage)) => format!(
                "this file was not taken: the device is on battery and has {}",
                stage.obligation()
            ),
            Self::Refusing(Refused::Full(too_large)) => {
                format!("this file was not taken: {too_large}")
            }
        }
    }
}

/// What the vault is allowed to say once the bytes are written.
///
/// **There is no `Durable` variant, and there will not be one.** ADR-0004 §0:
/// a sealed phone cannot drop its own storage rail, so nothing running on it can
/// tell a flash that honoured a flush from one that acknowledged it and did
/// nothing. A receipt claiming the file is safe would be the exact lie Charter
/// Article IV exists to prevent, issued by the feature meant to uphold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The name the file was stored under.
    pub name: Name,
    /// How many bytes were written.
    pub bytes: u64,
    /// What class of durability applies — never a measured one.
    pub durability: DurabilityClass,
}

impl Receipt {
    /// A receipt for a completed write.
    ///
    /// The class is fixed at [`DurabilityClass::AssumedUntrusted`] rather than
    /// taken as an argument. A caller able to choose the class is a caller able
    /// to choose a flattering one, on the one field a person would rely on.
    #[must_use]
    pub const fn new(name: Name, bytes: u64) -> Self {
        Self {
            name,
            bytes,
            durability: DurabilityClass::AssumedUntrusted,
        }
    }

    /// What to tell the person whose file it is.
    ///
    /// Says where the bytes are and what is still unknown. It never says
    /// "saved", because that word carries a promise this device cannot make.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{} — {} bytes are on this device and on nothing else. {} This device \
             cannot test whether its own storage honoured the flush, so keep a copy \
             elsewhere of anything you would mind losing.",
            self.name,
            self.bytes,
            self.durability.describe()
        )
    }
}
