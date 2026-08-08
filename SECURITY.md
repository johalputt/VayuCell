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
