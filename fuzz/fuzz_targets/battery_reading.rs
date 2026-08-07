// SPDX-License-Identifier: Apache-2.0
//! Whatever a vendor kernel decided to put in the power-supply nodes.
//!
//! A reading feeds the governor, and the governor decides whether to keep
//! charging a lithium cell. The failure worth fuzzing is not a crash: it is a
//! reading assembled out of nonsense that a threshold then compares against.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vayucell_core::host::FakeHost;
use vayucell_core::sysfs::{read_battery, SUPPLY};

const NODES: [&str; 8] = [
    "capacity",
    "voltage_now",
    "current_now",
    "temp",
    "cycle_count",
    "charge_full",
    "charge_full_design",
    "health",
];

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };

    // Split the input across the nodes, so the fuzzer explores partially valid
    // directories rather than only fully garbage ones — a device where three
    // nodes read cleanly and the fourth returns rubbish is the realistic case.
    let mut host = FakeHost::new();
    for (i, node) in NODES.iter().enumerate() {
        let chunk = text
            .split('\u{1}')
            .nth(i)
            .unwrap_or("");
        host = host.with_file(&format!("{SUPPLY}/{node}"), chunk);
    }

    if let Ok(reading) = read_battery(&host, SUPPLY) {
        // Percent is clamped by construction; if that ever stops being true a
        // threshold comparison downstream is reading a number nobody bounded.
        assert!(reading.capacity.get() <= 100);
        let _ = reading.state_of_health();
        let _ = reading.evidence();
        let _ = reading.is_charging();
    }
});
