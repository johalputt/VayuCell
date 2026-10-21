// SPDX-License-Identifier: Apache-2.0

//! Tier 3: what a verified image is, and whether this machine is running it.
//!
//! # The half of P8 that code can carry
//!
//! PLAN §11 gives P8 a gate — *verified images for the top device families* —
//! and that gate needs hardware, a build pipeline, and somebody to maintain
//! both. None of that is a coding problem, and PLAN §11 is explicit that P8 is
//! one of the phases that "none of them got pretend code".
//!
//! What code *can* carry is the half that keeps the claim honest: a written
//! description of what a verified image **is**, and a check of whether the
//! machine in front of us **is** that image. Without those two, "this device
//! is running a verified image" is a sentence in a README, and a README is not
//! a measurement. With them it is a question the software answers on the
//! device, with evidence, every time somebody asks it.
//!
//! # Tier 3 is the first tier where the device tree is visible
//!
//! T0 and T1 hide the charge control behind a vendor kernel; T2 hides it in
//! the Android host, which the guest can only ask about. T3 is different:
//! PLAN §4 puts the ceiling in the **device tree**, and the device tree is a
//! file this process can read. So for the first time in this project a
//! charge-ceiling check can be a *measurement* rather than a request.
//!
//! Which is also why T3 is easy to get wrong in a new way. Everywhere else in
//! this crate "I could not read it" and "it says something else" are close
//! enough to be confused. Here they are the whole answer: a device tree that
//! is absent, a device tree this process may not open, and a device tree that
//! names a different board are three different facts, and only the third one
//! is a disagreement with the manifest.
//!
//! # Four answers per check, and no default
//!
//! Every check here ends in one of four states — matched, differed, absent,
//! unreadable — and the report's headline is *computed* from them. There is no
//! all-good default and no way to set a headline by hand: a machine that
//! answers nothing is not a machine that passed, and a headline that could be
//! set independently of the rows will eventually disagree with them, in the
//! reassuring direction, because that is the direction nobody files a bug
//! about.
//!
//! # What this module is not
//!
//! It is not a signature scheme. It does not attest that an image is
//! *authentic* — only that the running system matches a description somebody
//! wrote down. It does not build images, publish them, or fetch them: there is
//! no network here and no path for one to appear. And it says nothing about
//! the health of a cell; it reports what the device tree *declares*, which is
//! a promise by whoever built the image, not a measurement of a battery.
//!
//! The images themselves remain the open gate. This module is the part that
//! can be written down, reviewed, and failed in a test today.

use core::fmt;
use core::fmt::Write as _;

use crate::host::Host;

/// The first line of every manifest file, and the version it is written in.
///
/// A file that does not begin with this is not a manifest, and a file that
/// cannot be understood is never read as an empty one. Something wrote it;
/// treating it as "this image declares nothing" would be a much stronger
/// claim than "this is not a file we understand", and it would be the
/// reassuring one.
const MANIFEST_HEADER: &str = "vayucell image manifest 1";

/// The path the running kernel publishes its release string at.
///
/// Read rather than asked for: a kernel that will not say which release it is
/// is not a kernel this program may claim anything about.
pub const OSRELEASE: &str = "/proc/sys/kernel/osrelease";

/// Where Linux exposes the flattened device tree.
///
/// Two paths, because which one exists depends on the kernel configuration and
/// on whether the tree is mounted. The tier probe reads the same two, and a
/// check that looked at one while the probe looked at the other would let a
/// machine be T3 by the probe and unreadable by the check at the same time.
pub const DEVICE_TREE_ROOTS: [&str; 2] = ["/proc/device-tree", "/sys/firmware/devicetree/base"];

/// The device-tree property that names the board, relative to a device-tree
/// root.
///
/// On Linux this property holds a **NUL-separated list**, most specific first,
/// so the check asks whether the manifest's string is *in* the list rather
/// than whether it equals the whole file.
pub const COMPATIBLE_PROPERTY: &str = "/compatible";

