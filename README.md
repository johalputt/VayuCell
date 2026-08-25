<!-- markdownlint-disable MD041 -->
<!-- The mark leads the README by convention and the H1 follows it, so
     the first-line-heading rule is off for this file only. Every other
     document still has to start with its title, and scripts/docs-gate.sh
     enforces the stronger form on the ADRs. -->
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)"  srcset="docs/assets/vayucell-logo-transparent-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="docs/assets/vayucell-logo-transparent.png">
    <img src="docs/assets/vayucell-logo-transparent.png" alt="VayuCell" width="380">
  </picture>
</p>

<h1 align="center">VayuCell</h1>

<p align="center">
  <strong>Turn a retired phone into a server you own.</strong><br>
  No account. No telemetry. No treasury.<br>
  The battery safety layer was written before anything that could serve traffic.
  Nothing here has run on real hardware, and every claim in this file is scoped
  to that.
</p>

<p align="center">
  <a href="https://github.com/johalputt/VayuCell/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/johalputt/VayuCell/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/johalputt/VayuCell/actions/workflows/supply-chain.yml"><img alt="Supply chain" src="https://github.com/johalputt/VayuCell/actions/workflows/supply-chain.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/johalputt/VayuCell/actions/workflows/scheduled.yml"><img alt="Scheduled" src="https://github.com/johalputt/VayuCell/actions/workflows/scheduled.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="Code: Apache-2.0" src="https://img.shields.io/badge/code-Apache--2.0-blue.svg"></a>
  <a href="LICENSE-CHARTER"><img alt="Charter: CC0-1.0" src="https://img.shields.io/badge/charter-CC0--1.0-lightgrey.svg"></a>
  <a href="core/src/lib.rs"><img alt="unsafe: forbidden" src="https://img.shields.io/badge/unsafe-forbidden-success.svg"></a>
  <a href="deny.toml"><img alt="runtime deps: zero" src="https://img.shields.io/badge/runtime%20deps-zero-success.svg"></a>
  <a href="docs/CI.md"><img alt="coverage: floor 80" src="https://img.shields.io/badge/coverage-gated_%E2%89%A580%25-success"></a>
  <a href="GOVERNANCE-CONSTITUTION.md"><img alt="constitution: 110 rules" src="https://img.shields.io/badge/constitution-110%20rules-blueviolet"></a>
  <a href="scripts/mutation-gate.sh"><img alt="mutations killed: 254/254" src="https://img.shields.io/badge/mutations%20killed-254%2F254-success"></a>
  <a href="scripts/gate-selftest.sh"><img alt="gates self-tested: 57 plants caught" src="https://img.shields.io/badge/gates%20self--tested-57%20plants%20caught-success"></a>
  <a href="CHARTER.md"><img alt="hardware tested: none yet" src="https://img.shields.io/badge/hardware%20tested-none%20yet-inactive"></a>
</p>

---

## About

A five-year-old flagship has eight 64-bit cores, several gigabytes of RAM, fast
onboard storage, Wi-Fi *and* a cellular modem, and an integrated battery that
behaves as an uninterruptible power supply. It idles at one to three watts.
Billions of them are in drawers.

**Vayu** is Sanskrit for *wind* — everywhere, and owned by nobody. VayuCell
applies that to the hardware already in the room.

VayuCell is a Rust core written to decide, from positive evidence, what a handset
can be trusted to do — and to refuse to report anything it could not read back.
It has made that decision about zero real handsets so far. It is not an
installer, a distribution or an app. It is the safety and honesty layer that a
phone-shaped server would have to be built on, written first and deliberately:
[Charter Article III.1](CHARTER.md) forbids shipping any capability that serves
traffic before the battery governor exists.

Nothing in the design depends on this project continuing to exist. That is not a
promise of goodwill; it is [Charter Article V.5](CHARTER.md), and the charter
gate fails the build on a hosted-only capability, a treasury, or a dependency on
a host this project runs.

> *What cannot be checked is reported as unverified, never as clean.*
> — [Charter Article IV.3](CHARTER.md)

---

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
hazardous-waste handling. Software cannot see that. You can. Physical inspection
is the definitive check, it is named as such on every panel this code renders,
and no amount of green rows replaces it.

---

## Status

**The safety layer was written first, on purpose. Nothing else could be.**

| | |
| --- | --- |
| Written | The capability registry, tier detection, the CSP and response security headers, **the battery safety governor** — state machine, verification loop, thresholds, recovery — the sysfs layer it drives, the sampling cadence, the mains-loss shed ladder, **the safety panel**, where a row that could not be checked is not allowed to read as one that was, **a published website**: a directory served to your own network, refused the moment the governor says the cell is in trouble, **file storage**: authenticated upload, download and delete, each device holding a credential that can be revoked on its own — and **onion ingress**: `all --onion-dir` publishes the site and the vault through your system's tor daemon, shed first by the governor, never claimed verified until a request arrives from outside |
| Not written | The fleet view, the hardware database itself, the Android shell, **anything that syncs on its own — storage is a request somebody makes, never a folder that mirrors itself** — and **relay ingress: described and governed in code, but not implemented**. The onion has never served an outside visitor either, because no handset has run this binary; what is written is the supervision, the contract and the honest wording, not a proof of reachability |
| Checked | 610 unit tests and 32 doctests, line coverage held above its 80% floor in CI, **254 mutations each re-broken and each required to turn its named test red**, and **57 violations planted in a scratch repository that the gates must catch citing the right rule — a count pinned to itself, like the mutation gate's**. None of that is evidence about hardware; all of it is evidence about the code |
| Unblocked | [Charter III.1](CHARTER.md) forbids anything serving traffic before the governor. The governor is now **written** — the ordering constraint is met in code, and the gate fails the build if it is ever removed. It is not met on hardware, and nothing serves traffic yet regardless |
| Never tested on hardware | Everything. Every device-facing behaviour is exercised through a fake host describing handsets nobody here is holding |

That last row is a permanent one. It stops being true the day somebody puts a
phone on a bench, and not before.

---

## What is built

Twenty-two modules in [`core/src`](core/src), `#![forbid(unsafe_code)]`,
`clippy::pedantic` at `-D warnings`, and a `[dependencies]` section that is
empty — not small, empty — with a charter gate that fails the build if anything
is ever added to it. Everything below describes code in this repository and
nothing else.

### 🔋 The battery safety governor

A lithium cell held near full, warm, for years is the condition this project asks
for, so the governor is the first subsystem and the charter forbids shipping
anything that serves traffic before it. It is written to write a charge ceiling
and then **read that ceiling back from the sysfs node** — never from a value the process
remembers writing — and splits the outcome three ways: applied, reverted by a
vendor daemon, or unverifiable. **All three derate; none is reported as success**,
because a ceiling nobody re-read is a configuration rather than a control.
Temperature is typed: `DeciCelsius` is what the kernel said, `Celsius` is what a
threshold is written in, and a `compile_fail` doctest proves the two cannot be
compared. Escalation to `DERATED` at **45 °C**, `PROTECT` at **52 °C** and `HALT`
at a hard stop of **60 °C** — configurable downward only — is automatic; state of
health below **80%** derates and below **60%** protects; three consecutive failed
reads, fifteen seconds at the alert cadence, derate on blindness alone. There is
no method that lowers the level, and recovery consumes the governor and demands a
recorded physical inspection, which a human performs and this software cannot.
**No cell has ever been governed by this code. Every write, read-back, revert and
escalation described here has only ever happened against `FakeHost`.**
*([decision →](docs/adr/ADR-0002-battery-safety-governor.md))*

