// SPDX-License-Identifier: Apache-2.0

//! Tests for the catalogue and its filter.
//!
//! Each test names the defect it forecloses. The failure mode this module
//! guards against is never a crash: it is a service that is quietly not
//! offered, or — worse — one that is offered to a device which cannot run
//! it, and a reader who is told nothing either way.

use super::catalogue::*;
use crate::capability::Tier;

/// A device with plenty of room and the given tier.
fn generous(tier: Tier) -> Budget {
    Budget::measured(Some(tier), 8192, 8192, ThermalClass::Heavy)
}

/// The same device, with no tier ever established.
fn unprofiled() -> Budget {
    Budget::measured(None, 8192, 8192, ThermalClass::Heavy)
}

#[test]
fn every_entry_declares_a_floor_and_an_envelope() {
    // The defect: an entry left with a default floor and a default envelope,
    // so the panel offers it to a device that cannot run it.
    let entries = catalogue();
    assert!(!entries.is_empty(), "the catalogue is empty");
    for entry in entries {
        assert!(!entry.id.trim().is_empty(), "an entry has no id");
        assert!(!entry.name.trim().is_empty(), "{} has no name", entry.id);
        assert!(!entry.purpose.trim().is_empty(), "{} has none", entry.id);
        assert!(entry.envelope.memory_mb > 0, "{} wants no memory", entry.id);
        assert!(entry.envelope.storage_mb > 0, "{} wants no disk", entry.id);
        assert!(matches!(
            entry.floor,
            Tier::T0 | Tier::T1 | Tier::T2 | Tier::T3
        ));
    }
    // Two services under one identifier, so a report cannot say which of
    // them it is talking about.
    let mut ids: Vec<&str> = entries.iter().map(|entry| entry.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "the catalogue holds a duplicate id");
}

#[test]
fn the_catalogue_carries_the_services_plan_section_eight_lists() {
    // The defect: a catalogue that quietly drops one of the services PLAN §8
    // promises, so the picker never offers the thing the plan named.
    let ids: Vec<&str> = catalogue().iter().map(|entry| entry.id).collect();
    for wanted in [
        VAYU_PRESS.id,
        VAYU_CLOUD.id,
        VAYU_BACKUP.id,
        VAYU_HOUSEHOLD.id,
        VAYU_INFERENCE.id,
    ] {
        assert!(ids.contains(&wanted), "{wanted} is missing");
    }
}

#[test]
fn only_the_report_is_offered_at_the_lowest_tier() {
    // The defect: a service with a T0 floor, offered on stock Android, which
    // CHARTER Article III forbids — nothing that serves traffic ships before
    // the governor, and the governor needs a charge ceiling.
    for entry in catalogue() {
        if entry.floor == Tier::T0 {
            assert_eq!(entry.id, VAYU_PANEL.id, "{} at T0", entry.id);
        }
    }
}

#[test]
fn private_model_inference_is_in_the_catalogue_and_asks_for_the_heavy_class() {
    // The defect: the one service PLAN §8 says is genuinely viable on phone
    // silicon, missing from the catalogue because it was awkward to write.
    let entry = catalogue()
        .iter()
        .find(|entry| entry.id == VAYU_INFERENCE.id)
        .expect("the inference service is listed");
    assert_eq!(entry.floor, VAYU_INFERENCE.floor);
    assert_eq!(entry.envelope.thermal, ThermalClass::Heavy);
    assert!(entry.envelope.memory_mb >= 2048, "2 GB or more");
}

#[test]
fn the_heavy_thermal_class_belongs_to_model_inference_and_to_nothing_else() {
    // The defect: a second heavy service, so the governor is asked to carry
    // two workloads of that class on a phone in a drawer.
    let heavy: Vec<&str> = catalogue()
        .iter()
        .filter(|entry| entry.envelope.thermal == ThermalClass::Heavy)
        .map(|entry| entry.id)
        .collect();
    assert_eq!(heavy, vec![VAYU_INFERENCE.id]);
}

