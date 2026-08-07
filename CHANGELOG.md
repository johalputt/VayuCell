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
- **Sovereign ingress** (ADR-0003) — four modes, each declaring seven
  properties with none of them optional, because the three fields the ADR's
  draft lacked each changed a decision. An onion depends on a commons rather
  than on nothing; it is not reachable by an ordinary browser, since `.onion` is
  a reserved name that is not in DNS; and it has the worst compromise story of
  the four, because the identity key is the address and there is no revocation.
  The default is local-only: publishing is irreversible disclosure, which
  Charter Article VIII.5 forbids without explicit confirmation, and it is the
  only default executable on T0.
- **The governor now outranks ingress, by construction.** The worst defect
  ADR-0003 records is that its draft made the highest-heat mode the default
  while the battery governor existed to suppress heat-driven ageing — and
  neither document mentioned the other. `shed_for` takes a governor level and
  has no parameter that overrides it: `DERATED` sheds high-thermal ingress
  first, `PROTECT` and `HALT` stop everything outward-facing, and local-only
  survives because stopping it would take the panel away from the person who
  most needs to read it. The heat cost, the audience limit and the permanent
  compromise are disclosed *before* the mode is chosen, and a device that
  cannot hold a charge ceiling is told that this combination has no mitigation
  available at all.
- **"The tunnel is up" is not expressible.** `Reachability` has no variant for a
  running process; verified means a request originating outside the device
  traversed the path and was served. A loopback test proves nothing about a path
  whose entire difficulty is external.
- **Storage durability** (ADR-0004) — the guarantee is a number rather than an
  adjective. `RecoveryPoint` has no variant meaning durable, and a
  `compile_fail` doctest keeps it that way: a phone is a replica, and that is a
  guarantee only for data older than the replication lag. The closest thing to
  good news the type can express is how far behind the off-device copy is, which
  still names the window in which data exists on one device only.
- **A backup nobody has restored can never read as proven.** Everything anybody
  checks on a written backup — its size, its checksum, that it appeared — is a
  property of the file rather than of the restore, and writing more backups is
  what people do instead of restoring one, so it never moves that row. Of the
  four things ADR-0004 records, the one that can read as verified is the shed
  ladder completing, because it measures this software's behaviour rather than
  the flash controller's honesty.
- **Assuming the flash lies is never itself reported as a fault.** It is the
  correct posture toward all consumer flash and true of every device; rendered
  as a warning it would appear on every panel forever, and a warning that is
  always on is one nobody reads. `lab_verified` cannot be claimed without naming
  the method, the fixture and the date, so it cannot be set by somebody who
  rebooted a phone and watched the database survive — which is the test ADR-0004
  withdrew.
- **`vayucell`, the binary** — the thing that owns the loop. `status` reads the
  device once, prints the panel and exits with the verdict: 0 protected, 1 not
  fully verified, 2 unsafe, 64 unusable arguments. A monitor gets the answer
  without parsing prose, and unmeasured stays a different number from failed,
  because collapsing them loses the distinction Article IV exists to keep.
  `run` holds the ceiling and stops when the governor halts. A `--ceiling` of
  200 is refused rather than clamped — 100 holds no ceiling at all, so clamping
  would make the unsafe reading the silent one on the single setting that
  governs a cell in somebody's home. Zero third-party dependencies here too;
  argument parsing is thirty lines of `std`.
- *Maintainers only:* **the mutation gate was corrupting a crate it did not know
  about.** It snapshotted `core/src` by name, so mutations naming files in the
  new `cli/src` were applied and never restored — five accumulated on disk.
  Nothing in the mutation output said so; the gate's own closing check that the
  suite must be green *after* the last restore is what caught it. It now
  enumerates every crate rather than naming one, the charter gate's
  no-dependencies rule was widened from `core/Cargo.toml` to every manifest, and
  the gate self-test plants a dependency in the CLI crate to prove that widening
  works.
- **The supervisor loop** — the piece that makes the rest a running thing. One
  tick reads the cell, enforces the ceiling, shows the reading to the governor,
  advances the shed ladder and returns how long to wait. The clock is a trait,
  so thirty simulated days — 86,400 ticks — is a unit test that finishes in
  milliseconds. That test says the composition does not drift or stop
  escalating over a long run; it says nothing about a real kernel or a real
  cell, and it is not the roadmap's P2 gate. The unreadable case is not an
  early return: it feeds the blind counter, tightens the cadence and fills in
  the same outcome as any other tick, because a loop whose error path is
  shorter than its success path goes quiet exactly when something is wrong.
  A governor that halted before a restart comes back halted, because the
  supervisor is handed one rather than building a fresh one.
- **ADR-0007** records the panel's design decisions and, more usefully, the
  alternatives that were rejected: a numeric risk score, a stored headline, a
  conditional inspection prompt, and dropping the charge-mechanism row on
  devices that have no charge mechanism. Each of those is the obvious design,
  and each fails in the reassuring direction.

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
