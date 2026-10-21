# VCIP-0007: The image manifest

- **Status:** Implemented (image gate still open)
- **Date:** 2026 (see `git log` for the exact day)
- **Relates to:** ADR-0016 (the verified image manifest), ADR-0001 (the tier
  model); PLAN.md phase P8

## The problem

Tier 3 is mainline Linux on the phone's own hardware, and its charge
mechanism is the device tree's maximum charge voltage. Saying "this phone
is running a verified image" is a statement about a build made before the
build, and the repository had no way to check one.

## The shape of the answer

A manifest is a small, strict text file:

```text
vayucell image manifest 1
name postmarketOS
version 0.1.0
kernel 6.6.44
compatible qcom,msm8916-deb
max-charge-mv 3800
```

Five keys, one per line, in the order above. Unknown keys, duplicate keys,
missing keys, extra words on a line and non-numbers are all refused, and a
file that is not a manifest is an error rather than an empty manifest. A
blank name, version, kernel or target board is refused — a manifest that
targets no board would match every board and verify nothing — and so is a
maximum charge voltage of zero, because zero would mean *no ceiling at
all*.

## What the check reads

| what | where | how it is compared |
| --- | --- | --- |
| the running kernel | `/proc/sys/kernel/osrelease` | exactly, not by prefix |
| the target board | the device tree's `compatible` | membership in the NUL-separated list |
| the charge ceiling | the device tree's maximum charge voltage | within ±25 mV, because the kernel rounds what it publishes |

Both device-tree roots are tried, the same two `core/src/tier.rs` reads, and
an unreadable first root does not stop the second from answering.

Three answers, never two: **matched**, **different** (a node that was read
and disagreed) and **unverified** (a node that is absent, or present and
unreadable, or present and not a number). We could not look is not the same
as it is different.

## Held by tests, held by wording

Thirty tests hold the manifest format and the check, each naming the defect
it forecloses: a manifest that cannot be parsed is an error and not an empty
manifest, a device tree that is absent is unverified and not a mismatch, a
charge voltage outside the tolerance does not pass because the tolerance
was widened, and a report whose findings are all silent never reads as
verified. The headline is computed from the findings and has no setter, so
a report cannot say *Verified* beside rows that say otherwise.

## What is not claimed

The device-tree property names in the manifest format are **names this
project has chosen**; the real images publish their own, and the check
reads these until they change. There is no signature in the manifest: this
says what an image *is*, not who published it, and signing one is the next
supply-chain question rather than this one.
