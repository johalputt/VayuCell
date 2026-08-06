<!-- SPDX-License-Identifier: CC0-1.0 -->

# The VayuCell Governance Constitution

**Version 1.0 · Licence: CC0-1.0 · Subordinate to [`CHARTER.md`](CHARTER.md)**

---

## Preamble

VayuCell asks a person to take a lithium cell that a manufacturer stopped
supporting, energise it, warm it, and leave it running unattended for years in
the building where they sleep. Then it asks them to put their mail on it.

Everything in this document follows from taking that sentence seriously.

A project making that request has to be governed differently from one that
ships a web framework. The failure mode is not a bad quarter — it is a fire in
somebody's home, or a backup that was never actually written, discovered on the
day it was needed. And the specific danger is not that we will decide to be
careless. It is that carelessness will arrive one reasonable-looking commit at a
time, on a week when someone is tired, in a diff where the weakening is a single
word and everything still appears to work.

So this constitution is not a statement of values. Values do not survive
deadlines. **It is a set of constraints, and wherever a constraint can be
enforced by a machine, it is enforced by a machine** — because the machine is not
tired, does not want the feature to ship, and does not know who wrote the patch.

Where a constraint *cannot* be automated, this document says so out loud rather
than leaving the reader to assume it is covered. A list of rules that all look
equally binding is a list where nobody knows which ones are real.

---

## Article 0 — Authority, hierarchy, and how to read this document

### 0.1 The hierarchy of authority **[REVIEW]**

```text
CHARTER.md                       Supreme. Articles III and V are beyond
  │                              ordinary amendment (Charter Article IX).
  │
  └── GOVERNANCE-CONSTITUTION.md This document. How the Charter is upheld in
        │                        practice. Subordinate in every conflict.
        │
        └── docs/adr/*.md        Specific technical decisions, each citing the
              │                  authority it acts under.
              │
              └── code, tests, gates, and the hardware database
```

Where this document and the Charter conflict, **the Charter wins and this
document is wrong**. That is stated first, and without hedging, because two
documents each claiming final authority is precisely how an organisation
acquires the ability to justify anything it later wants to do.

### 0.2 Traceability **[REVIEW]**

Every rule below is either traceable to a Charter article or accompanied by a
stated engineering reason. **A rule that is neither should be deleted, not
obeyed.** Rules accumulate; nothing removes them except a deliberate decision to,
and a rule nobody can justify is one that will eventually be used to block
something good.

### 0.3 Enforcement classification **[CI]**

This is the most important section in the document.

| Marker | Meaning | What it is worth |
|---|---|---|
| **[CI]** | A gate fails the build | Real. It does not depend on anyone's attention |
| **[REVIEW]** | A human must catch it in the diff | Real, and only as reliable as that person on that day |
| **[NORM]** | Stated preference, no enforcement | Honest about being advisory |

Three consequences follow, and they bind:

1. **A rule that could be [CI] and is left [REVIEW] is a defect in this
   document.** Anyone may open an issue on that basis alone and it is a valid
   issue.
2. **A rule may never be silently downgraded.** Deleting a gate while leaving the
   rule text in place produces a document that describes a project which no
   longer exists — the exact failure Charter Article IV forbids in a device
   report, committed against the reader of the governance instead.
3. **Appendix A counts them.** If the count is wrong, the document is lying about
   itself, which is the one thing it may not do.

### 0.4 On the tone of this document **[NORM]**

It is written to be read by someone deciding whether to trust this software with
their family's photographs, and by a contributor deciding whether the project is
serious. Both deserve specifics rather than adjectives. Where a rule has a cost,
the cost is named. Where a protection is weaker than it sounds, that is written
down next to it.

---

## Article 1 — Priority ordering

### 1.1 The order **[NORM]**

When two good things conflict, this is the order:

1. **Safety of persons.** Charter Article III.
2. **Honesty of reporting.** Charter Article IV.
3. **The owner's authority over their own hardware.** Charter Article V.3.
4. **Durability of the operator's data.**
5. **Security of the system.**
6. **Correctness.**
7. **Performance.**
8. **Convenience — the user's, and ours.**

### 1.2 It is lexicographic, not weighted **[REVIEW]**

