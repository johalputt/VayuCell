// SPDX-License-Identifier: Apache-2.0

//! The profiling pass: what this device can be told about itself, and how it
//! was told.
//!
//! PLAN §9's flow is *profile the device, establish the safety state, pick
//! from the catalogue filtered to the tier*. The first step existed as two
//! lines inside the picker, which meant the figures the catalogue was judged
//! against were assembled somewhere nobody would look for them. This module is
//! that step, and `vayucell profile` is that step as a command: the same
//! figures, printed, each one saying whether it was measured.
//!
//! # What is measured, and what is not
//!
//! Four figures, and each has a different kind of evidence behind it:
//!
//! - **The tier**, from the tier detection, which refuses to guess.
//! - **The memory**, from the kernel's own accounting in `/proc/meminfo`.
//! - **The storage**, from the one libc call in the project — see
//!   [`crate::statvfs`].
//! - **The thermal class**, from the cell's own temperature, put through the
//!   policy in [`vayucell_core::thermal`].
//!
//! A figure that could not be measured is named as such. It is never a zero,
//! and it is never a class: the difference between *no answer* and *an answer
//! that permits nothing* is the difference between a catalogue that says
//! *cannot be told* and one that refuses a service for a reason nobody
//! measured.
//!
//! # The storage is measured where the command was run
//!
//! The free space of a filesystem is the free space of a *mount*, and a phone
//! has several. This measures the one the command was run from, and the
//! report names the path it measured — because a figure of "41230 MB free"
//! that does not say which filesystem it is about is a figure about no
//! filesystem in particular.

use core::fmt::Write as _;
use std::path::Path;

use vayucell_core::battery::Celsius;
use vayucell_core::catalogue::Budget;
use vayucell_core::governor::Thresholds;
use vayucell_core::host::Host;
use vayucell_core::sysfs;
use vayucell_core::thermal::{ThermalClass, ThermalHistory};
use vayucell_core::tier::detect;

use crate::statvfs;

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

/// The free space on the filesystem holding `path`, in megabytes.
///
/// Thin on purpose: the measurement is the one libc call in the project, and
/// it lives in [`crate::statvfs`] where the unsafe block is.
#[must_use]
pub fn storage_mb(path: &Path) -> Option<u64> {
    statvfs::free_space_mb(path)
}

/// The class this cell is in, if the cell could be read.
///
/// The reading is the cell's own temperature, and the class is that reading
/// put through the governor's ladder. A cell that could not be read has no
/// class at all: see [`ThermalHistory::class`].
#[must_use]
pub fn thermal_class(host: &dyn Host, supply_dir: &str) -> Option<ThermalClass> {
    let reading = sysfs::read_battery(host, supply_dir).ok()?;
    let mut history = ThermalHistory::new();
    history.observe(&reading);
    history.class(&Thresholds::recommended())
}

/// The temperature of the cell, if the cell could be read.
#[must_use]
pub fn cell_temp(host: &dyn Host, supply_dir: &str) -> Option<Celsius> {
    let reading = sysfs::read_battery(host, supply_dir).ok()?;
    Some(reading.temp.to_celsius())
}

/// The budget this device is judged against, as far as it has been measured.
///
/// The tier comes from the tier detection, which refuses to guess. The memory
/// comes from the kernel. The storage is passed in rather than measured here,
/// because it is measured by the one libc call in the project and a test must
/// not measure the disk it happens to be running on. The thermal class comes
/// from the cell, and is absent when the cell could not be read.
#[must_use]
pub fn budget(host: &dyn Host, supply_dir: &str, storage_mb: Option<u64>) -> Budget {
    let tier = detect(host).verdict.tier();
    let thermal = thermal_class(host, supply_dir);
    Budget::new(tier, memory_mb(host), storage_mb, thermal)
}

