// SPDX-License-Identifier: Apache-2.0

//! Vault tests, in the attacker's voice.
//!
//! The interesting cases are not "does a file get stored". They are the name
//! that is really a path, the upload that arrives while the phone is on
//! battery, the file that does not fit, and the receipt that would like to say
//! "saved".

use crate::durability::DurabilityClass;
use crate::governor::Level;
use crate::host::FakeHost;
use crate::shed::Stage;
use crate::vault::{
    Admission, Name, NameError, Quota, Receipt, Refused, RootError, Step, TooLarge, VaultRoot,
    WritePlan,
};

const ROOT: &str = "/data/vault";

fn host() -> FakeHost {
    FakeHost::new().with_dir(ROOT)
}

fn root() -> VaultRoot {
    VaultRoot::open(&host(), ROOT).expect("the fixture creates it")
}

fn room() -> Quota {
    Quota::new(0, 1_000_000)
}

// ── Opening ───────────────────────────────────────────────────────────────────

#[test]
fn a_directory_the_host_cannot_see_is_not_opened() {
    assert_eq!(
        VaultRoot::open(&FakeHost::new(), "/nope"),
        Err(RootError::Missing("/nope".to_owned()))
    );
}

#[test]
fn an_empty_directory_name_is_refused_rather_than_defaulted() {
    let h = FakeHost::new();
    assert_eq!(VaultRoot::open(&h, ""), Err(RootError::Empty));
    assert_eq!(VaultRoot::open(&h, "/"), Err(RootError::Empty));
}

// ── Names ─────────────────────────────────────────────────────────────────────

#[test]
fn a_name_that_is_really_a_path_is_refused() {
    // The whole reason Name exists. A vault that joined an unchecked string to
    // its root would be a traversal with extra steps.
    for raw in [
        "../etc/passwd",
        "a/b",
        "a\\b",
        "/etc/passwd",
        "sub/dir/file",
    ] {
        assert_eq!(
            Name::new(raw).unwrap_err(),
            NameError::Separator,
            "{raw:?} was accepted"
        );
    }
}

#[test]
fn the_relative_names_are_refused_before_the_dot_rule_can_mask_them() {
    // "." and ".." both begin with a dot, so a Hidden refusal would be a true
    // statement that hides the more specific one. The order of the checks is
    // what makes the message useful.
    assert_eq!(Name::new(".").unwrap_err(), NameError::Relative);
    assert_eq!(Name::new("..").unwrap_err(), NameError::Relative);
}

#[test]
fn a_name_breaking_two_rules_reports_the_structural_one() {
    // `../secrets` is both hidden and a path. "Begins with a dot" is true and
    // sends somebody to fix the wrong half; the problem is that they handed
    // over a path where a name was asked for.
    assert_eq!(Name::new("../secrets").unwrap_err(), NameError::Separator);
    assert_eq!(Name::new(".ssh/id_rsa").unwrap_err(), NameError::Separator);
}

#[test]
fn a_hidden_name_is_refused_as_a_class() {
    for raw in [".env", ".git", ".ssh", ".anything"] {
        assert_eq!(Name::new(raw).unwrap_err(), NameError::Hidden, "{raw}");
    }
}

#[test]
fn a_name_carrying_a_control_character_is_refused() {
    // A newline in a filename rewrites any log line that prints it, and a NUL
    // truncates the name at the first C string that touches it.
    for raw in ["a\nb", "a\0b", "a\tb", "\u{7f}x"] {
        assert_eq!(Name::new(raw).unwrap_err(), NameError::Control, "{raw:?}");
    }
}

#[test]
fn a_name_ending_in_a_space_or_a_dot_is_refused() {
    // Several filesystems strip these silently, so the file somebody asked for
    // and the file that exists have different names — and the next request for
    // the name they typed returns nothing.
    for raw in ["report ", "report."] {
        assert_eq!(
            Name::new(raw).unwrap_err(),
            NameError::TrailingSpaceOrDot,
            "{raw:?}"
        );
    }
}

#[test]
fn the_length_limit_counts_bytes_rather_than_characters() {
    // A filesystem's limit is on bytes. Counting characters would accept a name
    // of 255 emoji — about a kilobyte — and the write would fail at the syscall
    // with an error nobody can act on.
    let long_ascii = "a".repeat(Name::MAX_BYTES);
    assert!(Name::new(&long_ascii).is_ok());
    assert_eq!(
        Name::new(&"a".repeat(Name::MAX_BYTES + 1)).unwrap_err(),
        NameError::TooLong(Name::MAX_BYTES + 1)
    );

    let emoji = "\u{1f600}".repeat(64); // 4 bytes each = 256
    assert_eq!(emoji.chars().count(), 64, "well under any character limit");
    assert_eq!(
        Name::new(&emoji).unwrap_err(),
        NameError::TooLong(256),
        "counted in characters this would have been accepted"
    );
}