This is a strict precedence, not a scoring rubric. **A lower item never outweighs
a higher one, regardless of how much of it is at stake.** There is no exchange
rate, and no quantity of item 7 that purchases a concession on item 1.

A weighted model is how a project ends up shipping something unsafe because it
was *very* convenient. The ordering is lexicographic specifically to remove that
move from the table.

### 1.3 Worked examples **[NORM]**

These are here because an abstract ordering is easy to agree with and hard to
apply.

| Situation | Resolution |
|---|---|
| A charge-ceiling read-back costs 40 ms per cycle and the UI feels sluggish | 1 beats 7. Keep the read-back. Fix the UI, or state the latency |
| A device cannot limit charging, and the red row makes the product look bad | 2 beats 8. The row stays red forever. Charter III.2 |
| A crash-safety fix requires an unreviewed dependency | 4 and 5 conflict; neither wins automatically. It needs an ADR, not a judgement call in a pull request |
| A remote "help me fix it" path would cut support effort enormously | 3 beats 8, and Charter V.3 is absolute. No |
| Detection is 200 ms faster if `Unknown` defaults to T0 | 2 beats 7. `Unknown` stays |

### 1.4 Showing the work **[REVIEW]**

When a decision is genuinely close, write down which two items are in tension and
which one won. **"Item 8 beats item 2" is never a valid outcome**, and being
forced to write it down is how that becomes obvious to its own author.

---

## Article 2 — Identity and non-goals

### 2.1 What VayuCell is **[NORM]**

Software that turns one retired phone into a server its owner controls, for that
owner, with the battery risk actively governed and honestly reported.

### 2.2 What it is not, permanently **[NORM]**

- A phone-farm orchestrator. One person, their own devices.
- Anything with a token, a treasury, a fee, or a mandatory account.
- A general-purpose Android application platform.
- A claim of datacentre reliability. One phone is one phone.
- A way to make a swollen battery safe. **Nothing is.**

### 2.3 The non-goals are enforced where they can be **[CI]**

No production source may contain an identifier associated with a token,
treasury, fee, mandatory account, telemetry, device fingerprinting, or a remote
control path. Thirteen forbidden identifiers, checked by
[`scripts/charter-gate.sh`](scripts/charter-gate.sh). Charter Article V.

### 2.4 Scope discipline **[REVIEW]**

A proposal that is good software and outside §2.1 is **rejected on scope, not on
quality**, and the rejection says so. A project that accepts every good idea
becomes a project that cannot say what it is, and then cannot say what it will
never do either.

---

## Article 3 — Safety governance

The heart of the document. Charter Article III.

### 3.1 Nothing serves traffic before the governor **[CI]**

No capability of class `Serving` may be registered while `core/src/governor.rs`
does not exist. Enforced by the charter gate.

This is deliberately inconvenient. It means the demo everybody wants — *look, it
serves a website* — cannot be built first. Charter III.1 puts that constraint
above every release schedule, and the gate is what makes it survive the week
somebody really wants the demo.

### 3.2 A control with no read-back does not compile **[CI]**

`Capability::verify` is a `VerifyFn`, not an `Option<VerifyFn>`. A capability
that sets something without reading it back **cannot be written down**. Charter
III.3.

Three `compile_fail` doctests hold that proof, on a public item, because rustdoc
collects doctests nowhere else — on a private item they run zero tests and print
`test result: ok`.

### 3.3 A safety capability may not degrade quietly **[CI]**

`Class::Safety` with `Disposition::Degrade` is refused by the registry. A device
that cannot limit charging does not keep serving behind a soft warning nobody
reads.

### 3.4 Power-path changes get a named reviewer and a named failure mode **[REVIEW]**

Any change touching charging, discharge, thermal limits, or power scheduling
requires a review that **explicitly states what happens on a device where the
change misbehaves**. "Looks good to me" is not a review of this class of change,
and a reviewer who cannot describe the failure should say they are not the right
reviewer.

### 3.5 Safety claims name their mechanism **[REVIEW]**

A safety statement in the interface names the mechanism it was verified through,
in language a non-technical person can act on. "Charging limited" is not a claim;
"charge ceiling held at 60% for 30 days, read back from
`charge_control_end_threshold`" is.

### 3.6 Physical inspection is always named **[NORM]**

