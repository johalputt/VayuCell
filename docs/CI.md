<!-- SPDX-License-Identifier: Apache-2.0 -->

# Continuous integration

This document describes every gate that runs against this repository, what each
one is actually checking, and every parameter it is checking with.

## The idea

[`CHARTER.md`](../CHARTER.md) is not a statement of intent. It is a set of
constraints the project may not violate — no capability that serves traffic
before the battery governor, no indicator that means anything less than
*verified*, no telemetry, no remote control path the owner cannot sever. A
constraint that only a reviewer checks is a constraint that erodes on a busy
week.

So the constraints are enforced by a machine, and the machines are themselves
tested. That second part is the unusual one and it is not decoration. A gate
whose pattern silently stops matching goes on printing `ok` forever and has no
other symptom. Both failure directions were hit while these gates were written:

- The first Article III.1 check matched the bare variant `Class::Serving` and so
  flagged `capability.rs` for *defining* the enum. It failed loudly, which looked
  like working, and was wrong.
- The first hardware honesty check read `battery.charge_limit.verified` — a field
  the schema does not have. It printed `ok` on every run while checking nothing.

The second kind is the dangerous one. [`scripts/gate-selftest.sh`](../scripts/gate-selftest.sh)
is how it gets caught: it plants each violation in a scratch copy of the
repository and requires the matching gate to fail, citing the right rule.

## Where the logic lives

No gate's logic is written inline in a workflow file. Every check is a script in
[`scripts/`](../scripts/), so the identical check runs on a laptop before the
push. The workflows only decide *when* things run and *with what strictness*.

