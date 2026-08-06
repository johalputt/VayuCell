<!-- SPDX-License-Identifier: Apache-2.0 -->

# Changelog

Every entry says what changed and, where it matters, what it means for someone
running this on hardware in their home. Entries that only a maintainer could
care about are still listed, but they are marked as such.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions are patch-only until the battery governor ships — see
[`CHARTER.md`](CHARTER.md) Article III.1, which forbids anything that serves
traffic before it.

## [Unreleased]

### Added

- **The Battery Safety Governor** (ADR-0002) — the subsystem Charter Article
  III.1 required before anything may serve traffic. State machine, verification
  loop, thresholds, and a recovery path that requires a person who looked at the
  phone.
- Response security headers as a set (ADR-0006 §3), with the posture committed
  to `docs/security-posture.txt` so weakening it is a visible diff.
- The **power-supply sysfs layer** (ADR-0002 §2–3): battery readings that refuse
  to be assembled from whatever happened to be readable, mechanism detection that
  records which node answered, and a charge ceiling that reads back from the
  hardware rather than from what this process remembers writing.
- The **sampling cadence** (ADR-0002 §3): thirty seconds when nothing is close
  to happening, five when the cell is charging or within five degrees of the
  lowest threshold. It is a function of the reading rather than a loop that owns
  a clock, so a cell warming over an hour is a handful of assertions instead of
  an hour of waiting — and so the device is not kept awake by the monitor that
  exists to protect its battery.
- **A governor that has gone blind now says so.** Three consecutive failed reads
  derate the device and name the reason. Before this, a phone whose power-supply
  nodes vanished — a kernel update, a permission change — produced no readings,
  no transitions, and a panel that still said `NORMAL`; a monitor that has
  stopped measuring and stays quiet is reporting a healthy device. A reading
  that actually arrived is the only thing that clears the counter. Unreadability
  also tightens the sampling cadence rather than backing it off, which is the
  direction a retry timer would naturally have taken it.
- **The mains-loss shed ladder** (ADR-0002 §8) — the inversion. A governed
  phone is a server with an integrated uninterruptible power supply, which no
  single-board computer can say without buying a UPS costing more than the
  board. On mains loss the node announces, sheds non-essential services at 60
  seconds, checkpoints and quiesces its database at 180, and shuts down while
  it still holds a reserve. Reaching that reserve shuts the node down whatever
  the clock says, and time alone never does: a node an hour into an outage
  still holding 70% is doing exactly what it was built to do.
- **The UPS claim is computed rather than written down.** A handset running
  with its pack removed has no cell to ride an outage on, so mains loss stops
  it immediately — and it reports that it cannot make the claim, instead of
  presenting three minutes of ladder it has no energy to run.
- **The safety panel** (ADR-0002 §5–6) — the one screen anybody actually
  reads, and so the one place where being wrong is guaranteed to reach them.
  Every row cites what it saw, including the rows that admit they could not
  check; there is no way to write "verified" without saying what verified it.
  The headline is computed from the rows rather than set beside them, and a
  single unchecked row is enough to take it off `PROTECTED` — four green rows
  and one nobody could read is not a protected device.
- **Swelling is estimated and never claimed.** The confidence attached to that
  estimate has no `High` setting and cannot be given one without editing the
  source, because software has no instrument for a millimetre of deformation.
  The panel renders it as an estimate and then asks for the check that does
  settle it: the phone face-down on a flat table, at every risk level rather
  than only the alarming ones — an estimate reading nominal is not evidence of
  a flat cell.
- **What the panel says is committed to `docs/panel-snapshot.txt`**, alongside
  the response security posture. Both the reassuring panel and the alarming one
  are rendered there, so softening the alarming one — the way status displays
  actually drift — produces a plain-text diff rather than an innocuous-looking
  edit to a Rust file.

### Changed

- Charter Article III.1 is now **satisfied**: the governor exists, so serving
  capabilities are permitted. The gate stays live in the other direction.
- *Maintainers only:* the doctest gate now asserts the **exact** number of
  compile-time proofs rather than a floor of one. Those proofs are collected
  only from public items, so a proof moved onto a private one runs nothing and
  still reports success — and under a floor of one, fifteen of sixteen could
  disappear without the gate noticing. The gate against silent passes was
  passing silently. Both directions are now covered by the gate self-test.

## [0.0.1] — 2026-08-06

The founding release. **Nothing here serves traffic**, and by charter nothing
will until the battery governor exists.

### Added

- **The charter** and a subordinate governance constitution — 93 rules, each
  marked with whether a machine or a human enforces it.
- **The capability registry** (ADR-0001). Obligations have no valid zero value:
  a capability that sets something without reading it back does not compile.
- **Tier detection** (ADR-0001 §2) from positive evidence only. A machine
  nothing recognises is `Unknown`, and `Unknown` satisfies no capability floor.
- **A Content Security Policy as a type** (ADR-0006). `Source` has no variant
  for `'unsafe-inline'` or `'unsafe-eval'`, so weakening it is an addition to a
  public enum rather than a one-word edit to a string.
- **The hardware compatibility database** (CC0) with a schema that refuses a
  verified charge ceiling which names no sysfs node.
- **CI that enforces the charter**, and gates that are themselves tested: 34
  planted violations must each be caught, and 20 mutations must each turn a test
  red.

### Known limits

- No end-to-end test on real hardware. Every device-facing behaviour is
  exercised through a fake host describing handsets nobody here is holding.
- Four charter articles are human review only — III.2, III.4, IV.4 and V.4. The
  charter gate prints them on every run rather than omitting them.