/// The report.
///
/// Every figure says how it was established, and a figure that was not
/// measured says so in the same words the catalogue uses, so that a reader
/// coming from either surface sees the same vocabulary.
#[must_use]
pub fn render(budget: &Budget, cell: Option<Celsius>, storage_path: Option<&Path>) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "PROFILE  this device, and what it can be told about itself"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  tier       {:<24} {}",
        match budget.tier {
            Some(tier) => tier.to_string(),
            None => "not established".to_owned(),
        },
        match budget.tier {
            Some(_) => "established from evidence",
            None => "the tier has not been established",
        }
    );
    let _ = writeln!(
        out,
        "  memory     {:<24} {}",
        match budget.memory_mb {
            Some(mb) => format!("{mb} MB"),
            None => "not measured".to_owned(),
        },
        match budget.memory_mb {
            Some(_) => "measured from /proc/meminfo",
            None => "the memory has not been measured",
        }
    );
    let _ = writeln!(
        out,
        "  storage    {:<24} {}",
        match budget.storage_mb {
            Some(mb) => format!("{mb} MB"),
            None => "not measured".to_owned(),
        },
        match (budget.storage_mb, storage_path) {
            (Some(_), Some(path)) =>
                format!("measured on the filesystem holding {}", path.display()),
            (Some(_), None) => "measured".to_owned(),
            (None, _) => "the storage has not been measured".to_owned(),
        }
    );
    let _ = writeln!(
        out,
        "  thermal    {:<24} {}",
        match budget.thermal {
            Some(class) => format!("{class} work"),
            None => "not established".to_owned(),
        },
        match budget.thermal {
            Some(_) => "measured from the cell's own temperature",
            None => "the thermal class has not been established",
        }
    );
    let _ = writeln!(
        out,
        "  cell       {:<24} {}",
        match cell {
            Some(temp) => temp.to_string(),
            None => "not measured".to_owned(),
        },
        match cell {
            Some(_) => "measured from the cell",
            None => "the cell could not be read",
        }
    );
    let _ = writeln!(out);
    let measured = [
        budget.tier.is_some(),
        budget.memory_mb.is_some(),
        budget.storage_mb.is_some(),
        budget.thermal.is_some(),
    ]
    .iter()
    .filter(|m| **m)
    .count();
    let _ = writeln!(
        out,
        "  {measured} of 4 figures were measured — the tier, the memory, the storage"
    );
    let _ = writeln!(
        out,
        "  and the thermal class. A figure that was not measured is never a zero."
    );
    out
}

/// The command.
///
/// Prints this device's own figures and says which of them are measurements.
/// The exit code is zero: this is a report, and a report that could not be
/// printed is the only failure worth a non-zero code.
pub fn run(host: &dyn Host, supply_dir: &str) -> i32 {
    let path = std::env::current_dir().ok();
    let storage = path.as_deref().and_then(storage_mb);
    let budget = budget(host, supply_dir, storage);
    let cell = cell_temp(host, supply_dir);
    print!("{}", render(&budget, cell, path.as_deref()));
    0
}

#[cfg(test)]
mod tests {
    use super::{budget, memory_mb, render, storage_mb, thermal_class, MEMINFO};
    use std::path::Path;

    use vayucell_core::capability::Tier;
    use vayucell_core::host::FakeHost;

    #[test]
    fn the_memory_is_read_from_the_kernels_own_accounting() {
        let host =
            FakeHost::new().with_file(MEMINFO, "MemTotal:        4018620 kB\nMemFree: 100 kB\n");
        assert_eq!(memory_mb(&host), Some(3924));
    }

    #[test]
    fn a_machine_that_will_not_say_how_much_memory_it_has_says_nothing() {
        // The defect: a missing /proc/meminfo read as zero megabytes, so every
        // service looked provably too big and the picker reported a failure it
        // had invented.
        let host = FakeHost::new();
        let budget = budget(&host, "/nowhere", None);
        assert_eq!(budget.memory_mb, None);
    }

