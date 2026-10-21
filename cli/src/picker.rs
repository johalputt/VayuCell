// SPDX-License-Identifier: Apache-2.0

//! `vayucell catalogue` — what this cell can offer, and what it cannot.
//!
//! PLAN §8 makes one click mean a catalogue rather than a shell prompt, and
//! PLAN §9 says the picker is filtered to the tier. This is that picker, as a
//! command somebody can run on a phone and read.
//!
//! # What it measures, and what it does not
//!
//! Two things are measured here, and both are facts about this machine: the
//! tier, which the tier detection establishes from evidence, and the memory,
//! which the kernel writes down in `/proc/meminfo`.
//!
//! Two things are not measured, and the picker says so on every row that
//! needs one. The free space on the filesystem cannot be read without a libc
//! call this project does not have, and the thermal class cannot be read at
//! all: it is what a cell is willing to be warmed to, and only a governor
//! that has watched a cell heat can say. PLAN §11 keeps the P9 gate open
//! exactly there.
//!
//! So the honest answer for most rows is *cannot be told*, and the honest
//! answer for the rows whose memory is too small is *does not fit* — a
//! failure that was measured, reported as one.
//!
//! # Why the memory is read at all
//!
//! Because "provably does not fit" is the answer a person can act on, and
//! the memory is the one requirement that can be established without a
//! profiler. Reading it is not a claim that the service will run: the tier
//! floor, the storage and the thermal class are still unchecked, and a row
//! that says *fits* needs all of them.

use core::fmt::Write as _;

use vayucell_core::catalogue::{self, Budget, Selection};
use vayucell_core::host::Host;
use vayucell_core::tier::detect;

/// Where the kernel writes how much memory the machine has.
pub const MEMINFO: &str = "/proc/meminfo";

/// How much memory this machine has, in megabytes, if it will say.
///
/// `None` when the file cannot be read, when it does not say, or when it says
/// something that is not a number. All three are the same answer for a
/// picker: nobody has measured it.
#[must_use]
pub fn memory_mb(host: &dyn Host) -> Option<u32> {
    let text = host.read(MEMINFO)?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemTotal:") else {
            continue;
        };
        let digits = rest.trim().trim_end_matches("kB").trim();
        let kilobytes: u64 = digits.parse().ok()?;
        return u32::try_from(kilobytes / 1024).ok();
    }
    None
}

/// The budget this device is judged against, as far as it has been measured.
///
/// The tier comes from the tier detection, which refuses to guess. The memory
/// comes from the kernel. The storage and the thermal class are left
/// unmeasured, and the picker reports every row that needs one of them as
/// *cannot be told*.
#[must_use]
pub fn budget(host: &dyn Host) -> Budget {
    let tier = detect(host).verdict.tier();
    Budget::new(tier, memory_mb(host), None, None)
}

/// The catalogue as the picker shows it.
///
/// Every entry appears. An entry this device cannot run is not dropped; it
/// is shown with the reason, because a catalogue that quietly hides what it
/// will not run teaches its reader that the software knows best.
#[must_use]
pub fn render(selection: &Selection<'_>, budget: &Budget) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "CATALOGUE  {} services, {} offered on this device",
        selection.total(),
        selection.fitting.len()
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "THIS DEVICE  {}", device_line(budget));
    let _ = writeln!(out);
    for entry in catalogue::catalogue() {
        let verdict = match selection.reason_for(entry.id) {
            None => "offered",
            Some(reason) => match reason {
                catalogue::Reason::TooSmall(_) => "does not fit",
                catalogue::Reason::TierUnknown { .. } => "cannot be told",
                catalogue::Reason::ResourcesUnknown { .. } => "cannot be told",
            },
        };
        let _ = writeln!(out, "  {:<16} {verdict}", entry.id);
        let _ = writeln!(out, "                  {}", wants(entry));
        if let Some(reason) = selection.reason_for(entry.id) {
            let _ = writeln!(out, "                  {reason}");
        }
    }
    out
}

/// What the entry wants, in one line.
fn wants(entry: &catalogue::Entry) -> String {
    format!(
        "at least {}, {} MB of memory, {} MB of storage, {} work",
        entry.floor, entry.envelope.memory_mb, entry.envelope.storage_mb, entry.envelope.thermal
    )
}

