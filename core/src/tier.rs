// SPDX-License-Identifier: Apache-2.0

//! Tier detection, per ADR-0001 §2.
//!
//! # Positive evidence only
//!
//! A tier is never concluded from the *absence* of a contrary signal. T1 is not
//! "we did not see a locked bootloader"; T1 is "this process asked the kernel
//! who it is and was told zero". The negative form silently promotes every
//! device whose probe merely failed to run, which is the same defect class as a
//! report that reads green because nothing checked it.
//!
//! # The honest limit: a guest cannot see the phone
//!
//! T2 is a Linux guest under the platform hypervisor. From *inside* that guest
//! the real hardware is not visible — the guest sees virtual devices, so it
//! cannot tell a phone from a laptop. Self-detection of T2 is therefore
//! **impossible to do honestly**, and this module does not pretend otherwise:
//! virtualisation alone yields [`Verdict::Unverified`] with the reason, and T2
//! is concluded only when the Android shell asserts it through
//! [`SHELL_ASSERTION_ENV`].
//!
//! That is a real limitation, discovered by asking what the probe could
//! actually see rather than by assuming it could see everything.

use crate::capability::Tier;
use crate::host::Host;

/// The environment variable through which the Android shell tells the core it
/// is running as a guest on a handset.
///
/// The shell has the platform APIs to know this; the guest does not. Trusting
/// it is a deliberate, named dependency rather than a guess.
pub const SHELL_ASSERTION_ENV: &str = "VAYUCELL_HOST_ASSERTION";

/// What a single probe concluded, with the evidence for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What was probed, in operator-readable words.
    pub what: &'static str,
    /// Whether the signal was found.
    pub found: bool,
    /// The path, value or reason behind the answer.
    pub evidence: String,
}

impl Finding {
    fn yes(what: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            what,
            found: true,
            evidence: evidence.into(),
        }
    }
    fn no(what: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            what,
            found: false,
            evidence: evidence.into(),
        }
    }
}

/// The outcome of tier detection.
///
/// There is no "assume T0" case. A machine that presents no recognised evidence
/// is [`Verdict::Unknown`], and a capability floor is satisfied by nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A tier was established from positive evidence.
    Established(Tier),
    /// Signals were found but do not settle the question, and guessing would be
    /// dishonest. Carries the reason.
    Unverified(String),
    /// Nothing recognised this machine as a target device at all.
    Unknown,
}

impl Verdict {
    /// The tier, if one was actually established.
    ///
    /// [`Verdict::Unverified`] and [`Verdict::Unknown`] both yield `None`, so a
    /// caller cannot accidentally treat "could not tell" as a tier.
    #[must_use]
    pub fn tier(&self) -> Option<Tier> {
        match self {
            Verdict::Established(t) => Some(*t),
            _ => None,
        }
    }
}

/// A verdict plus every finding that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    /// What was concluded.
    pub verdict: Verdict,
    /// Everything that was checked, in the order it was checked.
    pub findings: Vec<Finding>,
}

/// Detects which tier this machine provides.
///
/// See the module documentation for why virtualisation alone is not enough to
/// conclude T2.
#[must_use]
pub fn detect(host: &dyn Host) -> Detection {
    let mut findings = Vec::new();

    let signals = Signals {
        userspace: probe_android(host, &mut findings),
        privilege: probe_root(host, &mut findings),
        platform: probe_virtualised(host, &mut findings),
        silicon: probe_mobile_hardware(host, &mut findings),
        assertion: probe_shell_assertion(host, &mut findings),
    };

    Detection {
        verdict: decide(&signals),
        findings,
    }
}