#[test]
fn an_ordinary_name_survives_every_rule() {
    for raw in ["report.pdf", "photo 2026.jpg", "notes-v2.md", "a"] {
        assert_eq!(
            Name::new(raw).expect("ordinary").as_str(),
            raw,
            "{raw} was refused"
        );
    }
}

#[test]
fn every_refusal_says_what_to_change_rather_than_that_it_is_invalid() {
    // Somebody is holding a file. "Invalid filename" tells them nothing about
    // which part to fix.
    for raw in ["", ".", ".env", "a/b", "a\nb", "x "] {
        let msg = Name::new(raw).unwrap_err().to_string();
        // A sentence, not a label. Counting words rather than characters,
        // because a long label is still a label.
        assert!(
            msg.split_whitespace().count() >= 4,
            "{raw:?} produced a label rather than a sentence: {msg}"
        );
        for useless in ["invalid", "bad name", "error", "not allowed."] {
            assert!(
                !msg.to_lowercase().contains(useless),
                "{raw:?} was told {useless:?}, which names nothing to change: {msg}"
            );
        }
    }
}

// ── Room ──────────────────────────────────────────────────────────────────────

#[test]
fn a_file_that_does_not_fit_is_refused_with_the_amount_it_is_short() {
    let q = Quota::new(900, 1000);
    assert_eq!(q.free(), 100);
    let e = q.admits(250).unwrap_err();
    assert_eq!(e.offered, 250);
    assert_eq!(e.free, 100);
    assert_eq!(e.shortfall, 150);
    assert!(e.to_string().contains("150"), "{e}");
}

#[test]
fn a_file_that_exactly_fills_the_quota_is_accepted() {
    assert!(Quota::new(900, 1000).admits(100).is_ok());
    assert!(Quota::new(900, 1000).admits(101).is_err());
}

#[test]
fn a_quota_already_over_its_limit_reports_no_free_space_rather_than_wrapping() {
    // used > limit is reachable after a limit is lowered. Subtracting without
    // saturating would underflow to an enormous free figure and admit anything.
    let q = Quota::new(2000, 1000);
    assert_eq!(q.free(), 0);
    assert!(q.admits(1).is_err());
}

// ── The governor, which decides before the disk does ──────────────────────────

#[test]
fn a_healthy_device_on_mains_accepts_a_file() {
    assert_eq!(
        Admission::of(Level::Normal, Stage::Serving, Some(room()), 10),
        Admission::Accepting
    );
}

#[test]
fn a_derated_device_refuses_a_write_even_though_it_would_still_serve_a_site() {
    // The asymmetry that is the point of this module. site::Availability keeps
    // serving at DERATED because a read is not what is heating the device. A
    // write is refused there: a refused upload costs one retry, a half-written
    // file outlives the event.
    assert!(crate::site::Availability::of(Level::Derated, Stage::Serving).is_serving());
    assert_eq!(
        Admission::of(Level::Derated, Stage::Serving, Some(room()), 10),
        Admission::Refusing(Refused::Governor(Level::Derated))
    );
}

#[test]
fn protect_and_halt_refuse_writes() {
    for level in [Level::Protect, Level::Halt] {
        assert_eq!(
            Admission::of(level, Stage::Serving, Some(room()), 10),
            Admission::Refusing(Refused::Governor(level)),
            "{level}"
        );
    }
}

#[test]
fn the_announced_rung_refuses_because_an_upload_is_new_work() {
    // Not a new policy. That rung's own obligation is "told the fleet and
    // stopped accepting new work", and an upload is new work. The site keeps
    // serving there; the vault does not.
    assert!(Stage::Announced
        .obligation()
        .contains("stopped accepting new work"));
    assert!(crate::site::Availability::of(Level::Normal, Stage::Announced).is_serving());
    assert_eq!(
        Admission::of(Level::Normal, Stage::Announced, Some(room()), 10),
        Admission::Refusing(Refused::Outage(Stage::Announced))
    );
}

#[test]
fn every_rung_below_serving_refuses() {
    for stage in [
        Stage::Announced,
        Stage::Shed,
        Stage::Quiesced,
        Stage::ShuttingDown,
    ] {
        assert_eq!(
            Admission::of(Level::Normal, stage, Some(room()), 10),
            Admission::Refusing(Refused::Outage(stage)),
            "{stage:?}"
        );
    }
}

