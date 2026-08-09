# ADR-0004 — Storage durability: assume the flash lies

- **Status:** Proposed
- **Date:** 2026-08-06
- **Relates to:** ADR-0001 (capability registry), ADR-0002 (battery governor),
  ADR-0003 (ingress), CHARTER Article IV

## §0. The correction, first, because it removes this ADR's original centrepiece

The draft of this ADR was built around a **flush honesty test**: write records,
force a flush, cut power, verify on reboot that acknowledged data survived, and
demote any device that failed. It was going to be the thing that let VayuCell say
"we tested your storage rather than assuming".

**It is unimplementable as an on-device self-test, and shipping it would have
produced exactly the lie this project exists to prevent.**

The reasoning, recorded because it generalises:

1. **A sealed-battery phone cannot drop its own storage rail.** `reboot`,
   `reboot -f -p` and a sysrq-triggered reset all leave the flash part powered.
2. **Worse, the ordinary paths flush the cache on the way out.** The kernel's
   shutdown path issues a cache flush to the eMMC/UFS device before resetting. So
   a device with a volatile cache that never lost power writes that cache to
   media — and **an honest device and a maximally dishonest device produce
   byte-identical results.**
3. **The harness cannot verify its own precondition.** Neither eMMC's extended
   CSD nor the UFS health descriptor exposes a portable power-cycle counter, so
   the test has no way to prove the rail actually dropped before it writes a
   verdict.

Put together: whatever shipped under the name "flush honesty test" would have
been a *warm-reboot test* measuring the page cache and our own `fsync` discipline
— **a green light from a test that structurally cannot go red for the reason it
claims to.** That is Charter Article IV violated by the very feature meant to
uphold it.

Real power-fault injection needs a dummy-battery supply, a relay on the rail and
a triggered re-boot leg. That is a lab fixture. It is not something a shipped
binary can do to itself, and pretending otherwise was the defect.

**The honest response to "we cannot verify this" is not a better test. It is a
design that does not need the answer.** That is what the rest of this ADR is.

## §1. Decision: assume the storage lies, and make it not matter

VayuCell **assumes every phone's flash may acknowledge a flush it did not
perform**, and is designed so that assumption costs nothing to be right about.

This is strictly better than the draft even if the test had worked, because:

- It is correct for the ~95%+ of devices that will never be lab-tested.
- It cannot silently pass a lying device.
- It removes an entire class of "the indicator was green" failure.

Three consequences follow, and they are the substance of this document.

### 1.1 The durability guarantee is stated as a lag, not an adjective

The draft said "a phone is a replica, never the only copy". True, and — as the
adversarial pass pointed out — **that is a durability guarantee only for data
older than the replication lag**, which the draft never stated. Data written in
the last N seconds exists on exactly one device that may be lying about having
written it.

So VayuCell states the number:

> **Your recovery point is the replication lag.** The panel displays it
> continuously, as a live figure — *"off-device copy is 47 seconds behind"* —
> and the posture report warns when it exceeds the configured target.

That is the honest durability statement for a phone server, it is verifiable, and
it is the number an operator actually needs. An adjective is not.

**"Live" is the load-bearing word, and it needed a mechanism.** `RecoveryPoint`
originally carried `Behind(Duration)` — the lag and nothing else — and
implemented `Display`. A figure with no measurement time renders identically
whether it was taken a second ago or the morning the replicator died, so the
number this section prefers over an adjective was, structurally, an adjective
wearing a number's clothes. `47` said nothing about whether anyone was still
counting.

So the type now carries `Behind { lag, measured_at }`, the stamp is monotonic,
and **there is no `Display` impl** — `Display` is the hole, because
`format!("{rp}")` renders with no clock in scope and no way for the type to
object. `describe(now)` and `needs_attention(target, now)` require the clock's
reading; a measurement older than `MEASUREMENT_STANDS_FOR` (five minutes, against
a 60-second default target) reports itself as no longer live; and
`Posture::concerns` takes `now` for the same reason, because otherwise the panel
goes on presenting a dead replicator's last good reading as no concern at all.

This is the same defect as ADR-0003 §4.1, found by the same question, and repaired
before the replicator exists rather than after. **A claim in the present tense —
*"is 47 seconds behind"* — is a claim about now, and needs something that knows
what time it is.**

### 1.2 Writes are made survivable rather than trusted

Where the device cannot be trusted to have persisted an acknowledged write, the
mitigation is to make the loss of the last write survivable:

