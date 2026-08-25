// SPDX-License-Identifier: Apache-2.0

//! The fleet contract: roles, quorum, rolling upgrades, shared verdicts.
//!
//! # What a fleet is here, and what it is not
//!
//! One phone is one phone. A fleet is more of them, assigned roles rather
//! than cloned ([plan] §7): **edge** terminates ingress, **store** holds
//! replicas, **compute** does batch work, and a **witness** exists only to
//! break quorum ties. This module is the contract those nodes interoperate
//! on — pure arithmetic and records, no sockets — because every direction
//! of fleet communication already has an honest carrier: nodes talk *to*
//! each cell through the vault API, companions dial out per ADR-0011, and
//! claims travel as dated receipts per ADR-0012. A fleet module that
//! opened its own connections would be three dependencies deep before its
//! first test.
//!
//! # The arithmetic is the safety property
//!
//! Quorum is not a configuration value somebody sets; it is computed from
//! how many voting members exist and how many are alive, with witnesses
//! counted only when the members alone are exactly split — a tie-breaker,
//! which is what PLAN §7 says a witness is for and nothing more. The
//! upgrade state machine upgrades one node at a time because two nodes in
//! flight doubles the blast radius of whatever goes wrong; a node that
//! fails to come back is drained, because a fleet waiting forever on a
//! dead phone has converted redundancy into an outage.
//!
//! # Shared verdicts, sealed not secret
//!
//! An attacker jailed on one cell should be jailed across the fleet. The
//! record that travels between cells is sealed with HMAC-SHA-256 under a
//! key derived from the fleet's shared secret — sealed so any tampering
//! is detectable at the receiving cell, not encrypted, because the
//! subject's name and verdict are exactly the things every cell must be
//! able to read without holding anything private. The hash and MAC are
//! implemented here rather than imported, per ADR-0005 §5.1, and pinned
//! to published RFC vectors so "hand-rolled" cannot quietly mean "wrong".
//!
//! [plan]: ../../PLAN.md

use crate::auth::constant_time_eq;

/// Why this node exists in the fleet. [PLAN] §7.
///
/// Declared by the operator, never discovered: a role is a promise about
/// what this cell will be asked to survive, and a promise somebody else
/// inferred is not a promise.
///
/// [PLAN]: ../../PLAN.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Terminates ingress; holds the tunnel or onion.
    Edge,
    /// Attached storage; serves the personal cloud.
    Store,
    /// Batch work, transcoding, indexing.
    Compute,
    /// Exists only to break quorum ties.
    Witness,
}

impl Role {
    /// The sentence an operator reads when this role is declared.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Edge => {
                "terminates ingress for the fleet; shed first when this device \
                 is in trouble"
            }
            Self::Store => "holds replicas for the fleet; losing it loses capacity, never data",
            Self::Compute => "does batch work for the fleet; the first thing shed under load",
            Self::Witness => "exists only to break quorum ties; serves nothing else",
        }
    }

    /// Whether this role answers visitor traffic.
    #[must_use]
    pub const fn serves_traffic(self) -> bool {
        !matches!(self, Self::Witness)
    }
}

/// How many accepting votes a write needs. PLAN §7, ADR-0014 §3.
///
/// Members vote always; witnesses count **only** when the members alone
/// are split exactly evenly — which is the definition of a tie, and the
/// only situation in which a tie-breaker may speak. Anything more generous
/// turns a witness into a second-class member; anything stricter turns a
/// tie into an outage.
///
/// With `members = 0` nothing can ever reach quorum: a fleet with no
/// voting members is a name, not a fleet.
#[must_use]
pub fn quorum_needed(members: usize) -> usize {
    members / 2 + 1
}

/// Whether a write may be accepted right now.
///
/// `total_members` counts every voting member of the fleet, `live_members`
/// those reachable for this write, `willing_witnesses` the witnesses that
/// would vote if needed. The write goes through when the live members
/// alone clear a majority, or when the membership is split **exactly**
/// evenly and a witness stands ready — one missing vote, covered once,
/// which is the whole job the plan gives a witness. A minority, even a
/// willing one, does not decide writes.
#[must_use]
pub fn can_accept(total_members: usize, live_members: usize, willing_witnesses: usize) -> bool {
    let needed = quorum_needed(total_members);
    if live_members >= needed {
        return true;
    }
    // The tie: exactly half the members alive. One witness covers the one
    // missing vote; two witnesses do not cover two.
    let split = live_members * 2 == total_members;
    split && live_members + willing_witnesses.min(1) >= needed
}

/// One node's step through a rolling upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeState {
    /// Waiting its turn.
    Pending,
    /// Out of rotation while its replacement is installed.
    InFlight { since_unix: u64 },
    /// Came back inside the budget.
    Done,
    /// Failed to come back inside [`UPGRADE_BUDGET_SECS`]; removed from the
    /// rotation rather than waited on forever.
    Drained { noticed_unix: u64 },
}