```bash
# The full local gate, in the order CI runs it.
bash scripts/charter-gate.sh          # the constitution, enforced
bash scripts/attribution-gate.sh      # the permanent record
bash scripts/docs-gate.sh             # required docs, ADR integrity, links
bash scripts/hardware-gate.sh         # device database schema and honesty
bash scripts/gate-selftest.sh         # the gates above actually fire
bash scripts/mutation-gate.sh         # the tests would notice if code were wrong

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

`scripts/hardware-gate.sh` and `scripts/gate-selftest.sh` need one Python
package:

```bash
python3 -m pip install jsonschema
```

---

## Workflow: `ci.yml` — the required gate

Runs on every push to `main` and every pull request. Nothing merges without all
fourteen jobs green.

| Setting | Value | Why |
|---|---|---|
| `concurrency.group` | `<workflow>-<ref>` | One run per branch; rapid pushes supersede each other |
| `concurrency.cancel-in-progress` | `false` on `main`, `true` elsewhere | `main` always carries a *completed* verdict. A cancelled run reads as unknown, and unknown must never look like pass |
| `permissions` | `contents: read` | No job needs write. A compromised action cannot push |
| `RUSTFLAGS` / `RUSTDOCFLAGS` | `-D warnings` | A warning allowed to accumulate is a warning nobody reads |
| `CARGO_INCREMENTAL` | `0` | Incremental artefacts differ between runs and would defeat the reproducibility job |
| `CARGO_NET_RETRY` | `3` | Fail on a real outage, not on one dropped packet |
| `timeout-minutes` | per job, 5–20 | A hung job that runs to the six-hour default is a job nobody gets a verdict from |

### `charter` — Articles III–IX enforced

Runs [`scripts/charter-gate.sh`](../scripts/charter-gate.sh), then
[`scripts/gate-selftest.sh`](../scripts/gate-selftest.sh).

| Article | Checked how |
|---|---|
| **III.1** No traffic-serving capability before the governor | Any production source registering `class: Class::Serving` requires `core/src/governor.rs` to exist |
| **III.3** Never imply a safety property not read back | `Capability::verify` must remain a non-optional `VerifyFn`. As an `Option` a control with no read-back would compile |
| **III.3** The proof is real | `compile_fail` doctests must still be present on the **public** module — rustdoc collects them nowhere else, and on a private item they run zero tests and report success |
| **IV.1** No generic success variant | `Result_` may not gain `Ok`, `Pass`, `Clean`, `Fine` or `Good`; any of them would absorb "not checked" |
| **IV.2 / IV.3** Absent is never unverified | `Result_` must keep both `Absent` and `Unverified` as distinct variants |
| **IV.3** No default tier | `tier.rs` must keep its `Unknown` verdict rather than assuming T0 |
| **V.1 / V.2 / V.3** | Thirteen forbidden identifiers (`treasury`, `airdrop`, `subscription`, `billing`, `license_key`, `telemetry`, `phone_home`, `beacon_url`, `device_fingerprint`, `remote_command`, `remote_wipe`, `kill_switch`, …) may not appear in production source. Only code is scanned — prose is allowed to name what it forbids |
| **V.5** No dependency on a service we control | `core/Cargo.toml` must declare zero runtime dependencies, and no production source may reference a project-operated host |
| **VI** Licensing | `LICENSE` is Apache-2.0, `LICENSE-CHARTER` is CC0, and every `.rs` and `.sh` file carries an SPDX header in its first three lines |
| **VII** Governance | No contributor licence agreement may appear; `CONTRIBUTING.md` must document the DCO |
| **IX** Articles III and V | SHA-256 of each article is recorded in `.charter-digests`. An edit fails the build until it is re-recorded with `scripts/charter-gate.sh --record`, which puts the amendment in the diff where review will see it |

The gate also **prints what it cannot check** — III.2, III.4, IV.4 and V.4 are
human review — rather than omitting them. A gate list that appears complete
teaches its reader to stop looking for the gaps.

### `attribution` — the permanent record

Checks tracked files and commit messages for assistant attribution, and refuses
commits authored by a bot or `noreply` address. `fetch-depth: 0`, because a
shallow clone has no commit messages to inspect and would pass by checking
nothing. On a pull request only the branch's own commits are judged, so existing
history cannot fail somebody else's change.

### `docs` — required set, ADR integrity, links

- The twelve required documents exist **and are non-empty** — an emptied file is
  as broken as a deleted one, and easier to miss.
- Every ADR filename matches `ADR-NNNN-kebab-slug.md`, and its title names the
  **same number** as its filename. A mismatch sends a reader following a
  cross-reference to the wrong decision.
- ADR numbering is contiguous from 0001. A gap means an ADR was deleted rather
  than superseded, and a superseded decision has to stay readable.
- Every relative Markdown link in the repository resolves. External links are not
  followed: a gate that reaches the network makes the build depend on somebody
  else's uptime.
- No orphan ADRs — every ADR is referenced from outside itself.

Then `markdownlint-cli2` with the rules in
[`.markdownlint-cli2.jsonc`](../.markdownlint-cli2.jsonc).

### `shell`

`shellcheck --severity=warning` over every script. The gates decide whether a
release ships, so a quoting bug in one of them is a correctness bug in the
release process. Also asserts every script is executable and declares
`#!/usr/bin/env bash`.

### `hardware` — device database

Validates `hardware/schema.json` against its metaschema and every profile in
`hardware/devices/` against the schema, then applies four honesty rules the
schema cannot express:

- A ceiling with `verified_hold: true` must name the `node_path` it was read back
  from. An unreproducible safety claim is exactly what this project refuses to
  print.
- `available: false` may not coexist with a named mechanism, and `available: true`
  may not coexist with `mechanism: "none"`.
- `verified_hold: true` is refused where `available` is not `true`.
- A tier recorded as `present` must record how, and storage must state a
  `durability_class` rather than defaulting by omission.

`VAYUCELL_REQUIRE_SCHEMA_VALIDATOR=1` is set here. Locally, a missing
`jsonschema` makes the gate print `UNVERIFIED` and carry on; in the authoritative
run it is a hard failure. Article IV binds our own toolchain exactly as it binds
a device report — **a check that did not run may not be displayed as one that
passed.**

### `rust`

