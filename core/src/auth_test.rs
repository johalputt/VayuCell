// SPDX-License-Identifier: Apache-2.0

//! Credential tests, in the attacker's voice.
//!
//! The interesting cases are the store nobody filled in, the secret that leaks
//! through a log line, and the comparison that answers faster when the first
//! byte is right.

use crate::auth::{
    constant_time_eq, parse_store, readable_by_others, Credential, Credentials, DeviceError,
    DeviceName, Refusal, Secret, SecretError, StoreProblem, Verdict, SECRET_CHARS,
};

/// A distinct, well-formed secret. `n` picks which one.
fn secret(n: u8) -> String {
    let mut s = String::new();
    for i in 0..SECRET_CHARS {
        // Deterministic, inside the base64url alphabet, and different per n.
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let idx = (usize::from(n) * 7 + i * 3) % alphabet.len();
        s.push(alphabet[idx] as char);
    }
    s
}

fn store(devices: &[(&str, u8)]) -> Credentials {
    Credentials::new(
        devices
            .iter()
            .map(|(name, n)| Credential {
                device: DeviceName::new(name).expect("a plain name"),
                secret: Secret::new(&secret(*n)).expect("a minted secret"),
            })
            .collect(),
    )
}

// ── The default that would have been a disaster ───────────────────────────────

#[test]
fn an_empty_store_accepts_nothing() {
    // The single most dangerous thing this module could do is treat "no devices
    // enrolled" as "no authentication required". Every installation starts in
    // this state, so it is the one worth being loudest about.
    let empty = Credentials::empty();
    assert!(empty.is_empty());
    assert_eq!(
        empty.verify(Some(&secret(1))),
        Verdict::Refused(Refusal::StoreEmpty)
    );
    assert_eq!(empty.verify(None), Verdict::Refused(Refusal::NoneOffered));
    assert!(!empty.verify(Some(&secret(1))).is_authenticated());
}

#[test]
fn the_default_store_is_the_empty_one() {
    // A Default that enrolled anything would put a credential on every device
    // that never configured one.
    assert_eq!(Credentials::default(), Credentials::empty());
    assert!(Credentials::default().is_empty());
}

#[test]
fn an_empty_store_is_told_apart_from_a_wrong_secret() {
    // An operator who has not enrolled anything needs to hear that, not that
    // their secret is wrong — they would go looking for a typo that is not there.
    let empty = Credentials::empty().verify(Some(&secret(1)));
    let wrong = store(&[("laptop", 1)]).verify(Some(&secret(2)));
    assert_ne!(empty, wrong);
    assert_eq!(empty, Verdict::Refused(Refusal::StoreEmpty));
    assert_eq!(wrong, Verdict::Refused(Refusal::NotRecognised));
}

// ── Verification ──────────────────────────────────────────────────────────────

#[test]
fn an_enrolled_secret_authenticates_and_names_its_device() {
    // Naming the device is what makes a store prunable: an operator looking at
    // a log needs to know which thing to revoke.
    let s = store(&[("laptop", 1), ("phone", 2)]);
    assert_eq!(
        s.verify(Some(&secret(2))),
        Verdict::Authenticated(DeviceName::new("phone").expect("plain"))
    );
}

#[test]
fn a_secret_that_is_not_enrolled_is_refused() {
    let s = store(&[("laptop", 1)]);
    assert_eq!(
        s.verify(Some(&secret(9))),
        Verdict::Refused(Refusal::NotRecognised)
    );
}

#[test]
fn a_prefix_of_an_enrolled_secret_is_refused() {
    // The attack a length-tolerant comparison enables.
    let full = secret(1);
    let s = store(&[("laptop", 1)]);
    for cut in [0, 1, SECRET_CHARS / 2, SECRET_CHARS - 1] {
        assert_eq!(
            s.verify(Some(&full[..cut])),
            Verdict::Refused(Refusal::NotRecognised),
            "a {cut}-character prefix was accepted"
        );
    }
}

