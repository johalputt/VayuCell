// SPDX-License-Identifier: Apache-2.0

//! Wear tests, in the voice of somebody who wants the number to be small.
//!
//! Every assertion here forecloses a way of reading a device's own estimate more
//! kindly than the device meant it.

use crate::durability::WearIndicator;
use crate::host::FakeHost;
use crate::wear::{observe, parse, LIFE_TIME_NODES};

#[test]
fn a_range_is_reported_as_its_worse_end() {
    // 0x02 means somewhere in 10–20% used. Reporting 15 presents an estimate as
    // a measurement; reporting 10 rounds toward less wear, which is the
    // reassuring direction on the one figure whose purpose is to stop being
    // reassuring.
    assert_eq!(parse("0x02 0x01"), WearIndicator::Readable(20));
    assert_eq!(parse("0x01 0x01"), WearIndicator::Readable(10));
}

#[test]
fn the_worse_of_the_two_cell_types_is_the_answer() {
    // SLC and MLC wear at different rates and the device fails when either
    // does, so the better number is not the news.
    assert_eq!(parse("0x01 0x07"), WearIndicator::Readable(70));
    assert_eq!(parse("0x07 0x01"), WearIndicator::Readable(70));
}

#[test]
fn a_device_past_its_rated_life_reads_as_a_hundred_and_not_as_more() {
    // 0x0B is "exceeded", not 110%. Saturating is honest; inventing a number
    // above the scale is not.
    assert_eq!(parse("0x0B 0x0B"), WearIndicator::Readable(100));
    assert_eq!(parse("0x0A 0x0B"), WearIndicator::Readable(100));
}

#[test]
fn a_device_that_declines_to_estimate_is_not_reported_as_new() {
    // 0x00 means the device will not say. Treating it as zero would make the
    // least forthcoming flash look like the healthiest, which is the inversion
    // Article IV.3 exists to forbid.
    let out = parse("0x00 0x00");
    assert!(
        matches!(out, WearIndicator::Unreliable(_)),
        "{out:?} — declining to answer is not an answer of zero"
    );
    assert_ne!(out, WearIndicator::Readable(0));
}

#[test]
fn one_cell_type_declining_does_not_discard_the_other() {
    // A device that answers for one type and not the other has still told us
    // something, and the something is the type that answered.
    assert_eq!(parse("0x00 0x03"), WearIndicator::Readable(30));
    assert_eq!(parse("0x04 0x00"), WearIndicator::Readable(40));
}

#[test]
fn a_node_that_does_not_parse_is_unreliable_rather_than_absent() {
    // Absent means the device exposes nothing. This device exposed something
    // and it did not make sense, which is a different fact and a worse one —
    // something is there and cannot be trusted.
    for raw in ["garbage", "0xZZ 0x01", "", "   "] {
        let out = parse(raw);
        assert!(
            matches!(out, WearIndicator::Unreliable(_)),
            "{raw:?} produced {out:?}"
        );
    }
}

#[test]
fn a_value_above_the_defined_scale_is_refused_rather_than_scaled() {
    // 0xFF is not 2550% used. A device reporting outside the spec is a device
    // whose estimate cannot be used, and saying so beats printing a number.
    let out = parse("0xFF 0x01");
    assert!(matches!(out, WearIndicator::Unreliable(_)), "{out:?}");
}

#[test]
fn a_bare_value_without_the_hex_prefix_is_still_read() {
    // Not every kernel prints the prefix. Refusing an unprefixed value would
    // report a device that answered as one that did not.
    assert_eq!(parse("02 01"), WearIndicator::Readable(20));
}

#[test]
fn a_handset_that_exposes_nothing_reads_absent_and_never_healthy() {
    // The ordinary case. Most phones expose no life-time node at all, and that
    // is not a fault — but it must not read as good news either.
    assert_eq!(observe(&FakeHost::new()), WearIndicator::Absent);
}

#[test]
fn every_node_the_probe_claims_to_try_is_actually_tried() {
    // A probe that checked fewer paths than it lists would report Absent on a
    // device that answers. Each node is planted alone and must be found.
    for node in LIFE_TIME_NODES {
        let host = FakeHost::new().with_file(node, "0x05 0x01\n");
        assert_eq!(
            observe(&host),
            WearIndicator::Readable(50),
            "{node} is listed and not read"
        );
    }
}
