# ADR-0017 — The service catalogue: what this device may be asked to run

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by `core/src/catalogue.rs`. The device gate is still
open — see *Consequences*.

## Context

PLAN §8 is the catalogue: the VayuPress stack, a personal cloud, a backup
target, media and household services, and private model inference. Its own
sentence is the design brief: *every catalogue entry declares its tier floor
and its resource envelope, so the panel can grey out what this device cannot
run and say why*.

PLAN §9 then describes the flow — profile the device, establish the safety
state, pick from the catalogue filtered to the tier, pick the ingress,
provision and then verify. The filtering step is the part that can be
written here, and the part that is easiest to write dishonestly: a picker
that silently omits what a device cannot run is a picker that makes the
device look simpler than it is, and a picker that says "too small" without
saying *which* requirement failed is a picker that cannot be argued with.

The harder question is what to do when the device's own figures are
unknown. PLAN §9 §2 gets the tier and the resource figures from one
profiling pass, so a device whose tier was never established has not been
profiled, and its numbers are no more trustworthy than its tier.

## Decision

1. **Every entry declares a floor and an envelope.** A `Tier` floor — the
   same `Tier` the rest of the repository uses, not a new ladder — and a
   resource envelope: memory, storage, and a **thermal class** of
   `Idle`, `Light`, `Sustained` or `Heavy`. The heavy class is where
   private model inference sits, and its documentation says what that
   means: it is the class that heats a lithium cell, and the governor is
   what governs it.

2. **The reason names the requirement that failed.** A `Shortfall` is a
   typed value — `Tier`, `Memory`, `Storage` or `Thermal` — carrying the
   wanted and the had. "Too small" without the requirement is not an
   answer; "this device has 256 MB and the service wants 512" is.

3. **"Provably does not fit" and "cannot be told" are different answers.**
   A `Reason` is either `TooSmall` — a requirement was checked and failed —
   or `TierUnknown` — the device's tier was never established, so nothing
   about what it can run is known. The second is not a softer first: it
   says the question was not answered.

4. **An unestablished tier short-circuits the whole comparison.** The tier
   and the resource figures come from one profiling pass, so a device
   whose tier is unknown has not been profiled, and half an answer about
   memory would be no more trustworthy than the tier. Every entry is
   answered *cannot be told*, and nothing is claimed either way.

5. **Nothing is ever silently dropped.** Every catalogue entry appears in
   the result — fitting, or greyed out with its reason. `Selection::total`
   is the catalogue's length, and a test holds it there.

6. **The floors are chosen to be true minimums.** The safety report is
   tier 0: a device that cannot hold a charge ceiling must still be told
   plainly what it is, and a report that needs a daemon is a report Doze
   kills. Everything that serves traffic is tier 1, because nothing that
   serves traffic ships before the governor, and the governor needs a
   ceiling only a rooted device can hold. Private model inference is tier 1
   and deliberately **not** tier 2: a tier-2 guest cannot read the cell at
   all (ADR-0015), so heavy work there is ungoverned by definition.

7. **The heavy class belongs to one entry.** A test holds it, so a second
   heavy service cannot be added without somebody deciding that a second
   thing is allowed to heat the cell.

## Consequences

PLAN §9's catalogue and its filter exist: the services, their floors, their
envelopes, and the reason each one is or is not offered on a given device.
The picker can now say *why* — and the answer distinguishes a device that
cannot run a thing from a device nobody has measured.

What remains is the profiling pass and the silicon. The tier and the
resource figures have to come from a real device, the thermal envelope has
to be enforced by a governor that has watched a cell heat, and private
model inference has to run inside the envelope on a phone. **Nothing in
this ADR claims that any of that has happened.** The catalogue says what
would be offered; it does not say what has been run.
