// SPDX-License-Identifier: Apache-2.0

//! `vayucell catalogue` — what this cell can offer, and what it cannot.
//!
//! PLAN §8 makes one click mean a catalogue rather than a shell prompt, and
//! PLAN §9 says the picker is filtered to the tier. This is that picker, as a
//! command somebody can run on a phone and read.
//!
//! # What it measures, and what it does not
//!
//! The figures come from [`crate::profile`], which is the profiling pass and
//! is also a command of its own. The tier comes from the tier detection,
//! which establishes it from evidence; the memory comes from the kernel's own
//! accounting; the storage comes from the platform's own libc; and the
//! thermal class comes from the cell's own temperature.
//!
//! Whatever could not be measured is reported as *cannot be told*, on every
//! row that needs it — and the honest answer for the rows whose memory is
//! too small is *does not fit*, a failure that was measured, reported as one.
//!
//! # Why the memory is read at all
//!
//! Because "provably does not fit" is the answer a person can act on, and the
//! memory is the one requirement that can be established without a profiler.
//! Reading it is not a claim that the service will run: the tier floor, the
//! storage and the thermal class are separate requirements, and a row that
//! says *fits* needs all of them.

use core::fmt::Write as _;

use vayucell_core::catalogue::{self, Budget, Selection};
use vayucell_core::host::Host;

use crate::profile;

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
        Some(mb) => format!("{mb} MB of free storage"),
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
pub fn run(host: &dyn Host, supply_dir: &str) -> i32 {
    let path = std::env::current_dir().ok();
    let storage = path.as_deref().and_then(profile::storage_mb);
    let budget = profile::budget(host, supply_dir, storage);
    let selection = catalogue::select(&budget);
    print!("{}", render(&selection, &budget));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use vayucell_core::capability::Tier;
    use vayucell_core::host::FakeHost;
    use vayucell_core::thermal::ThermalClass;

    #[test]
    fn a_picker_that_measured_nothing_offers_nothing() {
        // The defect: a device nobody has profiled being offered every
        // service in the catalogue.
        let host = FakeHost::new();
        let budget = profile::budget(&host, "/nowhere", None);
        let selection = catalogue::select(&budget);
        assert!(selection.fitting.is_empty());
        assert_eq!(selection.total(), catalogue::catalogue().len());
    }

    #[test]
    fn the_picker_names_the_requirements_and_the_reason() {
        // The defect: a row that says "cannot be told" and nothing else, so
        // the reader is left looking for a measurement nobody made.
        let host = FakeHost::new();
        let budget = profile::budget(&host, "/nowhere", None);
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
        let host = FakeHost::new().with_file(profile::MEMINFO, "MemTotal:         524288 kB\n");
        let budget = Budget::new(
            Some(Tier::T1),
            profile::memory_mb(&host),
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

    #[test]
    fn a_device_without_room_is_told_it_does_not_fit() {
        // The defect: a device with a megabyte of room being offered a
        // service that wants eight gigabytes, because the storage was never
        // measured and the question was answered with a zero.
        let host = FakeHost::new().with_file(profile::MEMINFO, "MemTotal:         8388608 kB\n");
        let budget = Budget::new(
            Some(Tier::T3),
            profile::memory_mb(&host),
            Some(1),
            Some(ThermalClass::Heavy),
        );
        let selection = catalogue::select(&budget);
        let greyed = selection
            .greyed
            .iter()
            .find(|greyed| greyed.entry.id == catalogue::VAYU_CLOUD.id)
            .expect("the storage-hungry service is greyed out");
        assert!(matches!(
            greyed.reason,
            catalogue::Reason::TooSmall(catalogue::Shortfall::Storage { .. })
        ));
        let text = render(&selection, &budget);
        assert!(text.contains("does not fit"), "{text}");
        assert!(text.contains("MB of free storage"), "{text}");
    }

    #[test]
    fn a_hot_cell_greys_out_the_heavy_service_and_says_why() {
        // The defect: a cell the governor has already warned being offered
        // the workload that warms a lithium cell until it has to intervene.
        let host = FakeHost::new().with_file(profile::MEMINFO, "MemTotal:         8388608 kB\n");
        let budget = Budget::new(
            Some(Tier::T3),
            profile::memory_mb(&host),
            Some(64 * 1024),
            Some(ThermalClass::Light),
        );
        let selection = catalogue::select(&budget);
        let greyed = selection
            .greyed
            .iter()
            .find(|greyed| greyed.entry.id == catalogue::VAYU_INFERENCE.id)
            .expect("the heavy service is greyed out");
        assert!(matches!(
            greyed.reason,
            catalogue::Reason::TooSmall(catalogue::Shortfall::Thermal { .. })
        ));
        let text = render(&selection, &budget);
        assert!(text.contains("does not fit"), "{text}");
        assert!(text.contains("light work"), "{text}");
    }
}
