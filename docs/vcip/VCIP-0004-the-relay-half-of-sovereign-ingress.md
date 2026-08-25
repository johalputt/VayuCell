# VCIP-0004: The relay half of sovereign ingress

- **Status:** Implemented (device gate still open)
- **Date:** 2026 (see `git log` for the exact day)
- **Relates to:** ADR-0003 (sovereign ingress), ADR-0013 (relay ingress);
  Charter Articles II, IV, V, VIII; PLAN.md phase P3

## The problem

P3's table promised four modes. Three existed — local-only served, direct
declared, onion supervised through the system daemon — and the fourth,
the relay, existed as a fully declared profile with nothing behind it.
PLAN.md recorded the gap in plain words: *relay is not implemented.*

A relay is the one mode whose infrastructure is entirely outside the
device: a rented host with a DNS name, forwarding connections in. The
design question was what a program that cannot dial, cannot administer
remote machines, and cannot claim what it has not observed could honestly
add.

## The answer

Declaration without management. `--relay-via` validates the name at
typing time against the rules a DNS name must satisfy; startup prints the
profile-derived disclosures, the exact forwarding instruction for the
site and vault addresses, and unverified standing — before anything
binds. The panel is excluded by construction and pinned by test. There
is deliberately no tunnel abstraction, no health checking, and no state
beyond what the words say.

## What would erode it, and what holds

The failure mode to guard against is convenience creeping back: a flag
that also writes an ssh config here, a helper that pings the far end
there, each small enough to review alone. What holds is that every such
feature needs dialing code or unverifiable claims, both refused at the
root — and three mutation-gate entries pin the hostname validation, so
the banner cannot quietly start promising things about names that were
never checked.
