// SPDX-License-Identifier: Apache-2.0

//! What a replica that dials can prove to a cell that cannot.
//!
//! # The cell never dials, so the cell never measures
//!
//! Charter Article V.2 forbids this binary from opening an outbound
//! connection, and ADR-0004 §1.1 wants a *live* replication lag. Those two
//! sentences cannot both be produced by one process: a lag is measured by
//! asking the replica something, and asking is dialing. The design answer is
//! the same one every honest audit uses — **the party that can look, looks,
//! and leaves dated evidence**.
//!
//! The companion (`vayucell-sync replicate` and `vayucell-sync drill`) does
//! the dialing and, after a complete successful cycle, writes a receipt.
//! This module reads receipts. That is all it does, and the restraint is the
//! point: a receipt is a claim made by another program on another machine,
//! and the whole of this module's job is to turn such claims into
//! [`RecoveryPoint`] and [`BackupState`] values **without ever improving
//! them**.
//!
//! # The three ways a claim gets worse, and what happens to each
//!
//! 1. **It ages.** A replication receipt older than
//!    [`MEASUREMENT_STANDS_FOR`] is not a live figure any more, and it
//!    becomes [`RecoveryPoint::Unreachable`] — because a replicator that
//!    stopped leaving receipts is a replicator that stopped running, and
//!    *"nobody has claimed a successful cycle since"* is exactly what that
//!    variant exists to say. The companion is invoked, not resident, so this
//!    is the ordinary state between runs, and the panel naming the age is
//!    the honest price of never dialing.
//! 2. **It contradicts the clock.** A stamp ahead of this cell's clock
//!    cannot be aged, and an age that cannot be established is not an age.
//!    Same for a mirrored-file mtime ahead of the clock: machines with
//!    skewed clocks produce lag figures of zero or less meaning, so the
//!    receipt is refused rather than clamped — clamping is arithmetic
//!    pretending to be a measurement.
//! 3. **It was never legible.** A truncated or edited evidence file renders
//!    as unreachable-and-unusable, named as such, and nothing downstream
//!    guesses around a file nobody could fully read. The worst honest
//!    reading of garbage is not *"probably fine"*.
//!
//! # What this module refuses to invent
//!
//! A replication receipt alone says nothing about restores, so with no drill
//! claim it maps to [`BackupState::NeverRestored`] — the type whose own
//! words are *"what is verified is that files exist, not that they can be
//! recovered"*. Only a drill receipt maps to [`BackupState::Restored`], and
//! [`BackupState::is_proven`] then applies its own thirty-day expiry with no
//! help from here.

use crate::durability::{BackupState, RecoveryPoint, MEASUREMENT_STANDS_FOR};

/// One dated claim left by the companion, as written into the evidence file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Receipt {
    /// A complete pull cycle finished: everything listed was mirrored.
    Replication {
        /// Seconds since the Unix epoch when the cycle finished.
        completed_unix: u64,
        /// How many files the cycle mirrored.
        files: u64,
        /// How many bytes the mirror held across those files.
        bytes: u64,
        /// The newest file mtime the cycle confirmed seeing.
        ///
        /// The lag figure is computed from this, not from the completion
        /// time: data written *after* the newest thing the replica saw is
        /// data the replica has never confirmed, however recently the cycle
        /// ran.
        covered_mtime: u64,
    },
    /// Every listed file was downloaded afresh and matched the mirror byte
    /// for byte.
    RestoreDrill {
        /// Seconds since the Unix epoch when the drill finished.
        completed_unix: u64,
        /// How many files the drill restored and compared.
        files: u64,
        /// How many bytes the drill restored and compared, in total.
        bytes: u64,
    },
}