- **Idempotent, replayable writes** wherever the workload allows, so a lost tail
  is re-applied rather than lost.
- **Checkpoint boundaries the replica can resume from**, so recovery is
  "re-send from checkpoint", not "hope".
- **The database configured for the strongest ordering the platform offers**,
  while stating plainly that ordering guarantees rest on the device honouring
  flushes — which we assume it may not.
- **Power loss handled from the top**, not the bottom: the battery is a UPS
  (ADR-0002 §8), so the governor's shed ladder gives a clean, ordered shutdown
  with charge to spare. **This is the real durability mechanism on a phone** —
  far more effective than any flush test, because it means the common case is a
  *graceful* stop rather than a power cut at all.

That last point deserves emphasis. The draft chased the hard case (sudden power
loss) with an unimplementable test, while the governed battery already converts
most of that hard case into the easy one. **Design the graceful path well and the
untestable path stops being the dominant risk.**

### 1.3 Write shaping, corrected

The draft said to "batch and align writes; keep write-amplifying workloads off
internal flash". The adversarial pass called this *actively unsafe and aimed at
the wrong levers*, and it was right on both counts:

- **Batching writes to reduce wear directly enlarges the window of data that
  exists nowhere else.** Trading durability for endurance, silently, on a device
  whose durability is already the weak point, is the wrong trade — and it was not
  presented to the operator as a trade at all.
- **Endurance is not the binding constraint it was assumed to be.** Phone flash
  is not rated for datacentre write patterns, but a personal site, mailbox and
  file store on a modern part is not close to exhausting it. The draft optimised
  a constraint it had not measured.

The corrected position: **do not shape writes for endurance by default.** Report
observed wear where the device exposes it, act only if a real trend appears, and
if a durability-for-endurance trade is ever offered, present it as a trade with
its cost named.

## §2. What the hardware database records now

The schema's `flush_honest: pass | fail | untested` field is **withdrawn**. It
encoded a test that cannot run, keyed a probabilistic result to a model
identifier, and its dominant value would have been `untested` on a field that
reads as a health check.

It is replaced by fields that record only what can be observed:

| Field | Meaning |
| --- | --- |
| `durability_class` | `assumed_untrusted` (default, and the honest answer for essentially every device) or `lab_verified` |
| `lab_verification` | Optional. Present only where a contributor ran a **real power-fault rig**: method, fixture, date. Advisory, never a grant |
| `wear_indicator` | `readable` / `absent` / `unreliable` — whether the device exposes anything, not what it said |
| `graceful_shutdown_verified` | Whether the governor's shed ladder completed cleanly with the database consistent on restart — **this one is genuinely testable on-device** |

Note which field survived: the one measuring **our own behaviour**, not the
device's honesty. That is the general rule this ADR produces —

> **Test what you control. Assume the worst about what you do not.**

`durability_class` defaults to `assumed_untrusted`, and the panel renders it in
neutral language rather than as a warning, because it is not a fault in the
device — it is the correct posture toward all consumer flash.

## §3. The lab test, kept honestly

The power-fault test is not deleted; it is relocated to where it belongs.

- It is a **documented procedure for contributors with a fixture**, in
  `hardware/lab/`, not a self-test in the product.
- Its result is recorded as `lab_verified` for a **specific device, firmware and
  storage part** — and even then it is **advisory**. It never grants a tier, and
  it never causes VayuCell to relax the assumption on another user's handset,
  because that user's phone is a different physical part.
- The procedure explicitly states that a warm reboot is **not** a substitute, and
  why, so nobody reimplements the defect.

## §4. Backup: the part that stops at the upload

The draft said "off-device encrypted backup is on by default". The completeness
critic noted it **stops at the upload** — an uploaded archive nobody has ever
read back is not a backup, it is a hope with a filename.

VayuCell therefore adopts the sibling project's discipline directly: the backup
system restores an archive on a schedule, checks it, and **reports the time of
the last verified restore** rather than the time of the last upload. A cell whose
backups have never been verified reads as unverified, not as protected.

Two additions specific to a phone:

- **The restore drill is thermal-class-declared** (ADR-0003 §5) and is shed by
  the governor under load, because a verification job that cooks the battery is
  its own kind of failure.
- **The backup target must not be the cell itself, nor another cell in the same
  building.** Same disk is not a backup; same room is not off-site.

### 4.1 "On a schedule" needed a clock, and it took a third instance to notice

