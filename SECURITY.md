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

## Security vulnerabilities

Standard coordinated disclosure. Please report privately first and allow time for
a fix before publishing.

## What we will not do

- Claim a fix is verified before the read-back test passes.
- Silently correct a safety-affecting claim. If published wording overstated a
  guarantee, **the correction is published too**, naming what was wrong — the
  practice ADR-0150 in the sibling project established.