Wherever swelling is discussed, physical inspection is named as the definitive
check. Charter III.4. **Software cannot see a swollen cell. A person can.**

### 3.7 The safest configuration is offered where it exists **[NORM]**

Where a device runs without its battery, that is presented as the safest
configuration. Charter III.5.

### 3.8 A safety limit is stated, never softened **[REVIEW]**

The correct response to a safety limit is to state it plainly. It is never to
find a gentler wording. If the honest sentence makes the product look bad, the
product looks bad.

---

## Article 4 — Honest reporting governance

Charter Article IV, inherited deliberately from the Vayu family's audit
subsystems.

### 4.1 No generic success value **[CI]**

The result type may not gain `Ok`, `Pass`, `Clean`, `Fine`, or `Good`. Each would
provide somewhere for "not checked" to be recorded as "checked and fine".

### 4.2 Absent, unverified and present stay three answers **[CI]**

They are distinct variants and may not collapse. **Absence is never protection.
What could not be checked is never clean.**

### 4.3 No default tier **[CI]**

Tier detection keeps an `Unknown` verdict, and `Unknown` satisfies no capability
floor. A device nothing recognised is not quietly treated as a device we
understand.

### 4.4 A check that did not run reports *unverified* **[CI]**

This binds the project's own toolchain, not only the device report.
[`scripts/hardware-gate.sh`](scripts/hardware-gate.sh) prints `UNVERIFIED` when
its schema validator is missing, and `VAYUCELL_REQUIRE_SCHEMA_VALIDATOR=1` makes
that a hard failure in the authoritative run. **A gate that silently skipped is
not a gate that passed.**

### 4.5 Gates state what they cannot check **[CI]**

The charter gate prints the four Charter articles that are human-review-only on
every single run. A gate list that appears complete teaches its reader to stop
looking for the gaps — which is exactly how a gap survives.

### 4.6 Permanent failing rows stay red **[REVIEW]**

For every limit outside the project's control, a failing row exists and no
configuration clears it. Charter IV.4. **A report in which everything eventually
turns green teaches its reader to stop reading it**, and then the one row that
matters goes unread.

### 4.7 An empty log is not evidence **[NORM]**

No absence of reports — CSP violations, governor faults, backup failures — is
ever presented as evidence of health. It is presented as an absence of reports.

### 4.8 Uncertainty is reported at its real width **[REVIEW]**

Where the honest answer is a range or a "probably", it is shown as one.
Estimated values are labelled estimated. Swelling is estimated, never detected,
and is labelled so everywhere it appears.

---

## Article 5 — Evidence governance

The article that separates this project's engineering from its documentation.

### 5.1 Guards are mutation-tested **[CI]**

Every guard whose failure would be a safety or honesty problem is re-broken by
[`scripts/mutation-gate.sh`](scripts/mutation-gate.sh), and the matching test
must go red.

**A green suite proves the code passes its tests. It does not prove the tests
would notice if the code were wrong.** Those are different claims and only the
second one is worth anything.

### 5.2 Gates are self-tested **[CI]**

[`scripts/gate-selftest.sh`](scripts/gate-selftest.sh) plants each violation in a
scratch copy of the repository and requires the matching gate to catch it, citing
the correct rule.

A gate whose pattern silently stops matching prints `ok` forever and **has no
other symptom**. There is no error, no warning, no slowdown. Self-testing is the
only thing that finds it.

### 5.3 Proofs must actually run **[CI]**

`compile_fail` doctests live on public items, and CI asserts the **count** of
doctests run is non-zero rather than trusting the exit code. Moved onto a private
item, a proof runs zero tests and still prints `test result: ok`.

### 5.4 New behaviour names its failing test **[REVIEW]**

A pull request adding behaviour names the test that would fail if the behaviour
were wrong, or states why one is not possible. The pull request template asks for
exactly this. **A change with no test that could have caught it is a change
nobody has shown the suite can detect.**

### 5.5 Harness defects are recorded, not quietly fixed **[NORM]**

When a gate, test, or harness is found to be defective, the defect is written
into the commit message and into [`docs/CI.md`](docs/CI.md). Four have been so
far, and each is in the record because the pattern is more useful than the fix:

