# ADR-0001 — The tier model and the capability registry

- **Status:** Proposed
- **Date:** 2026-08-06
- **Relates to:** CHARTER Articles II and IV; ADR-0002 (battery governor)

## Context

### The failure this exists to prevent

Every "old phone as a server" guide has the same defect: it was written against
one device. The author's phone was rooted, or was a Pixel, or had a charge-control
node at a particular path, and the guide silently assumes the reader's phone is
the same. The reader follows it, something does not exist, and either it fails
loudly (annoying) or **it appears to succeed while doing nothing** (dangerous —
see ADR-0002, where the thing doing nothing is a charge ceiling).

VayuPress's ADR-0150 paid for this lesson in public: a subsystem was designed
from the *threat* rather than the *product*, promised phases the binary could not
execute, and a published article repeated the promise. The correction produced a
rule this project adopts at its foundation:

> **What can this actually execute, on the device in front of us?**

A phone-server project is far more exposed to that question than a server binary
is, because its substrate varies more than any other target in computing. Two
phones of the same model, same year, different carrier, can differ in whether the
bootloader will ever unlock.

### Why a single feature list is not possible

Consider four real devices and one capability — limiting charge to 60%:

| Device class | Charge limiting |
|---|---|
| Stock, locked, no root | **Impossible.** No API exposes it |
| Stock, rooted | Possible, via a vendor-specific sysfs node whose path differs by SoC |
| Android 16 with virtualisation | Possible, but only mediated by the host — the guest cannot reach the charger |
| Mainline Linux port | Possible and durable, in the device tree |

Any product presenting one checkbox labelled "limit charging" across those four
is lying to three of them.

## Decision

**VayuCell detects a tier, binds every capability to a tier floor, verifies each
capability on the actual device, and reports the result.** No capability is ever
assumed from a device name, a model number, or a tier alone.

### 1. The four tiers

```text
T0  Stock Android, unprivileged userspace runtime
T1  Stock Android, root available
T2  Virtualised Linux guest (protected KVM VM on the Android host)
T3  Mainline Linux (postmarketOS-class port)
```

| | T0 | T1 | T2 | T3 |
|---|---|---|---|---|
| Bind ports < 1024 | No | Yes | Guest-local, host-forwarded | Yes |
| Escape Doze / background kill | No | Partly (wakelocks) | **Yes** | **Yes** (no Doze) |
| Charge ceiling | **No** | Yes | Host-mediated | **Yes** |
| Kernel maintained | No — vendor EOL | No — vendor EOL | **Yes — host kernel** | **Yes — mainline** |
| Bootloader unlock required | No | Usually | **No** | Yes |
| Device availability | Universal | Wide | **Narrow** | Narrow, growing |
| Data survives OS update | Yes | Yes | Yes | N/A |

**T2 is the strategic tier.** Android's virtualisation framework runs a genuine
Linux guest under a protected hypervisor, with its own kernel and its own memory,
mutually distrusted from the host. That is a real server environment on a phone
that is **unrooted, still receiving vendor updates, and has a locked bootloader**
— a combination previously impossible. It escapes Doze, inherits a maintained
host kernel, and needs no unlock.

Its limit is stated as loudly as its promise: **virtualisation is not universally
available.** It is present on some device families and absent on much of the
market. VayuCell probes for it and reports the result; it never infers it from a
version number alone.

**T3 is the security endgame.** A vendor kernel abandoned in 2021 has years of
unpatched vulnerabilities, and nothing in userspace repairs that. Only a
maintained mainline kernel does. T3 is also where battery control is most
durable, because the ceiling lives in the device tree rather than in a runtime
node something else may reset.

### 2. Tier is detected, never declared

```go
// Tier is the highest environment VERIFIED on this device.
// tierUnset is not a valid answer: a device whose tier could not be
// established is refused, not defaulted.
type Tier uint8

const (
    tierUnset Tier = iota // zero value — invalid, never a result
    T0                    // unprivileged userspace
    T1                    // root on stock
    T2                    // virtualised guest
    T3                    // mainline linux
)
```

Detection is **positive evidence only**. T1 is not "we did not see a locked
bootloader"; T1 is "a privileged operation was attempted and succeeded". The
distinction matters because the negative form silently promotes devices whose
probe merely failed to run.

### 3. The capability registry

Following `vayushield/rule.go`, `vayuveil` and `vayuflow` in the sibling project:
every capability is a registered contract whose obligations have **invalid zero
values**.

