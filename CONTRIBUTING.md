# Contributing to VayuCell

## Before anything else

Read `CHARTER.md`. It is short, and it constrains what may be built here — in
particular Article III, which puts safety of persons ahead of every schedule.

## How a change lands

**Every change goes through a pull request, including a maintainer's own.**

That was not always true here. For the first stretch of this project changes
were pushed straight to `main`, and the supply-chain scan said so plainly:
`0/29 approved changesets`. The gates were built to be adversarial precisely
because nothing had a second pair of eyes — which is a reason to want review,
not a substitute for it. A machine that tries to break the code stands in for
the reviewer who is not there; it does not replace one who could be.

    git switch -c <branch>
    # ... work, with scripts/local-ci.sh green ...
    git push -u origin <branch>
    # open a pull request; CODEOWNERS requests the review

The full gate set runs on the pull request, so nothing merges that has not
passed the same checks a release does. A maintainer approving their own work is
weaker than a second person doing it, and it is stronger than nobody looking —
the point is that the diff is read before it lands, not who reads it.

Direct pushes to `main` remain possible and are reserved for reverting a broken
`main`. Anything else, including a one-line fix, goes through the same door.

## Sign your work

    git commit -s

This is a Developer Certificate of Origin sign-off. You keep your copyright; see
`GOVERNANCE.md` §1 for why there is no CLA.

## Adding a capability

Per ADR-0001, a capability is a registered contract. You must answer **all six**
obligations — the zero value of each is invalid and the registry test will fail:

| Obligation | Question |
| --- | --- |
| `Floor` | What is the lowest tier that can provide this? |
| `Class` | Safety, serving, storage or network? |
| `Detect` | How is presence established on *this* device? |
| `Apply` | How is it set? (may be nil only for observe-only capabilities) |
| `Verify` | **How is it read back?** Never nil. No exceptions |
| `OnAbsent` | Degrade, or refuse? |

Two rules the test enforces and review cannot waive:

- **`Verify` may never be nil.** A control that cannot be read back is
  indistinguishable from one that silently stopped working.
- **A `classSafety` capability may not use `dispDegrade`.** Safety controls
  refuse or are rendered permanently red; they never downgrade quietly.

## Contributing a device record

The most valuable contribution, and it needs no code. Run the profiler, review
the output — it contains no identifiers, no location, no account — and open a
pull request adding it to `hardware/devices/`.

**Record what you observed, not what the spec sheet says.** A record whose
`verified` field is false is advisory only and never grants a tier. Negative
results are as valuable as positive ones: a device that *cannot* limit charging
is exactly what the next person needs to know.

## Writing tests

In the attacker's voice, with the consequence in the name, and mutation-tested.
`TestChargeCeilingRevertedByVendorDaemonIsDetected` beats `TestCharge`.

## Durability claims: one rule with a citation

**A warm reboot is not a durability test.** Proposing one — under any name —
fails review, and ADR-0004 §0 is the citation. A sealed-battery device cannot
drop its own storage rail, and the ordinary reboot paths flush the device cache
on the way out, so an honest device and a maximally dishonest one return
identical results. Any on-device "flush honesty" verdict is therefore a green
light from a test that cannot go red for the reason it claims to.

Real power-fault testing needs a physical fixture and lives in `hardware/lab/`.
Its result is advisory, describes one physical part, and never transfers.

## What will be refused

- Any capability that reports success without reading the result back.
- Any wording that implies a safety property the code does not verify.
- Telemetry that identifies a device, a person or a location (Article V.2).
- A dependency on a service this project controls (Article V.5).
