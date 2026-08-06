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
is how it gets caught: it plants **forty violations** in a scratch copy of the
repository and requires the matching gate to fail, citing the right rule. It also
refuses to score a plant that changed nothing — a `sed` whose pattern has gone
stale edits nothing, the gate then passes for the honest reason, and a working
gate gets recorded as broken. That happened, so the harness now fingerprints the
sandbox before and after.

## Where the logic lives

No gate's logic is written inline in a workflow file. Every check is a script in
[`scripts/`](../scripts/), so the identical check runs on a laptop before the
push. The workflows only decide *when* things run and *with what strictness*.

```bash
scripts/local-ci.sh            # everything below, in the order CI runs it
scripts/local-ci.sh --fast     # skip mutation, coverage and the self-test
scripts/local-ci.sh --list     # show what would run, and stop
```

`local-ci.sh` prints nothing when a gate passes and the captured output only when
one fails. That is deliberate: a gate that is quiet on success is a gate people
run before pushing rather than one they mean to.

Individually:

```bash
scripts/charter-gate.sh          # the constitution, enforced
scripts/attribution-gate.sh      # the permanent record
scripts/docs-gate.sh             # required docs, ADR integrity, links, rule counts
scripts/constitution-gate.sh     # every [CI] rule names an enforcer that exists
scripts/hardware-gate.sh         # device database schema and honesty
scripts/release-gate.sh          # the version says the same thing everywhere
scripts/actions-gate.sh          # every workflow reference actually resolves
scripts/markdown-gate.sh         # markdown lint, at the one pinned version
scripts/doctest-count.sh         # a non-zero number of doctests actually ran
scripts/coverage.sh              # production-only line coverage against the floor
scripts/sbom.sh                  # CycloneDX bill of materials
scripts/gate-selftest.sh         # the gates above actually fire
scripts/mutation-gate.sh         # the tests would notice if the code were wrong
```

`scripts/hardware-gate.sh` and `scripts/gate-selftest.sh` need one Python
package:

```bash
python3 -m pip install jsonschema
```

---

## Workflow: `ci.yml` — the required gate

Runs on every push to `main` and every pull request. Nothing merges without all
sixteen jobs green.

The `docs` job carries two gates: the documentation gate and the constitution
gate below.

| Setting | Value | Why |
| --- | --- | --- |
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
| --- | --- |
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

- The required documents exist **and are non-empty** — an emptied file is as
  broken as a deleted one, and easier to miss.
- Every ADR filename matches `ADR-NNNN-kebab-slug.md`, and its title names the
  **same number** as its filename. A mismatch sends a reader following a
  cross-reference to the wrong decision.
- ADR numbering is contiguous from 0001. A gap means an ADR was deleted rather
  than superseded, and a superseded decision has to stay readable.
- Every relative Markdown link in the repository resolves. External links are not
  followed: a gate that reaches the network makes the build depend on somebody
  else's uptime.
- No orphan ADRs — every ADR is referenced from outside itself.
- **The constitution's enforcement table is checked against the document.**
  `GOVERNANCE-CONSTITUTION.md` classifies every rule as `[CI]`, `[REVIEW]` or
  `[NORM]`, and Appendix A totals them. Those totals were wrong in the first
  draft — off by six — and nothing would ever have noticed. A governance document
  that overstates how much of itself a machine actually enforces commits the
  error Article 4 forbids in a device report, against the reader of the
  governance instead. Adding a rule without updating the table now fails the
  build.

Then [`scripts/markdown-gate.sh`](../scripts/markdown-gate.sh) with the rules in
[`.markdownlint-cli2.jsonc`](../.markdownlint-cli2.jsonc).

**The linter runs from a script, not from the action, and this is the second
lesson of the same kind.** CI originally used
`DavidAnson/markdownlint-cli2-action@v24`, which bundles markdownlint v0.41,
while the local check ran `markdownlint-cli2@0.18.1`, which bundles v0.38. v0.41
added `MD060`, so the local gate printed **0 errors** and the push failed on 244
violations of a rule the laptop had never heard of.

That is worse than an ordinary red build: a gate whose local form is *weaker*
than its CI form actively misleads the person running it, and the entire reason
every check lives in `scripts/` is that the two should be the same check. The
version is now pinned in one file and both sides run it.

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
| --- | --- | --- |
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

Runs [`scripts/mutation-gate.sh`](../scripts/mutation-gate.sh). **Forty guards**, each re-broken in turn, each required to turn its matching test red:

