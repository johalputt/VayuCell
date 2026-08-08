// SPDX-License-Identifier: Apache-2.0

//! Per-device credentials — deciding *whose* file it is.
//!
//! [`crate::vault`] settled whether the device is in a condition to take a file.
//! This settles whether the thing offering it is allowed to. Neither is useful
//! without the other, and shipping the vault's decision without this one would
//! have put a writable endpoint on a home network.
//!
//! # Secrets are minted, never chosen, and that is what makes this honest
//!
//! Charter V.5 forbids third-party runtime dependencies, so there is no
//! `argon2` and no `bcrypt` here. That constraint is fine for exactly one design
//! and fatal for every other: **the human never picks the secret.**
//!
//! A password a person chose needs a memory-hard derivation to survive an
//! offline guessing attack, and hand-rolling one of those would be the worst
//! possible thing to do under a no-dependencies rule. A secret that is 256 bits
//! from the kernel has nothing to guess — an attacker with the whole store and
//! unlimited time is doing arithmetic against the search space, not against a
//! password list. So [`Secret`] is a fixed-length random value, and there is no
//! constructor that accepts a memorable one.
//!
//! # What this does not protect against, said here rather than implied
//!
//! **The store holds the secrets themselves, not hashes of them.** That is a
//! real limitation with a specific reason: an attacker who can read the store
//! is already the same user, on the same filesystem, as the vault it protects —
//! so hashing at rest would defend the credential and lose the files it guards
//! in the same breath. It buys nothing here that matters.
//!
//! What it does mean is that the file's permissions are load-bearing, and
//! "absence is never protection" applies to them too: [`readable_by_others`]
//! exists so a caller checks the mode rather than assuming it, and the binary
//! refuses to start on a store anyone else can read.
//!
//! # An empty store refuses everything
//!
//! The most dangerous default in this module would be for "no credentials
//! configured" to mean "no authentication required". [`Credentials::verify`]
//! returns [`Refusal::StoreEmpty`], and a test asserts it — because that is the
//! state every installation begins in.

use core::fmt;

/// How many bytes of kernel randomness a secret carries.
pub const SECRET_BYTES: usize = 32;

/// The exact length of an encoded secret, in base64url characters.
///
/// `ceil(32 / 3) * 4` minus the padding: 43 characters for 256 bits. Fixed, so
/// [`Secret::new`] can check it, and public, so nothing is leaked by a
/// comparison whose duration depends on it.
pub const SECRET_CHARS: usize = 43;

/// Compares two byte strings without letting the duration reveal the contents.
///
/// A `==` on a secret returns as soon as it finds a difference, so the time it
/// takes says how many leading bytes were right — which is enough to recover a
/// credential one byte at a time. This accumulates instead, and never returns
/// early.
///
/// The *length* is not hidden and does not need to be: every secret here is
/// exactly [`SECRET_CHARS`] long by construction, so length carries nothing an
/// attacker did not already know.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        difference |= x ^ y;
    }
    difference == 0
}

/// Why a proposed secret was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    /// Not [`SECRET_CHARS`] characters.
    WrongLength(usize),
    /// A character outside the base64url alphabet.
    IllegalCharacter,
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretError::WrongLength(n) => write!(
                f,
                "a secret is {SECRET_CHARS} characters and this is {n}; secrets are \
                 minted by the device, never typed by a person"
            ),
            SecretError::IllegalCharacter => f.write_str(
                "a secret contains a character outside base64url, so it was not \
                 produced by this program",
            ),
        }
    }
}