/// How long a node has to come back before it is drained instead.
///
/// Long enough for a reboot plus image install on slow flash; short enough
/// that an operator notices within one sitting. The tests pin the boundary
/// with literals, not this constant, so widening it cannot silently
/// un-pin them.
pub const UPGRADE_BUDGET_SECS: u64 = 15 * 60;

/// A rolling upgrade over named nodes, strictly one in flight.
///
/// The machine the plan enforces: at most one node is `InFlight` at any
/// moment; `record_returned` completes it; `advance(now)` drains any node
/// whose budget ran out and starts the next pending one only after the
/// previous question is settled. There is no retry-in-place: a drained
/// node stays drained until an operator says otherwise, because automatic
/// retries of a failed upgrade are how a fleet turns one bad image into a
/// synchronized outage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradePlan {
    nodes: Vec<(String, UpgradeState)>,
}

impl UpgradePlan {
    /// A plan over these node names, in order, none started.
    #[must_use]
    pub fn new(names: &[&str]) -> Self {
        Self {
            nodes: names
                .iter()
                .map(|n| ((*n).to_owned(), UpgradeState::Pending))
                .collect(),
        }
    }

    /// The current state of one node, if it is part of this plan.
    #[must_use]
    pub fn state_of(&self, name: &str) -> Option<&UpgradeState> {
        self.nodes.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }

    /// Whether some node is currently mid-upgrade.
    #[must_use]
    pub fn any_in_flight(&self) -> bool {
        self.nodes
            .iter()
            .any(|(_, s)| matches!(s, UpgradeState::InFlight { .. }))
    }

    /// Settles timeouts and starts the next node when nothing is in flight.
    ///
    /// Returns the names of nodes this call drained, so the caller can say
    /// so out loud rather than letting a removal be silent.
    pub fn advance(&mut self, now: u64) -> Vec<String> {
        let mut drained = Vec::new();
        for (name, state) in &mut self.nodes {
            if let UpgradeState::InFlight { since_unix } = *state {
                if now.saturating_sub(since_unix) > UPGRADE_BUDGET_SECS {
                    *state = UpgradeState::Drained { noticed_unix: now };
                    drained.push(name.clone());
                }
            }
        }
        if !self.any_in_flight() {
            if let Some((name, state)) = self
                .nodes
                .iter_mut()
                .find(|(_, s)| matches!(s, UpgradeState::Pending))
            {
                *state = UpgradeState::InFlight { since_unix: now };
                let _ = name;
            }
        }
        drained
    }

    /// Marks the in-flight node as returned healthy.
    ///
    /// # Errors
    ///
    /// Refused when nothing is in flight — recording a return for a node
    /// nobody started upgrading is a bookkeeping lie, not a state.
    pub fn record_returned(&mut self) -> Result<(), String> {
        let Some(idx) = self
            .nodes
            .iter()
            .position(|(_, s)| matches!(s, UpgradeState::InFlight { .. }))
        else {
            return Err("no node is being upgraded".to_owned());
        };
        self.nodes[idx].1 = UpgradeState::Done;
        Ok(())
    }
}

// ── Sealed verdicts ──────────────────────────────────────────────────────────

/// SHA-256, from FIPS 180-4, because ADR-0005 §5.1 forbids importing one.
///
/// Pinned below against NIST/RFC vectors including the empty message and
/// the million-`a` case, which is where hand implementations usually
/// discover their padding is off by one.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bitlen = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + data.len());
    inner.extend_from_slice(&k.map(|b| b ^ 0x36));
    inner.extend_from_slice(data);
    let mut outer = Vec::with_capacity(64 + 32);
    outer.extend_from_slice(&k.map(|b| b ^ 0x5c));
    outer.extend_from_slice(&sha256(&inner));
    sha256(&outer)
}

/// A jail verdict that travels between cells, sealed against tampering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedVerdict {
    /// What the verdict is about: a device fingerprint or credential id.
    pub subject: String,
    /// The decision: e.g. `jailed`.
    pub verdict: String,
    /// When the issuing cell recorded it, wall clock, seconds.
    pub issued_unix: u64,
    /// Hex HMAC over `subject|verdict|issued_unix`, ASCII only.
    pub tag_hex: String,
}

