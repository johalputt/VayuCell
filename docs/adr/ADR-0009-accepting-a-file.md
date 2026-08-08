# ADR-0009 — Accepting a file: the first surface that takes rather than gives

**Status:** accepted — the decision layer is implemented in `core/src/vault.rs`.
No network route accepts an upload yet; see §6.
**Supersedes:** nothing
**Related:** [ADR-0002](ADR-0002-battery-safety-governor.md) (the governor this
obeys), [ADR-0004](ADR-0004-storage-durability.md) §0 and §1 (why no receipt may
say "saved"), [ADR-0008](ADR-0008-publishing-a-site.md) (the read surface this is
deliberately stricter than), [CHARTER.md](../../CHARTER.md) Articles III.1 and IV

---

## §0. What is different about accepting

Every surface before this one gives. A panel renders what was read; a site serves
what the operator already put there. Both can be wrong, and when they are, the
cost is a bad answer.

Accepting a file is a different kind of act. The two failures that matter have no
counterpart in a reader:

1. **A write that half-lands.** The event that interrupted it is over in seconds;
   the damaged file is there until somebody notices, and "noticing" means opening
   the one document they needed.
2. **A write that lands when the device was in no condition to take it.** The
   phone is hot, or on battery, or three minutes from a shutdown — and it has
   just told somebody their file is on it.

Neither is fixed by care at the call site. They are fixed by deciding *before*
any byte moves, which is what this module is.

## §1. Decision: the decision layer performs no I/O

`core/src/vault.rs` validates the name, checks the room, asks the governor, and
returns the **ordering** a caller must follow. It never opens a file.

That is not fastidiousness. The interesting cases — a name that is really a path,
a quota that has been lowered below current usage, an upload arriving on the
`Announced` rung — are all reachable in a test with no filesystem, and a module
that owned the bytes could only be tested by writing them.

## §2. A write is refused earlier than a read, and the asymmetry is the point

ADR-0008 keeps a website served at `DERATED`, because reading a file is not what
is heating the device and shedding a negligible load is theatre.

A write is refused there.

| Condition | Site (ADR-0008) | Vault |
| --- | --- | --- |
| `NORMAL`, mains | serves | **accepts** |
| `DERATED` | serves | refuses |
| `PROTECT`, `HALT` | refuses | refuses |
| `Stage::Announced` | serves | refuses |
| `Stage::Shed` and below | refuses | refuses |

**A refused upload costs somebody one retry. A half-written file outlives the
event that interrupted it.** That is the entire argument, and it is why exactly
one of the twenty combinations of level and rung accepts anything — asserted
exhaustively, so a level or rung added later cannot fall through to a default
that takes files.

`Stage::Announced` refusing is **not a new policy**. That rung's own obligation,
written before this module existed, is *"told the fleet and stopped accepting new
work"*. An upload is new work. The ladder already said so; this reads it.

A cell that cannot be read yields `PROTECT`. Absence is never protection.

## §3. The refusal and the plan are one decision

`Admission::plan` returns `Option<WritePlan>` and yields `None` when the vault is
refusing. A caller cannot obtain a plan for a write the device has declined.

Splitting "may I?" from "how?" is how a check gets skipped by somebody in a
hurry, and the skip is invisible in review because both calls are still there.

## §4. The ordering is the correctness argument, so it is data

`WritePlan::steps()` returns four steps in the only order that survives a power
cut:

| Step | Why |
| --- | --- |
| `WriteTemporary` | so a crash leaves debris rather than a damaged file under the real name |
| `FlushFile` | so the rename does not publish a name whose bytes are still in cache |
| `RenameOverDestination` | the one atomic step: the old file or the new one, never half of either |
| `FlushDirectory` | so the rename itself survives — without it the entry is what is lost |

The last one is the step everybody forgets, and its absence is undetectable until
a real power cut.

The temporary is written **beside the destination**, not in `/tmp`: a rename
across filesystems is a copy, and a copy is not atomic. It is also **hidden**, so
a partially written upload can never be served by ADR-0008's site, which refuses
hidden names as a class. Two modules, one property, neither depending on the
other's discipline.

Returning the order as data rather than performing it means a test with no
filesystem asserts the *ordering*, and a mutation that swaps two steps turns that
test red.

## §5. No receipt may say the file is safe

`Receipt` has no `Durable` variant and will not get one.

ADR-0004 §0: a sealed phone cannot drop its own storage rail, so nothing running
on it can distinguish a flash that honoured a flush from one that acknowledged it
and did nothing. A receipt saying "saved" would be exactly the lie Charter
Article IV exists to prevent, issued by the feature meant to uphold it.

The class is fixed at `DurabilityClass::AssumedUntrusted` rather than taken as an
argument — a caller able to choose the class is a caller able to choose a
flattering one, on the single field a person would actually rely on. What the
receipt says is where the bytes are, that they are nowhere else, and that a copy
should exist somewhere the operator controls.

A test asserts the rendered text contains none of *saved*, *safe*, *durable*,
*guaranteed* or *backed up*.

## §6. What this is not, yet

**There is no upload route.** `serve::Method` still has only `Get` and `Head`.
Nothing on the network can reach this module, and adding a route is a separate
change that must arrive with the thing this deliberately does not include:

**Authentication.** An unauthenticated write endpoint on a home network is
writable by every device on that network, including the ones the operator did not
choose. Deciding *whether* to take a file is settled here; deciding *whose* file
it is is not, and shipping the first without the second would be worse than
shipping neither.

It has not run on a phone.