/// A device's secret.
///
/// The field is private, so a short or memorable value cannot become one:
///
/// ```compile_fail
/// use vayucell_core::auth::Secret;
/// // Every length and alphabet check would be bypassed by this one line.
/// let s = Secret("hunter2".to_owned());
/// ```
///
/// It deliberately does **not** derive `Debug`. A derived `Debug` puts the
/// secret into every `{:?}`, every `unwrap` panic and every log line that ever
/// prints a structure containing one, and none of those call sites look like a
/// disclosure when you read them.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Accepts a minted secret.
    ///
    /// # Errors
    ///
    /// Returns why it was refused. There is no path that accepts a value a
    /// person invented, which is the property the whole module rests on.
    pub fn new(raw: &str) -> Result<Self, SecretError> {
        if raw.chars().count() != SECRET_CHARS {
            return Err(SecretError::WrongLength(raw.chars().count()));
        }
        if !raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(SecretError::IllegalCharacter);
        }
        Ok(Self(raw.to_owned()))
    }

    /// The bytes, for comparison only.
    ///
    /// Deliberately not a `Display` or an `as_str`: the only legitimate use is
    /// [`constant_time_eq`], and a `&str` invites `format!`.
    #[must_use]
    pub fn expose_for_comparison(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Prints nothing. A secret in a log is a secret that has left the building.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(hidden)")
    }
}

/// Why a device name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceError {
    /// Nothing was given.
    Empty,
    /// Longer than [`DeviceName::MAX_BYTES`].
    TooLong(usize),
    /// A control character, which would rewrite the line it is logged on.
    Control,
    /// Whitespace, which the store's line format uses as its separator.
    Whitespace,
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::Empty => f.write_str("a device needs a name you will recognise later"),
            DeviceError::TooLong(n) => write!(
                f,
                "the name is {n} bytes and the limit is {}",
                DeviceName::MAX_BYTES
            ),
            DeviceError::Control => f.write_str(
                "a device name may not contain control characters; they rewrite the \
                 log line that reports them",
            ),
            DeviceError::Whitespace => f.write_str(
                "a device name may not contain spaces or tabs; the credential store \
                 separates its two fields with one",
            ),
        }
    }
}

/// What the operator calls a device.
///
/// Exists so a person can look at a list and revoke the right one. A store of
/// anonymous secrets is a store nobody can safely prune.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceName(String);

impl DeviceName {
    /// The longest device name accepted, in bytes.
    pub const MAX_BYTES: usize = 64;

    /// Checks a device name.
    ///
    /// # Errors
    ///
    /// Returns which rule it broke.
    pub fn new(raw: &str) -> Result<Self, DeviceError> {
        if raw.is_empty() {
            return Err(DeviceError::Empty);
        }
        if raw.len() > Self::MAX_BYTES {
            return Err(DeviceError::TooLong(raw.len()));
        }
        if raw.chars().any(char::is_control) {
            return Err(DeviceError::Control);
        }
        if raw.chars().any(char::is_whitespace) {
            return Err(DeviceError::Whitespace);
        }
        Ok(Self(raw.to_owned()))
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One device's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    /// What the operator calls it.
    pub device: DeviceName,
    /// What it presents.
    pub secret: Secret,
}

/// Why a request was not authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No credential was presented at all.
    NoneOffered,
    /// A credential was presented and matches nothing.
    NotRecognised,
    /// No devices are enrolled, so nothing can be recognised.
    ///
    /// Distinguished from [`Refusal::NotRecognised`] because it is the state
    /// every installation starts in, and an operator who has not enrolled a
    /// device needs to be told that rather than told their secret is wrong.
    StoreEmpty,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NoneOffered => f.write_str("no credential was presented"),
            Refusal::NotRecognised => f.write_str("that credential is not enrolled on this device"),
            Refusal::StoreEmpty => {
                f.write_str("no device is enrolled on this device yet, so nothing can be accepted")
            }
        }
    }
}

/// The result of checking a credential.
///
/// Two states. There is deliberately no third:
///
/// ```compile_fail
/// use vayucell_core::auth::Verdict;
/// // The type annotation is load-bearing: a bare variant name is a constructor.
/// let v: Verdict = Verdict::ProbablyFine;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// This device is enrolled, and this is which one.
    Authenticated(DeviceName),
    /// Refused, and why.
    Refused(Refusal),
}

impl Verdict {
    /// Whether the request may proceed.
    #[must_use]
    pub const fn is_authenticated(&self) -> bool {
        matches!(self, Verdict::Authenticated(_))
    }
}

/// Every device enrolled on this cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    entries: Vec<Credential>,
}

