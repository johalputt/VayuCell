# ADR-0009 — Accepting a file: the first surface that takes rather than gives

**Status:** accepted — implemented in `core/src/vault.rs`, with the route in
`serve::route_vault` and the write itself in `cli/src/listen.rs::write_durably`.
See §7 for what changed after this was first written.
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

### 4.1 The one operation that was not contained against links

ADR-0008 §2 records that containment against symbolic links belongs in the
binary, where real paths are resolved, and points at `read_contained`. The
`remove` path says the same thing in its own comment — canonicalised *"for the
same reason a read is"*. The sentence is equally true of a write, and a write was
the one operation of the three with no such check.

The temporary is the dangerous half. `OpenOptions::open` follows links, so a link
sitting at the `.partial` path is opened, truncated and filled with the uploaded
bytes **wherever it points** — and the `rename` afterwards moves the link rather
than the content, so the upload lands outside the vault and the vault looks
empty. The destination is the quieter half: `rename` replaces a link instead of
following it, so nothing escapes, but an operator's link is destroyed without a
word by a vault that would have refused to *read* through it. Two operations
disagreeing about the same file is its own defect.

Both are now refused before a byte is written, checked with `symlink_metadata`,
which reports the link rather than what it points at. **What that does not
close, stated rather than implied:** it is a check before an open, so a link
created in the gap between them is not caught by it. Winning that race needs
write access to the vault directory — the same user this process runs as — and
ADR-0010 §2 already says plainly that the same user is not an adversary this
design can hold off. The check is worth having because the ordinary way a link
ends up in a served directory is that the operator put it there.

### 4.2 A refusal tells the caller the class, and the log tells the operator where

`VaultIo`'s error was a `String`, documented as *"what went wrong, for the
operator's log"* — and `route_vault` put it straight into the response body. The
operator's log went to the caller. On a real deployment that meant a `PUT` was
answered with an absolute path to the vault directory.

ADR-0008 already settled this for reads, in as many words: a file that resolved
and could not be read answers exactly like a typo, because *"the operator gets
the reason in the log on the device they own; the wire gets the same 404 either
way."* The write path did the opposite of its sibling.

Two answers now, not one, because the status was also saying the wrong thing:

| | Status | When |
| --- | --- | --- |
| `StorageFailure::Conflict` | **409** | Something already stored blocks the write. The request was well formed and the server is not broken — the *target* is, and no retry will clear it |
| `StorageFailure::Failed` | **500** | The operation was attempted against the filesystem and did not complete |

A caller told 500 retries. A caller told 409 stops and tells somebody, which is
the only thing that will ever clear a symbolic link sitting in the vault.

`told()` is what reaches the wire and **never carries a filesystem path**; a test
asserts no separator appears in either variant's text. Which of the four ordering
steps failed is not told apart either — that is the operator's diagnostic, and
distinguishing them on the wire would leak the shape of the write ordering to
anything that can send traffic.

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

## §7. Since this was accepted

§6 said there was no upload route and that adding one required authentication
first. [ADR-0010](ADR-0010-per-device-credentials.md) settled the authentication;
`serve::Method` then gained `Put`, and `serve::route_vault` is the route.

`Put` and only `Put`. It names one file and replaces it, so it is idempotent and
a retry after a dropped connection is safe. `Delete` destroys somebody's data and
still deserves its own decision. `Post` has no meaning where nothing is appended
to.

`cli/src/listen.rs::write_durably` is the one place that acts on a `WritePlan`,
and it performs all four steps including the directory flush.

**One defect this found, recorded because it generalises.** `Response::render`
suppressed the body for any method that was not `GET` — correct when `Get` and
`Head` were the only verbs, and silently wrong the moment `Put` existed. Every
receipt and every error message a `PUT` produced arrived empty: an upload that
confirmed nothing, a refusal that explained nothing. The condition now asks which
verb *omits* a body, which is the form that stays correct when a verb is added.
It was found by running an upload, not by reading the diff.

## §8. Two things §2 claimed that the code was not doing

Recorded rather than quietly corrected, because both are the same failure: a
table in a document that no test held the code to.

**The read column existed only on paper.** §2's table says the vault refuses a
read at `PROTECT`, at `HALT` and at `Stage::Shed` and below, exactly as the site
does. `route_vault` consulted the device on the write path and nowhere else, so a
cell in enough trouble to stop serving a web page was still spinning storage up
for anybody enrolled. Reads now go through `site::Availability` — the same type
the site uses, so the two columns cannot drift apart again — and the refusal is
decided before the disk is touched, not filtered afterwards.

**The quota was a number rather than a limit.** `Quota` measured correctly and
`Admission` used it correctly; the caller built it once at startup with usage
fixed at zero and never asked again. The only upload it could refuse was one
file larger than the whole quota. Usage is now read from the directory before
every upload.

That measurement is I/O and I/O fails, which is the interesting half. A
directory that cannot be read refuses the write rather than counting as empty:
an unreadable usage figure is indistinguishable from free space, so treating it
as zero is a limit that silently stops being one. `Admission::of` takes an
`Option<Quota>` so no caller can skip the case, and `Refused::Unmeasured` is
kept apart from `Refused::Full` — "full" names a shortfall, which is a
measurement, and this refusal is precisely the absence of one.

A removal asks neither. `Admission::for_removal` consults the governor and the
ladder and does not look at the disk, because a vault that is full — or one
nobody can measure — is a vault somebody needs to be able to empty.

It has still not run on a phone.