| Step | Parameters | Why |
|---|---|---|
| `cargo fmt` | `--all -- --check` | Formatting is not reviewed by humans here |
| `cargo clippy` | `--workspace --all-targets --all-features -- -D warnings` | `--all-targets` covers tests: a lint that skips test code lets it drift into habits the library forbids. `clippy::pedantic` is on via `lib.rs` |
| `cargo build` | `--workspace --all-features` | |
| `cargo test` | `--workspace --all-features` | |
| doctest count | asserts **more than zero** doctests ran | The registry's strongest guarantees are `compile_fail` doctests. Moved onto a private item they run zero tests and still print `test result: ok`. The exit code cannot distinguish that from success; the count can |
| `cargo doc` | `--no-deps`, `RUSTDOCFLAGS=-D warnings` | Broken intra-doc links fail the build |
| unsafe | greps for both `#![forbid(unsafe_code)]` and `unsafe_code = "deny"` | Either alone can be dropped in a diff that looks unrelated |

### `msrv`

Reads `rust-version` from `core/Cargo.toml` and builds and tests on exactly that
toolchain. Declaring an MSRV and never building against it is a claim nobody
verified. `RUSTFLAGS` is cleared for this job: an older compiler emits different
lints, and failing on a lint that did not exist yet says nothing about whether
the code compiles there.

### `mutation`

Runs [`scripts/mutation-gate.sh`](../scripts/mutation-gate.sh). Ten guards, each
re-broken in turn, each required to turn its matching test red:

| Mutation | Test that must fail |
|---|---|
| A bare VM promoted to T2 without the shell's assertion | `a_guest_that_cannot_see_the_phone_reports_unverified_rather_than_guessing` |
| An unrecognised machine falling back to T0 | `a_machine_with_no_recognised_evidence_is_unknown_not_t0` |
| An unreadable device tree reported as absent hardware | `an_unreadable_device_tree_makes_the_verdict_unverified_not_unknown` |
| Any value of the assertion variable believed | `an_unrecognised_shell_assertion_is_refused_rather_than_believed` |
| An unprivileged handset granted the rooted tier | `stock_android_without_root_is_t0` |
| `Unverified` leaking a tier through `Verdict::tier()` | `a_guest_that_cannot_see_the_phone_…` |
| The device tree consulted before Android userspace | `android_outranks_the_device_tree_so_a_rooted_handset_is_t1_not_t3` |
| A silent `/proc/self/status` reading as root | `a_status_file_that_will_not_say_who_we_are_is_never_root` |
| The not-root sentinel becoming `0` | as above |
| The real uid read instead of the effective one | `the_effective_uid_is_read_not_the_real_one` |

Two harness defects were found while building this, both the same class as the
bug the harness exists to catch, and both are now guarded:

- Mutations are applied by exact-string replacement with the **match count
  asserted**. A mutation that silently fails to apply is otherwise reported as
  "survived" — a false green.
- The restore **stamps a fresh mtime**. `cp -a` preserved the snapshot's older
  timestamps, cargo fingerprints on mtime, and it was skipping the rebuild and
  re-running the mutant against restored source. The gate now also verifies the
  suite is green both before the first mutation and after the last.

### `coverage`

`cargo llvm-cov --fail-under-lines 80`, with test files excluded from the
measurement via `--ignore-filename-regex '_test\.rs$'`.

Counting the test files inflates the figure with the coverage of the tests
themselves — here it is the difference between 87.10% and the **81.77%** that
describes the actual code. A number that flatters itself is not worth having.

| File | Lines covered |
|---|---|
| `tier.rs` | 91.89% |
| `host.rs` | 84.81% |
| `capability.rs` | 68.89% |
| **Production total** | **81.77%** |

The floor is 80 against that measured 81.77%. The headroom is deliberately small:
a floor set far below the real figure accepts a large regression in silence.

Treated as a floor that catches whole modules landing untested, not a target to
optimise. A percentage says how much code ran, not whether anything was checked
while it ran — the `mutation` job is the one that answers that.

### `deps` — supply chain

`cargo deny check advisories bans licenses sources` against
[`deny.toml`](../deny.toml), plus `cargo machete` for unused declarations and
`cargo metadata --locked` to prove the lockfile does not drift.