impl Credentials {
    /// A store holding these devices.
    #[must_use]
    pub fn new(entries: Vec<Credential>) -> Self {
        Self { entries }
    }

    /// A store with nothing in it, which accepts nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// How many devices are enrolled.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no device is enrolled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The enrolled device names, for showing an operator what to revoke.
    #[must_use]
    pub fn devices(&self) -> Vec<&DeviceName> {
        self.entries.iter().map(|c| &c.device).collect()
    }

    /// Checks an offered secret against every enrolled device.
    ///
    /// **Every entry is compared, every time.** Returning on the first match
    /// would make the answer arrive sooner for a device enrolled early than for
    /// one enrolled late, which is a usable signal about the store's contents.
    /// The loop runs to the end and the match is recorded rather than returned.
    #[must_use]
    pub fn verify(&self, offered: Option<&str>) -> Verdict {
        let Some(offered) = offered else {
            return Verdict::Refused(Refusal::NoneOffered);
        };
        // An empty store is not an open door. This is the state every
        // installation begins in, so it is the one worth being loudest about.
        if self.entries.is_empty() {
            return Verdict::Refused(Refusal::StoreEmpty);
        }

        let offered = offered.as_bytes();
        let mut matched: Option<&DeviceName> = None;
        for entry in &self.entries {
            if constant_time_eq(entry.secret.expose_for_comparison(), offered) {
                matched = Some(&entry.device);
            }
        }
        match matched {
            Some(device) => Verdict::Authenticated(device.clone()),
            None => Verdict::Refused(Refusal::NotRecognised),
        }
    }
}

/// Whether a file mode lets anybody but the owner read the file.
///
/// Split out as a pure function of the mode so the rule is testable without a
/// filesystem, and so the binary has one place to ask rather than an inline
/// bitmask nobody reviews.
///
/// The store holds secrets in the clear — see the module documentation for why
/// that is the right trade here — which makes the mode the whole of the
/// protection. Absence is never protection, so it is checked rather than
/// assumed.
#[must_use]
pub const fn readable_by_others(mode: u32) -> bool {
    // Group and other, read or write or execute. Anything but owner-only.
    mode & 0o077 != 0
}

/// Parses a credential store.
///
/// One device per line, `name` and `secret` separated by whitespace. Blank
/// lines and `#` comments are skipped so an operator can annotate which device
/// is which.
///
/// # Errors
///
/// Returns the line number and what was wrong with it. A store that is
/// partially valid is **not** loaded partially: a typo on line four must not
/// silently leave a device unenrolled, because the symptom is a device that
/// stops working for a reason nobody connects to the edit.
pub fn parse_store(text: &str) -> Result<Credentials, StoreError> {
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(name), Some(secret)) = (fields.next(), fields.next()) else {
            return Err(StoreError {
                line: line_number,
                why: StoreProblem::NotTwoFields,
            });
        };
        if fields.next().is_some() {
            return Err(StoreError {
                line: line_number,
                why: StoreProblem::NotTwoFields,
            });
        }
        let device = DeviceName::new(name).map_err(|e| StoreError {
            line: line_number,
            why: StoreProblem::Device(e),
        })?;
        let secret = Secret::new(secret).map_err(|e| StoreError {
            line: line_number,
            why: StoreProblem::Secret(e),
        })?;
        entries.push(Credential { device, secret });
    }
    Ok(Credentials::new(entries))
}

/// What was wrong with a line of the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreProblem {
    /// The line was not exactly a name and a secret.
    NotTwoFields,
    /// The name was refused.
    Device(DeviceError),
    /// The secret was refused.
    Secret(SecretError),
}

/// A refused store, with the line to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError {
    /// Which line, counting from one.
    pub line: usize,
    /// What was wrong with it.
    pub why: StoreProblem,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: ", self.line)?;
        match &self.why {
            StoreProblem::NotTwoFields => {
                f.write_str("expected a device name and a secret, separated by a space")
            }
            StoreProblem::Device(e) => write!(f, "{e}"),
            StoreProblem::Secret(e) => write!(f, "{e}"),
        }
    }
}
