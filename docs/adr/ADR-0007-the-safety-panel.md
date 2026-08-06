# ADR-0007 — The safety panel: what a person is allowed to be told

**Status:** accepted — implemented in `core/src/panel.rs`, with the rendered
output committed to [`docs/panel-snapshot.txt`](../panel-snapshot.txt)
**Supersedes:** nothing
**Related:** [ADR-0001](ADR-0001-tier-model-and-capability-registry.md) (verdicts
that cannot leak a tier), [ADR-0002](ADR-0002-battery-safety-governor.md) §5–6
(the governor and the swelling estimate this renders),
[ADR-0006](ADR-0006-content-security-policy.md) (the same
make-it-unrepresentable technique, applied to a header),
[CHARTER.md](../../CHARTER.md) Article IV

---

## §0. Why this needs an ADR at all

Every other subsystem in this repository is judged by whether it does the right
thing. This one is judged by whether it *says* the right thing, and those come
apart in a way that is easy to miss: a governor that has stopped measuring is a
bug, but a governor that has stopped measuring while the panel says `NORMAL` is a
bug plus a lie, and only the second one reaches a person.

That asymmetry is the reason for the module and the reason for this document. A
panel is the one artefact where the failure is not detectable by the software
producing it, because a panel that is wrong looks exactly like a panel that is
right — it looks calm.

## §1. The pressure this is designed against

No adversary is required. Status displays drift toward green under ordinary
engineering pressure, and every step of the drift is locally reasonable:

| The step | Why it seems fine at the time |
| --- | --- |
| A row with no data is omitted | Rendering "unknown" looks broken |
| "Unknown" is grouped with "fine" | Both are non-alarming, and the user has no action |
| The headline is stored as a field | It saves recomputing it, and it starts out correct |
| An estimate loses its confidence | The confidence made the sentence long |
| `UNSAFE` is softened | It was alarming users who could do nothing about it |

None of these is a bug report anybody files. Each is a small pull request, and
each one individually keeps every existing test passing — which is precisely why
the defences below are structural rather than a review checklist.

## §2. Decision: absence is a finding, not a gap

`Finding` has three variants and **all three carry evidence**:

```rust
pub enum Finding {
    Verified(Evidence),
    Refused(Evidence),
    Unverified(Evidence),
}
```

Two decisions are packed in here.

**Three variants, not a boolean.** A boolean forces "checked and could not tell"
into one of the other two, and the one it gets forced into is always the
reassuring one — nobody defaults an unknown to `false` when `false` renders in
red. Charter Article IV states this as a rule; the type states it as an
impossibility.

**Evidence on every variant, including `Unverified`.** The variant that most
wants to be evidence-free is exactly that one: nothing was seen, so there is
nothing to write down. That intuition is backwards. `could not read
/sys/class/power_supply/battery/temp` is the most actionable line on the panel
and `unknown` on its own is the least, and a panel whose unknown rows are blank
is a panel that reads as calm.

`Evidence::new` refuses blank and whitespace-only strings, so the refusal happens
at construction — where a caller still has the context to say something true —
rather than at render time, where the only remaining options are to print nothing
or to invent something.

## §3. Decision: the headline is computed, never stored

`Panel::overall()` folds the rows and takes the maximum of an ordered `Overall`.
There is no field, no constructor argument and no setter.

A stored headline is not wrong on the day it is written; it is wrong on the day
somebody adds a row and does not update it. And the resulting disagreement is
never symmetric: a panel that says `UNSAFE` while the rows look fine generates a
bug report within a day, and a panel that says `PROTECTED` while one row is
unverified generates nothing at all, forever. The direction the drift takes is
the direction nobody notices.

Ordering `Overall` as `Protected < Unverified < Unsafe` means the precedence is
carried by `Ord` rather than by a table somebody has to keep correct. A single
`Unverified` row is enough to take the headline off `Protected`, however many
green rows surround it — four-fifths checked is not protected.

## §4. Decision: rows never disappear

`Panel::build` takes every input as a required argument, and the T0 case — a
stock handset with no charge control at all — produces a row that says so:

> `FAILED  charge mechanism  this device exposes no charge control, so no ceiling can be held`

This is the most common device VayuCell will ever run on, and the temptation is
to drop the row rather than show a permanent red on hardware the user cannot
change. That would leave a panel which never mentions charge ceilings on a device
that cannot hold one, and a user who reasonably assumes one is being held —
because the software would surely have said otherwise.

The same argument applies to the mechanisms that are not readable ceilings. A
`constant_charge_current_max` genuinely slows ageing and genuinely is not a
percentage anybody can read back; the row carries that difference, because it is
the difference between a verified ceiling and a hope.

## §5. Decision: swelling has no high-confidence setting

ADR-0002 §6 assembles a deformation risk from cycle count against age, capacity
fade, internal-resistance drift, accumulated time above 40 °C and falling charge
acceptance. Not one of those measures a millimetre of anything.

So `Confidence` has two variants, `Low` and `Moderate`, and **no `High`**. Adding
one requires editing the enum, which is a diff that has to be argued for rather
than a threshold that can be tuned upward one afternoon. A `compile_fail` doctest
pins it.

The rejected alternative is worth recording because it is the obvious one: a
numeric risk score. `risk: 0.91` reads as a measurement to every person who sees
it, and the number would be an unweighted blend of proxies with no calibration
behind it anywhere. A coarse level with an explicit confidence cannot be mistaken
for something an instrument produced, which is the entire requirement.

## §6. Decision: the inspection prompt is unconditional

The physical check renders on every panel, at every risk level:

> Put the phone face-down on a flat table. If it rocks, does not lie flat, or the
> screen or back cover is lifting at any edge, stop using it now and take it to
> hazardous-waste handling.

Showing it only on an elevated estimate is the natural design and it is wrong in
a specific way: it removes the prompt exactly where the estimate is mistaken. The
estimate exists *because* the quantity cannot be measured here, so a nominal
reading is not evidence of a flat cell — it is evidence that the proxies did not
object. The one instrument that can settle the question is a person with a flat
table, and the prompt has to survive the case where the software is wrong.

It also says which of the two parties can actually see the cell. Left implicit,
the instruction reads as a suggestion beside a panel that has already checked
five other things.

## §7. Decision: the rendering is committed

[`docs/panel-snapshot.txt`](../panel-snapshot.txt) holds two rendered panels —
the device where everything holds, and a stock handset with no charge control, a
derated governor and no cell behind it.

Property tests pin properties, and the way a panel degrades is not that a property
breaks. It is that the wording drifts, a row moves, a hedge is dropped for
brevity — while every individual assertion still passes. In a diff that reads as
a small edit to a Rust file.

Both panels are in the snapshot rather than just the good one, because the
alarming panel is the one under pressure to soften. The mutation gate confirms
this is load-bearing: renaming `UNSAFE` to something gentler breaks the snapshot
test and nothing else.

## §8. What this does not do

- **It does not decide anything.** Nothing here changes a charge ceiling, sheds a
  service or halts a workload. It renders what the other modules concluded, and a
  bug here produces a wrong sentence rather than a wrong device.
- **It does not detect swelling.** It renders an estimate assembled from proxies
  and then asks a person to look at the phone.
- **It has never been read by a user on a real device.** Nothing in this
  repository has. Every panel in the snapshot was rendered from a fake host
  describing a handset nobody here is holding.