    #[test]
    fn a_line_that_is_not_a_number_is_not_a_measurement() {
        // The defect: a kernel that wrote something unexpected being read as
        // a measurement of zero.
        let host = FakeHost::new().with_file(MEMINFO, "MemTotal:        not a number kB\n");
        assert_eq!(memory_mb(&host), None);
    }

    #[test]
    fn a_cell_that_cannot_be_read_has_no_thermal_class() {
        // The defect: a cell that could not be read defaulting to the lowest
        // class, which would make a device nobody has measured look like one
        // that does nothing — and would let every light service be offered on
        // the strength of a reading nobody took.
        let host = FakeHost::new();
        assert_eq!(thermal_class(&host, "/nowhere"), None);
    }

    #[test]
    fn a_cell_that_can_be_read_is_classed_from_its_own_temperature() {
        // The reading is a real one, from the fake machine, and the class
        // follows the governor's ladder: 47 °C is above the warn rung and
        // below the critical one.
        let host = FakeHost::new()
            .with_file("/sys/class/power_supply/battery/capacity", "58\n")
            .with_file("/sys/class/power_supply/battery/voltage_now", "3820000\n")
            .with_file("/sys/class/power_supply/battery/current_now", "-180000\n")
            .with_file("/sys/class/power_supply/battery/temp", "470\n")
            .with_file("/sys/class/power_supply/battery/cycle_count", "412\n")
            .with_file("/sys/class/power_supply/battery/charge_full", "3600000\n")
            .with_file(
                "/sys/class/power_supply/battery/charge_full_design",
                "4000000\n",
            );
        let class = thermal_class(&host, "/sys/class/power_supply/battery");
        assert_eq!(class, Some(vayucell_core::thermal::ThermalClass::Sustained));
    }

    #[test]
    fn a_figure_that_was_not_measured_is_named_as_such_and_is_never_a_zero() {
        // The defect this forecloses: an unmeasured figure rendered as a
        // number. A reader would take "0 MB" for a measurement of nothing,
        // and a catalogue would refuse a service for want of room that was
        // never looked for.
        let host = FakeHost::new();
        let budget = budget(&host, "/nowhere", None);
        let text = render(&budget, None, None);
        assert!(text.contains("not measured"), "{text}");
        assert!(text.contains("the memory has not been measured"), "{text}");
        assert!(text.contains("the storage has not been measured"), "{text}");
        assert!(
            text.contains("the thermal class has not been established"),
            "{text}"
        );
        assert!(!text.contains("0 MB"), "{text}");
    }

    #[test]
    fn a_measured_figure_says_where_it_came_from() {
        // The defect this forecloses: "41230 MB free" with nothing saying
        // which filesystem it was about. A phone has several, and the figure
        // is about a mount.
        let host = FakeHost::new().with_file(MEMINFO, "MemTotal: 1048576 kB\n");
        let budget = budget(&host, "/nowhere", Some(41230));
        let text = render(
            &budget,
            Some(vayucell_core::battery::Celsius::new(31)),
            Some(Path::new("/home/somebody")),
        );
        assert!(text.contains("41230 MB"), "{text}");
        assert!(
            text.contains("measured on the filesystem holding /home/somebody"),
            "{text}"
        );
        assert!(text.contains("31 °C"), "{text}");
    }

    #[test]
    fn the_storage_of_a_path_that_is_not_there_is_not_a_measurement() {
        // The defect this forecloses: a path that does not exist reported as
        // a filesystem with no room on it.
        assert_eq!(storage_mb(Path::new("/definitely/not/a/real/path")), None);
    }

    #[test]
    fn the_tier_is_not_established_by_a_machine_that_says_nothing() {
        // The device gate, held here: a machine that answers nothing has no
        // tier, and a budget that claimed one would let the catalogue offer
        // services to a device nobody has identified.
        let host = FakeHost::new();
        let budget = budget(&host, "/nowhere", Some(1024));
        assert_eq!(budget.tier, None);
        assert_ne!(budget.tier, Some(Tier::T0));
    }
}