- A mutation that never applied and was scored as *survived*.
- A restore that preserved mtimes, so cargo re-ran the mutant against restored
  source and the whole run was meaningless.
- A charter check that matched an enum's own definition and failed loudly for the
  wrong reason.
- A hardware check reading a field the schema does not have, printing `ok`
  forever while checking nothing.
- A CSP test that passed a single source and so never reached the branch it
  claimed to cover — found by the mutation gate, not by review.

### 5.6 Tests are named for consequences **[NORM]**

A test name states what is at stake if the behaviour breaks.
`a_status_file_that_will_not_say_who_we_are_is_never_root` tells a reader why
they should care. `test_parse_euid` does not.

### 5.7 Coverage is a floor, never a target **[CI]**

`cargo llvm-cov` with a line floor, measured over production code only —
counting test files inflates the figure with the coverage of the tests
themselves. The floor catches whole modules landing untested. **A percentage says
how much code ran, not whether anything was checked while it ran.** §5.1 is the
check that answers that.

---

## Article 6 — Security governance

### 6.1 Unsafe code is forbidden twice **[CI]**

`#![forbid(unsafe_code)]` in the crate **and** `unsafe_code = "deny"` in the
manifest. Both are checked, because either one alone can disappear in a diff that
looks unrelated to memory safety.

### 6.2 The unsafe CSP keywords are unrepresentable **[CI]**

The Content Security Policy is built from a type with **no variant** for
`'unsafe-inline'` or `'unsafe-eval'`. See
[ADR-0006](docs/adr/ADR-0006-content-security-policy.md).

Weakening it is not a one-word edit to a string constant on a Friday. It is an
addition to a public enum, next to documentation explaining why the variant is
absent — a diff nobody merges by accident.

### 6.3 The CSP denies by default **[CI]**

`default-src 'none'`, not `'self'`. With `'self'`, a directive nobody enumerated
silently inherits same-origin permission and the policy's coverage becomes a
question of what its author remembered. With `'none'`, a forgotten directive
**fails closed**.

### 6.4 Script executes only with a single-use nonce **[CI]**

`script-src 'nonce-…'` and never `'self'`. The `Nonce` type is not `Clone` and is
consumed when rendered, so reuse requires generating another. A repeated nonce is
exactly as strong as `'unsafe-inline'` while continuing to read as a strict
policy in every audit that only looks at the header.

### 6.5 Violation reports never leave the device **[CI]**

`report-uri` must be a same-origin path. Aggregate violation data across every
install would be genuinely useful to this project — **which is precisely why
Charter Article V.2 forbids collecting it.** The useful thing and the forbidden
thing are the same thing.

### 6.6 No secrets, anywhere, ever **[CI]**

No verified secret in the working tree or in the history. Not in an example, not
in a test fixture, not in a comment.

### 6.7 Supply chain **[CI]**

No published advisory, no wildcard version, no build script, no git source, no
unused declaration, no drifting lockfile. See [`deny.toml`](deny.toml).

The policy is strict while the dependency tree is empty **on purpose**, so that
the first crate anyone proposes has to argue against rules written before there
was any pressure to relax them.

### 6.8 A build script is a decision, not a default **[CI]**

`allow-build-scripts = []`. A build script runs arbitrary code on every
contributor's machine and every CI runner. For a project whose entire claim is
that you can verify what you are running, that is an ADR-level decision.

### 6.9 Private disclosure, then public **[REVIEW]**

Vulnerabilities are reported privately and disclosed once a fix exists or after
90 days, whichever comes first. See [`SECURITY.md`](SECURITY.md).

### 6.10 The vendor kernel is not trusted, and not defended **[NORM]**

An abandoned vendor kernel is not secure, and this project will not imply
otherwise. Charter Article II. Where the platform is the weak point, the
documentation says the platform is the weak point.

---

## Article 7 — Privacy and data governance

### 7.1 The device is the boundary **[CI]**

No production source may reference a host this project operates. An installed
cell whose owner never contacts the project again **must keep working
indefinitely** — Charter Article V.5, and the test of the whole Charter.

### 7.2 Measurement is aggregate, opt-in, and count-only, or it does not exist **[REVIEW]**