impl Receipt {
    /// The discriminator written into JSON, and matched when upserting.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Replication { .. } => "replication",
            Self::RestoreDrill { .. } => "restore-drill",
        }
    }

    /// When this claim was made.
    #[must_use]
    pub const fn completed_unix(&self) -> u64 {
        match self {
            Self::Replication { completed_unix, .. }
            | Self::RestoreDrill { completed_unix, .. } => *completed_unix,
        }
    }

    /// The canonical JSON for one receipt.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Replication {
                completed_unix,
                files,
                bytes,
                covered_mtime,
            } => format!(
                "{{\"kind\":\"replication\",\"completed_unix\":{completed_unix},\
                 \"files\":{files},\"bytes\":{bytes},\"covered_mtime\":{covered_mtime}}}"
            ),
            Self::RestoreDrill {
                completed_unix,
                files,
                bytes,
            } => format!(
                "{{\"kind\":\"restore-drill\",\"completed_unix\":{completed_unix},\
                 \"files\":{files},\"bytes\":{bytes}}}"
            ),
        }
    }
}

/// Parses the evidence file: a JSON array of receipts, kinds mixed.
///
/// Strict on purpose. Key order inside an object is tolerated; anything
/// else — a bare object instead of an array, an unquoted number, an unknown
/// field, trailing bytes after the closing bracket — is an error carrying
/// the reason. The caller shows the reason to an operator; nothing
/// downstream guesses around a file it could not fully read.
///
/// # Errors
///
/// The first structural problem, phrased with enough context to find it.
pub fn parse(text: &str) -> Result<Vec<Receipt>, String> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    p.expect(b'[', "the evidence to start with `[`")?;
    p.ws();
    let mut out = Vec::new();
    if p.take(b']') {
        p.ws();
        p.end()?;
        return Ok(out);
    }
    loop {
        p.ws();
        out.push(p.receipt()?);
        p.ws();
        if p.take(b']') {
            break;
        }
        p.expect(b',', "`,` between receipts")?;
    }
    p.ws();
    p.end()?;
    Ok(out)
}

/// Folds one fresh receipt into an evidence file's text, replacing any
/// earlier claim of the same kind.
///
/// The existing text is parsed before anything is written: a file this
/// function cannot read is a file the cell cannot read either, and silently
/// replacing it would destroy the only evidence there is. Refusing is the
/// repair that keeps somebody looking at the real problem.
///
/// # Errors
///
/// Whatever [`parse`] says about the existing text.
pub fn upsert(existing: Option<&str>, fresh: &Receipt) -> Result<String, String> {
    let mut receipts = match existing {
        None => Vec::new(),
        Some(text) => parse(text)?,
    };
    receipts.retain(|r| r.kind() != fresh.kind());
    receipts.push(fresh.clone());
    let rendered: Vec<String> = receipts.iter().map(Receipt::render).collect();
    Ok(format!("[{}]", rendered.join(",")))
}

/// The most recent receipt of one kind in a parsed file.
fn latest<'a>(kind: &'static str, all: &'a [Receipt]) -> Option<&'a Receipt> {
    all.iter()
        .filter(|r| r.kind() == kind)
        .max_by_key(|r| r.completed_unix())
}

/// The two posture fields a receipt file can speak to, as this cell must
/// render them.
///
/// `evidence` is the file's text — `None` when no path was configured or the
/// file does not exist, which are the same state here: **no claim has been
/// made**, and the answer is the same one a cell without a companion has
/// always given. `today` is `None` when the host would not say what day it
/// is, and every wall-clock judgement degrades honestly under that loss.
#[must_use]
pub fn posture_parts(
    evidence: Option<&str>,
    today: Option<u64>,
    since_start: core::time::Duration,
) -> (RecoveryPoint, BackupState) {
    let Some(text) = evidence else {
        return (RecoveryPoint::NoReplica, BackupState::NotConfigured);
    };
    let receipts = match parse(text) {
        Ok(r) => r,
        Err(why) => {
            // Not `NotConfigured` for the same reason as above: something has
            // been writing receipts, and "nothing is being backed up" would
            // be a second false sentence stacked on top of a broken file.
            return (
                RecoveryPoint::Unreachable(format!("its evidence file could not be read: {why}")),
                BackupState::RestoreFailed(format!("its receipt could not be parsed: {why}")),
            );
        }
    };

    let recovery = match today {
        // No wall clock, no age, no live figure — said in those words rather
        // than rendered as if the number were current.
        None if receipts.is_empty() => RecoveryPoint::NoReplica,
        None => RecoveryPoint::Unreachable(
            "this cell cannot tell what day it is, so the receipt's age cannot \
             be established, and an unageable claim is not a live figure"
                .to_owned(),
        ),
        Some(now) => match latest("replication", &receipts) {
            Some(Receipt::Replication { covered_mtime, .. }) => {
                let completed =
                    latest("replication", &receipts).map_or(now, Receipt::completed_unix);
                judge_lag(completed, *covered_mtime, now, since_start)
            }
            _ => RecoveryPoint::NoReplica,
        },
    };

    let backup = match latest("restore-drill", &receipts) {
        Some(Receipt::RestoreDrill { completed_unix, .. }) => BackupState::Restored {
            at_unix: *completed_unix,
        },
        _ if latest("replication", &receipts).is_some() => BackupState::NeverRestored,
        _ => BackupState::NotConfigured,
    };

    (recovery, backup)
}

