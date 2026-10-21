# ADR-0015 — The tier-2 guest contract: what a guest may say about its host

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by `core/src/guest.rs`, the guest record on disk in
`cli/src/guest.rs`, and the tier-aware rows in `core/src/panel.rs` and
`cli/src/report.rs`. The device gate is still open — see *Consequences*.

## Context

PLAN §3 makes tier 2 a **virtualised Linux guest** on an Android 16+ handset:
the pKVM guest runs unrooted, the Android host keeps the kernel, and the guest
reaches the network through the host. PLAN §4 gives tier 2 its own charge
mechanism — *requested from the Android host, verified from the guest* — and
PLAN §11 sets the gate: **the guest survives a host reboot; it escapes Doze**.

Nothing in the repository could express any of it, and the temptation was
strong: a tier-2 guest is a Linux box with a phone's hardware behind it, so
the obvious implementation is to read the same sysfs nodes the other tiers
read and render the same panel. That implementation would be wrong in three
ways at once, and each of them is a way to report a control that does not
work.

1. **The charge node lives in the host's kernel.** A guest cannot write it
   and cannot read it back. A panel row that says *"this device exposes no
   charge control, so no ceiling can be held"* is false at tier 2: the
   control exists, the host owns it, and the guest can ask for a ceiling.
   The same row read in tier-0 words turns a working arrangement into a
   reported failure.
2. **The guest is destroyed by every host reboot.** Anything the host said
   before the reboot went with it — the ceiling it was holding, the Doze
   exemption, whether the ingress was published. A guest that carried those
   claims forward would be reporting controls that no longer exist, which is
   precisely what Charter Article IV is written against.
3. **The host's statements are the host's word.** A guest that reports
   *"the cell is held at 60%"* is reporting something it was told, not
   something it measured. Collapsing the two is the defect class this whole
   project exists to refuse: a control that cannot be read back may not be
   reported at all (Charter Article IV.5).

## Decision

**A contract module, not a runtime.** Everything PLAN §3 gives tier 2 lives
in `core/src/guest.rs`, tested without a hypervisor:

1. **Three mediated controls, named.** The charge ceiling, the host's Doze
   policy towards the guest, and whether the host publishes the guest's
   ingress. These are the things a guest does not own and may only ask
   about. The claim carries the control **inside** it rather than beside it,
   so a reachability claim cannot be filed under the Doze control — the same
   defect class as the two transposed booleans in `core/src/tier.rs`.

2. **Every claim belongs to a boot.** The host states which boot of itself
   it is in, and the guest records it. A statement with no boot counter is
   refused outright, because a claim that cannot be dated is a claim that can
   never be invalidated. A counter of zero is refused too: it reads as
   "older than every real boot" and would silently void everything.

3. **A reboot invalidates the claims of the boot before it.** The guest
   notices the new boot and reports the old claims as **void** — the host
   said this, in that boot, and the host has since rebooted. Not *unknown*:
   the statement happened, and what happened to it is the interesting part.
   A statement from a boot the guest has already left is refused.

4. **A statement this build does not understand is refused, not partly
   believed.** Unknown keys, duplicate keys, two claims about one control, a
   ceiling above a hundred, a boot counter of zero: each is an error. A
   host that said two things about one control has said nothing, and
   choosing one of them would be this program deciding.

5. **The host's word is labelled as the host's word.** The standing of a
   control is `ATTESTED`, `VOID` or `UNKNOWN` — there is deliberately **no**
   `VERIFIED` variant, and a `compile_fail` doctest proves it cannot be
   added by accident. Every attested claim travels with the caveat that
   makes it honest: the charge node lives in the host's kernel, a guest
   cannot observe the host's Doze policy from inside, and whether the host
   publishes the guest's ports is the host's decision.

6. **Absence of a statement is not a statement.** A host that said nothing
   about the Doze policy has not promised to doze the guest. A host that
   said *"I hold no ceiling"* has said something definite, and the two are
   different answers.

7. **The guest's own record is a file, and it is written durably.** The
   guest keeps a record of the boot it last heard from and what the host
   said in it, at `$HOME/.vayucell/guest`, written the way the halt record
   is written: a temporary, a flush, a rename, and a flush of the
   directory. A record that exists and cannot be understood is **corrupt**,
   not empty — reading it as empty would turn a lost file into a clean bill
   of health.

8. **The panel and the report say the tier-2 arrangement out loud.** The
   charge-mechanism row at tier 2 is *unverified* and says the ceiling is
   the host's to hold; the governor row says the guest has no reading of
   the cell to act on; the outage-reserve row is *unverified* because the
   guest cannot read the cell, not *failed* because the cell is unsafe. The
   device report gains a GUEST section, at tier 2 only, listing the boot
   and the standing of each control.

The host's statement reaches the guest through an environment variable
(`VAYUCELL_HOST_ATTESTATION`) and the record on disk. **That channel is a
stand-in, and it is severable**: the contract in `core/src/guest.rs` does
not know where the statement came from, so a real pKVM host channel can
replace the variable without touching a rule.

## Consequences

PLAN §5's code half exists and the phase moves from ⬜ to ◐: the claims a
guest may make about its host, when they expire, and what it may not say
are all now decided in code and mutation-tested — twenty-four gate
mutations pin the load-bearing lines, and the two sentences that were wrong
at tier 2 are the ones a mutation would break first.

What remains is exactly what code cannot do. The pKVM channel is an
environment variable, and a real host has to speak it; a real handset has
to reboot underneath a running guest and the guest has to be seen to notice;
Doze has to be escaped in practice, on a phone, with the host's real
policy. **This ADR does not claim that any of those has happened.** It
claims that when somebody puts a guest on a bench, the software will say
what it knows and what it does not — and that the panel will not tell a
guest it has no charge control when the charge control is the phone's.