Charter V.2 permits exactly that and nothing more. Any proposal for measurement
states what is counted, what the count could reveal about one person, and how the
opt-in is obtained. **If the answer to "could this identify a device, a person, or
a place?" is anything but a clear no, the answer is no.**

### 7.3 The hardware database carries no identifiers **[CI]**

Device profiles are validated against a schema with `additionalProperties: false`,
so a field nobody designed cannot arrive in a contributed record.

### 7.4 The operator's data is theirs, in a format they can read **[NORM]**

Backups and exports are readable without VayuCell. A backup only this software
can restore is a dependency on this software, which is the thing the project
exists to reduce.

### 7.5 Deletion means deletion **[REVIEW]**

Where the operator deletes something, it is deleted — not flagged, not retained
for a window, not kept in an index.

---

## Article 8 — Architecture and code governance

### 8.1 Formatting and lint **[CI]**

`cargo fmt` clean. `clippy` pedantic at `-D warnings`, across **all targets
including tests** — a lint that skips test code lets test code drift into habits
the library forbids.

### 8.2 The declared MSRV is built, not merely declared **[CI]**

Declaring a minimum supported Rust version and never building against it is a
claim nobody verified, which Article 4 forbids everywhere else.

### 8.3 No third-party runtime dependency in the core **[CI]**

Unless an ADR admits it. ADR-0005 §5.1 and Charter V.5. Zero dependencies means
zero dependencies to audit for a network call nobody reviewed.

### 8.4 Every target that matters is compiled **[CI]**

64-bit and 32-bit Android, 64-bit and 32-bit mainline ARM, and a development
host. `fail-fast: false`, so one broken target does not hide the state of the
others.

### 8.5 The release build is reproducible **[CI]**

Built twice from a clean tree, compared by hash. People are asked to run this
unattended on hardware in their homes; **"check for yourself" has to be a real
offer**, and a binary that differs between two builds of the same source cannot
be independently verified by anyone.

### 8.6 Invalid states are made unrepresentable where possible **[NORM]**

The recurring technique of this codebase, and the reason for the language choice
in ADR-0005: a registry whose obligations have no valid zero value, an enum with
no unsafe variant, a nonce that cannot be reused, a verdict that cannot leak a
tier. **Where a rule can live in a type instead of in a reviewer's memory, it
lives in the type.**

### 8.7 Prefer deleting to configuring **[NORM]**

A configuration option is a permanent obligation to test both paths. Where one
path is right, ship one path.

### 8.8 SPDX headers **[CI]**

Every source and script file carries one.

---

## Article 9 — Decision records

### 9.1 Expensive-to-reverse decisions get an ADR first **[REVIEW]**

Before the code, not after. An ADR written afterwards is a justification, and a
justification is not a decision record.

### 9.2 An ADR records what was rejected **[REVIEW]**

And why. **The rejected options are the part a future reader actually needs** —
the chosen one is visible in the code.

### 9.3 ADR integrity is mechanical **[CI]**

Filenames, numbers and titles must agree; numbering must be contiguous from
0001; no ADR may be an orphan; every relative link in the documentation must
resolve. A dead link into the founding documents is a reader who cannot reach the
argument they were sent to check.

### 9.4 Superseded decisions stay readable **[NORM]**

Marked superseded, never deleted. **Deleting them is how a project forgets why it
stopped doing something**, and then does it again.

### 9.5 An ADR may record that its own first draft was wrong **[NORM]**

Three already do, in a §0 section at the top. This is not self-flagellation: the
wrong first answer is usually the answer a future reader is about to reach for,
and recording why it failed is more useful than presenting the conclusion as
though it were obvious.

---

## Article 10 — Release governance

### 10.1 Nothing ships that the gates have not passed **[CI]**

`ci-pass` is the single required check, and its list is generated from the whole
`needs` context rather than maintained by hand — a job added and forgotten would
otherwise be required in name and unenforced in fact.

### 10.2 A release states what was verified on hardware, and what was not **[REVIEW]**

Every device-facing behaviour in the test suite is exercised through a fake host
describing handsets nobody is holding. That is the right layer for a unit suite
and it is **not a substitute for a phone on a bench**. Release notes say which is
which.

### 10.3 No security fix ships silently **[REVIEW]**