#[test]
fn the_device_is_named_before_the_disk_when_both_would_refuse() {
    // A halted phone with a full disk is a halted phone. Telling somebody to
    // free up space would send them to fix the wrong thing.
    let full = Quota::new(1000, 1000);
    assert_eq!(
        Admission::of(Level::Halt, Stage::Serving, Some(full), 10),
        Admission::Refusing(Refused::Governor(Level::Halt))
    );
}

#[test]
fn only_one_combination_of_level_and_stage_accepts_anything() {
    // Exhaustive, so a level or a rung added later cannot fall through to a
    // default that accepts writes.
    let mut accepting = 0;
    for level in [Level::Normal, Level::Derated, Level::Protect, Level::Halt] {
        for stage in [
            Stage::Serving,
            Stage::Announced,
            Stage::Shed,
            Stage::Quiesced,
            Stage::ShuttingDown,
        ] {
            if Admission::of(level, stage, Some(room()), 1).is_accepting() {
                accepting += 1;
            }
        }
    }
    assert_eq!(accepting, 1, "only NORMAL on mains may take a file");
}

#[test]
fn a_refusal_explains_itself_to_whoever_offered_the_file() {
    let g = Admission::of(Level::Halt, Stage::Serving, Some(room()), 1).describe();
    assert!(g.contains("HALT"), "{g}");
    assert!(g.contains("not taken"), "{g}");

    let o = Admission::of(Level::Normal, Stage::Shed, Some(room()), 1).describe();
    assert!(o.contains("stopped non-essential services"), "{o}");

    let f = Admission::of(Level::Normal, Stage::Serving, Some(Quota::new(0, 5)), 10).describe();
    assert!(f.contains('5'), "{f}");
}

// ── A quota nobody could read ─────────────────────────────────────────────────

#[test]
fn a_vault_whose_usage_could_not_be_read_refuses_the_write() {
    // The whole point of the Option. Measuring what a directory holds is I/O and
    // I/O fails; falling back to "nothing is used" would admit every write on
    // the first unreadable directory, and admit it silently.
    assert_eq!(
        Admission::of(Level::Normal, Stage::Serving, None, 1),
        Admission::Refusing(Refused::Unmeasured)
    );
}

#[test]
fn an_unmeasured_vault_refuses_even_a_zero_byte_file() {
    // Not "it fits because it is empty". Nothing is known about the room, and a
    // zero-byte file still creates a directory entry.
    assert_eq!(
        Admission::of(Level::Normal, Stage::Serving, None, 0),
        Admission::Refusing(Refused::Unmeasured)
    );
}

#[test]
fn an_unmeasured_vault_is_never_reported_as_a_full_one() {
    // Full names a shortfall, which is a measurement. This refusal is the
    // absence of one, and dressing it as a shortfall invents a number.
    let a = Admission::of(Level::Normal, Stage::Serving, None, 10);
    assert_ne!(
        a,
        Admission::Refusing(Refused::Full(TooLarge {
            offered: 10,
            free: 0,
            shortfall: 10
        }))
    );
    let said = a.describe();
    assert!(said.contains("could not be read"), "{said}");
    assert!(!said.contains("would have to be freed"), "{said}");
}

#[test]
fn the_device_is_still_named_before_an_unmeasured_disk() {
    // Same ordering as a full one: a halted phone is a halted phone, and sending
    // its owner to look at a directory would send them to fix the wrong thing.
    assert_eq!(
        Admission::of(Level::Halt, Stage::Serving, None, 10),
        Admission::Refusing(Refused::Governor(Level::Halt))
    );
    assert_eq!(
        Admission::of(Level::Normal, Stage::Shed, None, 10),
        Admission::Refusing(Refused::Outage(Stage::Shed))
    );
}

// ── Removal, which asks the device but not the disk ───────────────────────────

#[test]
fn a_removal_is_admitted_when_a_write_of_the_same_moment_would_not_be() {
    // Either refusal would be a state with no way out of itself: the only
    // request that frees room, refused for want of room.
    let full = Some(Quota::new(1000, 1000));
    assert!(!Admission::of(Level::Normal, Stage::Serving, full, 1).is_accepting());
    assert!(!Admission::of(Level::Normal, Stage::Serving, None, 1).is_accepting());
    assert!(Admission::for_removal(Level::Normal, Stage::Serving).is_accepting());
}

#[test]
fn a_removal_still_obeys_the_governor_and_the_ladder() {
    // The disk is skipped; the device is not. Deleting a file is still work, and
    // a phone protecting its cell is not doing work.
    for level in [Level::Derated, Level::Protect, Level::Halt] {
        assert_eq!(
            Admission::for_removal(level, Stage::Serving),
            Admission::Refusing(Refused::Governor(level)),
            "{level}"
        );
    }
    for stage in [
        Stage::Announced,
        Stage::Shed,
        Stage::Quiesced,
        Stage::ShuttingDown,
    ] {
        assert_eq!(
            Admission::for_removal(Level::Normal, stage),
            Admission::Refusing(Refused::Outage(stage)),
            "{stage:?}"
        );
    }
}