/// The device-tree property that publishes the maximum charge voltage, in
/// millivolts, relative to a device-tree root.
///
/// This is the node T3 images are expected to publish. It is a **name this
/// project has chosen**, not one that exists anywhere yet: the real images
/// are the open gate on PLAN §11's P8, and whoever builds them has to publish
/// this property under this name for the check to mean anything at all. It is
/// named here anyway, rather than left implicit, because a check that reads
/// "some node somewhere" cannot be written down, reviewed, or failed.
///
/// The property sits under `charger/` because that is where a charge
/// controller's limits live in a device tree; the `/charger` segment is part
/// of the name for the same reason — an image that publishes it elsewhere is
/// an image this check cannot read, and the honest answer to that is
/// "unverified", not "different".
pub const MAX_CHARGE_VOLTAGE_PROPERTY: &str = "/charger/max-charge-voltage-mv";

/// How far a published charge voltage may sit from the manifest's and still
/// count as the same ceiling, in millivolts.
///
/// A tolerance exists because the kernel rounds: a device tree that says
/// 3800 mV may be read back as 3799 or 3801 depending on how the value was
/// scaled into the property, and a check that failed on one millivolt would
/// report a disagreement that is not one. Twenty-five millivolts is about
/// half a percent of a 3.8 V ceiling — far below any difference that could
/// change what a cell does, and far above the rounding that causes it.
///
/// It is a named constant rather than a literal at the comparison because a
/// number nobody can find is a number nobody will argue with when it turns
/// out to be wrong.
pub const CHARGE_TOLERANCE_MV: u32 = 25;

/// A description of a verified image: what it is, and what a machine running
/// it must look like.
///
/// Every field is required and validated at construction. A manifest is a
/// claim about a system, and a claim with a hole in it is a claim that
/// silently checks nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    name: String,
    version: String,
    kernel: String,
    compatible: String,
    max_charge_mv: u32,
}

impl Manifest {
    /// Describes a verified image.
    ///
    /// # Errors
    ///
    /// Refuses a manifest that names nothing, versions nothing, expects no
    /// kernel or targets no board, or declares a maximum charge voltage of
    /// zero.
    ///
    /// The zero ceiling is the refusal that matters most. A maximum charge
    /// voltage of zero does not mean "a very low ceiling" — it means *no
    /// ceiling at all*, and it must not be constructible by accident, because
    /// a manifest that named it would be a manifest saying this image holds
    /// the cell at nothing, which is the opposite of what a T3 image does.
    pub fn new(
        name: &str,
        version: &str,
        kernel: &str,
        compatible: &str,
        max_charge_mv: u32,
    ) -> Result<Self, ImageError> {
        // The ceiling is checked before the strings, because a manifest with
        // a zero ceiling is the one that would be dangerous rather than merely
        // empty, and the caller deserves to be told that first.
        if max_charge_mv == 0 {
            return Err(ImageError::NoCeiling);
        }
        Ok(Self {
            name: field(name, "name")?,
            version: field(version, "version")?,
            kernel: field(kernel, "kernel")?,
            compatible: field(compatible, "compatible")?,
            max_charge_mv,
        })
    }

    /// The name of the image.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The version of the image.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The kernel release the image expects to be running.
    ///
    /// Compared **exactly** against the running kernel's `osrelease`. The
    /// manifest therefore has to name the whole release string — the part
    /// after the version number included — because a prefix comparison would
    /// accept a kernel this image was never built against.
    #[must_use]
    pub fn kernel(&self) -> &str {
        &self.kernel
    }

    /// The device-tree `compatible` string the image targets.
    #[must_use]
    pub fn compatible(&self) -> &str {
        &self.compatible
    }

    /// The maximum charge voltage the image's device tree declares, in
    /// millivolts.
    ///
    /// Never zero. A manifest that carried a zero ceiling would be a manifest
    /// saying the image holds the cell at nothing, and it cannot be built.
    #[must_use]
    pub const fn max_charge_mv(&self) -> u32 {
        self.max_charge_mv
    }