/// What the probes saw.
///
/// These began as five positional `bool` parameters, and the lint that objected
/// was right for a reason worth writing down. Two booleans transposed at the
/// call site is a compiling, passing, *wrong* verdict — a rooted handset read as
/// a bare virtual machine — and no test necessarily catches the swap.
///
/// The deeper defect the rewrite exposed: `bool` forced
/// [`Silicon::Unreadable`] to be reported as `false`, identical to "no mobile
/// silicon here". The finding recorded the difference and the decision could not
/// see it. That is precisely the collapse CHARTER Article IV forbids —
/// unverified read as clean — hiding inside a return type. A three-variant enum
/// makes it unrepresentable.
struct Signals {
    userspace: Userspace,
    privilege: Privilege,
    platform: Platform,
    silicon: Silicon,
    assertion: Assertion,
}

/// Whether Android userspace is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Userspace {
    Android,
    NotAndroid,
}

/// What the kernel says about this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Privilege {
    Root,
    Unprivileged,
}

/// Whether a hypervisor is underneath.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Virtualised,
    Bare,
}

/// What the device tree says about the system-on-chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Silicon {
    /// The device tree names a mobile system-on-chip.
    Mobile,
    /// The device tree was read and names nothing mobile.
    NotMobile,
    /// The device tree exists but this process may not read it. Distinct from
    /// [`Silicon::NotMobile`] because "we could not look" is not an answer.
    Unreadable,
}

/// Whether the Android shell vouched for this guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assertion {
    AndroidGuest,
    Absent,
}

fn decide(s: &Signals) -> Verdict {
    // A guest that the shell vouched for is T2. The assertion is the only way
    // this can be known from inside, per the module documentation.
    if s.platform == Platform::Virtualised && s.assertion == Assertion::AndroidGuest {
        return Verdict::Established(Tier::T2);
    }
    // Android userspace: root decides between T1 and T0.
    if s.userspace == Userspace::Android {
        return Verdict::Established(match s.privilege {
            Privilege::Root => Tier::T1,
            Privilege::Unprivileged => Tier::T0,
        });
    }
    // Linux on recognisably mobile hardware, no Android userspace: a mainline
    // port. This is the one tier that CAN be concluded from the device tree,
    // because a mainline port is not hiding behind a hypervisor.
    if s.silicon == Silicon::Mobile {
        return Verdict::Established(Tier::T3);
    }
    if s.platform == Platform::Virtualised {
        return Verdict::Unverified(format!(
            "running in a virtual machine, but nothing establishes that the host is a handset; \
             a guest cannot see the real hardware. Set {SHELL_ASSERTION_ENV} from the Android \
             shell to assert it"
        ));
    }
    // The device tree is the last thing that could have named this machine, and
    // it was there but closed to us. Reporting Unknown would say "nothing
    // recognised this device", which is a stronger claim than we earned: the
    // honest answer is that the one remaining question went unanswered.
    if s.silicon == Silicon::Unreadable {
        return Verdict::Unverified(
            "the device tree exists but could not be read, so whether this is mobile silicon \
             is unanswered rather than answered no; re-run with permission to read it"
                .to_owned(),
        );
    }
    Verdict::Unknown
}

/// Android userspace leaves several unambiguous traces. Any one is positive
/// evidence; none of them is inferred from something else being missing.
fn probe_android(host: &dyn Host, out: &mut Vec<Finding>) -> Userspace {
    const MARKERS: &[&str] = &[
        "/system/build.prop",
        "/system/bin/getprop",
        "/init.environ.rc",
    ];
    for m in MARKERS {
        if host.exists(m) {
            out.push(Finding::yes("android userspace", format!("{m} exists")));
            return Userspace::Android;
        }
    }
    if let Some(v) = host.read("/proc/version") {
        if v.contains("Android") {
            out.push(Finding::yes(
                "android userspace",
                "/proc/version names Android",
            ));
            return Userspace::Android;
        }
    }
    if host.env("ANDROID_ROOT").is_some() || host.env("ANDROID_DATA").is_some() {
        out.push(Finding::yes(
            "android userspace",
            "ANDROID_ROOT/ANDROID_DATA set",
        ));
        return Userspace::Android;
    }
    out.push(Finding::no("android userspace", "no Android marker found"));
    Userspace::NotAndroid
}