### ⏱️ Sampling cadence and the mains-loss shed ladder

Two pure functions, neither of which owns a clock. The sampler returns an
interval of **30 s** when idle and **5 s** whenever the cell is charging, whenever temperature is within
**5 °C** of the lowest configured rung — 40 °C under the recommended thresholds,
derived from the rungs rather than hardcoded — and **5 s when the cell has stopped
answering entirely**, because backing off on a device that has gone silent turns
"we cannot see the cell" into "we look at it less often". Nothing in this
repository calls it on a schedule; the loop that would is not written. The shed
ladder maps time since mains loss onto the rungs of a graceful shutdown:
announce, shed non-essential services at
**60 s**, checkpoint and quiesce at **180 s**, and shut down when charge falls to
the reserve — **10%**, configurable upward only. Time alone never reaches
shutdown; shutdown is only ever a decision about charge. A tick that arrives three
minutes late returns **every rung it skipped, in order**, so a caller cannot drop
the flush without visibly discarding a returned value — a caller that does not
yet exist.
*([the decision these come from →](docs/adr/ADR-0002-battery-safety-governor.md), §3 and §8)*

### 🔁 The supervisor loop

The piece that makes the rest a running thing rather than a set of pure
functions. One `tick` reads the cell, enforces the ceiling, shows the reading to
the governor, advances the shed ladder and returns the interval until the next
pass. **The clock is a trait**, so `RealClock` sleeps and `FakeClock` advances a
counter and returns — which is why *thirty simulated days, 86,400 ticks*, is a
unit test that finishes in milliseconds rather than a month of waiting. That test
asserts the composition does not drift, does not stop escalating and accumulates
no state over a long run; it says **nothing** about Doze, a real kernel or a real
cell, and it is not the roadmap's P2 gate. Enforcement runs before the threshold
check, so a ceiling a vendor daemon reverted between ticks is caught on the same
pass that reads the temperature it was meant to be limiting. The unreadable case
is **not an early return**: it feeds the blind counter, tightens the cadence to
5 s and fills in the same `Outcome` struct as any other tick, because a loop
whose error path is shorter than its success path goes quiet exactly when
something is wrong. The governor is passed *in* rather than constructed, so a
device that halted before a restart comes back halted. **There is no binary that
runs this.** Every tick that has ever executed was driven by a fake clock over a
fake device.

### 🖥️ `vayucell` — the binary

Two commands and no dependencies: argument parsing is thirty lines of `std`,
because a project whose headline claim is that it has none should not acquire
its first one in order to read `--ceiling`. `status` reads the device once,
prints the panel and **exits with the verdict** — 0 protected, 1 not fully
verified, 2 unsafe, 64 unusable arguments — so a monitor gets the answer without
parsing prose, and `--help` documents every code because a usage text that
mentioned only 0 would make every real outcome look like a crash. `run` holds
the ceiling and watches the cell, and **stops when the governor halts**, since a
hard stop a restart clears is a log line rather than a state. `--ceiling 200` is
**refused, never clamped**: 100 is the value that holds no ceiling at all, so
silently clamping would make the unsafe outcome the quiet one, on the single
setting that governs a lithium cell in somebody's home. A flag with no value
after it is refused for the same reason — falling back to the default path would
point the governor at standard sysfs on a machine whose operator had just said
it was somewhere else. Run on a laptop it reports that it could not read a cell
and exits 2. **That is the only output anybody has ever seen from it.**

### 🧭 Tier detection and the capability registry

Four environments — **T0** stock Android unprivileged, **T1** stock Android with
root, **T2** a virtualised Linux guest, **T3** a mainline Linux port — and a tier
is concluded only from positive evidence. An Android marker on disk, an effective
uid of zero parsed out of `/proc/self/status`, a device tree naming mobile
silicon: each probe records a finding with the path or value behind it. Each of
those paths has, to date, only ever been answered by `FakeHost`; no probe in this
repository has read a real `/proc` or a real device tree. The
verdict is assembled from five typed signals rather than five booleans, because
two transposed booleans compile into a confident wrong answer. A guest cannot see
the phone underneath it, so **T2 is never self-detected** — it requires an
explicit assertion from the host shell, and virtualisation alone returns
unverified with the reason attached. Nothing defaults to a tier: `Verdict::tier()`
returns `None` for both unverified and unknown, and a `None` tier satisfies no
capability floor. Capabilities are contracts whose obligations are struct fields —
**a capability with no read-back does not compile**, `Tier` has no `Unset` variant
to defend against, and a safety capability that would degrade quietly is refused
at registration. *([decision →](docs/adr/ADR-0001-tier-model-and-capability-registry.md))*

### 🪟 The safety panel

The panel is the surface a person would read, so it is where being wrong is
guaranteed to reach them — which is why it is built so the reassuring failure
cannot be written. Today its only reader is a snapshot test. `Finding` has three variants, `Verified`, `Refused` and
`Unverified`, and **all three carry `Evidence`**; `Evidence::new` returns `None`
for a blank string, so a row without a citation is refused at construction rather
than rendered as a confident-looking claim. **The headline is computed from the
rows, never stored** — `Panel::overall()` folds them and takes the maximum of an
ordering declared in worry order, `Protected < Unverified < Unsafe`, so a single
unverified row takes the headline off `PROTECTED` however many green rows surround
it. Rows never disappear: every input is a required argument, and a handset with
no charge control gets a permanent red row saying so rather than a shorter panel.
`Confidence` has no `High` variant and there is no numeric risk score, because
swelling here is assembled from proxies — cycle count, capacity fade, time above
40 °C, charge acceptance — and `risk: 0.91` would read as a measurement to
everyone who saw it. The prompt to inspect the phone physically is unconditional,
because a conditional one is absent in exactly the case the estimate is wrong.
*([decision →](docs/adr/ADR-0007-the-safety-panel.md))*

### 🔒 Content Security Policy and response headers

A policy written as a string constant is edited by whoever is shipping something
on a Friday, and `'unsafe-inline'` is one word: the page keeps working, the header
still reads strict in every audit, and the failure has no symptom. So the policy is
not a string. It is built from a `Source` enum that **has no variant for
`'unsafe-inline'` or `'unsafe-eval'`**, with `compile_fail` doctests and a
positive control proving it, so restoring one is an addition to a public enum
rather than a one-word edit. The baseline is `default-src 'none'`, so a directive
nobody enumerated fails closed. Script executes only under a per-response `Nonce`
that is **deliberately not `Clone`** and is consumed by `render`, with a third
`compile_fail` proof that it cannot serve two responses. `Referrer` cannot express
`unsafe-url`; thirteen browser features are named in `Permissions-Policy` by
enumeration rather than omission; `Hsts::ONE_YEAR` is checked against a 180-day
floor by a compile-time assertion that stops the crate building. **Nothing here
serves HTTP yet, and these headers have never been sent to a browser.**
*([decision →](docs/adr/ADR-0006-content-security-policy.md))*

