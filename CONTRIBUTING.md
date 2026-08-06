# Contributing to VayuCell

## Before anything else

Read `CHARTER.md`. It is short, and it constrains what may be built here — in
particular Article III, which puts safety of persons ahead of every schedule.

## Sign your work

    git commit -s

This is a Developer Certificate of Origin sign-off. You keep your copyright; see
`GOVERNANCE.md` §1 for why there is no CLA.

## Adding a capability

Per ADR-0001, a capability is a registered contract. You must answer **all six**
obligations — the zero value of each is invalid and the registry test will fail:

| Obligation | Question |
|---|---|
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

## What will be refused

- Any capability that reports success without reading the result back.
- Any wording that implies a safety property the code does not verify.
- Telemetry that identifies a device, a person or a location (Article V.2).
- A dependency on a service this project controls (Article V.5).
