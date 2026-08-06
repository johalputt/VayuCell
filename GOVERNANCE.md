# VayuCell governance

**Licence: CC0-1.0** — like the charter, this document belongs to nobody.

## 1. Contributions: DCO, never a CLA

Contributors sign off with a Developer Certificate of Origin. They **keep their
copyright**. There is no Contributor Licence Agreement and there will not be one.

This is a structural protection, not a preference. A CLA concentrates rights in
one entity, and an entity holding all the rights can relicense the project
unilaterally — the standard mechanism by which open projects are taken private.
With copyright distributed across every contributor, **relicensing VayuCell is
practically impossible**, including by its own founders. That is the intent.

Sign off with:

    git commit -s

## 2. VCIP — VayuCell Improvement Proposals

Modelled on VayuWeb's VWIP process.

| Change | Route |
|---|---|
| Bug fix, documentation, device record | Ordinary pull request |
| New capability, new tier behaviour, protocol change | **VCIP required** |
| Charter amendment | **VCIP required**, with the weakened protection named explicitly |

A VCIP is numbered, archived in `docs/vcip/`, and public from the moment it is
opened. It must state: the problem, the proposed change, what it costs
operationally, what it forecloses, and — for anything touching Article III — the
safety analysis.

**Charter Articles III and V may not be weakened by any VCIP.** They may be
strengthened, clarified or extended. A proposal to weaken them is closed as out
of order.

## 3. Release discipline

Inherited from the sibling projects, and binding here:

1. **The adversarial pass gates a release; it does not trail it.** Attacks are
   written first, as failing tests, in the attacker's voice.
2. **Every fix is mutation-tested.** Re-break the defence; the test must go red
   again. A test that passes against the broken version proves nothing.
3. **Assertions are artifact-level, not transport-level.** Assert on what the
   hardware reports back, never on whether a write returned success.
4. **Releases are reproducible and signed.** A user must be able to verify that a
   published binary corresponds to published source.

## 4. Decision-making

Rough consensus among maintainers, in public, on the VCIP. Where consensus fails,
the maintainers decide and **record the dissent in the VCIP**. A decision whose
objections are unrecorded is not a decision anyone can revisit.

Safety objections under Article III are different: **any maintainer may block a
release on a safety ground**, and clearing that block requires the objection to
be answered on the record, not outvoted.

## 5. Forking

Forking is a right. The trademark policy exists only to stop a fork claiming to
be official — never to impede forking itself. A fork that renames itself is
entirely legitimate and needs no one's permission.
