# VCIP-0005: The fleet contract

- **Status:** Implemented (device gate still open)
- **Date:** 2026 (see `git log` for the exact day)
- **Relates to:** ADR-0005 (no third-party runtime dependencies), ADR-0012
  (replication by receipt), ADR-0014 (the fleet contract); PLAN.md phase
  P7

## The problem

P7 promised redundancy, rolling upgrades and shared defence. The
repository had a vault, receipts, and honest types — and nothing that
could say what a fleet *is*: how many votes decide a write, when a
witness may speak, what happens to a node that does not come back, or
how a jail verdict proves it was issued by the fleet rather than forged
by the attacker it names.

## The shape of the answer

One pure module, `core/src/fleet.rs`: the four roles as declared
promises, quorum as arithmetic with witnesses restricted to tie-breaking,
a one-at-a-time upgrade machine whose only failure path is a named drain,
and HMAC-sealed verdict records pinned to published RFC vectors. The CLI
gains `--fleet-role` on the posture surfaces, rendered as DECLARED — the
section states what the operator chose and says so in its first line.

## Held by tests, held by wording

Three gate mutations pin the load-bearing lines: halving instead of a
majority, witnesses promoting minorities, and the drain boundary moving
one second early. The rest is held by wording that refuses to overclaim:
the ceiling sentence prints in the same section as the role, and it ends
with *not a datacentre*, because a fleet raises ceilings without removing
them.
