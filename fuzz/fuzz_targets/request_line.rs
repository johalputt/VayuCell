// SPDX-License-Identifier: Apache-2.0
//! The only parser in this project that reads bytes off a socket.
//!
//! Every other input arrives from sysfs, which is at least written by a kernel.
//! This one is written by whoever connected, so it is the first thing worth
//! fuzzing and the reason this harness exists.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vayucell_core::serve::{parse_request_line, route};

fuzz_target!(|data: &[u8]| {
    let Ok(line) = core::str::from_utf8(data) else {
        return;
    };

    // The contract: parse either refuses with a named reason or returns a path
    // that has already been established not to leave the document root. There
    // is no third outcome, and a panic is not one of them — this runs on a
    // phone somebody is asleep next to.
    if let Ok(request) = parse_request_line(line) {
        assert!(
            request.path.starts_with('/'),
            "an accepted path must be rooted: {:?}",
            request.path
        );
        // Checked per SEGMENT, which is what normalise actually guarantees.
        // The first version of this asserted the path contained no ".."
        // anywhere, and the fuzzer produced "/a..b" within seconds — a perfectly
        // ordinary filename. The assertion was wrong, not the parser, and an
        // over-strict fuzz oracle costs exactly as much attention as a real bug.
        assert!(
            request.path.split('/').all(|seg| seg != ".." && seg != "."),
            "an accepted path must have no traversal segment: {:?}",
            request.path
        );
        assert!(
            !request.path.contains('%'),
            "an accepted path must not be percent-encoded: {:?}",
            request.path
        );
        // Routing an arbitrary accepted path must also not panic.
        let _ = route(&request, "panel");
    }
});