`BackupState::Restored` carried `when: String` — a free-form date nothing in the
crate ever compared to anything — and `is_proven()` returned `true` for it
forever. So this section's central discipline was half-built: an *unrestored*
backup read as unverified, correctly and permanently, while a backup restored
once in March read as proven in December. The failure §4 exists to catch is a
chain that breaks **silently**, where the upload keeps succeeding and the only
thing that would notice is the restore nobody has run since — and a drill with no
expiry is exactly the instrument that cannot notice it.

This is the third instance of one defect: a fact whose honesty depends on time,
stored without a time. The others are ADR-0003 §4.1 and §1.1 above. The repair is
the same, with one difference that matters:

- **`Restored { at_unix }`**, and `is_proven(today)` / `describe(today)` take the
  current date. There is no `Display` impl; a `compile_fail` doctest proves it.
- **`DRILL_STANDS_FOR` is a month.** Long enough that the drill is not a standing
  thermal load — this section already makes it shed by the governor for that
  reason — and short enough that a broken chain is found inside a month.
- **The stamp is wall-clock, not monotonic**, and that is the difference. A
  replication lag is a duration inside one process, where `Clock::elapsed` is the
  only safe answer. A restore drill happened *before this process started*: a
  monotonic clock that begins at zero on boot cannot date March. So `Clock` gains
  `wall_clock_unix()`, and nothing in the governor, the sampler or the shed
  ladder may call it — a wall clock that steps backwards would hand them an
  outage that ran in reverse, which is the hazard `elapsed` was written to avoid.
- **It returns `Option`, and `None` is not recent.** A phone with no network and
  a dead RTC is an ordinary phone. A cell that cannot tell what day it is cannot
  tell whether a drill is current, and Article IV.3 settles what that reports as.
  Reading `None` as recent would make the least capable device the most confident
  one.

## §5. Root, and what is actually readable

The draft "silently presupposed root", and most of the storage introspection it
assumed is unavailable on the tier most devices are stuck at. Corrected:

| Capability | T0 | T1 | T2 | T3 |
| --- | --- | --- | --- | --- |
| Wear / health indicators | Almost never | Sometimes, vendor-dependent | Guest sees virtual storage, not the part | Best, still vendor-dependent |
| External storage | Limited | Yes | Host-mediated | Yes |
| Filesystem control | No | Partly | In-guest | Yes |

At T2 there is a further honesty point the draft missed entirely: **the guest
sees virtualised storage.** Health data read there describes an abstraction, not
the physical part. It is reported as unverified rather than as a device reading.

Where nothing is readable, VayuCell reports **absent** — never "healthy".

## §6. What this will never claim

Permanent failing rows, per Charter Article IV:

1. **Not** that this device's storage honours flushes. That cannot be tested
   on-device, and we assume it may not.
2. **Not** that a `lab_verified` result transfers to another handset of the same
   model. It describes one physical part.
3. **Not** that data newer than the replication lag is safe. It is not, and the
   lag is displayed for exactly that reason.
4. **Not** that wear indicators are accurate, or present. Most devices expose
   nothing, and vendors disagree about what the numbers mean.
5. **Not** that an uploaded backup is a backup. Only a verified restore is.
6. **Not** that any of this survives physical loss of the device — see ADR-0003
   §8, the gap both ADRs share and neither closes.

## §7. Test gates

| Gate | Proves |
| --- | --- |
| Warm reboot proposed as a durability test | **Fails review by rule** — §0 is cited in the contributing guide |
| Replication lag exceeds target | Posture warns; the live figure is never hidden |
| Backup uploaded but never restored | Reads **unverified**, never protected |
| Mains removed under load | Shed ladder completes; database consistent on restart; `graceful_shutdown_verified` set |
| Device exposes no wear data | Reports **absent**, never "healthy" |
| T2 guest health read | Reported **unverified**, not as a device reading |
| Any code path that reports `durability_class: lab_verified` without a `lab_verification` record | Fails the registry test |

## §8. Open decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | Default replication target | **Off-device, off-site.** Another cell in the same building is availability, not backup |
| 2 | Replication lag target | **60 s** default, displayed live, configurable — a number the operator can hold us to |
| 3 | Whether to ever ship an on-device durability verdict | **No.** §0 is the reason, and it is permanent unless the hardware changes |
| 4 | Endurance-for-durability trades | **Never by default.** Only as an explicit operator choice, with the cost named |
| 5 | Whether `assumed_untrusted` should look like a warning | **No.** It is the correct posture toward all consumer flash, not a defect in this device |