#[test]
fn a_secret_with_something_appended_is_refused() {
    let s = store(&[("laptop", 1)]);
    let longer = format!("{}x", secret(1));
    assert_eq!(
        s.verify(Some(&longer)),
        Verdict::Refused(Refusal::NotRecognised)
    );
}

#[test]
fn revoking_a_device_takes_effect_by_the_entry_being_gone() {
    // Removal is the whole revocation story. Absence must mean refusal, never
    // "no rule for this one, so allow".
    let before = store(&[("laptop", 1), ("phone", 2)]);
    assert!(before.verify(Some(&secret(2))).is_authenticated());

    let after = store(&[("laptop", 1)]);
    assert_eq!(
        after.verify(Some(&secret(2))),
        Verdict::Refused(Refusal::NotRecognised)
    );
    assert_eq!(after.len(), 1);
}

#[test]
fn every_entry_is_compared_so_position_in_the_store_is_not_observable() {
    // Returning on the first match would answer sooner for a device enrolled
    // early than one enrolled late. Timing is not assertable here, so what is
    // asserted is the observable consequence: the last entry authenticates
    // exactly as the first does.
    let s = store(&[("a", 1), ("b", 2), ("c", 3), ("d", 4)]);
    for (name, n) in [("a", 1u8), ("b", 2), ("c", 3), ("d", 4)] {
        assert_eq!(
            s.verify(Some(&secret(n))),
            Verdict::Authenticated(DeviceName::new(name).expect("plain")),
            "{name}"
        );
    }
}

// ── The comparison itself ─────────────────────────────────────────────────────

#[test]
fn the_comparison_agrees_with_equality_on_every_case_that_matters() {
    assert!(constant_time_eq(b"", b""));
    assert!(constant_time_eq(b"abc", b"abc"));
    assert!(!constant_time_eq(b"abc", b"abd"));
    assert!(!constant_time_eq(b"abc", b"ab"));
    assert!(!constant_time_eq(b"ab", b"abc"));
    assert!(!constant_time_eq(b"", b"a"));
    // Differing in the last byte only — the case an accumulator must still catch.
    assert!(!constant_time_eq(
        b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab",
        b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaac"
    ));
    // Differing in the first byte only.
    assert!(!constant_time_eq(b"xaaaaaaa", b"yaaaaaaa"));
}

#[test]
fn the_comparison_is_exhaustively_right_on_short_inputs() {
    // A hand-written comparison is worth checking against the language's own,
    // over every short case rather than a chosen few.
    let alphabet = *b"ab";
    let mut cases: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for c in &cases {
            for byte in alphabet {
                let mut v = c.clone();
                v.push(byte);
                next.push(v);
            }
        }
        cases.extend(next);
    }
    for a in &cases {
        for b in &cases {
            assert_eq!(constant_time_eq(a, b), a == b, "{a:?} vs {b:?}");
        }
    }
}

// ── Secrets ───────────────────────────────────────────────────────────────────

#[test]
fn a_memorable_secret_cannot_be_enrolled() {
    // The property the whole module rests on. With no memory-hard derivation
    // available under the no-dependencies rule, the only safe secret is one
    // nobody chose.
    for weak in ["hunter2", "password", "", "abc"] {
        assert!(
            matches!(Secret::new(weak), Err(SecretError::WrongLength(_))),
            "{weak:?} was accepted"
        );
    }
}

#[test]
fn a_secret_of_the_right_length_but_the_wrong_alphabet_is_refused() {
    let bad = "!".repeat(SECRET_CHARS);
    assert_eq!(Secret::new(&bad), Err(SecretError::IllegalCharacter));
}

#[test]
fn a_secret_never_appears_in_its_own_debug_output() {
    // A derived Debug puts the secret into every {:?}, every unwrap panic and
    // every log line that prints a structure containing one — and not one of
    // those call sites reads like a disclosure.
    let raw = secret(1);
    let s = Secret::new(&raw).expect("minted");
    let rendered = format!("{s:?}");
    assert!(!rendered.contains(&raw), "the secret leaked: {rendered}");
    assert_eq!(rendered, "Secret(hidden)");

    // And through a structure that contains one.
    let c = Credential {
        device: DeviceName::new("laptop").expect("plain"),
        secret: Secret::new(&raw).expect("minted"),
    };
    let rendered = format!("{c:?}");
    assert!(!rendered.contains(&raw), "the secret leaked: {rendered}");

    // And through the store.
    let rendered = format!("{:?}", store(&[("laptop", 1)]));
    assert!(!rendered.contains(&raw), "the secret leaked: {rendered}");
}

