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

## [0.0.2] — 2026-08-08

The first release with something a person can actually download and run.

### Fixed

- **The release published library files, not a program.** Every tagged release
  would have collected `libvayucell-<target>.rlib` — a Rust static library,
  which nobody can run — so `install.sh` would never have found a usable build
  and *every* install would have silently fallen back to compiling from source
  on the phone. Twenty minutes, on a device chosen for being old, for a download
  that should have taken seconds. The build was green the entire time, because
  nothing connected the name the release writes to the name the installer asks
  for. The release now cross-links a real binary for all five targets — the
  Android ones against the NDK at API 24, which is the same Android 7 floor
  `docs/INSTALL.md` promises — and publishes `vayucell-<target>.tar.gz` with a
  fixed mtime and sorted entries so the tarball stays reproducible.
- **`scripts/install-gate.sh` now checks the two names agree**, in both
  directions: every target the installer downloads must be one the release
  matrix builds, and the release must publish a runnable binary under the name
  the installer asks for. This is the check that would have caught the above.
- **The release gate checked one manifest out of two.** It compared
  `core/Cargo.toml` against `.release-version` and never looked at
  `cli/Cargo.toml` or at the `vayucell-core = { version = "…" }` pin between
  them — so a version bump left the CLI behind, pinning a version of the core
  that no longer existed. That is a release which fails at dependency
  resolution *after* the tag is public. Manifests are now discovered rather than
  listed, so a crate added later is not exempt by never having been named.
- **Two self-test plants had gone stale by hardcoding `0.0.1`.** They were
  scored `STALE` and `MISSED` on the first release the project ever cut — the
  moment they mattered most. Both are now version-agnostic. The harness caught
  this itself; that is what the fingerprint check is for.

### Added

- The installer now resolves a full Rust target triple rather than a bare
  processor name, because the triple is the string the release names its
  artefacts with and a friendly name is one translation step where drift hides.
- **A one-command installer for a phone** (`install.sh`) and
  [`docs/INSTALL.md`](docs/INSTALL.md), written for somebody who has never
  opened a terminal. It names the battery risk and waits for an explicit `yes`
  **before writing anything**, installs what is missing, verifies the checksum
  of a published build or falls back to building from source, and refuses to
  claim success until the program it installed has actually run. Every failure
  path says what to do next rather than printing an error code. It never asks
  for root and writes nothing outside `~/.vayucell`, so removing it is one
  `rm -rf`. The guide states plainly that no release has been installed on a
  physical phone, that `UNSAFE` is the expected and correct verdict on an
  ordinary handset, and that hosting a website or storing files is not built
  yet — the safety layer had to come first.
- **An install gate** (`scripts/install-gate.sh`), because `install.sh` is the
  only file here that runs on a stranger's device and the only one the test
  suite cannot reach — which made it the least-tested and most exposed file in
  the repository. It requires every failure path to name both what happened and
  what to do, requires the battery warning to precede the first write to disk,
  requires the physical-inspection instruction to be present, refuses an
  installer that escalates privileges, and installs from a clean `HOME` twice
  over, running the result. It prints that Termux itself is not exercised
  rather than letting green ticks imply a device was involved. Four plants in
  the gate self-test prove it fires; the count there is now 52.

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
- **The fuzzer found a real bug within seconds of first running.**
  `charge_full * 100` overflowed `i64` in the state-of-health calculation —
  `charge_full` is whatever a vendor kernel wrote into the node, parsed with no
  upper bound, so a device reporting nonsense there panicked under debug
  assertions and silently wrapped to a negative without them. It sat inside the
  reading the governor uses to decide whether to keep charging a cell. Now a
  `checked_mul` whose overflow reports `Unknown`, because a capacity that cannot
  be scaled is unverified rather than a number.
- *Maintainers only:* the fuzz target for the request line asserted that an
  accepted path contained no `..` anywhere, and the fuzzer produced `/a..b`
  within seconds — an ordinary filename. The oracle was wrong, not the parser,
  and it now checks per segment, which is what the parser actually guarantees.
  An over-strict fuzz oracle costs exactly as much attention as a real bug.
- **The schema validator is pinned by hash, not just by name.** Three workflows
  ran `pip install jsonschema`, which resolves to whatever the index serves at
  that moment — the same moving-reference problem the action tags had, in the
  job that decides whether a device profile is valid. `requirements/schema.txt`
  now pins every package and every published artefact hash, installed with
  `--require-hashes` so pip refuses when anything in the resolved set lacks one.
  All 116 of rpds-py's per-platform wheels are listed, because a hash set
  covering only the machine that generated it fails on every other runner.
- **`SECURITY.md` says how to report and what to expect back.** It described two
  kinds of defect and never named a route or a timeframe. It now points at
  private vulnerability reporting and commits to acknowledgement in 7 days,
  assessment in 14, and a fix or a stated refusal within 90 — with an escalation
  path if those pass in silence, because a disclosure process nobody answers is
  worse than none: it persuades a reporter to stay quiet.
- **Every GitHub Action is pinned to a commit SHA.** Sixty-three references
  across the workflows were tags — and a tag is whatever its owner repoints it
  at tomorrow, with no diff appearing in this repository. That is the
  supply-chain attack a project asking people to run a binary unattended in
  their home has no other defence against. The actions gate now requires the
  pin rather than merely resolving the reference, and requires the commit to be
  fetchable, because a typo in a SHA looks exactly like a legitimate one.
- **The auto-merge workflow no longer grants write at the top level.** It runs
  on `pull_request`, where the branch is proposed by whoever opened it, and a
  workflow-wide `contents: write` hands that token to every job the file will
  ever gain — including one added later by somebody who did not read the header.
  Now `permissions: {}` at the top and the two scopes it needs on the one job
  that needs them.
- **Static analysis by a tool that did not write this code.** A CodeQL workflow
  on push, on pull request, and weekly — because new queries ship after the code
  does, so a repository that only scans on push stops learning the day the last
  commit lands. Everything already here was written by somebody who believed the
  code was correct, which is precisely the belief a second analyser does not
  share.
- **Fuzzing, on the three places a string this project did not write becomes a
  decision it acts on:** the HTTP request line, a battery reading from a vendor
  kernel, and a CSP nonce. The harness carries `libfuzzer-sys` and is therefore
  excluded from the workspace, so nothing it touches can reach the binary — and
  the charter gate checks that exclusion rather than trusting the comment
  explaining it, because an exemption nobody verifies is a hole.
- **A local-only listener** — the first thing in this project a browser has
  ever spoken to, which makes the CSP and the response headers real rather than
  rendered into a snapshot. `vayucell serve` binds loopback by default; reaching
  the rest of your network is a flag you type. Every response carries the full
  posture including the errors, because a 404 without a CSP is still a page a
  browser will execute script in. The nonce is minted per response and consumed
  by the render. Traversal is refused rather than normalised, and
  percent-encoding refused rather than decoded, because both bypasses work
  precisely when the check runs against a different string from the one that
  arrived. Parsing and routing own no socket, so a malformed request is a unit
  test.
- *Maintainers only:* the first version of the nonce minter called
  `std::fs::read("/dev/urandom")`. That device has no end, so the read never
  returned — it allocated until the process was killed, and the listener died on
  its first request. Replaced with a `read_exact` into a fixed buffer. The good
  outcome was that it failed immediately and visibly; the same mistake against a
  file that is merely large would have shipped as a slow leak.
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