/// What was measured about this device, in one line.
fn device_line(budget: &Budget) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(match budget.tier {
        Some(tier) => format!("tier {tier}"),
        None => "the tier has not been established".to_owned(),
    });
    parts.push(match budget.memory_mb {
        Some(mb) => format!("{mb} MB of memory"),
        None => "the memory has not been measured".to_owned(),
    });
    parts.push(match budget.storage_mb {
        Some(mb) => format!("{mb} MB of storage"),
        None => "the storage has not been measured".to_owned(),
    });
    parts.push(match budget.thermal {
        Some(class) => format!("{class} work"),
        None => "the thermal class has not been established".to_owned(),
    });
    parts.join(", ")
}

/// The command.
///
/// Prints the catalogue and what this device can be told about itself. The
/// exit code is zero: this is a report, and a report that could not be
/// printed is the only failure worth a non-zero code.
pub fn run(host: &dyn Host) -> i32 {
    let budget = budget(host);
    let selection = catalogue::select(&budget);
    print!("{}", render(&selection, &budget));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use vayucell_core::capability::Tier;
    use vayucell_core::catalogue::ThermalClass;

    /// A host that answers with fixed text.
    struct Fake {
        meminfo: Option<String>,
    }

    impl Host for Fake {
        fn read(&self, path: &str) -> Option<String> {
            if path == MEMINFO {
                return self.meminfo.clone();
            }
            None
        }
        fn exists(&self, _path: &str) -> bool {
            false
        }
        fn is_file(&self, _path: &str) -> bool {
            false
        }
        fn euid(&self) -> u32 {
            0
        }
        fn env(&self, _key: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn the_memory_is_read_from_the_kernels_own_accounting() {
        let host = Fake {
            meminfo: Some("MemTotal:        4018620 kB\nMemFree: 100 kB\n".to_owned()),
        };
        assert_eq!(memory_mb(&host), Some(3924));
    }

    #[test]
    fn a_machine_that_will_not_say_how_much_memory_it_has_says_nothing() {
        // The defect: a missing /proc/meminfo read as zero megabytes, so every
        // service looked provably too big and the picker reported a failure it
        // had invented.
        let host = Fake { meminfo: None };
        assert_eq!(memory_mb(&host), None);
        let budget = budget(&host);
        assert_eq!(budget.memory_mb, None);
    }

    #[test]
    fn a_line_that_is_not_a_number_is_not_a_measurement() {
        // The defect: a kernel that wrote something unexpected being read as
        // a measurement of zero.
        let host = Fake {
            meminfo: Some("MemTotal:        not a number kB\n".to_owned()),
        };
        assert_eq!(memory_mb(&host), None);
    }

    #[test]
    fn a_picker_that_measured_nothing_offers_nothing() {
        // The defect: a device nobody has profiled being offered every
        // service in the catalogue.
        let host = Fake { meminfo: None };
        let budget = budget(&host);
        let selection = catalogue::select(&budget);
        assert!(selection.fitting.is_empty());
        assert_eq!(selection.total(), catalogue::catalogue().len());
    }

    #[test]
    fn the_picker_names_the_requirements_and_the_reason() {
        // The defect: a row that says "cannot be told" and nothing else, so
        // the reader is left looking for a measurement nobody made.
        let host = Fake { meminfo: None };
        let budget = budget(&host);
        let selection = catalogue::select(&budget);
        let text = render(&selection, &budget);
        assert!(text.contains("vayu.panel"), "{text}");
        assert!(text.contains("MB of memory"), "{text}");
        assert!(text.contains("cannot be told"), "{text}");
        assert!(text.contains("has not been measured"), "{text}");
    }

    #[test]
    fn a_device_with_too_little_memory_is_told_it_does_not_fit() {
        // The defect: a phone with 512 MB of memory being offered a service
        // that wants 4 GB, so the failure arrives as thrashing.
        let host = Fake {
            meminfo: Some("MemTotal:         524288 kB\n".to_owned()),
        };
        let budget = Budget::new(
            Some(Tier::T1),
            memory_mb(&host),
            None,
            Some(ThermalClass::Heavy),
        );
        let selection = catalogue::select(&budget);
        let greyed = selection
            .greyed
            .iter()
            .find(|greyed| greyed.entry.id == catalogue::VAYU_INFERENCE.id)
            .expect("the heavy service is greyed out");
        assert!(matches!(
            greyed.reason,
            catalogue::Reason::TooSmall(catalogue::Shortfall::Memory { .. })
        ));
        // And the row says so in words, because a reason nobody can read is a
        // reason nobody acts on.
        let text = render(&selection, &budget);
        assert!(text.contains("does not fit"), "{text}");
        assert!(text.contains("this device has 512 MB of memory"), "{text}");
    }
}