    /// Reads a manifest from its text form.
    ///
    /// The form is a header line, then one `key value` pair per line:
    ///
    /// ```text
    /// vayucell image manifest 1
    /// name postmarketOS-trinity
    /// version 2024.06
    /// kernel 6.9.0-1-postmarketos-trinity
    /// compatible samsung,exynos2100
    /// max-charge-mv 3800
    /// ```
    ///
    /// # Errors
    ///
    /// Returns why the text is not a manifest this build understands. Unknown
    /// keys are refused rather than ignored, a key given twice is refused
    /// rather than resolved, a required key that is missing is an error rather
    /// than a default, and a value may not carry words this build does not
    /// know.
    ///
    /// Every one of those refusals is the same rule: **a manifest that cannot
    /// be understood is an error, never an empty manifest.** A file this build
    /// could not read is not a manifest that declares nothing, and reading it
    /// as one would turn a typo in a text file into a claim about a cell.
    pub fn parse(text: &str) -> Result<Self, ImageError> {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default().trim();
        if header != MANIFEST_HEADER {
            return Err(ImageError::NotAManifest {
                found: header.to_owned(),
            });
        }

        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        let mut kernel: Option<String> = None;
        let mut compatible: Option<String> = None;
        let mut max_charge_mv: Option<u32> = None;

        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let Some(word) = words.next() else {
                continue;
            };
            // An unknown key is refused before anything else is looked at,
            // because a key this build does not know is a key whose value
            // this build would not know how to check. Ignoring it would let
            // anything be smuggled past a rule that was never taught.
            let Some(key) = Key::of(word) else {
                return Err(ImageError::UnknownKey {
                    key: word.to_owned(),
                });
            };
            let Some(value) = words.next() else {
                return Err(ImageError::Blank { key: key.as_str() });
            };
            if let Some(extra) = words.next() {
                return Err(ImageError::ExtraWords {
                    key: key.as_str(),
                    words: extra.to_owned(),
                });
            }
            match key {
                Key::Name => set_once(&mut name, value, key.as_str())?,
                Key::Version => set_once(&mut version, value, key.as_str())?,
                Key::Kernel => set_once(&mut kernel, value, key.as_str())?,
                Key::Compatible => set_once(&mut compatible, value, key.as_str())?,
                Key::MaxChargeMv => set_millivolts(&mut max_charge_mv, value, key.as_str())?,
            }
        }

        let name = name.ok_or(ImageError::MissingKey { key: "name" })?;
        let version = version.ok_or(ImageError::MissingKey { key: "version" })?;
        let kernel = kernel.ok_or(ImageError::MissingKey { key: "kernel" })?;
        let compatible = compatible.ok_or(ImageError::MissingKey { key: "compatible" })?;
        let max_charge_mv = max_charge_mv.ok_or(ImageError::MissingKey {
            key: "max-charge-mv",
        })?;

        // Built through the same constructor as every other manifest, so that
        // there is exactly one place where a manifest is judged valid.
        Self::new(&name, &version, &kernel, &compatible, max_charge_mv)
    }

    /// The manifest as it is written to disk.
    ///
    /// The keys come out in a fixed order and the header comes first, so the
    /// text a manifest renders to is the text this build would write itself.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        // Writing into a String cannot fail, and a manifest is not a place to
        // start propagating an error that cannot happen.
        let _ = writeln!(out, "{MANIFEST_HEADER}");
        let _ = writeln!(out, "name {}", self.name);
        let _ = writeln!(out, "version {}", self.version);
        let _ = writeln!(out, "kernel {}", self.kernel);
        let _ = writeln!(out, "compatible {}", self.compatible);
        let _ = writeln!(out, "max-charge-mv {}", self.max_charge_mv);
        out
    }
}

/// Formats a voltage in millivolts as volts, to three decimals.
///
/// Three decimals because that is the resolution a charge ceiling is talked
/// about in: 3800 mV is 3.800 V, and writing "3.8" would hide the difference
/// between a rounding and a change.
#[must_use]
pub fn volts(millivolts: u32) -> String {
    format!("{}.{:03} V", millivolts / 1000, millivolts % 1000)
}