### 🧪 The host seam, and the fake behind it

`Host` reads and `Writer` writes, and they are separate traits on purpose:
folding the write into `Host` would make every probe in the codebase capable of
changing how a lithium cell charges in somebody's home, and the capability would
stop being visible at the call site. A function taking `&dyn Host` cannot write.
`read` collapses absent and unreadable into `None` because at that layer they are
the same fact — the *interpretation* belongs to the probe, which is why an
unreadable device tree yields `Silicon::Unreadable` rather than "no mobile
hardware here". The real host reads its effective uid out of `/proc/self/status`
rather than calling libc, and falls back to `u32::MAX` rather than `0`, so a
machine that answered nothing is treated as least privileged. **Every
device-facing behaviour in this repository is exercised against `FakeHost`** — a
handset that exists only as a set of answers a test decided to give — including
`with_read_only`, a node that refuses the write, and `with_revert`, which models a
vendor charging daemon putting the ceiling back after a cable event and is the
failure the whole verification loop exists for. No ADR records the seam itself;
the reasoning is in the module documentation.

### 💾 Storage durability — the guarantee is a number, not an adjective

ADR-0004 opens by withdrawing its own centrepiece, and the module is built
around what was left. A flush-honesty test cannot run on a sealed-battery
phone: it cannot drop its own storage rail, and the kernel flushes the device
cache on the way out, so **an honest device and a maximally dishonest one
produce byte-identical results**. Whatever shipped under that name would have
been a green light from a test that structurally could not go red for the
reason it claimed to. So the flash is assumed to lie, and the design is
arranged so that assumption costs nothing to be right about. `RecoveryPoint`
has **no variant meaning durable** — a `compile_fail` doctest pins it — because
a phone is a replica and that is a guarantee only for data older than the
replication lag. The closest thing to good news the type can express is *"the
off-device copy is 47 seconds behind"*, which still names the window in which
data exists on one device — **and it carries when it was measured**, because
`47` renders identically whether it was taken a second ago or the morning the
replicator died. That is also why there is **no `Display` impl**, pinned by its
own `compile_fail` proof: `Display` renders with no clock in scope, so a figure
the ADR promises will be *live* could be printed hours after anybody took it. An
unreachable replica is never filtered out as noise, and a lag nobody is still
measuring is never quiet; **`NeverReplicated` is a distinct state from a large
lag**, because
twelve hours behind means twelve hours is at risk and the other means all of it
is. `BackupState::NeverRestored` can never read as proven, which is the
roadmap's P6 gate: everything anybody checks on a written backup is a property
of the file, not of the restore. **And a drill that ran once is not a
schedule** — the failure the ADR is guarding against is a chain that breaks
silently, where the upload keeps succeeding and the only thing that would notice
is the restore nobody has run since March, so a restore proves the backup for
`DRILL_STANDS_FOR` and then says how old it is. That stamp is **wall-clock**
rather than monotonic, because the drill happened before this process started
and a clock that begins at zero on boot cannot date March — and it is an
`Option`, because a phone with no network and a dead RTC is an ordinary phone,
and **a cell that cannot tell what day it is does not get to call a drill
current**. And of the four things the ADR records, the one
that can read as verified is the shed ladder completing — **the only one that
measures this software's behaviour rather than the device's honesty.**

Those types now have two producers, which they did not for most of their life.
The first is this device: a `STORAGE` section in `vayucell report` carries the
flash posture, the wear estimate and every standing concern, and
**`vayucell vault` says at startup that this phone is the only copy** — the
sentence ADR-0004 exists to make somebody read, at the moment they start keeping
files on a handset. The second is the companion's receipt, once you point
`--replica-evidence` at it: the same sentences then carry the replica's dated
claim — lag inside the five-minute window with its measurement time, past it
only that nobody is still measuring, a stamp ahead of this clock refused whole,
an unreadable file breaking both halves openly — and the section says in its
first line that everything under it is *"as claimed by the replica's own
receipt"*, because this phone has no socket out and did not measure it. The wear estimate is
the one storage property a device can answer about itself: a coarse step from
eMMC or UFS, reported at the **worse** end of its range, taking the worse of the
two cell types, and with *"the device declines to say"* reported as unreliable
rather than as new flash.
*([decision →](docs/adr/ADR-0004-storage-durability.md))*

### 🧅 Sovereign ingress — and the conflict nobody noticed

ADR-0003 also opens by dismantling its own draft, and the third correction is
the one worth the section. The draft made an onion service the default because
it ranked it as having *no* external dependency — and an onion service is
sustained cryptographic work, sustained work is heat, and heat is precisely the
ageing the battery governor exists to suppress. **Neither document mentioned the
other.** The flagship safety subsystem and the default ingress mode were in
direct conflict and nothing in the design noticed. `shed_for` is the repair: it
takes a governor `Level` and there is **no parameter by which a mode outranks
it**. At `DERATED` the high-thermal mode is shed first, before storage or
serving work, because it is the load making the device hot; at `PROTECT` and
`HALT` nothing outward-facing runs, and local-only survives because stopping it
would take the panel away from the person who most needs to read it. The other
two corrections are recorded as required fields rather than prose: an onion
depends on a **commons** and not on nothing — a better dependency than a
supplier, but calling it nothing was a ruler chosen to flatter the default — and
it is **not reachable by an ordinary browser**, because `.onion` is a reserved
name that is not in DNS, which makes "serve a real site from a drawer" true
about the transport and overstated about the audience. The default is
**local-only**: publishing is irreversible disclosure, and Charter Article
VIII.5 forbids that without explicit confirmation. `Reachability` has no variant
for a running process — a request from **outside** must traverse the path and be
served — so "the tunnel is up" is not expressible. **And it expires.** ADR-0003
§4 always said the check re-runs on a schedule, and `Unverified` always said in
its own doc comment that it was the state a mode returns to when the check is
overdue; nothing computed overdue, so one round trip verified a path for the
life of the process — the type *was* the "verification that never expires" its
comment named as unable to notice the failure that matters. A standing now lasts
`FRESH_FOR`, and you cannot ask `is_verified` without saying **when**. **Nothing
here opens a socket.** *([decision →](docs/adr/ADR-0003-sovereign-ingress.md))*