// ── The plan, which is the ordering ───────────────────────────────────────────

#[test]
fn a_refused_write_yields_no_plan() {
    // The refusal and the plan are the same decision. Handing back a plan the
    // device refused is how a check gets skipped by a caller in a hurry.
    let refused = Admission::of(Level::Halt, Stage::Serving, Some(room()), 1);
    assert!(refused
        .plan(&root(), &Name::new("a.txt").expect("ordinary"))
        .is_none());
}

#[test]
fn an_accepted_write_names_a_temporary_beside_the_destination() {
    // Beside it, not in /tmp: a rename across filesystems is a copy, and a copy
    // is not atomic.
    let name = Name::new("report.pdf").expect("ordinary");
    let plan = Admission::of(Level::Normal, Stage::Serving, Some(room()), 10)
        .plan(&root(), &name)
        .expect("an accepted write has a plan");

    assert_eq!(plan.destination(), "/data/vault/report.pdf");
    assert_eq!(plan.temporary(), "/data/vault/.report.pdf.partial");
    assert_eq!(plan.directory(), ROOT);
    assert_eq!(
        plan.temporary().rsplit_once('/').expect("has a parent").0,
        plan.destination().rsplit_once('/').expect("has a parent").0,
        "the temporary must share a directory with the destination"
    );
}

#[test]
fn the_temporary_is_hidden_so_debris_is_recognisable_as_debris() {
    // And so the site, which refuses hidden names as a class, can never serve a
    // partially written file.
    let name = Name::new("photo.jpg").expect("ordinary");
    let plan = Admission::of(Level::Normal, Stage::Serving, Some(room()), 10)
        .plan(&root(), &name)
        .expect("accepted");
    let leaf = plan.temporary().rsplit_once('/').expect("has a parent").1;
    assert!(leaf.starts_with('.'), "{leaf}");
    assert!(
        crate::vault::Name::new(leaf).is_err(),
        "the temporary's own leaf must not be an acceptable name"
    );
}

#[test]
fn the_steps_are_the_only_order_that_survives_a_power_cut() {
    // The whole correctness argument, asserted as data. Flushing after the
    // rename publishes a name whose bytes may still be in cache; skipping the
    // directory flush loses the rename itself.
    assert_eq!(
        WritePlan::steps(),
        [
            Step::WriteTemporary,
            Step::FlushFile,
            Step::RenameOverDestination,
            Step::FlushDirectory,
        ]
    );
}

#[test]
fn the_file_is_flushed_before_it_is_renamed() {
    // Stated as an ordering rather than as a list, so a reordering that keeps
    // all four steps still fails.
    let steps = WritePlan::steps();
    let at = |s: Step| steps.iter().position(|x| *x == s).expect("present");
    assert!(at(Step::FlushFile) < at(Step::RenameOverDestination));
    assert!(at(Step::WriteTemporary) < at(Step::FlushFile));
    assert!(at(Step::RenameOverDestination) < at(Step::FlushDirectory));
}

#[test]
fn every_step_says_what_it_is_for() {
    for step in WritePlan::steps() {
        let why = step.why();
        assert!(why.len() > 30, "{step:?} explains nothing: {why}");
    }
}

// ── The receipt, which must not say "saved" ───────────────────────────────────

#[test]
fn a_receipt_never_claims_the_file_is_durable() {
    // ADR-0004 section 0: nothing running on a sealed phone can tell a flash
    // that honoured a flush from one that acknowledged it and did nothing. A
    // receipt saying "saved" would be that lie, issued by the feature meant to
    // prevent it.
    let r = Receipt::new(Name::new("report.pdf").expect("ordinary"), 4096);
    assert_eq!(r.durability, DurabilityClass::AssumedUntrusted);
    assert!(!r.durability.is_lab_verified());

    let text = r.describe();
    for forbidden in ["saved", "safe", "durable", "guaranteed", "backed up"] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "a receipt said {forbidden:?}: {text}"
        );
    }
    assert!(text.contains("nothing else"), "{text}");
    assert!(text.contains("keep a copy elsewhere"), "{text}");
}

#[test]
fn a_receipt_carries_the_name_and_the_size_it_actually_wrote() {
    let r = Receipt::new(Name::new("a.bin").expect("ordinary"), 17);
    assert_eq!(r.name.as_str(), "a.bin");
    assert_eq!(r.bytes, 17);
    assert!(r.describe().starts_with("a.bin"), "{}", r.describe());
}