An operator who does not know a fix was security-relevant cannot prioritise
installing it.

### 10.4 Version numbers do not imply stability that does not exist **[NORM]**

While the governor is unwritten, the version says so.

---

## Article 11 — Hardware database governance

### 11.1 Observations only **[NORM]**

A device profile records what was **observed on real hardware**, never what a
specification sheet claims.

### 11.2 An empty field is honest; a guessed one is not **[NORM]**

A field nobody tested is left empty. **A guessed field is worse than nothing,
because somebody will trust it with their mail.**

### 11.3 A verified claim names its evidence **[CI]**

A charge ceiling recorded as holding must name the sysfs node it was read back
from. An unreproducible safety claim is exactly the shape of thing this project
refuses to print.

### 11.4 Contradictory records are refused **[CI]**

A mechanism cannot be named where the capability is reported unavailable. A
ceiling cannot be verified to hold on a device with no mechanism. A tier recorded
as achieved must record how.

### 11.5 The database is CC0 and belongs to nobody **[NORM]**

Including to this project. It is the artefact most useful to people who never
run VayuCell at all, and it should outlive the software.

---

## Article 12 — Contribution, attribution, and conduct

### 12.1 DCO, never a CLA **[CI]**

Contributors keep their copyright. A contributor licence agreement concentrates
rights in one entity, and an entity holding all the rights can relicense
unilaterally — the standard mechanism by which open projects are taken private.
With copyright distributed, **relicensing VayuCell is practically impossible,
including by its own founders. That is the intent.** Charter Article VII.

### 12.2 The permanent record names a person **[CI]**

No assistant attribution in commit messages, source, or any pushed artefact, and
no commit authored by a bot address. Assistant tooling is welcome and unrestricted
in how it is used; what it may not do is appear in the record as an author,
because a reader years from now needs somebody to ask.

The single exemption is Dependabot, and it is narrow: a dependency bump carries no
decision until a person reviews and merges it, and **that human act is the
accountability**. The exemption covers authorship only.

### 12.3 Review is on the argument, not the author **[NORM]**

Seniority is not an argument. Neither is having written the original code.

### 12.4 Disagreement resolves on Article 1 **[NORM]**

Publicly, by naming which priorities are in tension. See Article 15.

### 12.5 A contributor may not be asked to certify what they cannot know **[NORM]**

A device report asks what was observed. It does not ask a contributor to vouch
that a phone is safe.

---

## Article 13 — Capture resistance and continuity

### 13.1 No hosted edition, ever **[REVIEW]**

There is one edition. Charter V.4. **A feature existing only in a hosted tier is
how a project acquires an interest in its users being unable to self-host**, and
that interest, once acquired, quietly shapes every roadmap decision afterwards.

### 13.2 No infrastructure the project cannot lose **[CI]**

Charter V.5. If this project's infrastructure vanished tomorrow, every installed
cell must keep working. §7.1 enforces the code half.

### 13.3 The founder is not a single point of failure **[REVIEW]**

Anything only one person can do — signing, publishing, the domain, the release
key — is documented and recoverable by someone else. **A project that dies with
its founder was never really the user's.**

### 13.4 The Charter's core is beyond ordinary amendment **[CI]**

Charter Articles III and V may not change without re-recording their SHA-256
digest, which puts the amendment in the diff where review will see it rather than
arriving inside an unrelated change. Charter Article IX.

### 13.5 Everything here is CC0 **[NORM]**

The Charter, this constitution, the hardware database, and the specifications.
Fork them, rewrite them, use them for a project that competes with this one. **A
governance model only its author may copy is not governance, it is branding.**

---

## Article 14 — Sustainability, stated honestly

### 14.1 There is no revenue model, and that is a real risk **[NORM]**

No token, no treasury, no fee, no hosted tier, no telemetry to sell. Charter
Article V.1 forecloses every ordinary way of funding this.

The honest consequence: **VayuCell is maintained on donated attention, and
donated attention is not reliable.** This is written down rather than glossed
because a user deciding to depend on this software is entitled to know it.

### 14.2 The mitigation is design, not funding **[NORM]**

Which is why §13.2 and Charter V.5 matter more here than anywhere else. The
project is designed so that **its own death is survivable by its users**. That is
a weaker promise than "we will be here", and it is one that can actually be kept.