/// The keys a manifest may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    /// The image's name.
    Name,
    /// The image's version.
    Version,
    /// The kernel release the image expects.
    Kernel,
    /// The device-tree `compatible` string the image targets.
    Compatible,
    /// The maximum charge voltage, in millivolts.
    MaxChargeMv,
}

impl Key {
    /// The key as it is written in a manifest file.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Version => "version",
            Self::Kernel => "kernel",
            Self::Compatible => "compatible",
            Self::MaxChargeMv => "max-charge-mv",
        }
    }

    /// The key a word names, if it names one this build knows.
    fn of(word: &str) -> Option<Self> {
        match word {
            "name" => Some(Self::Name),
            "version" => Some(Self::Version),
            "kernel" => Some(Self::Kernel),
            "compatible" => Some(Self::Compatible),
            "max-charge-mv" => Some(Self::MaxChargeMv),
            _ => None,
        }
    }
}

/// A required string field, refused when it is blank.
fn field(value: &str, key: &'static str) -> Result<String, ImageError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ImageError::Blank { key });
    }
    Ok(trimmed.to_owned())
}

/// Records one string field, refusing a second statement about it.
///
/// A manifest that says the same thing twice has said nothing: resolving it
/// would be this program deciding which of the two the author meant.
fn set_once(slot: &mut Option<String>, value: &str, key: &'static str) -> Result<(), ImageError> {
    if slot.is_some() {
        return Err(ImageError::DuplicateKey { key });
    }
    *slot = Some(value.to_owned());
    Ok(())
}

/// Records the maximum charge voltage, refusing a second statement about it.
fn set_millivolts(
    slot: &mut Option<u32>,
    value: &str,
    key: &'static str,
) -> Result<(), ImageError> {
    if slot.is_some() {
        return Err(ImageError::DuplicateKey { key });
    }
    let millivolts: u32 = value.parse().map_err(|_| ImageError::NotANumber {
        key,
        value: value.to_owned(),
    })?;
    *slot = Some(millivolts);
    Ok(())
}

/// Why a manifest could not be read, or could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// The text does not begin with the manifest header, so it is not a
    /// manifest this build knows.
    NotAManifest {
        /// The first line that was found instead.
        found: String,
    },
    /// A key this build does not know.
    UnknownKey {
        /// The key.
        key: String,
    },
    /// A key was given twice.
    DuplicateKey {
        /// The key.
        key: &'static str,
    },
    /// A required key was absent.
    MissingKey {
        /// The key.
        key: &'static str,
    },
    /// A required field was blank.
    Blank {
        /// The key.
        key: &'static str,
    },
    /// A value that has to be a number was not one.
    NotANumber {
        /// The key.
        key: &'static str,
        /// What was there instead.
        value: String,
    },
    /// A value carried words this build does not know.
    ExtraWords {
        /// The key.
        key: &'static str,
        /// The words that were there.
        words: String,
    },
    /// The manifest declared a maximum charge voltage of zero, which means no
    /// ceiling at all.
    NoCeiling,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAManifest { found } => {
                write!(
                    f,
                    "this is not a manifest: it begins with {found:?}, not {MANIFEST_HEADER:?}"
                )
            }
            Self::UnknownKey { key } => {
                write!(f, "{key:?} is not a key this build knows")
            }
            Self::DuplicateKey { key } => {
                write!(
                    f,
                    "{key:?} is given twice, and a manifest says each thing once"
                )
            }
            Self::MissingKey { key } => {
                write!(
                    f,
                    "the manifest does not say {key:?}, and a manifest says everything"
                )
            }
            Self::Blank { key } => {
                write!(f, "{key:?} is blank, which says nothing")
            }
            Self::NotANumber { key, value } => {
                write!(f, "{key:?} is {value:?}, which is not a number")
            }
            Self::ExtraWords { key, words } => {
                write!(
                    f,
                    "{key:?} is followed by {words:?}, which this build does not know"
                )
            }
            Self::NoCeiling => {
                write!(
                    f,
                    "a maximum charge voltage of zero is not a low ceiling, it is no ceiling at all"
                )
            }
        }
    }
}

