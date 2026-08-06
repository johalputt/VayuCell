# The VayuCell Charter

**Version 1.0 · Licence: CC0-1.0 (public domain dedication)**

This charter is dedicated to the public domain. It belongs to nobody, and
therefore to everybody. Any person or project may implement it, fork it, embed
it, standardise it, or compete with it, with no permission and no attribution
required.

---

## Article I — Purpose

VayuCell exists to convert retired mobile hardware into servers that their owners
control.

Billions of functional smartphones have been retired. Each contains a multi-core
64-bit processor, several gigabytes of memory, fast onboard storage, two
independent network paths, and an integrated battery. Each idles at a small
fraction of the power a dedicated server board consumes. Almost all of them are
in drawers, in bins, or in a waste stream.

At the same time, the people who own that hardware rent their infrastructure from
companies that read their data, raise their prices, and can remove them without
appeal.

**Those two facts are the same problem seen from two sides.** VayuCell exists to
join them: to make the hardware people already own into the infrastructure they
would otherwise rent.

---

## Article II — The claim, and its limits

VayuCell makes one claim, worded so it can be defended under hostile reading:

> **VayuCell turns a retired phone into a server whose capabilities are
> enumerated rather than assumed, whose battery risk is actively governed rather
> than ignored, and which never reports a capability it has not verified on the
> device actually in front of it.**

The project binds itself to what that claim does **not** say:

1. **Not** that every phone can be a server.
2. **Not** that a phone matches a datacentre.
3. **Not** that the battery is safe. Risk is measured and bounded, never removed.
4. **Not** that an abandoned vendor kernel is secure.
5. **Not** that a rented relay is independence.

A project that overstates its guarantees puts real hardware in real homes on the
strength of a sentence nobody checked. **Overstating a safety property is the one
failure this charter treats as disqualifying.**

---

## Article III — Safety of persons comes first

VayuCell asks people to leave lithium cells energised, warm, and unattended, for
years, in buildings where they sleep.

Therefore, as a permanent constraint that no release schedule may override:

1. **No capability that serves traffic may ship before the battery governor.**
2. **A device on which charge cannot be limited must be told so plainly**, in the
   first screen the user sees, in language a non-technical person understands.
3. **The project must never imply a safety property it cannot verify by reading
   the result back** from the hardware.
4. **Physical inspection is always named as the definitive check**, because
   swelling is visible to a person and not measurable by software.
5. **Where a device supports operating without its battery**, that option is
   presented as the safest configuration.

The correct response to a safety limit is to state it. It is never to soften the
wording.

---

## Article IV — Honest reporting

Inherited deliberately from the Vayu family's audit subsystems, and binding here:

1. **A passing indicator means verified**, not configured, not enabled, not
   attempted.
2. **Absence is never protection.** A capability that is not present is reported
   as absent, never as defended.
3. **What cannot be checked is reported as unverified**, never as clean.
4. **Permanent failing rows exist** for every limit outside the project's
   control, and no configuration clears them. A report in which everything
   eventually turns green teaches its reader to stop reading it.
5. **A control that cannot be read back after being set may not be reported at
   all**, because it is indistinguishable from one that silently stopped working.

---

## Article V — What VayuCell will never contain

1. **No token, no treasury, no fee, no mandatory account.** A project whose
   purpose is reducing dependence must not create a new one.
2. **No telemetry that identifies a device, a person, or a location.** Aggregate,
   opt-in, count-only measurement is permitted; nothing else is.
3. **No remote control path the owner cannot sever.** The owner of the hardware
   is the final authority over it, without exception.
4. **No capability that exists only in a hosted edition.** There is one edition.
5. **No dependency on a service the project controls.** If the project's
   infrastructure vanished tomorrow, every installed cell must keep working.

Article V.5 is the test of the whole charter. A cell whose owner never contacts
the project again must continue to function indefinitely.

---

## Article VI — Licensing

| Artefact | Licence |
| --- | --- |
| This charter, and all specifications | **CC0-1.0** |
| Source code | **Apache-2.0** |
| Hardware compatibility database | **CC0-1.0** |
| Documentation | **CC-BY-4.0** |
| Name, logo, visual identity | Trademark policy — see `TRADEMARK.md` |

**Why the code is Apache-2.0 and not MIT.** VayuCell operates charging circuitry,
power management, virtualisation and radio behaviour — among the most
patent-dense territory in consumer electronics. Apache-2.0 grants every user an
express patent licence from every contributor, and terminates that licence for
any party who initiates patent litigation over the work. MIT grants no patent
rights at all. For a project inviting the public to repurpose hardware built by
large patent holders, that clause is what keeps the work usable in a decade.

**Why the charter and specifications are CC0.** The rules of a commons must not
be owned. CC0 permits an entirely independent implementation with no relationship
to this project — which is the property that makes the specification, rather than
this codebase, the durable artefact.

---

## Article VII — Governance, and resistance to capture

1. **Contributions are made under a Developer Certificate of Origin, never a
   Contributor Licence Agreement.** Contributors keep their copyright. Because no
   single entity holds the rights, **no single entity can relicense the project**
   — closing the standard route by which open projects are taken private.
2. **Changes to this charter require a public proposal** through the VayuCell
   Improvement Proposal process (`GOVERNANCE.md`): numbered, archived, and
   discussed in the open before adoption.
3. **Releases are reproducible and signed.** A user must be able to verify that a
   published binary corresponds to published source.
4. **The specification is complete enough to reimplement.** If this codebase were
   ever abandoned or mismanaged, the charter and specifications survive it intact
   and unencumbered.
5. **Forking is a right, not a failure.** The trademark policy exists solely to
   prevent a fork misrepresenting itself as official — never to impede forking.

---

## Article VIII — Standing obligations to the user

Every VayuCell installation owes its owner:

1. **The truth about the device**, including what it will never be able to do.
2. **A way out.** Data is exportable in documented formats, at any time, without
   contacting anyone.
3. **A recovery path that needs no console**, because the target user has a phone
   and no serial cable.
4. **An itemised account of every external dependency** — every relay, every
   remote service — counted as a dependency and named as one.
5. **No irreversible action without an explicit confirmation** that states what
   cannot be undone.

---

## Article IX — Amendment

This charter may be amended only through the proposal process in
`GOVERNANCE.md`, and only by a proposal that states plainly what protection is
being removed or weakened, and why.

**Articles III and V may not be weakened by amendment.** They may be
strengthened, clarified, or extended. A proposal to weaken them is out of order,
and any implementation that violates them is not VayuCell, whatever it is called.

---

*Dedicated to the public domain under CC0-1.0. No rights reserved.*
