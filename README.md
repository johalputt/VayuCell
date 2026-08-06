<!-- markdownlint-disable MD041 -->
<!-- The mark leads the README by convention and the H1 follows it, so
     the first-line-heading rule is off for this file only. Every other
     document still has to start with its title, and scripts/docs-gate.sh
     enforces the stronger form on the ADRs. -->
<p align="center">
  <img src="docs/assets/vayucell-logo-transparent.png#gh-light-mode-only" alt="VayuCell" width="380">
  <img src="docs/assets/vayucell-logo-transparent-dark.png#gh-dark-mode-only" alt="VayuCell" width="380">
</p>

# VayuCell

[![CI](https://github.com/johalputt/VayuCell/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/johalputt/VayuCell/actions/workflows/ci.yml)
[![Supply chain](https://github.com/johalputt/VayuCell/actions/workflows/supply-chain.yml/badge.svg?branch=main)](https://github.com/johalputt/VayuCell/actions/workflows/supply-chain.yml)
[![Scheduled](https://github.com/johalputt/VayuCell/actions/workflows/scheduled.yml/badge.svg)](https://github.com/johalputt/VayuCell/actions/workflows/scheduled.yml)
[![Code: Apache-2.0](https://img.shields.io/badge/code-Apache--2.0-blue.svg)](LICENSE)
[![Charter: CC0-1.0](https://img.shields.io/badge/charter-CC0--1.0-lightgrey.svg)](LICENSE-CHARTER)
[![unsafe: forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](core/src/lib.rs)
[![deps: zero](https://img.shields.io/badge/runtime%20deps-zero-success.svg)](deny.toml)

**Turn a retired phone into a server you own.**

A five-year-old flagship has eight 64-bit cores, several gigabytes of RAM, fast
onboard storage, Wi-Fi *and* a cellular modem, an integrated battery that works
as an uninterruptible power supply — and it idles at one to three watts. Billions
of them are in drawers.

VayuCell turns one into a server that hosts your website, your mail, your files
and your backups. It is free, open source, and **designed so that this project
disappearing would not stop your device working**.

## Status

**Early, and honest about it.**

| | |
| --- | --- |
| Written | The capability registry, tier detection, the CSP and response security headers, and **the battery safety governor** — state machine, verification loop, thresholds, recovery |
| Not written | The sysfs mechanisms the governor drives, the sampling loop, the outage shed ladder, the panel |
| Unblocked | [Charter III.1](CHARTER.md) forbade anything serving traffic before the governor. The governor exists, so that constraint is now satisfied — and the gate still fails the build if it is ever removed |
| Never tested on hardware | Everything. Every device-facing behaviour is exercised through a fake host describing handsets nobody here is holding |

That last row is a permanent one. It stops being true the day somebody puts a
phone on a bench, and not before.

## What it costs you to try

Nothing, and it is designed to keep costing nothing:

- **No account.** There is nothing to sign up for.
- **No telemetry.** Not aggregate-but-identifying, not "anonymous". None.
  Enforced by a gate that scans for the concept, not just the word.
- **No token, no treasury, no fee, no hosted tier.** There is one edition.
- **No dependency on us.** If this project vanished tomorrow, an installed cell
  keeps working. That is [Charter Article V.5](CHARTER.md), and it is the test
  the whole charter is built around.

The core carries **zero third-party runtime dependencies**, and CI fails if the
published bill of materials ever contains one. You do not have to take that on
trust — the SBOM ships with every release.

| Document | What it is |
| --- | --- |
| [`CHARTER.md`](CHARTER.md) | The supreme law. CC0. Read this first |
| [`GOVERNANCE-CONSTITUTION.md`](GOVERNANCE-CONSTITUTION.md) | How the charter is upheld in practice — 110 rules, each marked with whether a machine, a human, or nothing enforces it, and each `[CI]` rule naming the file that enforces it |
| [`PLAN.md`](PLAN.md) | The full project plan |
| [`ADR-0001`](docs/adr/ADR-0001-tier-model-and-capability-registry.md) | Tier model and capability registry |
| [`ADR-0002`](docs/adr/ADR-0002-battery-safety-governor.md) | The Battery Safety Governor |
| [`ADR-0003`](docs/adr/ADR-0003-sovereign-ingress.md) | Reaching a server that has no address |
| [`ADR-0004`](docs/adr/ADR-0004-storage-durability.md) | Storage durability: assume the flash lies |
| [`ADR-0005`](docs/adr/ADR-0005-implementation-language.md) | Rust for the core, Kotlin for the shell |
| [`ADR-0006`](docs/adr/ADR-0006-content-security-policy.md) | Content Security Policy: the unsafe keywords made unrepresentable |
| [`docs/CI.md`](docs/CI.md) | Every gate, and every parameter it checks with |
| [`docs/BRAND.md`](docs/BRAND.md) | The mark: how it is constructed, and the rules for using it |
| [`CHANGELOG.md`](CHANGELOG.md) | What changed, and what it means for someone running this |
| [`hardware/`](hardware/) | Device compatibility database (CC0) |

## Read this before you plug anything in

VayuCell asks you to leave a lithium battery energised and warm, for years, in a
building where you sleep. That is the condition under which cells age fastest, and
a swollen cell is a fire hazard.

**Not every phone can limit its own charging.** On an unrooted stock phone, none
can. VayuCell will tell you which case yours is, on the first screen, before you
rely on it — and the safety row for a device that cannot limit charge stays red
forever, because it is.

**Put your phone face-down on a flat table now and then.** If it rocks, or the
screen or back is lifting at any edge, stop using it and take it to
hazardous-waste handling. Software cannot see that. You can.

## What it will never claim

1. That every phone can be a server.
2. That the battery is safe — risk is governed, never eliminated.
3. That an abandoned vendor kernel is secure.
4. That one phone is datacentre reliability.
5. That a rented relay is independence.

## How this is checked

The charter is not a statement of intent — it is a set of constraints, and a
constraint that only a reviewer checks is one that erodes on a busy week. So the
constraints are enforced by a machine, and **the machines are themselves tested**:
every gate is re-broken in a scratch copy of the repository and required to
notice, because a check whose pattern silently stops matching prints `ok` forever
and has no other symptom.

| Gate | What it refuses to let through |
| --- | --- |
| **Charter** | A serving capability before the governor. A control with no read-back. `Absent` collapsing into `Unverified`. Telemetry, a treasury, a kill switch, a dependency on a host this project runs. An edit to Article III or V that was not recorded as an amendment |
| **Security** | A CSP that permits `'unsafe-inline'` — the type has no variant for it, so weakening it is an addition to a public enum rather than a one-word edit to a string. A reusable nonce. A referrer policy that leaks cross-origin. A report-only header on a release. An HSTS max-age too short to mean anything |
| **Provenance** | A version that disagrees with itself, a tag that does not match the tree, a release with no signature, an SBOM containing a dependency the charter says does not exist, or a workflow referencing an action tag that was never published |
| **Mutation** | Forty safety and honesty guards, each re-broken, each required to turn its test red. A green suite proves the code passes its tests; this proves the tests would notice if the code were wrong |
| **Gate self-test** | Forty-two planted violations that the gates above must each catch, citing the right rule |
| **Constitution** | A rule claiming `[CI]` while naming no enforcer, or naming one that has been deleted. The document may not claim enforcement it does not have |
| **Rust** | `cargo fmt`, `clippy` pedantic at `-D warnings`, build, test, and an assertion that the `compile_fail` doctests actually ran — on a private item they run zero tests and still report success |
| **Hardware** | A device profile that fails its schema, or claims a verified charge ceiling without naming the sysfs node it was read back from |
| **Targets** | The core failing to compile for 64- and 32-bit Android, mainline ARM, or a contributor's laptop |
| **Reproducible** | A release binary that is not byte-identical when rebuilt. You are asked to run this unattended in the building where you sleep; "check for yourself" has to be a real offer |
| **Supply chain** | An advisory, a wildcard version, a build script, a git dependency, an unused dependency, a drifting lockfile, a verified secret anywhere in history |
| **Docs** | A dead link into the founding documents, an ADR whose title and filename name different decisions, a gap in the decision log, **or a constitution that overstates how many of its own rules a machine actually enforces** |

Every gate is a script in [`scripts/`](scripts/), never inline YAML, so the
identical check runs on your laptop before you push. [`docs/CI.md`](docs/CI.md)
documents each job, each parameter, and the four charter articles that **cannot**
be checked mechanically — printed on every run rather than quietly dropped.

One command runs everything CI runs, in the order CI runs it:

```bash
scripts/local-ci.sh            # every gate
scripts/local-ci.sh --fast     # skip mutation, coverage and the self-test
scripts/local-ci.sh --list     # show what would run, and stop
```

It prints nothing when a gate passes, which is what makes it a thing you will
actually run before pushing rather than a thing you mean to.

Individually:

```bash
scripts/charter-gate.sh        # the charter, enforced
scripts/constitution-gate.sh   # every [CI] rule names an enforcer that exists
scripts/gate-selftest.sh       # ...and proof the gates actually enforce
scripts/mutation-gate.sh       # proof the tests would catch a regression
scripts/actions-gate.sh        # every workflow reference resolves
scripts/release-gate.sh        # the version says the same thing everywhere
cargo test --workspace
```

### The security posture is a file you can read

[`docs/security-posture.txt`](docs/security-posture.txt) is the exact header set
every response carries, committed and checked by a test. It exists because the
individual guards are not enough on their own: a change that weakens the posture
while keeping each assertion true reads, in a diff, as a small edit to a Rust
file, and nobody reviewing it sees the headers change.

With the snapshot committed, weakening anything produces a diff in a plain text
file — something a reviewer notices without knowing the codebase at all.

## Read this before you plug anything in — the short version

The long version is two sections up and you should read it, but if you read
nothing else:

**Put your phone face-down on a flat table now and then.** If it rocks, or the
screen or back is lifting at any edge, stop using it and take it to
hazardous-waste handling. Software cannot see that. You can.

## Contributing

The one thing worth knowing before you open a pull request: **a change that adds
behaviour has to name the test that would fail if the behaviour were wrong**, or
say why one is not possible. The template asks for it. It is the only unusual
demand in [`CONTRIBUTING.md`](CONTRIBUTING.md), and it is the reason the rest of
the gates are worth anything.

Device reports are the most useful thing most people can contribute, and they
need no code — see the
[device report template](.github/ISSUE_TEMPLATE/device-report.yml). Record what
you **observed**. A field you did not test is left empty; an empty field is
honest, and a guessed one is worse than nothing because somebody will trust it
with their mail.

## Licence

| Artefact | Licence |
| --- | --- |
| Charter and specifications | CC0-1.0 |
| Source code | Apache-2.0 |
| Hardware database | CC0-1.0 |
| Documentation | CC-BY-4.0 |

Apache-2.0 rather than MIT is deliberate: this project touches charging circuits,
power management and virtualisation — patent-dense territory — and Apache-2.0
carries an express patent grant that MIT does not. See `CHARTER.md` Article VI.

No token. No treasury. No fee. No account.