/// What was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// The release string of the running kernel.
    Kernel,
    /// The device tree's `compatible` property.
    DeviceTree,
    /// The device tree's maximum charge voltage.
    ChargeVoltage,
}

impl Subject {
    /// Every subject a report covers, in the order it is checked.
    pub const ALL: [Self; 3] = [Self::Kernel, Self::DeviceTree, Self::ChargeVoltage];

    /// The name a report prints.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Kernel => "running kernel",
            Self::DeviceTree => "device tree",
            Self::ChargeVoltage => "maximum charge voltage",
        }
    }
}

/// What one check concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The value was found, and it agrees with the manifest.
    Matched,
    /// The value was found, and it does not agree with the manifest.
    Differed,
    /// The node is not on this system.
    Absent,
    /// The node is on this system, and its value could not be read.
    Unreadable,
}

impl Status {
    /// The single word a report prints.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Matched => "MATCH",
            Self::Differed => "DIFFERS",
            Self::Absent => "ABSENT",
            Self::Unreadable => "UNREADABLE",
        }
    }

    /// The report-level outcome this status contributes.
    ///
    /// A check that could not be performed is [`Outcome::Unverified`], never
    /// [`Outcome::Verified`]: "we could not look" is not "it is fine", and a
    /// boolean would have forced the two into one variant.
    #[must_use]
    pub const fn outcome(self) -> Outcome {
        match self {
            Self::Matched => Outcome::Verified,
            Self::Differed => Outcome::Mismatched,
            Self::Absent | Self::Unreadable => Outcome::Unverified,
        }
    }
}

/// What a report concluded about the running system.
///
/// The variants are ordered by how much they should worry a reader, so the
/// worst finding wins by [`Ord`] rather than by a precedence table somebody
/// has to keep correct. A definite disagreement with the manifest outranks a
/// question that went unanswered: the first is a fact about this machine, and
/// the second is a gap in what we know about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    /// Every check was performed, and every one of them agreed.
    Verified,
    /// At least one check could not be performed.
    Unverified,
    /// At least one check was performed, and it did not agree.
    Mismatched,
}

impl Outcome {
    /// The word a report prints as its headline.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Unverified => "NOT FULLY VERIFIED",
            Self::Mismatched => "MISMATCHED",
        }
    }

    /// The worse of two outcomes.
    #[must_use]
    pub const fn worse(self, other: Self) -> Self {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// How much this outcome should worry a reader. Bigger is worse.
    const fn rank(self) -> u8 {
        match self {
            Self::Verified => 0,
            Self::Unverified => 1,
            Self::Mismatched => 2,
        }
    }
}

/// One check, with the evidence behind it.
///
/// The fields are private on purpose. A finding that could be built by a
/// caller with an empty evidence string would render as a confident-looking
/// row with nothing behind it, which is the exact shape of the failure this
/// whole module exists to prevent. Every finding in this crate comes from a
/// check that looked at something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    subject: Subject,
    status: Status,
    evidence: String,
}

impl Finding {
    /// What was checked.
    #[must_use]
    pub const fn subject(&self) -> Subject {
        self.subject
    }

    /// What came of it.
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// What was found, or what stopped the check.
    #[must_use]
    pub fn evidence(&self) -> &str {
        &self.evidence
    }

    /// Records a check and its evidence.
    fn new(subject: Subject, status: Status, evidence: String) -> Self {
        Self {
            subject,
            status,
            evidence,
        }
    }
}

/// What a check of the running system against a manifest found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    manifest: Manifest,
    findings: Vec<Finding>,
}

impl Report {
    /// The manifest this report was made against.
    #[must_use]
    pub const fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Every check, in the order it was performed.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// The check about one subject, if there was one.
    #[must_use]
    pub fn finding(&self, subject: Subject) -> Option<&Finding> {
        self.findings.iter().find(|f| f.subject == subject)
    }

