# Security and safety reporting

Two intake paths, because this project has two kinds of serious defect.

## Safety defects — highest priority

Anything where VayuCell **reports a safety property it has not verified**, or
fails to enforce one it claims. In particular:

- A charge ceiling that is reported as held but is not.
- A governor state transition that does not fire at its threshold.
- Any surface implying charge control exists where it does not.
- Wording that implies swelling is detected rather than estimated.

These are treated as critical regardless of exploitability, because the
consequence is hardware in an unsafe state in someone's home, and the user has
been told it is fine. Per `GOVERNANCE.md` §4, **any maintainer may block a
release on a safety ground.**

## How to report

**Use GitHub's private vulnerability reporting:**
<https://github.com/johalputt/VayuCell/security/advisories/new>

That route is preferred over a public issue for anything in either category
above, including the safety defects — a surface that claims a charge ceiling it
is not holding is exploitable in the sense that matters, which is that somebody
is relying on it.

What to expect:

| | |
| --- | --- |
| Acknowledgement | Within 7 days |
| Assessment, with a severity and a plan | Within 14 days |
| Fix or a stated reason there will not be one | Within 90 days of the report |

If a report goes unacknowledged past those windows, escalate by opening a public
issue that says only that a private report is outstanding — no details. A
disclosure process nobody answers is worse than none, because it persuades a
reporter to stay quiet.

## Supported versions

There is one edition and one supported version: the current `main`. This project
publishes no long-term-support branch and will not claim to backport fixes it
does not backport.

## Standard coordinated disclosure

Please report privately first and allow time for a fix before publishing. If you
intend to publish on a fixed date, say so in the report — a deadline stated up
front is easier to work with than one that arrives later.

## What we will not do

- Claim a fix is verified before the read-back test passes.
- Silently correct a safety-affecting claim. If published wording overstated a
  guarantee, **the correction is published too**, naming what was wrong — the
  practice ADR-0150 in the sibling project established.

## Open Scorecard findings, and which of them we intend to close

OpenSSF Scorecard runs on every push to `main` (`.github/workflows/ci.yml`) and
its results appear under **Security → Code scanning**. **Three** checks are
open, and none of them is closed by changing code. They are recorded here
because an alert nobody explains is the same defect this project refuses
everywhere else: a red row that reads as "checked and failing" when it means
something narrower.

### Branch-Protection — protected, and the check now passes

`main` **is** protected. It requires the status check *CI Pass — all required
checks*, it refuses force-pushes, and it refuses deletion of the branch. The
scan agrees: its SARIF reports three results and Branch-Protection is not one
of them. The finding this section used to carry is closed.

One qualification, and it is the tool's own rule rather than ours: a run
without an administrative token scores the administrator-only requirements as
met. The requirements the scan *can* read are satisfied for real — no
force-push, no deletion, one required status check — and the job prints which
token it used on every run, so a number that is partly assumed never reads the
same as one that is wholly measured. A fine-grained token with
`Administration: read`, stored as the repository secret `SCORECARD_TOKEN`,
would make the rest measured too.

The classic protection is the one in force; `.github/rulesets/main.json` is
the same thing expressed as a ruleset, for anyone who would rather manage it
that way. It restricts deletions and blocks force pushes, and it deliberately
does **not** require a pull request, because that would contradict the next
section. Applying it is optional while classic protection is on:

```sh
gh api -X POST repos/johalputt/VayuCell/rulesets --input .github/rulesets/main.json
```

### Code-Review — a deliberate deviation

Scorecard measures whether commits arrived through an approved pull request.
Every commit here is pushed directly to `main`, so this check scores zero and
will keep scoring zero.

That is the stated development model, not an oversight, and the trade is written
down rather than hidden: this project has a single maintainer, so a self-approved
pull request would be review theatre — the metric satisfied and nobody's eyes on
the diff. What replaces it is mechanical and runs before every push: twenty-one
gates, a mutation gate that re-breaks each safety and honesty guard and requires
its named test to go red, and a gate self-test that plants violations to prove
each gate fires. **A second reviewer would be better than all of it.** Until
there is one, the honest statement is that this code is not peer-reviewed.

### Maintained — an artefact of a new repository

This check grades activity over ninety days against a repository created on
2026-08-06, so it resolves itself on **2026-11-04**. There is nothing to do
about it, and a project claiming otherwise would be claiming a history it does
not have.

### CII-Best-Practices — not registered

The badge requires registering the project at <https://www.bestpractices.dev/>.
Nothing here is missing for it; it has not been submitted. Adding a badge image
to the README would move nothing and would assert a certification nobody
awarded.

One more thing, checked on 2026-10-21: the endpoint the check queries,
`bestpractices.coreinfrastructure.org/api/projects`, now redirects to
`www.bestpractices.dev/api/projects` and answers **404 for every query,
including for projects that hold the badge today**. So the registration is
still worth doing — the API will come back and the check will ask — but
registering alone would not clear the finding while the API is in that state.
