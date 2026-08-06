# ADR-0002 — The Battery Safety Governor

- **Status:** Accepted — the governor core is implemented in
  `core/src/governor.rs` and `core/src/battery.rs`. The state machine, the
  verification loop, the thresholds and the recovery path exist and are
  mutation-tested. The sysfs layer is implemented in
  `core/src/sysfs.rs`: readings, mechanism detection, and the `EndThreshold`
  ceiling. The **sampling cadence** of §3 is implemented in
  `core/src/sampler.rs` as a pure function of the reading, together with what
  the governor reports once the cell stops being readable at all. The **shed
  ladder of §8** is implemented in `core/src/shed.rs`, including the reserve the
  node shuts down holding and the battery-absent case that may not claim a UPS
  at all. The **panel** of §5–6 is implemented in `core/src/panel.rs`, including
  the swelling estimate rendered as an estimate and the physical-inspection
  prompt that appears at every risk level. **Not yet built:** the daemon that
  runs the sampling loop on a device, and the non-ceiling mechanisms as controls
  in their own right. Nothing here has run on a phone.
- **Date:** 2026-08-06
- **Relates to:** CHARTER Article III (safety of persons); ADR-0001 (tiers)
- **Priority:** P1 — **nothing that serves traffic ships before this**

## The claim, worded to be defensible

> **VayuCell continuously measures the state of the cell it is asking you to
> leave energised, holds a charge ceiling where the device permits one and
> verifies that ceiling by reading it back, degrades and then stops the workload
> when temperature or health thresholds are crossed, and states plainly — on the
> first screen — when it can do none of these things on your device.**

Read what that does **not** say. It does not say the battery is safe. It does not
say swelling will be prevented, or even detected — swelling is a physical
deformation, and software cannot see it. It says the risk is **measured, bounded
and reported**, and it names physical inspection as the definitive check.

## Context

### Why this is the first subsystem, not a later one

Every other blocker in this project is an engineering inconvenience. This one is
a fire in somebody's home.

VayuCell's entire proposition is *leave this phone plugged in forever*. That is
precisely the condition under which a lithium-ion cell ages fastest: held at high
state of charge, at elevated temperature, for years. The end state of that
process is a swollen cell. Waste authorities treat swelling as battery damage and
a fire hazard, and direct people to hazardous-waste handling rather than ordinary
disposal.

A project that invites millions of people into that condition and does not govern
it is not "enterprise-grade with a caveat". It is negligent. So the charter makes
this immovable: **no capability that serves traffic may ship before the
governor.** Shipping a convenient demo first would put real hardware into a risky
state in real homes to hit a milestone.

### The physics, stated plainly enough to design against

Three stressors, multiplicative rather than additive:

1. **State of charge.** Time spent near full is the dominant ageing term. A cell
   held at ~50% ages a small fraction as fast as one held at 100%.
2. **Temperature.** Ageing roughly doubles for each ~10 °C rise. A phone running
   a workload while charging is warm by definition.
3. **Charge current.** Fast charging generates heat and stresses the anode. A
   server has no reason to charge fast; it has all day.

The design follows directly: **hold the charge low, hold the temperature down,
and charge slowly.** Everything below is mechanism for those three sentences.

## Decision

### 1. The mechanism differs per tier, and so does the honesty

Charge limiting is not one feature. It is four different mechanisms with four
different reliabilities, and the panel must say which one it is using.

| Tier | Mechanism | Reliability |
| --- | --- | --- |
| **T0** | **None exists.** No unprivileged Android API limits charging | **Permanently red** |
| **T1** | Vendor sysfs node, written as root | Good, but vendor-specific and revertible |
| **T2** | Requested from the Android host; the guest cannot reach the charger directly | Mediated — verify from the guest, never assume |
| **T3** | Device-tree maximum charge voltage, plus standard sysfs thresholds | **Best** — survives reboot, set below the runtime |

**T0 deserves its own paragraph, because it is the most common device and the
worst case.** On an unrooted stock phone there is no supported way to stop the
charger at 60%. VayuCell must not imply otherwise. The first screen on a T0
device says, in plain language: *this phone cannot limit its own charging; the
safe options are to run it on a smart plug that cycles power, to remove the
battery if this model boots without one, or to accept and monitor the risk.* That
sentence is worth more to the user than any feature.

### 2. The nodes actually used

