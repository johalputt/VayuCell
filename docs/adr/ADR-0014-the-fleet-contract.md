# ADR-0014 — The fleet contract: roles, quorum, upgrades, sealed verdicts

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by `core/src/fleet.rs` and the `--fleet-role`
surface on `report`, `status` and `all`.

## Context

PLAN §7 promises a fleet — edge, store, compute and witness roles;
replication across nodes; rolling upgrades that drain a node which fails
to come back; and verdict sharing so an attacker jailed on one cell is
jailed across all of them. Nothing in the repository could express any of
it, and a fleet built by improvising connections between phones would
have been three dependencies deep before its first test.

The same two constraints as always apply: zero third-party runtime
dependencies (ADR-0005 §5.1), which means no consensus library, and the
cell never dials (charter Article V), which means no fleet protocol that
originates from the phone.

## Decision

**A contract module, not a runtime.** Everything PLAN §7 names that is
arithmetic or records lives in one place, tested without sockets:

1. **Roles are declared, never discovered.** A role is a promise about
   what this device will be asked to survive; discovering one would be
   guessing at somebody else's promise. The four roles carry their
   sentences, and `serves_traffic` marks the witness as serving nothing.
   The surface renders only what was declared — an undeclared role means
   the section does not exist, not an invented role called none.

2. **Quorum is computed, never configured.** A write needs a majority of
   voting members (`members / 2 + 1`). Witnesses vote **only** when the
   membership is split exactly evenly, cover exactly one missing vote,
   and can never promote a minority into a decision. With zero members
   nothing ever reaches quorum: a fleet with no voters is a name.

3. **Rolling upgrades are a state machine with a drain.** One node in
   flight, always; a node that has not returned after fifteen minutes is
   drained and named to its operator rather than waited on; there is no
   automatic retry, because retries of failed upgrades are how one bad
   image becomes a synchronized outage.

4. **Verdicts travel sealed, not secret.** A jail record carries subject,
   verdict, timestamp, and an HMAC-SHA-256 tag under a fleet key. Sealed
   because tampering must be detectable at the receiving cell; plain
   because every cell must be able to read what it is enforcing. SHA-256
   and HMAC are implemented here under ADR-0005 §5.1 and pinned against
   NIST and RFC 4231 vectors, including the empty message and the
   million-byte case where padding bugs surface. Verification compares
   tags in constant time through the same helper credentials use.

Replication across nodes reuses what already exists: the vault API plus
the ADR-0012 receipts. There is deliberately no new wire format.

## Consequences

PLAN §7's code half exists, and its gate — *one node killed mid-write
loses nothing* — is now answerable by simulation against this contract
rather than by prose. What remains outside code is everything physical:
real handsets holding real roles, a witness nobody has watched break a
tie, and a shared verdict no attacker has yet tried to forge.