| Mutation | Test that must fail |
| --- | --- |
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
| `data:`/`https:` accepted on `script-src` | `a_passive_source_cannot_be_smuggled_onto_an_executable_directive` |
| The CSP report endpoint allowed off-device | `violation_reports_never_leave_the_device` |
| A guessable nonce accepted | `a_weak_nonce_is_refused_rather_than_rendered` |
| A nonce carrying a quote that rewrites the policy | `a_nonce_cannot_carry_a_character_that_escapes_the_directive` |
| The CSP baseline defaulting to `'self'` | `the_baseline_denies_everything_it_was_not_asked_about` |
| `'none'` surviving beside a real source | `allowing_a_source_clears_the_none_that_was_there` |
| Any origin admitted into the policy | `an_origin_outside_the_closed_allowlist_is_refused` |
| `script-src` falling back to `'self'` | `script_may_run_only_with_the_per_response_nonce` |
| The page made framable | `a_page_cannot_be_framed_or_have_its_base_rewritten` |
| **`Source` gains an unsafe variant** | the `compile_fail` doctest. The mutation adds the variant *and* its match arm, so the proof compiles and the doctest must go red |
| A release ships report-only, enforcing nothing | `the_production_set_enforces_rather_than_reports` |
| Content sniffing permitted | `content_sniffing_is_never_permitted` |
| Legacy framing refusal downgraded | `the_page_is_refused_to_framers_by_two_independent_mechanisms` |
| A token HSTS max-age accepted | `a_token_hsts_max_age_is_refused_rather_than_sent` |
| The referrer policy starts leaking | `the_referrer_never_leaks_a_path_to_another_origin` |
| Device permissions fall back to defaults | `device_permissions_are_denied_by_enumeration_not_by_omission` |
| The browsing context stops being isolated | `the_browsing_context_is_isolated` |
| Development pins HTTPS it cannot honour | `development_sends_no_hsts_because_it_cannot_honour_it` |
| **`Referrer` gains a leaking variant** | its `compile_fail` doctest |
| The kernel's decidegrees read as degrees | `the_decidegree_reading_is_not_mistaken_for_degrees` |
| An unmeasurable state of health collapsing to a number | `an_unmeasurable_state_of_health_is_unknown_not_zero` |
| The governor ladder becoming two-way | `a_cooling_cell_does_not_walk_back_down_on_its_own` |
| A reverted charge ceiling accepted as held | `a_ceiling_that_was_quietly_reverted_is_detected` |
| A stricter hardware ceiling misread as a revert | `a_hardware_ceiling_below_what_was_asked_for_is_satisfying_it` |
| An unreadable ceiling treated as working | `a_ceiling_that_cannot_be_read_back_is_unverified_never_working` |
| The hard stop firing above its threshold | `each_temperature_threshold_fires_at_its_own_level` |
| The hard stop configurable upward | `the_hard_stop_may_be_lowered_but_never_raised` |
| An unreachable rung accepted | `an_unordered_ladder_is_refused_rather_than_silently_unreachable` |
| A cell seen deforming allowed to resume | `a_deformed_cell_does_not_recover_whatever_the_sensors_say_next` |
| A spent cell still serving | `a_degraded_cell_derates_and_a_spent_one_stops_serving` |

Two of these survived on their first run, and both were defects in the **tests**
rather than the code:

- `a_hardware_ceiling_below_what_was_asked_for_is_satisfying_it` used a fixture
  whose `apply()` overwrote its own held value, so `verify()` always returned
  exactly what was asked for. The test asserted the right property against a
  mechanism that could not exhibit it.
- `a_cooling_cell_does_not_walk_back_down_on_its_own` only observed readings that
  cross no threshold, so nothing ever *attempted* to lower the level and the
  guard against lowering was never reached.

### The security posture snapshot

[`docs/security-posture.txt`](security-posture.txt) is the exact header set a
production response carries, committed and compared by a test.

Every guard in `csp` and `headers` tests one property in isolation, which is
right and is not sufficient. A change that weakens the posture while keeping each
individual assertion true reads, in a diff, as a small edit to a Rust file —
nobody reviewing it sees the header set change.

With the snapshot committed, weakening anything produces a diff in a plain text
file that states, in order, what every response will carry. That is a thing a
reviewer notices without knowing the codebase.

Regenerating is deliberate and separate:

```bash
cargo test --workspace -- --ignored regenerate_the_security_posture
```

The command is the easy part and it is not the point — the diff is.

Three defects were found while building this, all the same class as the bug the
gates exist to catch, and all are now guarded:

- Mutations are applied by exact-string replacement with the **match count
  asserted**. A mutation that silently fails to apply is otherwise reported as
  "survived" — a false green.
- The restore **stamps a fresh mtime**. `cp -a` preserved the snapshot's older
  timestamps, cargo fingerprints on mtime, and it was skipping the rebuild and
  re-running the mutant against restored source. The gate now also verifies the
  suite is green both before the first mutation and after the last.
- **A CSP test asserted a property that was true for the wrong reason.**
  `allowing_a_source_clears_the_none_that_was_there` passed a single source, so
  it never reached the branch that strips a stale `'none'`, and would have gone
  on passing with the guard deleted. The mutation gate found it; review had not.