**The onion half of that table is now built**, as far as a process that never
dials can build it. `vayucell all --onion-dir <DIR>` supervises **your system's
tor daemon** as one more surface ([VCIP-0001](docs/vcip/VCIP-0001-onion-ingress-via-system-tor.md)):
it writes a configuration you can read back byte for byte, starts the daemon as
a child, and reads the `.onion` address from the file the daemon publishes —
shape-checked before it is shown (length, base32 alphabet, the version
character), with the checksum honestly *not* verified, because checking it
needs crypto code ADR-0005 §5.1 forbids. The identity key is generated and
held by the daemon inside that directory; nothing here reads it, copies it, or
prints it, and the custody story — rotation breaks every link, backup is
encrypted or nothing, theft has no revocation — is printed the first time,
before the mode starts. `SocksPort 0` refuses the proxy role: this daemon
publishes one cell and nothing else rides it. The introduction-point rate
limit is requested by default per ADR-0003 §10.2, and the proof-of-work
defence is deliberately never claimed — whether a given daemon compiled it in
cannot be read back from here. The governor's authority is structural:
`should_run` delegates to `shed_for`, so DERATED sheds the onion first,
PROTECT stops it outright — and the exit paths stop the daemon *before*
`process::exit`, because a publisher outliving its governor is the one orphan
this mode must never leave behind. What none of it does is claim reachability:
until a request has arrived from outside through the path, the word used out
loud stays **unverified**, which is also why the panel is never published —
the battery report of somebody's home is not the thing this mode exists to
hand the world.

### 🌐 The local-only listener

The first thing in this project that a browser has ever spoken to. `vayucell
serve` binds **loopback by default** — reaching the rest of your network is a
flag you type, because binding every interface would make a weaker version of
the disclosure decision ADR-0003 reserves for the operator — and it prints the
address it actually bound rather than the one it was asked for, since a port of
`0` resolves to something else. Every response carries the **full security
posture including the errors**, because a 404 without a CSP is still a page a
browser will execute script in, and error paths are where headers get dropped.
The nonce is minted per response from `/dev/urandom` and consumed by the render,
so the type will not let it serve twice. `Method` has no `Post`, `Put` or
`Delete` variant, so a route that mutated something could not be written without
first widening a public enum. Traversal is **refused rather than normalised**:
stripping `..` and serving what is left means `/../../etc/passwd` quietly
becomes `/etc/passwd` and the log records the second one — and percent-encoding
is refused rather than decoded, because `%2e%2e` works precisely when the check
runs against a different string from the one that arrived. The request line is
bounded at 8 KiB. Parsing and routing own no socket at all, so a malformed
request is a unit test rather than a fixture.

---

## Quick start

There is a binary now, and it does something honest on a machine that is not a
phone: it reports that it could not read a cell, and exits non-zero saying so.

```bash
git clone https://github.com/johalputt/VayuCell.git
cd VayuCell
cargo run -p vayucell -- status      # read the device once, print the panel
cargo run -p vayucell -- help        # every flag, and what each exit code means
```

### Hosting a website from it

```bash
vayucell site --dir ~/mysite --bind 0.0.0.0:8080
```

That is the whole of it. A directory of files, served to your own network.

What makes it different from any other file server is the part you cannot turn
off: **the governor is consulted on every single request.** If the cell gets hot
or the phone drops to `PROTECT`, the site stops answering and says why. If mains
is lost and the shed ladder reaches the rung whose obligation is "stopped
non-essential services", the site is one of them.

It also refuses, by construction rather than by checklist: any path with `..` in
it, any name beginning with a dot — so the `.git` and `.env` sitting beside your
site never leave the building — any symlink resolving outside the directory, and
any directory without an `index.html`, because a generated listing publishes
everything you happened to leave in a folder. Every one of those refusals is the
same 404 **and the same sentence**, so the difference between them cannot be used
to map your directory — a real directory, an absent name and a hidden one are
word for word the same answer, and a traversal attempt is not told apart either.
The unified status shipped first and the bodies still discriminated, which is a
directory listing delivered one body at a time rather than one status at a time;
it was found by probing the running binary. The real reason goes to your log,
which is on the device you own and which — until this was fixed — did not
actually exist for four of the six refusals. See
[ADR-0008](docs/adr/ADR-0008-publishing-a-site.md).

`--dir` has no default. A `site` command that published whatever folder you were
standing in is the worst thing it could do.

### Storing files on it

```bash
vayucell enrol --device laptop          # prints a secret, once
vayucell vault --dir ~/files --bind 0.0.0.0:8080
```

Then, from the laptop:

```bash
S='Bearer <the secret>'
curl -T ./report.pdf  -H "Authorization: $S" http://<phone>:8080/report.pdf   # store
curl                  -H "Authorization: $S" http://<phone>:8080/report.pdf   # read
curl -X DELETE        -H "Authorization: $S" http://<phone>:8080/report.pdf   # remove
```

Hand-`curl`ing one file at a time is fine for one report. For a folder that
changes, the companion does it in one command:

```bash
cargo install --path sync                 # builds vayucell-sync
vayucell-sync plan --dir ~/files <phone>:8080     # shows what would move; sends nothing
VAYUCELL_TOKEN=<the secret> vayucell-sync push --dir ~/files <phone>:8080
```

`plan` never deletes; `push` uploads what differs by size or mtime, and removes
remote copies of files you deleted locally **only when you pass `--prune`**.
The cell is dialed only while the command runs — the phone never reaches back,
never schedules anything, and a `plan` that ends mid-air has moved nothing.
Plain HTTP is all it speaks: over Tor, the onion path is already encrypted, and
there is deliberately no TLS stack under this roof to trust instead.

*([decision →](docs/adr/ADR-0011-synchronising-a-folder-to-a-vault.md))*

A folder kept in step is a *replica*, and this project makes one more
honest claim about it than "it exists": **when it was last proven to
restore**. Two commands, each required to name the file its evidence goes
in:

```bash
vayucell-sync replicate ~/mirror <phone>:8082 --receipt ~/mirror/receipts.json
VAYUCELL_TOKEN=... vayucell-sync drill    ~/mirror <phone>:8082 --receipt ~/mirror/receipts.json
```

`replicate` pulls the vault into the mirror; `drill` downloads every file
afresh and compares it against the mirror byte for byte — the comparison,
not the copying, is what the receipt records. Both write their dated
claim only when they finished completely; a run that dies halfway leaves
yesterday's receipt standing, where it ages out on the phone into
*"nothing is still measuring"*. Start the vault (or `all`) with
`--replica-evidence ~/mirror/receipts.json` and its startup banner and
report quote that file — worded, every line, as a claim from the replica's
own receipt, because the cell has no socket out and measured none of it
([ADR-0012](docs/adr/ADR-0012-replication-by-receipt.md)).

`vayucell devices` lists what is enrolled — never a secret — and
`vayucell revoke --device <name>` removes one.

The credential is **minted, never chosen** — 256 bits from the kernel — because
this project has no dependencies and hand-rolling a password hash under that rule
would be the worst possible use of it. It is shown once; there is no command that
prints it back. Revoking one takes its line out of the store and leaves every
other device working.