#[test]
fn a_budget_below_the_floor_greys_the_entry_out_with_a_tier_shortfall() {
    // The defect: a service offered to a device that cannot reach its floor,
    // which is the failure PLAN §8 names — installing something that will
    // thrash a four-year-old handset.
    let selection = select(&generous(Tier::T0));
    for greyed in &selection.greyed {
        if greyed.entry.id == VAYU_PANEL.id {
            continue;
        }
        assert!(
            matches!(greyed.reason, Reason::TooSmall(Shortfall::Tier { .. })),
            "{}: {:?}",
            greyed.entry.id,
            greyed.reason
        );
    }
    assert!(selection.fits(VAYU_PANEL.id));
}

#[test]
fn a_budget_short_of_memory_greys_the_entry_out_with_a_memory_shortfall() {
    // The defect: a device with too little memory offered a service that
    // wants more, so the failure arrives as thrashing rather than as a
    // refusal.
    let selection = select(&Budget::measured(
        Some(Tier::T3),
        512,
        8192,
        ThermalClass::Heavy,
    ));
    let greyed = selection
        .greyed
        .iter()
        .find(|greyed| greyed.entry.id == VAYU_INFERENCE.id)
        .expect("the heavy service is greyed out");
    match greyed.reason {
        Reason::TooSmall(Shortfall::Memory { wanted_mb, have_mb }) => {
            assert_eq!(wanted_mb, VAYU_INFERENCE.envelope.memory_mb);
            assert_eq!(have_mb, 512);
        }
        other => panic!("expected a memory shortfall, got {other:?}"),
    }
}

#[test]
fn a_budget_short_of_storage_greys_the_entry_out_with_a_storage_shortfall() {
    // The defect: a device with no room left offered the service anyway,
    // because a shortage of disk was counted as a shortage of memory.
    let selection = select(&Budget::measured(
        Some(Tier::T3),
        8192,
        64,
        ThermalClass::Heavy,
    ));
    let greyed = selection
        .greyed
        .iter()
        .find(|greyed| greyed.entry.id == VAYU_PRESS.id)
        .expect("the press stack is greyed out");
    match greyed.reason {
        Reason::TooSmall(Shortfall::Storage { wanted_mb, have_mb }) => {
            assert_eq!(wanted_mb, VAYU_PRESS.envelope.storage_mb);
            assert_eq!(have_mb, 64);
        }
        other => panic!("expected a storage shortfall, got {other:?}"),
    }
}

#[test]
fn a_cool_budget_greys_out_the_heavy_service_on_thermal_grounds() {
    // The defect: a thermal ceiling ignored, so the heavy workload runs on a
    // cell that was never meant to carry it.
    let selection = select(&Budget::measured(
        Some(Tier::T3),
        8192,
        8192,
        ThermalClass::Sustained,
    ));
    // The sustained work still fits; the heavy work does not.
    assert!(selection.fits(VAYU_HOUSEHOLD.id));
    let greyed = selection
        .greyed
        .iter()
        .find(|greyed| greyed.entry.id == VAYU_INFERENCE.id)
        .expect("the heavy service is greyed out");
    match greyed.reason {
        Reason::TooSmall(Shortfall::Thermal { wanted, allowed }) => {
            assert_eq!(wanted, ThermalClass::Heavy);
            assert_eq!(allowed, ThermalClass::Sustained);
        }
        other => panic!("expected a thermal shortfall, got {other:?}"),
    }
}

#[test]
fn an_unestablished_tier_cannot_tell_anything_and_never_claims_a_fit() {
    // The defect: a device whose tier was never established being told it
    // fits, because "unknown" was quietly read as "the lowest tier".
    let selection = select(&unprofiled());
    assert!(
        selection.fitting.is_empty(),
        "an unprofiled device was offered a service"
    );
    assert_eq!(selection.total(), catalogue().len());
    for greyed in &selection.greyed {
        assert!(
            matches!(greyed.reason, Reason::TierUnknown { .. }),
            "{}: {:?}",
            greyed.entry.id,
            greyed.reason
        );
    }
}

#[test]
fn every_catalogue_entry_appears_in_the_selection() {
    // The defect: a service silently dropped from the result, so the picker
    // never says "not on this device" and the reader is left thinking the
    // catalogue is smaller than it is.
    let selection = select(&Budget::measured(
        Some(Tier::T1),
        512,
        512,
        ThermalClass::Idle,
    ));
    assert_eq!(selection.total(), catalogue().len());
    for entry in catalogue() {
        let offered = selection.fits(entry.id);
        let refused = selection.reason_for(entry.id).is_some();
        assert!(offered ^ refused, "{} appears in neither list", entry.id);
    }
}