/// Root is concluded from the kernel's answer about this process, never from
/// the absence of a restriction.
fn probe_root(host: &dyn Host, out: &mut Vec<Finding>) -> Privilege {
    let euid = host.euid();
    if euid == 0 {
        out.push(Finding::yes("root", "effective uid is 0"));
        Privilege::Root
    } else {
        out.push(Finding::no("root", format!("effective uid is {euid}")));
        Privilege::Unprivileged
    }
}

/// Virtualisation has several honest signals; none of them says *what* is
/// underneath, which is the whole difficulty.
fn probe_virtualised(host: &dyn Host, out: &mut Vec<Finding>) -> Platform {
    if let Some(t) = host.read("/sys/hypervisor/type") {
        let t = t.trim();
        if !t.is_empty() {
            out.push(Finding::yes(
                "virtualised",
                format!("/sys/hypervisor/type is {t}"),
            ));
            return Platform::Virtualised;
        }
    }
    if let Some(c) = host.read("/proc/cpuinfo") {
        if c.contains("hypervisor") {
            out.push(Finding::yes(
                "virtualised",
                "/proc/cpuinfo reports a hypervisor",
            ));
            return Platform::Virtualised;
        }
    }
    for (path, want) in [
        ("/sys/class/dmi/id/sys_vendor", "QEMU"),
        ("/sys/devices/virtual/dmi/id/sys_vendor", "QEMU"),
    ] {
        if let Some(v) = host.read(path) {
            if v.contains(want) {
                out.push(Finding::yes(
                    "virtualised",
                    format!("{path} reports {}", v.trim()),
                ));
                return Platform::Virtualised;
            }
        }
    }
    out.push(Finding::no("virtualised", "no hypervisor signal"));
    Platform::Bare
}

/// Mobile hardware is read from the device tree, which names the board and the
/// system-on-chip on a real handset and is absent inside a guest.
fn probe_mobile_hardware(host: &dyn Host, out: &mut Vec<Finding>) -> Silicon {
    const DT: &[&str] = &[
        "/proc/device-tree/compatible",
        "/sys/firmware/devicetree/base/compatible",
    ];
    const MOBILE_SOC: &[&str] = &[
        "qcom,",
        "mediatek,",
        "exynos",
        "samsung,",
        "hisilicon",
        "rockchip",
    ];

    for path in DT {
        // exists-but-unreadable must not be read as "no mobile hardware".
        if host.exists(path) && host.read(path).is_none() {
            out.push(Finding::no(
                "mobile hardware",
                format!("{path} exists but is unreadable"),
            ));
            return Silicon::Unreadable;
        }
        if let Some(c) = host.read(path) {
            let lower = c.to_ascii_lowercase();
            if let Some(m) = MOBILE_SOC.iter().find(|m| lower.contains(**m)) {
                out.push(Finding::yes("mobile hardware", format!("{path} names {m}")));
                return Silicon::Mobile;
            }
        }
    }
    out.push(Finding::no(
        "mobile hardware",
        "device tree names no mobile system-on-chip",
    ));
    Silicon::NotMobile
}

/// The shell's assertion that this guest sits on a handset.
fn probe_shell_assertion(host: &dyn Host, out: &mut Vec<Finding>) -> Assertion {
    match host.env(SHELL_ASSERTION_ENV).as_deref() {
        Some("android-guest") => {
            out.push(Finding::yes(
                "shell assertion",
                format!("{SHELL_ASSERTION_ENV}=android-guest"),
            ));
            Assertion::AndroidGuest
        }
        Some(other) => {
            out.push(Finding::no(
                "shell assertion",
                format!("{SHELL_ASSERTION_ENV} set to unrecognised value {other:?}"),
            ));
            Assertion::Absent
        }
        None => {
            out.push(Finding::no("shell assertion", "not set"));
            Assertion::Absent
        }
    }
}