Detection probes these in order and records **which one answered** — the path is
part of the device profile and the hardware database:

```text
# Mainline / standard (preferred where present)
/sys/class/power_supply/battery/charge_control_end_threshold      # percent
/sys/class/power_supply/battery/charge_control_start_threshold    # percent

# Charge-current limiting (slow charging; widely present)
/sys/class/power_supply/battery/constant_charge_current_max       # µA

# Vendor charge suspend (common on Qualcomm platforms)
/sys/class/power_supply/battery/input_suspend                     # 1 = stop

# Terminal voltage (device-tree preferred on T3; runtime node where exposed)
/sys/class/power_supply/battery/voltage_max                       # µV
```

**The T3 device-tree approach is the most durable and is preferred where the port
allows it.** Lowering the pack's maximum charge voltage from a typical ~4.4 V to
~3.8 V holds the cell at roughly 40–50% state of charge permanently, below the
runtime, where no userspace process can revert it. This is the technique the
mainline porting community demonstrated for exactly this use case, and VayuCell
adopts it rather than inventing an alternative.

### 3. Telemetry: what is read, and how often

```text
capacity            %          reported state of charge
voltage_now         µV         cell voltage — the honest SoC signal
current_now         µA         sign indicates charge/discharge
temp                0.1 °C     pack temperature (decidegrees)
cycle_count         count      lifetime equivalent full cycles
charge_full         µAh        present capacity when full
charge_full_design  µAh        original design capacity
status / health     enum       vendor's own opinion, recorded but never trusted
```

Sampling is **adaptive**: every 30 s in steady state, every 5 s within 5 °C of a
threshold or during a charge transition. A phone doing nothing must not be kept
awake by its own monitor.

Two derived values do most of the work:

- **State of health** = `charge_full / charge_full_design`. Below 80% the cell is
  degraded; below 60% it should be retired from unattended duty.
- **Internal-resistance drift**, estimated from voltage response to known current
  steps. A rising trend is the best software proxy available for a cell going
  bad, and it is reported **as an estimate that says it is an estimate**.

### 4. The governor state machine

States are explicit, and every transition is logged with the reading that caused
it.

```text
                 ┌──────────┐
                 │  NORMAL  │  ceiling held & verified; temp nominal
                 └────┬─────┘
        temp > warn   │   SoH < 80%  │  ceiling verify failed
                      ▼
                 ┌──────────┐
                 │ DERATED  │  workload shed; charge current reduced;
                 └────┬─────┘  panel warns; fleet notified
        temp > critical  │  SoH < 60%  │  resistance drift alarm
                      ▼
                 ┌──────────┐
                 │ PROTECT  │  serving stopped; charging suspended;
                 └────┬─────┘  data checkpointed; user told to inspect
        temp > hard-stop │  cell voltage anomaly
                      ▼
                 ┌──────────┐
                 │ HALT     │  clean shutdown. Requires a human to clear
                 └──────────┘
```

Three rules govern the machine:

- **Escalation is automatic; de-escalation from `PROTECT` and `HALT` is not.**
  A cell that reached a critical threshold has told you something about itself.
  Clearing it requires a person, who is prompted to look at the phone.
- **`HALT` is a real shutdown**, not a paused service. If the hardware is in a
  state this project considers hazardous, the correct behaviour is to stop being
  a server.
- **Every transition names its reading.** "Halted" is useless; "halted: pack
  temperature 61.2 °C exceeded hard stop 60.0 °C at 14:22" is actionable.

### 5. The verification loop — the part that makes it a control

```go
// A ceiling that was set once and never re-read is not a control.
// It is a configuration, and vendor kernels revert configurations.
func (g *Governor) enforce(ctx context.Context) error {
    if err := g.mech.Apply(g.target); err != nil {
        return g.degrade(ReasonApplyFailed, err)
    }
    got, err := g.mech.Verify()          // READ IT BACK FROM THE HARDWARE
    if err != nil {
        return g.degrade(ReasonUnverifiable, err)  // unverified ≠ working
    }
    if !g.target.Satisfies(got) {
        return g.degrade(ReasonReverted, nil)      // something undid us
    }
    return nil
}
```

`ReasonReverted` is the interesting one and the reason this loop exists.
Vendor charging daemons reset these nodes on cable events, thermal events and
firmware updates. A ceiling that held for six weeks and then quietly stopped is
the exact failure mode a one-shot writer cannot see, and it is indistinguishable
— from the user's side — from a governor that never worked at all.