#[test]
fn the_selection_keeps_catalogue_order_and_is_stable_across_runs() {
    // The defect: a picker whose rows move between launches, so a user
    // cannot find the service they used yesterday.
    let first = select(&generous(Tier::T1));
    let second = select(&generous(Tier::T1));
    let first_ids: Vec<&str> = first.fitting.iter().map(|entry| entry.id).collect();
    let second_ids: Vec<&str> = second.fitting.iter().map(|entry| entry.id).collect();
    assert_eq!(first_ids, second_ids);
    assert_eq!(
        first_ids,
        catalogue()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<&str>>()
    );
    assert_eq!(first.greyed, [] as [Greyed<'_>; 0]);

    // The greyed-out list keeps the catalogue order too, so the two lists can
    // be read side by side.
    let refused = select(&generous(Tier::T0));
    let greyed_ids: Vec<&str> = refused
        .greyed
        .iter()
        .map(|greyed| greyed.entry.id)
        .collect();
    assert_eq!(
        greyed_ids,
        vec![
            VAYU_PRESS.id,
            VAYU_CLOUD.id,
            VAYU_BACKUP.id,
            VAYU_HOUSEHOLD.id,
            VAYU_INFERENCE.id,
        ]
    );
}

#[test]
fn a_resource_comparison_names_the_failing_requirement() {
    // The defect: a comparison that answers "too small" without saying which
    // requirement was too small, so the operator cannot act on it.
    let envelope = Envelope::new(512, 1024, ThermalClass::Sustained);
    let roomy = Budget::measured(Some(Tier::T3), 4096, 4096, ThermalClass::Heavy);
    assert!(envelope.shortfall(&roomy).is_none());

    match envelope.shortfall(&Budget::measured(
        Some(Tier::T3),
        128,
        4096,
        ThermalClass::Heavy,
    )) {
        Some(Shortfall::Memory { wanted_mb, have_mb }) => {
            assert_eq!(wanted_mb, 512);
            assert_eq!(have_mb, 128);
        }
        other => panic!("expected a memory shortfall, got {other:?}"),
    }

    match envelope.shortfall(&Budget::measured(
        Some(Tier::T3),
        4096,
        128,
        ThermalClass::Heavy,
    )) {
        Some(Shortfall::Storage { wanted_mb, have_mb }) => {
            assert_eq!(wanted_mb, 1024);
            assert_eq!(have_mb, 128);
        }
        other => panic!("expected a storage shortfall, got {other:?}"),
    }

    match envelope.shortfall(&Budget::measured(
        Some(Tier::T3),
        4096,
        4096,
        ThermalClass::Idle,
    )) {
        Some(Shortfall::Thermal { wanted, allowed }) => {
            assert_eq!(wanted, ThermalClass::Sustained);
            assert_eq!(allowed, ThermalClass::Idle);
        }
        other => panic!("expected a thermal shortfall, got {other:?}"),
    }
}

#[test]
fn a_greyed_out_reason_names_the_requirement_that_failed() {
    // The defect: a reason that says "cannot run" and stops there, leaving
    // the reader to guess which of four requirements was the problem.
    let memory = Reason::TooSmall(Shortfall::Memory {
        wanted_mb: 4096,
        have_mb: 512,
    });
    let text = memory.to_string();
    assert!(text.contains("memory"), "{text}");
    assert!(text.contains("4096"), "{text}");
    assert!(text.contains("512"), "{text}");

    let tier = Reason::TooSmall(Shortfall::Tier {
        floor: Tier::T1,
        detected: Tier::T0,
    });
    let text = tier.to_string();
    assert!(text.contains('T'), "{text}");
    assert!(text.contains("needs"), "{text}");
}

#[test]
fn a_provable_shortfall_and_a_cannot_tell_are_different_answers() {
    // The defect: the two collapsing into one, which would report a device
    // that cannot run a service as merely unmeasured — CHARTER Article IV
    // says absence is never protection, and neither is ignorance.
    let shortfall = Reason::TooSmall(Shortfall::Tier {
        floor: Tier::T1,
        detected: Tier::T0,
    });
    let unknown = Reason::TierUnknown { floor: Tier::T1 };
    assert_ne!(shortfall, unknown);
    assert_ne!(shortfall.to_string(), unknown.to_string());
    assert!(unknown.to_string().contains("cannot be told"));
}

#[test]
fn a_higher_tier_satisfies_a_lower_floor() {
    // The defect: a comparison that demanded the exact tier, greying out
    // services on a device that runs them perfectly well.
    let selection = select(&generous(Tier::T3));
    assert!(selection.fits(VAYU_PANEL.id));
    assert!(selection.fits(VAYU_INFERENCE.id));
    assert_eq!(selection.greyed, [] as [Greyed<'_>; 0]);
}

#[test]
fn the_thermal_classes_are_ordered_from_idle_to_heavy() {
    // The defect: a comparison that called light work worse than heavy, so
    // the governor was told to allow work it should have refused.
    assert!(ThermalClass::Heavy.worse_than(ThermalClass::Sustained));
    assert!(ThermalClass::Sustained.worse_than(ThermalClass::Light));
    assert!(ThermalClass::Light.worse_than(ThermalClass::Idle));
    assert!(!ThermalClass::Idle.worse_than(ThermalClass::Heavy));
    assert!(!ThermalClass::Light.worse_than(ThermalClass::Light));
}

#[test]
fn meeting_the_floor_does_not_buy_a_service_the_resources_cannot_carry() {
    // The defect: the tier check treated as the last word, so a device at
    // the highest tier was offered everything regardless of its memory.
    let selection = select(&Budget::measured(
        Some(Tier::T3),
        128,
        8192,
        ThermalClass::Heavy,
    ));
    assert!(selection.fits(VAYU_PANEL.id));
    assert!(!selection.fits(VAYU_INFERENCE.id));
    for greyed in &selection.greyed {
        assert!(
            matches!(greyed.reason, Reason::TooSmall(_)),
            "{}: {:?}",
            greyed.entry.id,
            greyed.reason
        );
    }
}

#[test]
fn a_caller_can_filter_a_catalogue_of_its_own() {
    // The defect: the filter hard-wired to the built-in list, so nothing
    // else could ever be filtered by it.
    let mine = [Entry {
        id: "test.small",
        name: "Small",
        purpose: "A test entry.",
        floor: Tier::T0,
        envelope: Envelope::new(64, 32, ThermalClass::Idle),
    }];
    let fits = select_from(
        &mine,
        &Budget::measured(Some(Tier::T0), 64, 32, ThermalClass::Idle),
    );
    assert_eq!(fits.fitting.len(), 1);
    assert_eq!(fits.greyed, [] as [Greyed<'_>; 0]);

    let refused = select_from(
        &mine,
        &Budget::measured(Some(Tier::T0), 16, 32, ThermalClass::Idle),
    );
    assert_eq!(refused.fitting, [] as [&Entry; 0]);
    assert_eq!(refused.total(), 1);
}

#[test]
fn each_shortfall_names_its_own_requirement() {
    // The defect: two shortfalls rendered in the same words, so a reader
    // with a full disk is told about their memory and goes looking in the
    // wrong place. Each requirement has to name itself.
    let memory = Reason::TooSmall(Shortfall::Memory {
        wanted_mb: 512,
        have_mb: 256,
    });
    let storage = Reason::TooSmall(Shortfall::Storage {
        wanted_mb: 512,
        have_mb: 256,
    });
    let thermal = Reason::TooSmall(Shortfall::Thermal {
        wanted: ThermalClass::Heavy,
        allowed: ThermalClass::Idle,
    });

    assert!(memory.to_string().contains("memory"), "{memory}");
    assert!(storage.to_string().contains("storage"), "{storage}");
    assert!(!storage.to_string().contains("memory"), "{storage}");
    assert!(thermal.to_string().contains("idle"), "{thermal}");
    assert!(thermal.to_string().contains("heavy"), "{thermal}");
}

#[test]
fn the_panel_can_ask_why_a_service_is_greyed_out() {
    // The defect: a greyed-out entry with no reason attached, so the panel
    // can only say "unavailable" and the user has to guess.
    let selection = select(&unprofiled());
    let reason = selection
        .reason_for(VAYU_INFERENCE.id)
        .expect("the reason is there");
    assert!(matches!(reason, Reason::TierUnknown { .. }));
    // The report is greyed out too: an unestablished tier is the device's
    // problem, not the service's, so nothing at all is claimed to fit.
    assert!(matches!(
        selection.reason_for(VAYU_PANEL.id),
        Some(Reason::TierUnknown { .. })
    ));
    assert!(!selection.fits(VAYU_PANEL.id));

    // A greyed row names the service it is talking about: a row that
    // rendered a reason with no service attached leaves the operator to
    // guess what it was about.
    let row = selection.greyed[0].to_string();
    assert!(row.starts_with(selection.greyed[0].entry.id), "{row}");
    assert!(row.contains("cannot be told"), "{row}");
}

#[test]
fn a_requirement_that_was_never_measured_is_not_a_failure() {
    // The defect: an unmeasured requirement read as zero, so every service
    // looked provably too big and the picker reported a failure it had
    // invented rather than found.
    let budget = Budget::new(Some(Tier::T3), Some(8192), None, Some(ThermalClass::Heavy));
    let selection = select(&budget);
    let greyed = selection
        .greyed
        .iter()
        .find(|greyed| greyed.entry.id == VAYU_PRESS.id)
        .expect("the press stack is greyed out");
    match greyed.reason {
        Reason::ResourcesUnknown { missing, .. } => {
            assert!(missing.storage, "{missing:?}");
            assert!(!missing.memory, "{missing:?}");
            assert!(!missing.thermal, "{missing:?}");
        }
        other => panic!("expected an unmeasured reason, got {other:?}"),
    }
}

#[test]
fn a_provable_shortfall_survives_an_unmeasured_requirement() {
    // The defect: an unmeasured requirement swallowing a failure that was
    // actually measured, so a phone with 512 MB of memory is told that
    // whether it fits cannot be told.
    let budget = Budget::new(Some(Tier::T3), Some(512), None, Some(ThermalClass::Heavy));
    let selection = select(&budget);
    let greyed = selection
        .greyed
        .iter()
        .find(|greyed| greyed.entry.id == VAYU_INFERENCE.id)
        .expect("the heavy service is greyed out");
    match greyed.reason {
        Reason::TooSmall(Shortfall::Memory { wanted_mb, have_mb }) => {
            assert_eq!(wanted_mb, VAYU_INFERENCE.envelope.memory_mb);
            assert_eq!(have_mb, 512);
        }
        other => panic!("expected a memory shortfall, got {other:?}"),
    }
}

#[test]
fn a_device_with_nothing_measured_is_never_told_a_service_fits() {
    // The defect: "nothing measured" read as "nothing missing", so a device
    // nobody has profiled was offered every service in the catalogue.
    let budget = Budget::new(Some(Tier::T3), None, None, None);
    let selection = select(&budget);
    assert_eq!(selection.fitting, [] as [&Entry; 0]);
    assert_eq!(selection.total(), catalogue().len());
    for greyed in &selection.greyed {
        let Reason::ResourcesUnknown { missing, .. } = greyed.reason else {
            panic!("{}: {:?}", greyed.entry.id, greyed.reason);
        };
        // Every one of them, by name: a requirement that was never measured
        // and a requirement that was measured are different, and a reader
        // sent to the wrong one is sent to nothing.
        assert!(missing.memory, "{}: {missing:?}", greyed.entry.id);
        assert!(missing.storage, "{}: {missing:?}", greyed.entry.id);
        assert!(missing.thermal, "{}: {missing:?}", greyed.entry.id);
    }
}

#[test]
fn the_reason_names_the_requirements_that_were_never_measured() {
    // The defect: "cannot be told" without saying what could not be told, so
    // the operator is left looking for a measurement nobody made.
    let one = Reason::ResourcesUnknown {
        floor: Tier::T1,
        missing: Unmeasured {
            memory: true,
            storage: false,
            thermal: false,
        },
    };
    let text = one.to_string();
    assert!(text.contains("memory"), "{text}");
    assert!(text.contains("cannot be told"), "{text}");

    let two = Reason::ResourcesUnknown {
        floor: Tier::T1,
        missing: Unmeasured {
            memory: false,
            storage: true,
            thermal: true,
        },
    };
    let text = two.to_string();
    assert!(text.contains("storage"), "{text}");
    assert!(text.contains("thermal class"), "{text}");
    assert!(!text.contains("memory"), "{text}");
}