```go
// Capability is one thing VayuCell might be able to do on a device.
// A registration that leaves any obligation unanswered fails a test,
// not a review.
type Capability struct {
    ID        CapID       // "charge.ceiling", "net.port.privileged", …
    Floor     Tier        // lowest tier that CAN provide it; tierUnset invalid
    Class     Class        // classSafety | classServing | classStorage | classNet
    Detect    DetectFn    // establish presence on THIS device; nil invalid
    Apply     ApplyFn     // may be nil ONLY when Class == classObserve
    Verify    VerifyFn    // read the result BACK; nil is ALWAYS invalid
    OnAbsent  Disposition // dispDegrade | dispRefuse; dispUnset invalid
    Rationale string      // why these answers, in prose, rendered in the panel
}

// Complete reports whether every obligation was answered.
// Called by a test over the whole registry; a failure fails the build.
func (c Capability) Complete() error
```

Three properties, each enforced by test rather than by review:

1. **No capability without verification.** `Verify` may never be nil. A control
   that cannot be read back after being set is indistinguishable from one that
   silently stopped working, and reporting it would be exactly the lie Article IV
   of the charter forbids.
2. **Exhaustiveness.** The device profiler emits an inventory of every
   capability-bearing interface it found. An interface present on the device and
   absent from the registry **fails the build**. That is the only honest meaning
   of "no gaps": not that everything was thought of, but that anything not
   thought of cannot be introduced silently.
3. **Safety capabilities cannot degrade quietly.** Any capability with
   `Class == classSafety` and `OnAbsent == dispDegrade` is rejected at
   registration. A missing safety control must either refuse the operation or be
   rendered as a permanent failing row — never quietly downgraded.

### 4. The verification contract

Every capability follows the same three-step lifecycle, and the third step is
not optional:

```text
Detect  →  is the mechanism present on THIS device?
Apply   →  set it
Verify  →  READ IT BACK. Does the hardware agree?
```

A capability that reports success on the strength of a write returning no error
has told the operator nothing. Flash controllers acknowledge flushes they did not
perform; sysfs nodes accept writes they ignore; vendor kernels revert values on
the next charging event. **The read-back is the report.**

Verification is also **continuous, not one-shot**. Safety-class capabilities are
re-verified on a schedule, because the failure mode that matters is the ceiling
that held for six weeks and then quietly stopped.

### 5. What the profiler collects

A device profile is the evidence behind the tier, and it is stored so a support
conversation starts from facts:

| Group | Fields |
|---|---|
| Identity | SoC, model family, RAM, storage class and size, Android or distribution version, kernel version and build date |
| Privilege | Root available, bootloader unlockable, virtualisation present, SELinux mode |
| Power | Charge-control mechanism and path, battery design capacity, present full capacity, cycle count, thermal sensors |
| Storage | Filesystem, flush honesty result (ADR-0004), observed wear indicators |
| Network | Interfaces, reachability class (direct / NAT / carrier-NAT), modem present |

**The kernel build date is deliberately prominent.** It is the single best proxy
for how long this device has been running unpatched code, and it drives the
permanent security row in the posture report.

### 6. The hardware database, and why it is CC0

Profiles that a user chooses to contribute — with no identifiers, no location, no
account — build a public compatibility database: *this model, this firmware, this
charge-control path, this tier achieved*.

It is **CC0** because the facts about a device are a public good and should be
usable by competing projects, repair communities, and researchers without
permission. It is also the artefact most likely to outlive the code.

The database **advises and never decides**. A device is never granted a tier
because the database says its model reached it — it is granted a tier because the
probe on that handset succeeded. The database is used to *predict* before install
and to *explain* after failure, never to substitute for verification.

## Consequences

**Good.** A user is told the truth about their specific device before they rely
on it. A capability cannot be added without deciding its tier floor, its
detection, its verification and its behaviour when absent. Safety controls cannot
degrade silently. Nothing lands undeclared.

**Costly.** Four tiers is four test matrices, and contributors must answer six
questions to add a capability rather than writing a function. That cost is
accepted: it is the difference between a project people can leave running in
their homes and a weekend guide.

**Accepted limits.** Detection can be wrong on a device nobody has seen, which is
why unknown mechanisms report *unverified* and never *absent*. Some devices are
permanently capped by a locked bootloader, and no amount of software changes
that; the honest response is to say so in the first screen.

## Open decisions

| # | Decision | Recommendation |
|---|---|---|
| 1 | Which tier the installer targets by default | **Highest verified**, with an explicit downgrade path the user can choose |
| 2 | Whether T1 requires a specific root implementation | **No** — probe for the capability, not the brand |
| 3 | Whether to attempt automatic bootloader unlock | **Never.** It wipes user data and can brick devices. Instruct, never automate |
| 4 | Database contribution default | **Opt-in**, per charter Article V.2 |