    /// The headline, computed from the findings.
    ///
    /// Never stored and never passed in. A headline that could be set
    /// independently of the rows is a headline that will eventually disagree
    /// with them, and the disagreement will be in the reassuring direction
    /// because that is the direction nobody files a bug about.
    ///
    /// The worst finding wins, by the ordering of [`Outcome`]'s variants
    /// rather than by a precedence table somebody has to keep correct.
    ///
    /// A report with no findings at all is [`Outcome::Unverified`]. A machine
    /// that answered nothing has not passed.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        self.findings
            .iter()
            .map(|f| f.status.outcome())
            .max()
            .unwrap_or(Outcome::Unverified)
    }

    /// The report as text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "IMAGE {} {} — {}\n\n",
            self.manifest.name(),
            self.manifest.version(),
            self.outcome().label()
        );
        for finding in &self.findings {
            // Writing into a String cannot fail, and a report is not a place
            // to start propagating an error that cannot happen.
            let _ = writeln!(
                out,
                "  {:<11} {:<24} {}",
                finding.status.label(),
                finding.subject.label(),
                finding.evidence
            );
        }
        out
    }
}

/// Checks the running system against a manifest.
///
/// Every check reads the machine through [`Host`], so the whole of this
/// function can be exercised against a fake describing a device nobody in
/// this room is holding — which is the only way a check like this can be
/// written before the images exist.
#[must_use]
pub fn check(host: &dyn Host, manifest: &Manifest) -> Report {
    let findings = vec![
        check_kernel(host, manifest),
        check_device_tree(host, manifest),
        check_charge_voltage(host, manifest),
    ];
    Report {
        manifest: manifest.clone(),
        findings,
    }
}

/// Checks the running kernel against the manifest.
fn check_kernel(host: &dyn Host, manifest: &Manifest) -> Finding {
    match host.read(OSRELEASE) {
        Some(text) => {
            let running = text.trim();
            if running.is_empty() {
                return Finding::new(
                    Subject::Kernel,
                    Status::Unreadable,
                    format!(
                        "{OSRELEASE} is empty, so which kernel is running is unanswered \
                         rather than known"
                    ),
                );
            }
            if running == manifest.kernel() {
                Finding::new(
                    Subject::Kernel,
                    Status::Matched,
                    format!("{OSRELEASE} is {running}, the kernel this image was verified against"),
                )
            } else {
                Finding::new(
                    Subject::Kernel,
                    Status::Differed,
                    format!(
                        "{OSRELEASE} is {running}, but this image was verified against {}",
                        manifest.kernel()
                    ),
                )
            }
        }
        None => {
            if host.exists(OSRELEASE) {
                Finding::new(
                    Subject::Kernel,
                    Status::Unreadable,
                    format!(
                        "{OSRELEASE} exists but could not be read, so which kernel is running \
                         is unanswered rather than known"
                    ),
                )
            } else {
                Finding::new(
                    Subject::Kernel,
                    Status::Absent,
                    format!(
                        "{OSRELEASE} is not on this system, so which kernel is running is \
                         unanswered rather than known"
                    ),
                )
            }
        }
    }
}

/// Checks the device tree against the manifest.
fn check_device_tree(host: &dyn Host, manifest: &Manifest) -> Finding {
    match read_tree(host, COMPATIBLE_PROPERTY) {
        TreeRead::Found { path, value } => {
            // The property is a NUL-separated list on Linux, and a list is
            // what a device tree publishes: the most specific string first,
            // then its parents. So the question is whether the manifest's
            // string is in the list, not whether it is the whole file.
            let listed: Vec<&str> = value.split('\0').filter(|s| !s.is_empty()).collect();
            if listed.is_empty() {
                return Finding::new(
                    Subject::DeviceTree,
                    Status::Unreadable,
                    format!("{path} is empty, so which board this is could not be read"),
                );
            }
            if listed.contains(&manifest.compatible()) {
                Finding::new(
                    Subject::DeviceTree,
                    Status::Matched,
                    format!(
                        "{path} names {}, the board this image was verified against",
                        manifest.compatible()
                    ),
                )
            } else {
                Finding::new(
                    Subject::DeviceTree,
                    Status::Differed,
                    format!(
                        "{path} names {listed:?}, none of which is {}",
                        manifest.compatible()
                    ),
                )
            }
        }
        TreeRead::Unreadable { path } => Finding::new(
            Subject::DeviceTree,
            Status::Unreadable,
            format!(
                "{path} exists but could not be read, so which board this is is unanswered \
                 rather than known"
            ),
        ),
        TreeRead::Absent => Finding::new(
            Subject::DeviceTree,
            Status::Absent,
            format!(
                "no device tree on this system publishes {COMPATIBLE_PROPERTY}: neither {} \
                 nor {} is here, so this is not a T3 machine",
                DEVICE_TREE_ROOTS[0], DEVICE_TREE_ROOTS[1]
            ),
        ),
    }
}