#[test]
fn the_length_is_counted_in_characters_of_the_encoding() {
    assert!(Secret::new(&secret(1)).is_ok());
    assert_eq!(
        Secret::new(&secret(1)[..SECRET_CHARS - 1]),
        Err(SecretError::WrongLength(SECRET_CHARS - 1))
    );
    assert_eq!(SECRET_CHARS, 43, "43 base64url characters is 256 bits");
}

// ── Device names ──────────────────────────────────────────────────────────────

#[test]
fn a_device_name_may_not_carry_whitespace_because_the_store_separates_on_it() {
    // "my laptop" is what somebody actually types, and the space is the whole
    // problem: the store puts the name and the secret on one line.
    assert_eq!(DeviceName::new("my laptop"), Err(DeviceError::Whitespace));
    assert_eq!(DeviceName::new(" leading"), Err(DeviceError::Whitespace));

    // A tab is whitespace and a control character both. The control check runs
    // first and names the more serious class, which is the right answer for a
    // byte that arrived by paste rather than by typing.
    assert_eq!(DeviceName::new("a\tb"), Err(DeviceError::Control));
}

#[test]
fn a_device_name_may_not_rewrite_the_log_line_that_reports_it() {
    assert_eq!(DeviceName::new("a\nb"), Err(DeviceError::Control));
    assert_eq!(DeviceName::new("a\0b"), Err(DeviceError::Control));
}

#[test]
fn an_ordinary_device_name_is_kept_as_written() {
    for raw in ["laptop", "ankush-phone", "kitchen_tablet", "n8"] {
        assert_eq!(DeviceName::new(raw).expect("plain").as_str(), raw);
    }
    assert_eq!(DeviceName::new(""), Err(DeviceError::Empty));
}

// ── The store file ────────────────────────────────────────────────────────────

#[test]
fn a_store_parses_names_secrets_comments_and_blank_lines() {
    let text = format!(
        "# devices allowed to write here\n\
         laptop  {}\n\
         \n\
         phone {}\n",
        secret(1),
        secret(2)
    );
    let parsed = parse_store(&text).expect("a well-formed store");
    assert_eq!(parsed.len(), 2);
    assert_eq!(
        parsed
            .devices()
            .iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>(),
        ["laptop", "phone"]
    );
    assert!(parsed.verify(Some(&secret(2))).is_authenticated());
}

#[test]
fn an_empty_store_file_parses_to_a_store_that_accepts_nothing() {
    for text in ["", "\n\n", "# nothing here yet\n"] {
        let parsed = parse_store(text).expect("valid but empty");
        assert!(parsed.is_empty(), "{text:?}");
        assert_eq!(
            parsed.verify(Some(&secret(1))),
            Verdict::Refused(Refusal::StoreEmpty)
        );
    }
}

#[test]
fn a_bad_line_refuses_the_whole_store_rather_than_loading_part_of_it() {
    // A typo on line four must not silently leave one device unenrolled. The
    // symptom would be a device that stopped working for a reason nobody
    // connects to the edit.
    let text = format!(
        "laptop {}\nphone hunter2\ntablet {}\n",
        secret(1),
        secret(3)
    );
    let e = parse_store(&text).expect_err("line 2 is not a secret");
    assert_eq!(e.line, 2);
    assert!(matches!(e.why, StoreProblem::Secret(_)));
    assert!(e.to_string().starts_with("line 2:"), "{e}");
}

