# ADR-0016 — The verified image manifest: what a mainline image has to prove

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by `core/src/image.rs`. The image gate is still open —
see *Consequences*.

## Context

PLAN §3 makes tier 3 **mainline Linux** — a postmarketOS-class image on the
phone's own hardware, with the device tree and sysfs visible to it. PLAN §4
gives tier 3 its charge mechanism: the **device-tree maximum charge
voltage** (≈3.8 V), held at 40–50%. PLAN §11 sets the gate: *verified images
for the top device families*.

That gate is a supply-chain claim, and the project has never had a way to
make one. Everything the repository says about a device is either read from
the device at the moment or written by this project. "This phone is running
a verified image" is neither: it is a statement about a **build**, made
before the build, and checked afterwards.

The temptation was to assert it. A tier-3 image that ships with a version
string, and a panel row that says *VERIFIED — image v1.2.3*, would read
well and mean nothing: the image would be able to claim any version it
liked. A version string in a file the image itself wrote is not evidence of
anything.

## Decision

**A manifest, and a check of the running system against it.**

1. **A manifest says what the image is.** Name, version, the kernel release
   the image was built against, the device-tree `compatible` string it
   targets, and the maximum charge voltage its device tree declares. It is
   a strict line-based text format with a header. Unknown keys, duplicate
   keys, missing keys, extra words and non-numbers are all **refused**: a
   manifest that cannot be understood is an error, never an empty manifest.
   A manifest with a blank name, a blank version, a blank kernel or a
   blank target board is refused, because a manifest that targets no board
   would match every board and verify nothing.

2. **A zero maximum charge voltage is refused.** Zero would mean *no
   ceiling at all*, and a manifest that cannot express a ceiling must not
   be constructible by accident — the same reasoning as the governor's
   refusal to accept a ceiling of zero.

3. **The check is of the running system, not of a file the image wrote.**
   The kernel release comes from `/proc/sys/kernel/osrelease` and is
   compared **exactly**: a prefix check would accept a kernel the image was
   never built against. The device tree's `compatible` list is matched by
   membership, because a device tree publishes several strings, most
   specific first. The maximum charge voltage is read from the device tree
   and compared with a stated tolerance of ±25 mV, because the kernel
   rounds what it publishes.

4. **Absent, unreadable and different are three answers.** A device tree
   that is **absent** is *unverified* — we could not look, which is not the
   same as "it is different". A device tree that is **unreadable** is
   *unverified* too, and says so differently from absent. Only a node that
   was read and disagreed is a **mismatch**.

5. **The headline is computed from the findings.** A report has no setter
   for its outcome, and a machine that answers nothing yields *unverified*
   rather than a green tick — the same rule the safety panel follows
   (ADR-0007).

6. **A definite disagreement outranks an unanswered question.** The outcome
   is ordered `Verified < Unverified < Mismatched`, worst wins, so a known
   disagreement is never softened into "we could not tell". A gap in
   knowledge is not a smaller problem than a proven mismatch; it is a
   different one.

## Consequences

PLAN §8's code half exists: what a verified image *is* is now a thing the
software can check, and a device that is not running the image it claims to
be running says so, in the same shape as every other check in this
repository.

Two things this ADR does **not** do, stated plainly because they are the
obvious next questions. The device-tree node names are **names this project
has chosen**; the real images will publish their own, and the check reads
the ones named here until they are changed. And there is no signature in
the manifest: this says what an image *is*, not who published it. Signing
a manifest is the next supply-chain question, and it is not answered here
— the repository has no key management and inventing one inside a contract
module would be worse than leaving the question open.