**The ladder's last rung is performed, not announced.** At the reserve, or with
a cell it cannot measure, the node prints the rung and then **stops, with charge
remaining** — it used to print *"shut down cleanly with charge remaining"* and go
on ticking until the cell was flat, which is the ungraceful death the ladder
exists to prevent. No halt record is written for it: an outage is not a governor
halt, and mains returning is the whole remedy.

**A halted phone serves nothing.** `site`, `vault`, `all` and `run` all refuse
to start while a halt record stands — asked once in the dispatch, from
`Command::serves_traffic()`, so a subcommand added later cannot forget. This was
written per command and the two added later did not get it, so a cell that
crossed a hard threshold went on serving pages and accepting uploads after a
restart. The panel is the deliberate exception: it comes up and **reports the
halt**, because that is what somebody needs to read at that moment.

**An empty store refuses everything.** "Nobody enrolled" never means
"authentication off", and that is the state every installation starts in.

**A name cannot be enrolled twice, on either path.** `enrol` refuses it and so
does the parser — the store is a text file operators edit by hand, and the rule
used to be enforced only where this software writes. Two rows sharing a name
means two different secrets authenticate as one device, so nothing can say which
presented a credential and revoking that name takes both. The store is refused
whole, naming both lines.

**The quota is measured, not assumed.** What the vault already holds is read
from the directory before every upload rather than captured at startup, and a
directory that cannot be read refuses the write instead of counting as empty —
an unreadable usage figure is indistinguishable from free space, and treating it
as zero is a limit that quietly stops being one.

A write is refused *earlier* than a read: the website keeps serving at `DERATED`,
the vault does not, and the outage ladder stops accepting uploads one rung before
it stops serving pages. A refused upload costs one retry; a half-written file
outlives the event. Nothing is written under its real name until it has been
flushed and renamed, and **no receipt ever says "saved"**.

**All three operations are contained against symbolic links, not two.** A read
canonicalises and a delete canonicalises; a write did not, and `OpenOptions`
follows links — so a link at the temporary path would have taken an upload
outside the vault while the vault looked empty, and a link at the destination
would have been destroyed without a word by a surface that refuses to *read*
through one.

**A refusal tells the caller the class; the log tells the operator where.** No
answer on the wire carries a filesystem path — a test asserts no separator
appears in one — because a caller can act on none of it and on a device somebody
else has already reached it is a map. And something stored in the way answers
**409, not 500**: the request was well formed and the server is not broken, the
target is. A caller told 500 retries; a caller told 409 stops and tells
somebody, which is the only thing that clears it. See
[ADR-0009](docs/adr/ADR-0009-accepting-a-file.md) and
[ADR-0010](docs/adr/ADR-0010-per-device-credentials.md).

`status` exits **0** only when every row was checked and held, **1** when
something could not be checked, and **2** when something was checked and does
not hold — the verdict in the one form a monitor can read without parsing prose.
On a laptop it exits 2, because there is no cell and no charge control, and both
of those are answers rather than errors. `vayucell run` holds the ceiling and
watches the cell until the governor halts; it stops when it does, because a hard
stop that a restart clears is a log line rather than a state.

### Putting it on a phone

