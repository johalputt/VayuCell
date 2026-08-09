// SPDX-License-Identifier: Apache-2.0

//! What the flash says about how much of its life it has used.
//!
//! ADR-0004 §2 lists wear observation as one of the four storage properties, and
//! it is the only one of them a device can answer about itself. [`observe`] is
//! that answer, and it is the first producer the durability module has ever had:
//! [`crate::durability::Posture`] existed with no caller, so every honesty rule
//! written into those types applied to nothing.
//!
//! # What the kernel actually exposes
//!
//! eMMC and UFS both report life used as a coarse estimate, not a percentage.
//! The eMMC spec (JESD84 `EXT_CSD_DEVICE_LIFE_TIME_EST_TYP_A/B`) defines one
//! byte per cell type, and the kernel prints them as hex:
//!
//! ```text
//! $ cat /sys/block/mmcblk0/device/life_time
//! 0x02 0x01
//! ```
//!
//! `0x01` means 0–10% used, `0x02` means 10–20%, up to `0x0A` for 90–100% and
//! `0x0B` for a device past its rated life. `0x00` means the device declines to
//! say, which is **not** the same as new.
//!
//! # A range is reported as its worse end
//!
//! `0x02` means somewhere in 10–20%. This reports 20, not 15 and not 10. An
//! estimate that splits the difference is a number nobody can act on being
//! presented as a measurement, and rounding toward *less* wear is rounding in
//! the reassuring direction on the one figure whose whole purpose is to stop
//! being reassuring.
//!
//! # Two cell types, one answer
//!
//! A device reports type A (SLC) and type B (MLC) separately and they wear at
//! different rates. The worse of the two is the answer, because the device fails
//! when either does.

use crate::durability::WearIndicator;
use crate::host::Host;

/// Where eMMC and UFS report life used, in the order they are tried.
///
/// Listed rather than globbed because [`Host`] cannot enumerate a directory, and
/// a probe that silently checked fewer paths than it claimed would report
/// [`WearIndicator::Absent`] on a device that answers — the reassuring direction
/// again.
pub const LIFE_TIME_NODES: [&str; 4] = [
    "/sys/block/mmcblk0/device/life_time",
    "/sys/block/sda/device/health_descriptor/life_time_estimation_a",
    "/sys/class/block/mmcblk0/device/life_time",
    "/sys/devices/platform/soc/ufs/health_descriptor/life_time_estimation_a",
];

/// The value a device reports when it will not say.
const DECLINES_TO_SAY: u8 = 0x00;

/// The highest defined step. Anything above it is past rated life.
const PAST_RATED_LIFE: u8 = 0x0B;

/// What the flash on this device says about its own wear.
///
/// [`WearIndicator::Absent`] when no node answers, which is the ordinary case
/// and not a fault: most handsets expose nothing. It is reported as absent and
/// never as healthy, because the variant names *whether the device said*, not
/// whether the news was good.
#[must_use]
pub fn observe(host: &dyn Host) -> WearIndicator {
    for node in LIFE_TIME_NODES {
        if let Some(raw) = host.read(node) {
            return parse(&raw);
        }
    }
    WearIndicator::Absent
}

/// Reads one `life_time` value.
///
/// Public so the parsing is testable without a filesystem, which is the same
/// reason every other probe in this crate is split this way.
#[must_use]
pub fn parse(raw: &str) -> WearIndicator {
    let fields: Vec<&str> = raw.split_whitespace().collect();
    if fields.is_empty() {
        return WearIndicator::Unreliable("the node was empty".to_owned());
    }

    let mut worst: Option<u8> = None;
    for field in &fields {
        let Some(step) = step_of(field) else {
            return WearIndicator::Unreliable(format!("{field:?} is not a life-time estimate"));
        };
        // 0x00 is "will not say" and must not be mistaken for a low reading.
        // Skipping it rather than treating it as zero is the difference between
        // "one cell type answered" and "this flash is new".
        if step == DECLINES_TO_SAY {
            continue;
        }
        worst = Some(worst.map_or(step, |w: u8| w.max(step)));
    }

    match worst {
        // Every field present and every one of them declining to answer. The
        // device is there and it told us nothing, which is not absence.
        None => WearIndicator::Unreliable(
            "the device reports a life-time node and declines to estimate in it".to_owned(),
        ),
        Some(step) if step > PAST_RATED_LIFE => {
            WearIndicator::Unreliable(format!("{step:#04x} is above the highest defined step"))
        }
        // The top step is not 110% used; it means the device is past what it was
        // rated for and has stopped counting. Reported at 100 with the estimate
        // saturated rather than invented.
        Some(PAST_RATED_LIFE) => WearIndicator::Readable(100),
        Some(step) => WearIndicator::Readable(step.saturating_mul(10)),
    }
}

/// One hex field as its step number.
fn step_of(field: &str) -> Option<u8> {
    let trimmed = field.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u8::from_str_radix(digits, 16).ok()
}