### 14.3 Complexity is a running cost paid by future maintainers **[REVIEW]**

Every option, dependency, and abstraction is borrowed against attention that may
not exist. Prefer the boring implementation.

---

## Article 15 — Conflict resolution

### 15.1 Procedure **[NORM]**

1. State the disagreement in terms of Article 1: which priorities are in tension.
2. Establish the facts. Most disagreements are about facts wearing the costume of
   a disagreement about values.
3. If the facts settle it, it is settled.
4. If not, the higher priority in Article 1 wins.
5. If both sides are at the same priority, it needs an ADR — which forces the
   rejected option to be written down, and often resolves it.

### 15.2 Safety and honesty concerns are never closed on procedure **[REVIEW]**

An issue raised under Article 3 or Article 4 is not closed for being stale,
off-template, badly worded, or raised by someone with no standing. **It is closed
when it is answered.**

### 15.3 Disagreements are public **[NORM]**

The reasoning has to be readable later by someone who was not there, including
someone deciding whether to trust the result.

---

## Article 16 — Amendment

### 16.1 Ordinary amendment **[REVIEW]**

A pull request changing this document, reviewed like any other change, stating
which rule changes and why.

### 16.2 Void amendments **[REVIEW]**

An amendment is **void**, not merely rejected, if it would:

- place this document above `CHARTER.md`;
- weaken Charter Article III or V indirectly through an operational rule;
- remove the enforcement classification in §0.3;
- or introduce a rule with no traceable authority under §0.2.

Article 0 is the ordering, and **the ordering is not amendable from below**.

### 16.3 Downgrading a rule is an explicit act **[REVIEW]**

Moving a rule from [CI] to [REVIEW], or [REVIEW] to [NORM], requires the reason
in the commit message. Silently deleting a gate while leaving the rule text is
forbidden by §0.3.2.

### 16.4 Charter digests **[CI]**

Enforced mechanically. See §13.4.

### 16.5 Fork freely **[NORM]**

CC0. If this governance stops serving its users, the correct response is to take
it and do better, and that response should be available without anyone's
permission.

---

## Appendix A — Rules by enforcement

| Enforcement | Count | What it means |
|---|---|---|
| **[CI]** | 39 | A gate fails the build |
| **[REVIEW]** | 25 | A person must catch it |
| **[NORM]** | 29 | Advisory, and labelled as such |
| **Total** | 93 | |

**These counts are checked by [`scripts/docs-gate.sh`](scripts/docs-gate.sh).**
They were wrong in the first draft of this document — off by six — and nothing
would have noticed. A governance document that miscounts how much of itself is
actually enforced is committing the precise error Article 4 forbids in a device
report, against the reader of the governance instead. So the count is now a gate,
and a rule added without updating this table fails the build.

Every [CI] rule is documented job by job in [`docs/CI.md`](docs/CI.md), and each
gate is proven to fire by
[`scripts/gate-selftest.sh`](scripts/gate-selftest.sh).

---

## Appendix B — What this constitution cannot do

Stated here on the same principle the gates apply to themselves. A governance
document that lists only its strengths is making the exact error Article 4
forbids.

1. **The [REVIEW] rules are the weak point, and they include the most important
   ones.** §3.4 — a real review of a power-path change — depends entirely on one
   person's attention on one day. No gate can read a diff and tell whether the
   reviewer genuinely thought about what happens at 45 °C. Anyone who can see how
   to make one of these mechanical should say so; that is a valid issue.

2. **It cannot make a reviewer competent.** It can only ensure the right question
   is asked.

3. **It cannot prevent a determined maintainer with commit access from
   dismantling it.** The digests, the gates and the CC0 licence make dismantling
   it *visible* and make forking *possible*. That is the whole of the protection,
   and it is worth being precise that it is not more.

4. **It cannot fund the project.** See Article 14, which is the honest version of
   this document's largest risk.

5. **It cannot verify anything on real hardware.** Every gate here runs on a
   Linux runner. The phone on the bench is still the phone on the bench.

6. **It is version 1.0 and has not been tested by an actual conflict.** Governance
   is only proven by the first time it stops somebody from doing something they
   badly wanted to do. That has not happened yet.