/// One replication receipt, judged against the clock this cell actually has.
fn judge_lag(
    completed_unix: u64,
    covered_mtime: u64,
    now: u64,
    since_start: core::time::Duration,
) -> RecoveryPoint {
    if completed_unix > now || covered_mtime > now {
        return RecoveryPoint::Unreachable(
            "its receipt is stamped ahead of this cell's clock, so how old the \
             claim is cannot be established"
                .to_owned(),
        );
    }
    if now - completed_unix > MEASUREMENT_STANDS_FOR.as_secs() {
        return RecoveryPoint::Unreachable(format!(
            "its last receipt is {}s old and nobody has claimed a successful \
             cycle since — nothing is still measuring",
            now - completed_unix
        ));
    }
    RecoveryPoint::Behind {
        lag: core::time::Duration::from_secs(now.saturating_sub(covered_mtime)),
        measured_at: since_start,
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn peek_is(&self, c: u8) -> bool {
        self.b.get(self.i) == Some(&c)
    }

    fn take(&mut self, c: u8) -> bool {
        if self.peek_is(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn next_byte(&mut self) -> Option<u8> {
        let c = self.b.get(self.i).copied();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    fn expect(&mut self, c: u8, what: &str) -> Result<(), String> {
        if self.take(c) {
            Ok(())
        } else {
            Err(format!(
                "expected {what}, found {}",
                spell(self.next_byte())
            ))
        }
    }

    fn end(&mut self) -> Result<(), String> {
        if self.i == self.b.len() {
            Ok(())
        } else {
            Err(format!(
                "{} byte(s) remain after the end of the evidence",
                self.b.len() - self.i
            ))
        }
    }

    /// One receipt object. Field order free; each field at most once; every
    /// field its kind requires.
    fn receipt(&mut self) -> Result<Receipt, String> {
        self.expect(b'{', "a receipt to start with `{`")?;
        let mut kind: Option<String> = None;
        let mut completed_unix: Option<u64> = None;
        let mut files: Option<u64> = None;
        let mut bytes: Option<u64> = None;
        let mut covered_mtime: Option<u64> = None;

        self.ws();
        if self.peek_is(b'}') {
            self.i += 1;
            return Err("a receipt object with no fields".to_owned());
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.expect(b':', "`:` between a field and its value")?;
            self.ws();
            macro_rules! once {
                ($slot:expr, $name:literal) => {{
                    if $slot.is_some() {
                        return Err(format!("the field {} appears twice", $name));
                    }
                    $slot = Some(self.u64_value()?);
                }};
            }
            match key.as_str() {
                "kind" => {
                    if kind.is_some() {
                        return Err("the field \"kind\" appears twice".to_owned());
                    }
                    kind = Some(self.string()?);
                }
                "completed_unix" => once!(completed_unix, "\"completed_unix\""),
                "files" => once!(files, "\"files\""),
                "bytes" => once!(bytes, "\"bytes\""),
                "covered_mtime" => once!(covered_mtime, "\"covered_mtime\""),
                other => return Err(format!("unknown field {other:?} in a receipt")),
            }
            self.ws();
            if self.take(b'}') {
                break;
            }
            self.expect(b',', "`,` between fields")?;
        }

        let fail = |what: &str| format!("a receipt without {what}");
        let completed = completed_unix.ok_or_else(|| fail("\"completed_unix\""))?;
        let files = files.ok_or_else(|| fail("\"files\""))?;
        let bytes = bytes.ok_or_else(|| fail("\"bytes\""))?;
        match kind.as_deref() {
            Some("replication") => {
                let covered = covered_mtime.ok_or_else(|| {
                    fail("\"covered_mtime\", which is what the lag figure is computed from")
                })?;
                Ok(Receipt::Replication {
                    completed_unix: completed,
                    files,
                    bytes,
                    covered_mtime: covered,
                })
            }
            Some("restore-drill") => Ok(Receipt::RestoreDrill {
                completed_unix: completed,
                files,
                bytes,
            }),
            Some(other) => Err(format!(
                "unknown receipt kind {other:?}; expected \"replication\" or \"restore-drill\""
            )),
            None => Err(fail("\"kind\"")),
        }
    }

    /// A plain, escape-free, ASCII string. Receipt kinds and field names are
    /// fixed words; anything beyond that is somebody editing the file by
    /// hand, and gets named rather than decoded.
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"', "a string to start with `\"`")?;
        let start = self.i;
        loop {
            match self.next_byte() {
                Some(b'"') => {
                    return Ok(String::from_utf8_lossy(&self.b[start..self.i - 1]).into_owned())
                }
                Some(b'\\') => {
                    return Err("escapes have no place in these receipts; the words in \
                         them are plain"
                        .to_owned())
                }
                Some(c) if c < 0x20 => return Err(format!("a control byte ({c}) inside a string")),
                Some(c) if c >= 0x80 => {
                    return Err(format!(
                        "byte {c:#04x} is not ASCII; these strings are plain words"
                    ))
                }
                Some(_) => {}
                None => return Err("a string that never ends".to_owned()),
            }
        }
    }

    fn u64_value(&mut self) -> Result<u64, String> {
        let start = self.i;
        while matches!(self.b.get(self.i), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if start == self.i {
            let found = spell(self.b.get(self.i).copied());
            return Err(format!("expected a number, found {found}"));
        }
        let digits = &self.b[start..self.i];
        if digits.len() > 1 && digits[0] == b'0' {
            return Err(format!(
                "{} has a leading zero, which is not one of this format's numbers",
                String::from_utf8_lossy(digits)
            ));
        }
        digits
            .iter()
            .try_fold(0u64, |acc, d| {
                acc.checked_mul(10)
                    .and_then(|a| a.checked_add(u64::from(d - b'0')))
            })
            .ok_or_else(|| "a number that does not fit in this format's range".to_owned())
    }
}

fn spell(c: Option<u8>) -> String {
    match c {
        None => "the end of the input".to_owned(),
        Some(b'\n') => "a newline".to_owned(),
        Some(c) if c.is_ascii_graphic() => format!("`{}`", c as char),
        Some(c) => format!("byte {c:#04x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, posture_parts, upsert, Receipt};
    use crate::durability::{BackupState, RecoveryPoint};
    use core::time::Duration;

    const FRESH: Duration = Duration::from_secs(1_000);
    const DAY: u64 = 24 * 60 * 60;

    fn rep(completed: u64, covered: u64) -> String {
        Receipt::Replication {
            completed_unix: completed,
            files: 3,
            bytes: 90,
            covered_mtime: covered,
        }
        .render()
    }

    fn drill(completed: u64) -> String {
        Receipt::RestoreDrill {
            completed_unix: completed,
            files: 3,
            bytes: 90,
        }
        .render()
    }

    #[test]
    fn both_kinds_round_trip_through_the_canonical_form() {
        for r in [
            Receipt::Replication {
                completed_unix: 7,
                files: 1,
                bytes: 2,
                covered_mtime: 3,
            },
            Receipt::RestoreDrill {
                completed_unix: 9,
                files: 4,
                bytes: 5,
            },
        ] {
            let text = format!("[{}]", r.render());
            assert_eq!(parse(&text), Ok(vec![r.clone()]), "{text}");
        }
    }

    #[test]
    fn key_order_inside_a_receipt_is_free() {
        let text =
            r#"[{"covered_mtime":5,"bytes":9,"files":3,"completed_unix":7,"kind":"replication"}]"#;
        assert_eq!(
            parse(text),
            Ok(vec![Receipt::Replication {
                completed_unix: 7,
                files: 3,
                bytes: 9,
                covered_mtime: 5
            }])
        );
    }

    #[test]
    fn whitespace_between_tokens_is_not_structure() {
        let text = format!("  [\n  {} ,\n\t{} ]  ", rep(7, 6), drill(8));
        assert_eq!(parse(&text).map(|v| v.len()), Ok(2));
    }

    #[test]
    fn an_empty_file_is_an_empty_claim_and_that_is_the_honest_reading() {
        assert_eq!(parse("[]"), Ok(vec![]));
        let (r, b) = posture_parts(Some("[]"), Some(1_000), FRESH);
        assert_eq!(r, RecoveryPoint::NoReplica);
        assert_eq!(b, BackupState::NotConfigured);
    }

    #[test]
    fn every_way_to_malform_a_receipt_is_named_rather_than_guessed_around() {
        let cases = [
            ("[", "expected"),
            ("[}", "a receipt to start with"),
            ("[{]", "a string to start with"),
            ("[{\"kind\":\"replication\"}]", "without \"completed_unix\""),
            (
                "[{\"kind\":\"replication\",\"completed_unix\":1,\"files\":1,\"bytes\":1}]",
                "covered_mtime",
            ),
            (
                "[{\"kind\":\"restore-drill\",\"completed_unix\":1,\"files\":1}]",
                "\"bytes\"",
            ),
            ("[{\"kind\":\"weather\",\"completed_unix\":1,\"files\":1,\"bytes\":1,\"covered_mtime\":1}]", "unknown receipt kind"),
            ("[{\"kind\":\"replication\",\"completed_unix\":1,\"completed_unix\":1,\"files\":1,\"bytes\":1,\"covered_mtime\":1}]", "appears twice"),
            ("[{}]", "no fields"),
            (r#"[{"kind":"rep\lation","completed_unix":1}]"#, "escapes"),
            ("[{\"kind\":\"réplication\",\"completed_unix\":1}]", "not ASCII"),
            (
                "[{\"kind\":\"replication\",\"completed_unix\":01,\"files\":1,\"bytes\":1,\"covered_mtime\":1}]",
                "leading zero",
            ),
            ("[{\"kind\":\"replication\",\"completed_unix\":99999999999999999999999,\"files\":1,\"bytes\":1,\"covered_mtime\":1}]", "range"),
            ("[{\"kind\":\"replication\",\"completed_unix\":true,\"files\":1,\"bytes\":1,\"covered_mtime\":1}]", "expected a number"),
            ("{\"kind\":\"replication\"}", "`[`"),
            (
                "[{\"kind\":\"replication\",\"files\":1,\"bytes\":1,\"covered_mtime\":1}]",
                "\"completed_unix\"",
            ),
        ];
        // One case is built at runtime, so it lives beside the table rather
        // than in it.
        let trailing = format!("[{}] x", rep(7, 6));
        let extra: [(&str, &str); 1] = [(trailing.as_str(), "remain after the end")];
        for (text, needle) in cases.iter().chain(extra.iter()) {
            let e = parse(text).expect_err(text);
            assert!(e.contains(needle), "{text} → {e}");
        }
    }

    #[test]
    fn a_fresh_receipt_becomes_a_live_lag_measured_from_the_newest_covered_change() {
        // The cycle finished at noon; the newest file it saw was written at
        // five past. Ten minutes later the lag is those ten minutes, however
        // young the cycle itself is.
        let now = 10 * 60 + 5 * 60;
        let (r, _) = posture_parts(
            Some(&format!("[{}]", rep(now - 60, now - 300))),
            Some(now),
            FRESH,
        );
        assert_eq!(
            r,
            RecoveryPoint::Behind {
                lag: Duration::from_secs(300),
                measured_at: FRESH
            }
        );
    }

    #[test]
    fn data_written_after_the_last_cycle_counts_as_lag_even_when_the_cycle_was_recent() {
        // Uploaded at T, mirrored up to T-600 at T-60: the replica has never
        // confirmed the upload, and the lag says so.
        let now = 5_000;
        let (r, _) = posture_parts(
            Some(&format!("[{}]", rep(now - 60, now - 600))),
            Some(now),
            FRESH,
        );
        assert_eq!(
            r,
            RecoveryPoint::Behind {
                lag: Duration::from_secs(600),
                measured_at: FRESH
            }
        );
    }

    #[test]
    fn a_receipt_past_the_standing_window_is_unreachable_not_eternally_young() {
        // Five minutes is how long a measurement stands. A companion invoked
        // once a day leaves receipts that age out within minutes, and the
        // panel must say nobody is measuring — not show yesterday's number
        // as if it were live.
        let now = 100_000;
        let stale = format!("[{}]", rep(now - 301, now - 301));
        let (r, _) = posture_parts(Some(&stale), Some(now), FRESH);
        match &r {
            RecoveryPoint::Unreachable(why) => {
                assert!(why.contains("301s old"), "{why}");
                assert!(why.contains("nothing is still measuring"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        // And inside the window it stays live.
        let fresh = format!("[{}]", rep(now - 299, now - 299));
        let (r, _) = posture_parts(Some(&fresh), Some(now), FRESH);
        assert!(matches!(r, RecoveryPoint::Behind { .. }), "{r:?}");
    }

    #[test]
    fn a_stamp_ahead_of_this_clock_refuses_whole_instead_of_clamping_to_zero() {
        // Clamping a negative lag to zero would be arithmetic improving a
        // claim: the honest answer is that this machine cannot date it.
        let ahead = format!("[{}]", rep(2_000, 1_900));
        let (r, _) = posture_parts(Some(&ahead), Some(1_000), FRESH);
        assert!(
            matches!(&r, RecoveryPoint::Unreachable(w) if w.contains("ahead of this cell's clock")),
            "{r:?}"
        );

        let skewed_mirror = format!("[{}]", rep(500, 2_000));
        let (r, _) = posture_parts(Some(&skewed_mirror), Some(1_000), FRESH);
        assert!(
            matches!(&r, RecoveryPoint::Unreachable(w) if w.contains("ahead of this cell's clock")),
            "{r:?}"
        );

        // Completion ahead while the covered mtime is not: the half-guard
        // would wave this through and clamp a completion it cannot date.
        let skewed_completion = format!("[{}]", rep(2_000, 900));
        let (r, _) = posture_parts(Some(&skewed_completion), Some(1_000), FRESH);
        assert!(
            matches!(&r, RecoveryPoint::Unreachable(w) if w.contains("ahead of this cell's clock")),
            "{r:?}"
        );
    }

    #[test]
    fn without_a_wall_clock_an_existing_claim_cannot_be_called_live() {
        let text = format!("[{}]", rep(500, 499));
        let (r, _) = posture_parts(Some(&text), None, FRESH);
        assert!(
            matches!(&r, RecoveryPoint::Unreachable(w) if w.contains("cannot tell what day")),
            "{r:?}"
        );
    }

    #[test]
    fn a_drill_speaks_for_the_backup_and_only_within_its_own_expiry() {
        let today = 200 * DAY;
        let recent = format!("[{},{}]", rep(today - 100, today - 150), drill(today - 100));
        let (_, b) = posture_parts(Some(&recent), Some(today), FRESH);
        assert_eq!(
            b,
            BackupState::Restored {
                at_unix: today - 100
            }
        );

        // Thirty-one days later the drill is history, not evidence.
        let (_, b) = posture_parts(Some(&recent), Some(today + 31 * DAY), FRESH);
        assert!(!b.is_proven(Some(today + 31 * DAY)));
    }

    #[test]
    fn replication_without_any_drill_reads_as_never_restored_not_as_nothing() {
        // "Nothing is being backed up" would be false: something is mirroring.
        // The type built for exactly this distinction is the one used.
        let text = format!("[{}]", rep(500, 499));
        let (_, b) = posture_parts(Some(&text), Some(501), FRESH);
        assert_eq!(b, BackupState::NeverRestored);
    }

    #[test]
    fn a_drill_alone_proves_a_restore_and_says_nothing_about_a_replica() {
        let text = format!("[{}]", drill(500));
        let (r, b) = posture_parts(Some(&text), Some(501), FRESH);
        assert_eq!(r, RecoveryPoint::NoReplica);
        assert_eq!(b, BackupState::Restored { at_unix: 500 });
    }

    #[test]
    fn an_unreadable_evidence_file_breaks_both_sentences_rather_than_one() {
        let (r, b) = posture_parts(Some("[{]"), Some(1_000), FRESH);
        assert!(
            matches!(&r, RecoveryPoint::Unreachable(w) if w.contains("could not be read")),
            "{r:?}"
        );
        // NotConfigured here would print "nothing is being backed up" beside
        // a file that clearly says otherwise.
        assert!(matches!(b, BackupState::RestoreFailed(_)), "{b:?}");
    }

    #[test]
    fn no_path_configured_is_exactly_yesterdays_answer() {
        let (r, b) = posture_parts(None, Some(1_000), FRESH);
        assert_eq!(r, RecoveryPoint::NoReplica);
        assert_eq!(b, BackupState::NotConfigured);
    }

    #[test]
    fn the_latest_claim_of_a_kind_wins_and_the_other_kind_is_untouched() {
        let text = format!("[{},{},{}]", rep(100, 90), drill(120), rep(200, 190));
        let parsed = parse(&text).expect("parses");
        assert_eq!(parsed.len(), 3);

        let (r, b) = posture_parts(Some(&text), Some(201), FRESH);
        assert_eq!(
            r,
            RecoveryPoint::Behind {
                lag: Duration::from_secs(11),
                measured_at: FRESH
            }
        );
        assert_eq!(b, BackupState::Restored { at_unix: 120 });
    }

    #[test]
    fn upsert_replaces_its_own_kind_and_keeps_the_rest() {
        let existing = format!("[{},{}]", rep(100, 90), drill(120));
        let next = upsert(
            Some(&existing),
            &Receipt::Replication {
                completed_unix: 300,
                files: 5,
                bytes: 50,
                covered_mtime: 290,
            },
        )
        .expect("upserts");
        let parsed = parse(&next).expect("still parses");
        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().any(|r| r.kind() == "restore-drill"));
        assert_eq!(
            posture_parts(Some(&next), Some(301), FRESH).0,
            RecoveryPoint::Behind {
                lag: Duration::from_secs(11),
                measured_at: FRESH
            }
        );
    }

    #[test]
    fn upsert_refuses_to_overwrite_evidence_it_cannot_read() {
        let e = upsert(
            Some("garbage["),
            &Receipt::RestoreDrill {
                completed_unix: 1,
                files: 0,
                bytes: 0,
            },
        )
        .expect_err("garbage");
        assert!(e.contains("expected"), "{e}");
    }

    #[test]
    fn upsert_into_nothing_starts_the_file() {
        let out = upsert(None, &drill_receipt()).expect("empty history always upserts");
        assert_eq!(out, format!("[{}]", drill_receipt().render()));
    }

    fn drill_receipt() -> Receipt {
        Receipt::RestoreDrill {
            completed_unix: 9,
            files: 1,
            bytes: 2,
        }
    }
}