#[test]
fn a_name_enrolled_twice_refuses_the_store_and_names_both_lines() {
    // `enrol` already refuses to write a duplicate. That guarded the path this
    // software writes; the store is a text file the operator is told to edit by
    // hand — the enrolment error says to remove the existing line first — so the
    // one path a duplicate actually arrives by had no check on it at all.
    //
    // The harm is identity. `verify` matches on the secret and answers with the
    // name, so two rows sharing a name means two different credentials
    // authenticate as one device and nothing can say which presented it.
    let text = format!(
        "laptop {}\nphone {}\nlaptop {}\n",
        secret(1),
        secret(2),
        secret(3)
    );
    let e = parse_store(&text).expect_err("laptop is enrolled twice");
    assert_eq!(e.line, 3);
    assert_eq!(e.why, StoreProblem::Duplicate { first_seen: 1 });

    let said = e.to_string();
    assert!(said.starts_with("line 3:"), "{said}");
    assert!(
        said.contains("line 1"),
        "both rows have to be findable: {said}"
    );
    assert!(said.contains("which of them presented"), "{said}");
}

#[test]
fn the_duplicate_names_the_line_it_was_first_seen_on_not_its_place_in_the_list() {
    // Blank lines and comments are skipped, so an entry's position in the parsed
    // list is not its line number. A message off by the number of comments above
    // it sends somebody to edit the wrong row of a file full of secrets.
    let text = format!(
        "# the household devices\n\nlaptop {}\n\n# added later\nlaptop {}\n",
        secret(1),
        secret(2)
    );
    let e = parse_store(&text).expect_err("laptop is enrolled twice");
    assert_eq!(e.line, 6, "the offending line");
    assert_eq!(
        e.why,
        StoreProblem::Duplicate { first_seen: 3 },
        "line 3, not entry 1"
    );
}

#[test]
fn two_devices_with_different_names_are_not_a_duplicate() {
    // The check must refuse a repeated name and nothing else. A store that
    // refused two distinct devices would make the feature this module exists for
    // impossible to use.
    let text = format!("laptop {}\nphone {}\n", secret(1), secret(2));
    let creds = parse_store(&text).expect("two devices is the ordinary case");
    assert_eq!(creds.len(), 2);
}

#[test]
fn a_name_that_only_appears_in_a_comment_is_not_a_duplicate() {
    // Comments are skipped before anything is parsed, and a revoked device left
    // commented out for the record is an ordinary thing for an operator to do.
    let text = format!("# laptop was revoked\nlaptop {}\n", secret(1));
    let creds = parse_store(&text).expect("a comment enrols nobody");
    assert_eq!(creds.len(), 1);
}

#[test]
fn a_line_that_is_not_two_fields_is_refused_by_number() {
    for (text, line) in [
        ("laptop\n".to_owned(), 1usize),
        (format!("laptop {} extra\n", secret(1)), 1),
        (format!("laptop {}\nphone\n", secret(1)), 2),
    ] {
        let e = parse_store(&text).expect_err("not two fields");
        assert_eq!(e.line, line, "{text:?}");
        assert_eq!(e.why, StoreProblem::NotTwoFields);
    }
}

#[test]
fn a_store_error_never_prints_the_secret_that_was_on_the_line() {
    // The error goes to a terminal and, more importantly, to a log.
    let raw = secret(1);
    let text = format!("has a space in it {raw}\n");
    let e = parse_store(&text).expect_err("three fields");
    assert!(!e.to_string().contains(&raw), "{e}");
}

// ── The permissions the store depends on ──────────────────────────────────────

#[test]
fn a_store_readable_by_anyone_else_is_reported_as_such() {
    // The store holds secrets in the clear, so the mode is the whole of the
    // protection — and absence is never protection, so it is checked.
    assert!(!readable_by_others(0o600));
    assert!(!readable_by_others(0o400));
    assert!(readable_by_others(0o640), "group read");
    assert!(readable_by_others(0o604), "other read");
    assert!(readable_by_others(0o660), "group write");
    assert!(readable_by_others(0o666));
    assert!(readable_by_others(0o777));
}

#[test]
fn the_permission_check_ignores_the_owners_own_bits() {
    // Owner read, write and execute are all fine; only group and other matter.
    for owner in [0o000, 0o100, 0o200, 0o400, 0o700] {
        assert!(
            !readable_by_others(owner),
            "{owner:o} was reported as exposed"
        );
    }
}