### 6. Swelling: inferred, never claimed

Software cannot measure a millimetre of deformation. What it can do is combine
the signals that correlate with a cell approaching that state:

| Signal | What it suggests |
| --- | --- |
| Cycle count vs. age | How hard this cell has already worked |
| State of health trend | Capacity fade, the classic ageing curve |
| Internal-resistance drift | The strongest available proxy for degradation |
| Temperature history above 40 °C | Accumulated thermal stress |
| Voltage-curve anomalies | Cell imbalance or damage |
| Charge acceptance falling | The charger fighting the cell |

These produce a **risk estimate with an explicit confidence**, and the panel
renders it as an estimate. Then it does the one thing that actually resolves the
question:

> **Physical inspection is the definitive check.** Put the phone face-down on a
> flat table. If it rocks, does not lie flat, or the screen or back cover is
> lifting at any edge, **stop using it now** and take it to hazardous-waste
> handling. Software cannot see this. You can.

Scheduling that prompt is a feature: at install, at every risk-estimate increase,
and on a fixed calendar regardless of readings.

### 7. Battery-absent operation

Some devices boot and run on USB power with the cell physically removed. Where
that is true, it is **the safest configuration available** and VayuCell says so.

- Detection is empirical — a profile flag set by a user who has done it, recorded
  in the hardware database per model.
- In battery-absent mode the governor switches to **mains-loss posture**: there
  is no UPS, so an outage is an immediate hard stop, and write-durability policy
  tightens accordingly (ADR-0004).
- Where a device **refuses to boot without a battery** — many do — VayuCell says
  that, rather than implying a choice the hardware does not offer.

### 8. The inversion: the battery as the best feature you have

Once governed, the thing that was the project's largest risk becomes its clearest
advantage over every competing option.

A governed phone is **a server with an integrated uninterruptible power supply**.
On mains loss the cell carries the node, and the governor runs a defined ladder:

```text
mains lost      → notify fleet; stop accepting new work
 -60s           → shed non-essential services (media, indexing, inference)
 -180s          → checkpoint state; flush and fsync; quiesce the database
 threshold      → clean shutdown with charge remaining
```

A single-board computer at any price cannot do this without buying a UPS that
costs more than the board. **A governed cell at 50% state of charge is both the
safest way to hold the battery and enough energy to ride out a typical outage** —
the same decision serving both goals, which is the sign of a correct design.

## What this will never claim

Each appears in the posture report as a permanent failing row that no
configuration clears:

1. **Not** that the battery is safe. Risk is governed, never eliminated.
2. **Not** that swelling is detected. It is *estimated*, and inspection is named
   as the real check, every time.
3. **Not** that charge limiting is available on every device. On T0 it is not,
   and that row stays red forever.
4. **Not** that a governed cell will not eventually need replacing. Every cell
   ages; governing slows it.
5. **Not** protection against a cell already damaged before VayuCell was
   installed.

## Test gates

Every one written in the attacker's voice, and mutation-tested — the defence is
re-broken and the test confirmed to go red again.

| Gate | Proves |
| --- | --- |
| Ceiling is applied, then externally reverted | The loop detects it and enters `DERATED` |
| Verify path made to fail | State becomes *unverified*, **never** *working* |
| Temperature ramped past each threshold | Each transition fires, in order, with the reading logged |
| A `classSafety` capability registered with `dispDegrade` | **Fails registration** (ADR-0001 §3.3) |
| Mains removed under load | Full shed ladder completes; database is consistent on restart |
| T0 device profile | First screen states the limitation; no green safety row is renderable |
| `HALT` reached | Cannot be cleared programmatically; requires human action |

## Open decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | Default ceiling | **60%** — meaningful ageing reduction while retaining useful UPS runtime |
| 2 | Hard-stop temperature | **60 °C pack** — conservative; configurable downward only |
| 3 | Behaviour when the mechanism is absent (T0) | **Serve, but render the safety row permanently red** and prompt inspection on a schedule. Refusing to run would push users to worse, ungoverned alternatives |
| 4 | Smart-plug integration as a T0 mitigation | **Yes, P2.** Cycling mains power is the only charge control a T0 device can have |
| 5 | Whether to trust vendor `health` enum | **Record, never act on it.** Vendors report `Good` on visibly swollen cells |