`deny.toml` is strict while the dependency tree is empty on purpose, so the first
crate anyone proposes argues against rules written before there was pressure to
relax them: `multiple-versions = "deny"`, `wildcards = "deny"`,
`build-script = "deny"`, `unknown-git = "deny"`, no advisory exceptions,
crates.io as the only permitted source.

### `targets` — the devices this actually runs on

| Target | Why it is in the matrix |
|---|---|
| `aarch64-linux-android` | T0/T1 — stock and rooted 64-bit handsets |
| `armv7-linux-androideabi` | T0/T1 — 32-bit handsets, exactly the drawer phones this is aimed at |
| `aarch64-unknown-linux-gnu` | T2/T3 — the Android Terminal guest, and mainline ports |
| `armv7-unknown-linux-gnueabihf` | T3 — 32-bit mainline |
| `x86_64-unknown-linux-gnu` | Not a device; where contributors work |

`fail-fast: false`, so one broken target does not hide the state of the other
four. Currently `cargo check` rather than `cargo build`: the core has no
dependencies and no linked runtime yet, so type-checking is what there is to
verify. This becomes a cross-linked build when the governor lands with real
syscalls.

### `secrets`

TruffleHog over the working tree and full history, `--only-verified --fail`. Only
the image *pull* is retried; the scan runs exactly once and its exit code is
authoritative, so a retry can never mask a real detection by re-running until it
passes.

### `reproducible`

Builds twice from a clean tree and compares SHA-256 of the artefacts, with
`--remap-path-prefix` so absolute build paths do not leak into debug info.

This project asks people to run software unattended on hardware in the building
where they sleep. Determinism is what makes "you can check for yourself" a real
offer rather than a slogan: a binary that differs between two builds of the same
source cannot be independently verified by the person installing it.

### `ci-pass`

The single required status check. Its result list is generated from the whole
`needs` context rather than a hand-maintained list — a job added above but
forgotten here would otherwise be required in name and unenforced in fact.

---

## Workflow: `scheduled.yml`

The checks whose answer changes without the code changing. An advisory published
on Tuesday makes Monday's green build wrong retroactively, and nothing in the
repository moved.

Deliberately **not** part of the required gate: a newly-published advisory in a
dependency should page a maintainer, not block an unrelated documentation fix at
two in the morning.

Monday 05:00 UTC — a working morning, not a weekend, so a red result is seen the
same day.

| Job | What it answers |
|---|---|
| `advisories` | `cargo deny check advisories` and `cargo audit --deny warnings` |
| `freshness` | `cargo outdated --root-deps-only --exit-code 1`. Should stay empty by charter; exists to go red the week that stops being true |
| `beta-and-nightly` | Advisory, `continue-on-error`. A lint added to nightly is information about work coming, not a defect in code that is correct today |
| `miri` | The crate forbids unsafe, so this should find nothing — which is the point. It is what makes the `forbid` a verified property rather than a comment |
| `fuzz` | No targets yet. They are due with the governor: the sysfs and `/proc` parsers read attacker-adjacent text from a vendor kernel, the first input in this project not written by us. The job says so out loud rather than passing silently |
| `gate-selftest` | Repeated on a schedule because this is the check with no natural symptom when it rots |

---

## Known gaps

Stated here rather than left for a reader to discover, on the same principle the
gates themselves apply.

1. **Actions are pinned to tags, not digests.** `actions/checkout@v4` is a moving
   reference. Pinning to a commit digest is strictly stronger and is the next
   hardening step; Dependabot's `github-actions` ecosystem is already configured
   to keep them current in the meantime.
2. **Coverage has a floor, not a ratchet.** Nothing stops coverage falling from
   95 percent to 81.
3. **No end-to-end test on real hardware.** Every device-facing behaviour is
   exercised through `FakeHost`, which describes handsets nobody here is holding.
   That is the right layer for a unit suite and it is not a substitute for a
   phone on a bench. [ADR-0002](adr/ADR-0002-battery-safety-governor.md) defines
   the physical test gates; they are not automatable in CI and will not be
   claimed as such.
4. **Four charter articles are human review only** — III.2, III.4, IV.4 and V.4.
   The gate prints them on every run.