One command inside [Termux](https://f-droid.org/packages/com.termux/), installed
from F-Droid rather than the Play Store:

```bash
curl -fsSL https://raw.githubusercontent.com/johalputt/VayuCell/main/install.sh | bash
```

It names the battery risk and waits for you to type `yes` **before it writes
anything**, installs what is missing, and refuses to claim success until the
program it installed has actually run. Every failure says what to do next rather
than printing an error code. Nothing needs root, nothing is written outside
`~/.vayucell`, and removing it is `rm -rf ~/.vayucell`.

Piping a script into a shell means trusting whatever the server sends;
[`docs/INSTALL.md`](docs/INSTALL.md) gives the download-then-read form, and is
written for somebody who has never opened a terminal.

It downloads a **signed, checksummed build** — seconds, not a compile — and
refuses to install one whose checksum does not match. Builds are published for
64- and 32-bit Android and for three Linux targets; anything else falls back to
building from source and says so first. The checksum file is signed with a
keyless certificate bound to the release workflow, so you can check it yourself:

```bash
cosign verify-blob --certificate SHA256SUMS.txt.pem --signature SHA256SUMS.txt.sig \
  --certificate-identity-regexp 'https://github.com/johalputt/VayuCell/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS.txt
```

Expect `vayucell status` to say `UNSAFE`. That is the correct answer: an
ordinary unrooted phone has no supported way to stop charging at 60%, so that
row stays red permanently because it is permanently true.

**No handset has run it.** To run the gates instead — exactly what CI runs:

```bash
git clone https://github.com/johalputt/VayuCell.git
cd VayuCell
python3 -m pip install jsonschema   # the schema and self-test gates need it

scripts/local-ci.sh            # every gate
scripts/local-ci.sh --fast     # skip mutation, coverage and the self-test
scripts/local-ci.sh --list     # show what would run, and stop
```

It prints one line per gate — the name, `ok` or `FAIL`, and how long it took —
and shows a gate's own output only when that gate fails, which is what makes it a
thing you will actually run before pushing rather than a thing you mean to.

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

A full run exercises **610 unit tests and 32 doctests** (2 ignored — the two
snapshot regenerators), kills **254 mutations**, catches **57 planted violations** and two count proofs,
and holds line coverage above a floor of 80. That is a suite that
has been shown to fail when the code is wrong — the mutation gate is the proof,
and it asserts its own match count so a mutation that failed to apply cannot be
scored as one the code survived. **What none of it establishes, and none of it
can, is that this behaves correctly on a phone.**

---

## Why a retired phone

This compares **hardware classes, not products**. Every column but the first
describes something you can buy today; the first describes a device VayuCell does
not yet run on. VayuCell loses the last row and there is no way to win it:

| | A retired phone | Single-board computer | Rented VPS | NAS appliance |
| --- | --- | --- | --- | --- |
| **Hardware cost** | Already owned, in a drawer | Board, case, PSU, storage, all bought | Nothing up front, rent forever | The most expensive of the four |
| **Integrated UPS** | Yes — the cell is physically a UPS; the shed ladder is written to treat it as one, and has never done so outside a test | No, unless you buy one | Somebody else's problem, and somebody else's promise | Rarely; usually an external unit |
| **Cellular fallback** | Yes — the hardware has a modem. **No VayuCell code touches it** | No | Not applicable | No |
| **Idle draw** | One to three watts, per published device figures — not measured by this project | Two to eight watts, plus peripherals | Billed, not measured by you | Tens of watts with disks spinning |
| **E-waste** | Diverts a working device from disposal | Manufactures a new device | None made, none diverted | Manufactures a new device |
| **Who owns the data** | You, on hardware in your building | You, on hardware in your building | The provider holds the disk and the hypervisor | You, on hardware in your building |
| **Battery risk** | **A lithium cell held warm and near full for years, in a building where you sleep** | None — no cell to govern | None — no cell to govern | None — no cell to govern |

The battery row is why the governor is the first subsystem and not a later one,
and why the charter treats overstating a safety property as the single
disqualifying failure. The other three options have no cell to be wrong about.

---

## How the core fits together

Sixteen modules, one crate, one direction of dependency — fifteen above the seam, plus
`host.rs` below it. The boundary at the bottom is the honest part of the picture:

```text
                    ┌──────────────────────────────────────────────┐
                    │      vayucell-core (Rust) — above the seam   │
                    │                                              │
                    │  runtime.rs    the supervisor loop — one     │
                    │                tick; the clock is injected   │
                    │                                              │
                    │  panel.rs      what a person is allowed to   │
                    │                be told — rows, evidence,     │
                    │                computed headline             │
                    │                                              │
                    │  ── safety ───────────────────────────────   │
                    │  governor.rs   levels, verify loop, recovery │
                    │  battery.rs    typed units, thresholds       │
                    │  sampler.rs    cadence (pure, owns no clock) │
                    │  shed.rs       mains-loss ladder             │
                    │  sysfs.rs      the one writable ceiling node │
                    │                                              │
                    │  durability.rs recovery point, wear, backup   │
                    │                proof — no "durable" variant   │
                    │                                              │
                    │  ingress.rs    four modes, seven declared     │
                    │                properties; the governor wins  │
                     │  onion.rs     the contract with the system    │
                     │                tor daemon; the key is the     │
                     │                daemon's, never this crate's   │
                    │                                              │
                    │  ── identity ─────────────────────────────   │
                    │  tier.rs       T0/T1/T2/T3 from evidence     │
                    │  capability.rs registry; no read-back,       │
                    │                no compile                    │
                    │                                              │
                    │  ── response ─────────────────────────────   │
                    │  serve.rs      request → response; owns no    │
                    │                socket                        │
                    │  csp.rs        policy as types, not strings  │
                    │  headers.rs    the nine-header posture       │
                    └──────────────────────┬───────────────────────┘
                                           │  Host (read) / Writer (write)
                                           ▼
                    ┌──────────────────────────────────────────────┐
                    │  core/src/host.rs — same crate, below the    │
                    │  seam — FakeHost                             │
                    │  a machine described by a test rather than   │
                    │  inhabited by one. THERE IS NO REAL DEVICE   │
                    │  ON THE OTHER SIDE OF THIS LINE.             │
                    └──────────────────────────────────────────────┘

    NOT BUILT: relay ingress ·
    the Android shell · the fleet view · the hardware database ·
    any code path that has ever touched a handset. The onion is built as far
    as this machine can build it — supervised, governed, honestly worded — and
    no further: nothing outside has ever fetched a byte through it.
```

`RealHost` exists and implements both traits over `std::fs`; nothing in this
repository has ever pointed it at a phone. The decision records in
[`docs/adr/`](docs/adr/) carry the reasoning, including the ones that record a
withdrawal rather than a design.

---

## Showcase

There are no screenshots, because there is nothing running to screenshot. What
there is, is the panel — the real user-facing output, rendered from
[`core/src/panel.rs`](core/src/panel.rs), committed to the tree and checked by a
test. Both devices are shown, because showing only the calm one would be the
dishonest edit:

```text
--- a device where everything was checked and holds ---

BATTERY SAFETY: PROTECTED

  VERIFIED     device tier                  T1 established from positive evidence on this device
  VERIFIED     charge mechanism             charge_control_end_threshold answered at a readable ceiling
  VERIFIED     charge ceiling               60% written to charge_control_end_threshold and read back
  VERIFIED     battery governor             governor at NORMAL; no threshold crossed
  VERIFIED     outage reserve               a cell carries this node and it shuts down holding 10%

Swelling risk: Nominal, low confidence — an estimate from cycle count against age, capacity fade, not a measurement.

Physical inspection is the definitive check, and this software cannot perform it. Put the phone face-down on a flat table. If it rocks, does not lie flat, or the screen or back cover is lifting at any edge, stop using it now and take it to hazardous-waste handling.
```

*Five rows, each with the evidence that produced it. `60% … written and read back`
is the only phrasing under which this project is allowed to call a ceiling
verified. `PROTECTED` is the highest headline this code can produce and it means
every row was checked and held — it does **not** mean the battery is safe. Risk
is governed, never eliminated; the inspection prompt below the rows is
unconditional for exactly that reason.*

```text
--- a stock handset with no charge control and no cell ---

BATTERY SAFETY: UNSAFE

  VERIFIED     device tier                  T0 established from positive evidence on this device
  FAILED       charge mechanism             this device exposes no charge control, so no ceiling can be held
  UNVERIFIED   charge ceiling               no mechanism exists to hold a ceiling on this device
  FAILED       battery governor             governor at DERATED; the workload has been reduced or stopped
  FAILED       outage reserve               no battery is carrying this node; mains loss stops it immediately

Swelling risk: Elevated, moderate confidence — an estimate from capacity fade, time above 40 °C, charge acceptance falling, not a measurement.

Physical inspection is the definitive check, and this software cannot perform it. Put the phone face-down on a flat table. If it rocks, does not lie flat, or the screen or back cover is lifting at any edge, stop using it now and take it to hazardous-waste handling.
```

*The T0 case — an ordinary unrooted handset, and the most common device there is.
The failing rows do not clear, no configuration setting removes them, and the
row that could not be checked reads `UNVERIFIED` rather than being omitted.*

> Both blocks above are the body of
> [`docs/panel-snapshot.txt`](docs/panel-snapshot.txt) — the file's
> "generated, do not edit by hand" header is the only thing omitted —
> generated from `core/src/panel.rs` and asserted by a test. Renaming `UNSAFE` to
> something gentler breaks that test and nothing else — the mutation gate confirms
> it. **Both devices are fakes. No person has ever read this panel on a phone.**

### The security posture is a file you can read

The body of [`docs/security-posture.txt`](docs/security-posture.txt) is the exact
header set a response would carry, committed and checked by a test. It exists because the
individual guards are not enough on their own: a change that weakens the posture
while keeping each assertion true reads, in a diff, as a small edit to a Rust
file, and nobody reviewing it sees the headers change. With the snapshot
committed, weakening anything produces a diff in a plain text file — something a
reviewer notices without knowing the codebase at all.

<details>
<summary><strong>The nine headers, in full</strong> — as rendered, with a placeholder nonce</summary>

```text
Content-Security-Policy: base-uri 'none'; connect-src 'self'; default-src 'none'; font-src 'self'; form-action 'self'; frame-ancestors 'none'; img-src 'self' data:; object-src 'none'; script-src 'nonce-r4nd0mBase64urlValue00'; style-src 'self'; report-uri /csp-report
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: no-referrer
Permissions-Policy: accelerometer=(), autoplay=(), camera=(), display-capture=(), encrypted-media=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), midi=(), payment=(), usb=(), xr-spatial-tracking=()
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Resource-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
Strict-Transport-Security: max-age=31536000; includeSubDomains
```

Ten of the eleven CSP directives are sorted by name, with `report-uri` appended
last, so a reordering of the ten is not a diff. The nonce is
a fixed placeholder in the snapshot; in a response it would be a fresh value used
once.

</details>

---

## What it will never claim

Each of these is a limit in [Charter Article II](CHARTER.md), in the order the
charter states them, and each is stated rather than managed:

1. **That every phone can be a server.** T0 — an ordinary unrooted handset, and
   the most common device there is — cannot limit its own charging, and its
   safety row stays red permanently.
2. **That one phone is datacentre reliability.** One cell, one flash part, one
   radio, one room.
3. **That the battery is safe.** Risk is governed, never removed. `Confidence`
   has no `High` variant and there is no numeric risk score, because swelling
   here is assembled from proxies.
4. **That an abandoned vendor kernel is secure.** It is not patched and it is
   not going to be. No policy in userspace changes that.
5. **That a rented relay is independence.** If reaching the device depends on
   somebody else's endpoint, the sovereignty claim belongs to them.

---

## How this is checked

The charter is not a statement of intent — it is a set of constraints, and a
constraint that only a reviewer checks is one that erodes on a busy week. So the
constraints are enforced by a machine, and **the machines are themselves tested**:
every gate is re-broken in a scratch copy of the repository and required to
notice, because a check whose pattern silently stops matching prints `ok` forever
and has no other symptom.

| Gate | What it refuses to let through |
| --- | --- |
| **Charter** | A serving capability registered while the governor is gone. `Capability::verify` demoted to an `Option`, so a control with no read-back would compile. A generic success variant that would absorb "not checked". `Absent` and `Unverified` collapsing into one answer. A tier detector that defaults to T0. Telemetry, a treasury, a kill switch, a remote wipe, a dependency on a host this project runs. A `[dependencies]` section with anything in it. An edit to Article III or V whose SHA-256 no longer matches `.charter-digests` |
| **Gate self-test** | A gate that has only ever been observed passing. **Fifty-seven violations** planted in a scratch copy — the governor deleted, `verify` made optional, telemetry added, an outbound connection opened in the binary crate, a third-party dependency added to the CLI crate, a bot-authored commit, a workflow tag that resolves to nothing — each of which the matching gate must catch **citing the right rule**. A plant that changed nothing is scored `STALE`, not `caught`: the sandbox is fingerprinted before and after |
| **Mutation** | **Two hundred and fifty-four** safety and honesty guards, each re-broken in turn, each required to turn its named test red. A green suite proves the code passes its tests; this proves the tests would notice if the code were wrong. Every mutation asserts its own match count, because one that failed to apply would otherwise be reported as one the code survived |
| **Doctests** | A `compile_fail` proof that stopped being collected. The count is asserted **exactly, in both directions** — too few means a proof moved onto a private item, where rustdoc runs zero tests and still prints `ok`; too many means somebody added a proof without raising the number |
| **Rust** | Unformatted code, a `clippy::pedantic` warning at `-D warnings` over all targets, a failed build or test, a broken intra-doc link, and the removal of *either* `#![forbid(unsafe_code)]` or `unsafe_code = "deny"` — either alone can be dropped in a diff that looks unrelated |
| **Coverage** | Production line coverage below **80%**, with test files excluded so the figure is not inflated by the coverage of the tests themselves. A missing coverage tool is a failure, never a pass |
| **Constitution** | A `[CI]` rule claiming enforcement while naming no enforcer, or naming a file that has been deleted. It explicitly does *not* claim the cited file enforces the sentence attached to it, and prints that limitation on every run |
| **Docs** | A required document missing **or emptied**, an ADR whose title names a different number than its filename, a gap in the decision log, an ADR nothing links to, a dead relative link anywhere in the repository, **or a constitution whose own totals disagree with the rules in it** |
| **Hardware** | A device profile that fails [`hardware/schema.json`](hardware/schema.json), a verified charge ceiling with no sysfs node named, `available: false` beside a named mechanism, or a storage block with the durability class omitted rather than chosen |
| **Attribution** | An assistant name in any tracked file, a "generated by" line, an assistant co-author trailer, or a commit authored by a bot or `noreply` address — over the full history, because a shallow clone would pass by checking nothing |
| **Release** | A `.release-version` that is missing, malformed or carries a trailing newline; a crate version that disagrees with it; a changelog with no section for the release; a tag that already exists |
| **Actions** | A `ci.yml` job that is not in the required-checks list, so it can fail while CI reports green. A workflow reference that is **not pinned to a full commit SHA**, or one naming a commit that cannot be fetched, **or a `pip install` without `--require-hashes`** — pinning the actions and then installing an unpinned package inside one closes the front door and leaves the side door open. A tag is whatever its owner repoints it at tomorrow, and repointing produces no diff here. Also an extraction pattern that has gone stale and found nothing. Without network it prints `UNVERIFIED`; CI requires the network so the authoritative run cannot skip it |
| **Credentials** | A store with nobody enrolled behaving as though authentication were off — the state every installation starts in. A secret a person could choose, where no memory-hard derivation is available to protect one. A comparison that stops at the shorter input, so a one-character secret matches everything, or that returns at the first mismatching byte, which leaks the secret one byte at a time. A secret that prints in a debug line. A store readable by anyone but its owner. A malformed line that silently unenrolls a device |
| **Vault** | A filename that is really a path, a dotfile written into the store, a quota that underflows once usage passes the limit, an upload accepted while the cell is derated or the phone is on battery, a write plan handed back for a write that was refused, a rename before the flush that makes it safe, or a receipt that tells somebody their file is *saved* — which no device here can know |
| **Site** | A path that leaves the site directory, a hidden name being served, a directory becoming a generated listing, a symbolic link resolving outside the root, a refusal whose status differs from the others, or the governor ceasing to outrank a visitor's request. Ten of the mutations above re-break exactly these |
| **Shell** | A `shellcheck` finding in any script, a script that is not executable, or one with no `bash` shebang. The gates decide whether a release ships, so a quoting bug in one is a correctness bug in the release process |
| **Install** | An installer that stops naming the battery risk **before it writes anything**, drops the physical-inspection instruction, acquires root, or has a failure path that says what broke without saying what to do about it. It also installs from a clean `HOME` and runs the result, twice — `install.sh` is the only file here that executes on a stranger's device, and the only one the test suite cannot reach. What it cannot check is Termux itself, and it prints that rather than implying otherwise |
| **Targets** | The core failing to type-check for 64- and 32-bit Android, mainline ARM, or an ordinary Linux laptop — five targets, `fail-fast: false`, so one broken target does not hide the other four |
| **Supply chain** | An advisory, a banned or duplicated crate, a disallowed licence, a source that is not crates.io, an unused declared dependency, a lockfile that would change on build, an SBOM component whose name is not ours, or a verified secret anywhere in history |
| **Reproducible** | A release artefact that is not byte-identical when rebuilt from a clean tree, compared by SHA-256. You are asked to run this unattended in the building where you sleep; "check for yourself" has to be a real offer |

Gate logic lives in [`scripts/`](scripts/) rather than inline YAML, so the check
that decides a release is the check you can run on your laptop before you push.
That directory holds sixteen files: eleven gate scripts, the `local-ci.sh` runner,
coverage, the doctest count, the SBOM job and the logo generator. Some checks in the
table above — Rust, Shell, Targets, Supply chain and Reproducible — are invoked
directly as jobs and commands rather than living in a script of their own. Seven workflows call all of it:
`ci.yml`, `supply-chain.yml`, `scheduled.yml`, `release.yml`, `codeql.yml`,
`fuzz.yml` and `dependabot-automerge.yml`. [`docs/CI.md`](docs/CI.md) documents each job and
each parameter.

**And every gate prints what it cannot check.** The charter gate names four
articles as human review on every single run — III.2, that the plain-language
warning is genuinely understandable; III.4, that physical inspection is named as
the definitive check in the UI; IV.4, that permanent failing rows read as
permanent to an actual operator; V.4, that no capability exists only in a hosted
edition. A gate list that appears complete teaches its reader to stop looking for
the gaps.

---

## Governance

`CHARTER.md` is CC0 — nine articles, dedicated to the public domain, so an
entirely independent implementation may take it — and Articles III (safety) and V
(what VayuCell will never contain) sit beyond ordinary amendment: their SHA-256
digests are recorded in `.charter-digests`, so an edit stops the build until it is
re-recorded in the diff where review will see it. Beneath it, a **110-rule
constitution splits itself 52 `[CI]`, 28 `[REVIEW]`, 30 `[NORM]`**, counts the
split in an appendix, and gates that count — a governance document that miscounts
how much of itself is actually enforced would be committing its own honesty rule's
error against its reader. Contribution is by DCO sign-off, never a CLA, and the
charter gate fails if a contributor licence agreement ever appears.

The constitution's own closing appendix names seven things it cannot do, and they
are worth reading before the articles: roughly half of it is not machine-enforced,
the review rules are the weak point and include the most important ones, it cannot
make a reviewer competent, it cannot stop a determined maintainer with commit
access from dismantling it — only make dismantling visible and forking possible —
it cannot fund the project, **it cannot verify anything on real hardware**, and it
is version 1.0 and has never been tested by an actual conflict.

---

## Security and sovereignty

- **No `unsafe`, and two independent latches saying so.** `#![forbid(unsafe_code)]`
  in the crate root and `unsafe_code = "deny"` in the manifest; CI greps for both,
  because removing one alone reads as tidying.
- **Zero third-party runtime dependencies, asserted rather than asserted-about.**
  The charter gate fails on a non-empty `[dependencies]` section, and the SBOM job
  fails if any component in the published bill of materials carries a name that is
  not ours. You do not have to take that on trust.
- **Reproducibility and signing are gated requirements.** The release workflow
  requires a release artefact to rebuild byte-identically from a clean tree,
  compared by SHA-256 with the build path remapped, and signs checksums at
  publish time. No release has been published yet.
- **Weakening the response posture is a visible act.** The CSP has no
  representable `'unsafe-inline'`, the referrer policy has no leaking variant, and
  the rendered header set is committed as plain text — so the diff shows the
  change to a reviewer who has never opened the crate.
- **None of this makes an abandoned vendor kernel secure.** It is not patched, it
  is not going to be, and no amount of policy in userspace changes that. It is
  [Charter Article II](CHARTER.md) limit 4, and it is stated rather than managed.

---

## What it costs you to try

Nothing, and it is designed to keep costing nothing:

- **No account.** There is nothing to sign up for.
- **No telemetry.** Not aggregate-but-identifying, not "anonymous". None.
  Enforced by a gate that scans for the concept, not just the word.
- **No token, no treasury, no fee, no hosted tier.** There is one edition.
- **No dependency on us.** If this project vanished tomorrow, an installed cell
  keeps working. That is [Charter Article V.5](CHARTER.md), and it is the test
  the whole charter is built around.

---

## If you read nothing else

**Put your phone face-down on a flat table now and then.** If it rocks, or the
screen or back is lifting at any edge, stop using it and take it to
hazardous-waste handling. Software cannot see that. You can — and this is the one
check in the whole system that no gate, no snapshot and no green row can stand in
for.

---

## Documentation

| Document | What it is |
| --- | --- |
| [`CHARTER.md`](CHARTER.md) | The supreme law. CC0. Read this first |
| [`GOVERNANCE-CONSTITUTION.md`](GOVERNANCE-CONSTITUTION.md) | How the charter is upheld in practice — 110 rules, 52 `[CI]`, 28 `[REVIEW]`, 30 `[NORM]`, each `[CI]` rule naming the file that enforces it |
| [`GOVERNANCE.md`](GOVERNANCE.md) | Who decides what, and how a proposal is made |
| [`PLAN.md`](PLAN.md) | The full project plan |
| [`ADR-0001`](docs/adr/ADR-0001-tier-model-and-capability-registry.md) | The tier model and the capability registry |
| [`ADR-0002`](docs/adr/ADR-0002-battery-safety-governor.md) | The Battery Safety Governor |
| [`ADR-0003`](docs/adr/ADR-0003-sovereign-ingress.md) | Sovereign ingress: reaching a server that has no address |
| [`ADR-0004`](docs/adr/ADR-0004-storage-durability.md) | Storage durability: assume the flash lies |
| [`ADR-0005`](docs/adr/ADR-0005-implementation-language.md) | Implementation language: Rust for the core, Kotlin for the shell |
| [`ADR-0006`](docs/adr/ADR-0006-content-security-policy.md) | Content Security Policy: the browser as the last enforcement point |
| [`ADR-0007`](docs/adr/ADR-0007-the-safety-panel.md) | The safety panel: what a person is allowed to be told |
| [`ADR-0008`](docs/adr/ADR-0008-publishing-a-site.md) | Publishing a site: serving strangers from a governed phone |
| [`ADR-0009`](docs/adr/ADR-0009-accepting-a-file.md) | Accepting a file: the first surface that takes rather than gives |
| [`ADR-0010`](docs/adr/ADR-0010-per-device-credentials.md) | Per-device credentials: deciding whose file it is |
| [`ADR-0011`](docs/adr/ADR-0011-synchronising-a-folder-to-a-vault.md) | Synchronising a folder to a vault: the companion that dials, so the cell never has to |
| [`ADR-0012`](docs/adr/ADR-0012-replication-by-receipt.md) | Replication by receipt: the companion claims, the cell quotes |
| [`docs/INSTALL.md`](docs/INSTALL.md) | Putting it on a phone, written for somebody who has never opened a terminal |
| [`docs/CI.md`](docs/CI.md) | Every gate, and every parameter it checks with |
| [`docs/BRAND.md`](docs/BRAND.md) | The mark: how it is constructed, and the rules for using it |
| [`CHANGELOG.md`](CHANGELOG.md) | What changed, and what it means for someone running this |
| [`hardware/`](hardware/) | The device database schema and two example profiles. No real device reports exist yet (CC0) |

---

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

---

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