/// Checks the device tree's maximum charge voltage against the manifest.
fn check_charge_voltage(host: &dyn Host, manifest: &Manifest) -> Finding {
    let expected = manifest.max_charge_mv();
    match read_tree(host, MAX_CHARGE_VOLTAGE_PROPERTY) {
        TreeRead::Found { path, value } => match value.trim().parse::<u32>() {
            Ok(found) => {
                if found.abs_diff(expected) <= CHARGE_TOLERANCE_MV {
                    Finding::new(
                        Subject::ChargeVoltage,
                        Status::Matched,
                        format!(
                            "{path} publishes {found} mV ({}), within {CHARGE_TOLERANCE_MV} mV \
                             of the {expected} mV ({}) this image declares",
                            volts(found),
                            volts(expected)
                        ),
                    )
                } else {
                    Finding::new(
                        Subject::ChargeVoltage,
                        Status::Differed,
                        format!(
                            "{path} publishes {found} mV ({}), but this image declares \
                             {expected} mV ({})",
                            volts(found),
                            volts(expected)
                        ),
                    )
                }
            }
            Err(_) => Finding::new(
                Subject::ChargeVoltage,
                Status::Unreadable,
                format!(
                    "{path} contains {:?}, which is not a voltage in millivolts, so the \
                     ceiling in force could not be read",
                    value.trim()
                ),
            ),
        },
        TreeRead::Unreadable { path } => Finding::new(
            Subject::ChargeVoltage,
            Status::Unreadable,
            format!(
                "{path} exists but could not be read, so the ceiling in force is unanswered \
                 rather than known"
            ),
        ),
        TreeRead::Absent => Finding::new(
            Subject::ChargeVoltage,
            Status::Absent,
            format!(
                "no device tree on this system publishes {MAX_CHARGE_VOLTAGE_PROPERTY}, so \
                 the ceiling in force is unanswered rather than known"
            ),
        ),
    }
}

/// What a device-tree property said when it was asked for.
enum TreeRead {
    /// The property was read, at this path.
    Found {
        /// The path it was read from.
        path: String,
        /// What it said.
        value: String,
    },
    /// The property exists at this path, and could not be read.
    Unreadable {
        /// The path.
        path: String,
    },
    /// No device tree on this system publishes the property.
    Absent,
}

/// Reads a device-tree property, keeping absent and unreadable apart.
///
/// The two roots are tried in turn, because which one exists depends on the
/// kernel and on the mount. A root whose property exists but cannot be read
/// does not stop the other root from being tried — a machine that publishes
/// the tree in two places and opens one of them is still a machine that
/// publishes it.
fn read_tree(host: &dyn Host, property: &str) -> TreeRead {
    let mut unreadable: Option<String> = None;
    for root in DEVICE_TREE_ROOTS {
        let path = format!("{root}{property}");
        if !host.exists(&path) {
            continue;
        }
        if let Some(value) = host.read(&path) {
            return TreeRead::Found { path, value };
        }
        if unreadable.is_none() {
            unreadable = Some(path);
        }
    }
    match unreadable {
        Some(path) => TreeRead::Unreadable { path },
        None => TreeRead::Absent,
    }
}