### `coverage`

`cargo llvm-cov --fail-under-lines 80`, with test files excluded from the
measurement via `--ignore-filename-regex '_test\.rs$'`.

Counting the test files inflates the figure with the coverage of the tests
themselves — here it is the difference between 87% and the **82.83%** that
describes the actual code. A number that flatters itself is not worth having.

| File | Lines covered |
| --- | --- |
| `tier.rs` | 91.89% |
| `csp.rs` | 85.12% |
| `host.rs` | 84.81% |
| `capability.rs` | 68.89% |
| **Production total** | **82.83%** |

The floor is 80 against that measured 82.83%. The headroom is deliberately small:
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
| --- | --- |
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

### `constitution` — the document may not claim enforcement it does not have

Runs [`scripts/constitution-gate.sh`](../scripts/constitution-gate.sh).

`GOVERNANCE-CONSTITUTION.md` marks 52 rules `[CI]`, and §0.3 says plainly that
`[CI]` means a gate fails the build. That is the load-bearing claim of the whole
document — it is what lets a reader tell which rules are real.

**When this gate was first written, 43 of the 50 `[CI]` rules named nothing at
all.** The count in Appendix A was correct and the claim behind it was
unverifiable: nobody, including the maintainer, could tell whether "a gate fails
the build" was true of any particular rule.

Every `[CI]` rule now ends with `**Enforced by:** \`path\``, and the gate checks
that path exists. A rule whose enforcer is deleted stops the build.

What the gate explicitly does **not** claim is that the cited file genuinely
enforces the sentence attached to it. No script can read a sentence and confirm
that. It prints that limitation on every run rather than letting the tick imply
otherwise.

### `actions` — workflow references resolve

Runs [`scripts/actions-gate.sh`](../scripts/actions-gate.sh), which extracts
every `uses:` reference from the workflows and confirms each resolves to a real
tag or branch with `git ls-remote`.

This gate exists because of a real failure. Two workflows referenced
`google/osv-scanner-action@v2` and `ossf/scorecard-action@v2` — **neither of
which is a tag either project publishes**. They were valid YAML, they parsed
locally, they reviewed fine, and they failed on their first run with *"unable to
find version v2"*.

A pinned version nobody verified is a pinned version, not a verified one. When
the gate was first run it caught a third: `taiki-e/install-action@cargo-outdated`
in the scheduled workflow, which had not run yet and would have failed weeks
later on a Monday morning.

Without network the gate reports `UNVERIFIED` and carries on; CI sets
`VAYUCELL_REQUIRE_NETWORK=1` so the authoritative run cannot silently skip it.

### `release-meta`

Runs [`scripts/release-gate.sh`](../scripts/release-gate.sh) on **every push**,
not only at tag time: `.release-version`, `core/Cargo.toml` and `CHANGELOG.md`
must agree, `.release-version` must carry no trailing newline, nothing may be
stranded under `[Unreleased]`, and the tag must not already exist.

Checking it continuously is the point. A version that has been inconsistent for
three weeks is discovered while trying to ship, and by then the fix is competing
with the release.

---

## Workflow: `release.yml`

Triggered by a `v*.*.*` tag.

| Job | What it does |
| --- | --- |
| `preflight` | The release gate, plus an assertion that the **tag matches `.release-version`**. A tag that does not match the tree is a release whose artefacts cannot be traced to their source |
| `build` | All five targets, `--locked`, with `--remap-path-prefix` so the build directory does not leak into the artefact and break reproducibility |
| `publish` | Keyless **cosign** signature over the checksum file, CycloneDX SBOM, and verification instructions written into the run summary |

One signature over `SHA256SUMS.txt` rather than one per artefact: the checksums
already bind every file, and a verifier has one thing to check instead of a list
they might not finish. The certificate is bound to this workflow — there is no
private key to steal, and nothing to trust that cannot be checked.

---

## Workflow: `supply-chain.yml`

| Job | What it answers |
| --- | --- |
| `sbom` | Generates a CycloneDX SBOM and **fails if it contains a third-party runtime component**. An SBOM nobody asserts anything about is a file, not a check |
| `osv` | Scans `Cargo.lock` against the OSV database — a second opinion alongside cargo-deny, drawing on overlapping but not identical data |
| `scorecard` | OpenSSF Scorecard, on `main` only: it grades repository settings, which a pull request branch cannot change and should not be judged on |

---

## Workflow: `dependabot-automerge.yml`

Auto-merge for **patch and minor** bumps only. Auto-merge does not merge
immediately — GitHub merges once the required checks pass, so the full gate still
runs and nothing broken can land. Majors are left for a person, because a major
is a behaviour change wearing a version number.

The attribution gate exempts `dependabot[bot]` from the human-author rule for
exactly this flow: the accountable act is the review and the merge.

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
| --- | --- |
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