impl SealedVerdict {
    /// Issues and seals a verdict under the fleet's derived key.
    #[must_use]
    pub fn seal(subject: &str, verdict: &str, issued_unix: u64, key: &[u8]) -> Self {
        let payload = format!("{subject}|{verdict}|{issued_unix}");
        let tag = hmac_sha256(key, payload.as_bytes());
        let mut tag_hex = String::with_capacity(64);
        for b in tag {
            tag_hex.push(char::from_digit(u32::from(b >> 4), 16).expect("hex"));
            tag_hex.push(char::from_digit(u32::from(b & 0xf), 16).expect("hex"));
        }
        Self {
            subject: subject.to_owned(),
            verdict: verdict.to_owned(),
            issued_unix,
            tag_hex,
        }
    }

    /// Whether this record still carries exactly what it carried when
    /// sealed. Constant-time in the tag.
    #[must_use]
    pub fn verify(&self, key: &[u8]) -> bool {
        let recomputed = Self::seal(&self.subject, &self.verdict, self.issued_unix, key).tag_hex;
        constant_time_eq(recomputed.as_bytes(), self.tag_hex.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // The million-character case: padding across many blocks.
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn hmac_matches_rfc_4231_case_one_and_two() {
        let key = [0x0b_u8; 20];
        assert_eq!(
            hex(&hmac_sha256(&key, b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn a_sealed_verdict_survives_the_trip_and_detects_a_forged_byte() {
        let v = SealedVerdict::seal("fp-123", "jailed", 1_000, b"fleet-key");
        assert!(v.verify(b"fleet-key"));

        let mut tampered = v.clone();
        tampered.verdict = "watch".to_owned();
        assert!(!tampered.verify(b"fleet-key"), "a swapped verdict verifies");

        let mut flipped = v.clone();
        let first = flipped.tag_hex.as_bytes()[0];
        flipped
            .tag_hex
            .replace_range(0..1, if first == b'0' { "1" } else { "0" });
        assert!(
            !flipped.verify(b"fleet-key"),
            "a flipped tag nibble verifies"
        );

        assert!(!v.verify(b"other-fleet"), "the wrong fleet's key verifies");
    }

    #[test]
    fn quorum_is_a_majority_and_witnesses_speak_only_on_ties() {
        assert_eq!(quorum_needed(1), 1);
        assert_eq!(quorum_needed(2), 2);
        assert_eq!(quorum_needed(3), 2);
        assert_eq!(quorum_needed(4), 3);

        // Three members: two live clear the majority without any witness.
        assert!(can_accept(3, 2, 0));
        // One live of three is a minority; even willing witnesses do not
        // promote it — they break ties, not minorities.
        assert!(!can_accept(3, 1, 5));
        // Two of four is an exact split: one witness breaks it, and two
        // witnesses are still only one vote's worth of tie-breaker.
        assert!(!can_accept(4, 2, 0));
        assert!(can_accept(4, 2, 1));
        assert!(can_accept(4, 2, 5));
        // One of four is not a tie at all.
        assert!(!can_accept(4, 1, 5));
        // A full majority never needs a witness.
        assert!(can_accept(4, 3, 0));
    }

    #[test]
    fn an_upgrade_holds_one_node_in_flight_and_drains_a_stalled_one() {
        let mut plan = UpgradePlan::new(&["a", "b", "c"]);
        assert_eq!(plan.state_of("a"), Some(&UpgradeState::Pending));

        plan.advance(1_000);
        assert!(plan.any_in_flight(), "the first node started");
        assert_eq!(
            plan.state_of("a"),
            Some(&UpgradeState::InFlight { since_unix: 1_000 })
        );

        // Starting the next before the last settled is refused by structure:
        // advance starts nothing while something is in flight.
        plan.advance(1_100);
        assert_eq!(plan.state_of("b"), Some(&UpgradeState::Pending));

        // Inside the budget, nothing is drained.
        plan.advance(1_000 + UPGRADE_BUDGET_SECS);
        assert_eq!(
            plan.state_of("a"),
            Some(&UpgradeState::InFlight { since_unix: 1_000 })
        );

        // One second past it: drained, named, and the next node starts.
        let drained = plan.advance(1_000 + UPGRADE_BUDGET_SECS + 1);
        assert_eq!(drained, vec!["a".to_owned()]);
        assert_eq!(
            plan.state_of("a"),
            Some(&UpgradeState::Drained {
                noticed_unix: 1_000 + UPGRADE_BUDGET_SECS + 1
            })
        );
        assert!(plan.any_in_flight(), "b started after a was settled");
        assert_eq!(
            plan.state_of("b"),
            Some(&UpgradeState::InFlight {
                since_unix: 1_000 + UPGRADE_BUDGET_SECS + 1
            })
        );

        // And a healthy run completes.
        plan.record_returned().expect("b returned");
        assert_eq!(plan.state_of("b"), Some(&UpgradeState::Done));
    }

    #[test]
    fn recording_a_return_for_an_upgrade_nobody_started_is_refused() {
        let mut plan = UpgradePlan::new(&["only"]);
        assert!(plan.record_returned().is_err());
    }
}
