# ADR-0012 — Replication by receipt: the companion claims, the cell quotes

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by `core/src/replica.rs`, the `--replica-evidence`
flag on `vault`/`all`/`report`/`status`, and the `replicate`/`drill`
commands of the `vayucell-sync` crate.

## Context

[ADR-0004] fixed what this project may claim about its own storage: no
variant meaning *durable*, a lag that carries when it was measured, a
restore drill that expires, and an unrestored backup that can never read
as proven. For most of this repository's life those types had no producer,
and the roadmap said so plainly — P6's gate could not close while the lag
and the drill described a subsystem that did not exist.

The missing subsystem is a replicator. Two earlier decisions constrain
what one can be here:

- The cell never dials ([charter] Article V, [ADR-0011]). Whatever makes
  copies, it is not the phone reaching out to learn the world.
- A claim is not a control ([constitution], Article IV). Code whose job is
  to *assert* something about a backup is not the same as code whose job
  is to have *verified* it, and the difference has to survive contact
  with the panel that renders it.

So there are two machines and exactly one direction of honesty between
them. The laptop-side companion may dial the vault; it is the only thing
that can know anything about replication. The cell cannot measure any of
it — but the cell is also where the operator looks.

The naive bridge is for the cell to run the numbers itself from a file the
companion leaves behind. That inverts the honesty: a phone rendering
"your backup is 40 seconds behind" from arithmetic it performed on a
timestamp is presenting somebody else's claim as its own measurement, and
every failure mode of the chain — the companion died, the clock skewed,
the file was truncated — would surface as a confidently wrong number.

## Decision

**Replication by receipt.** The companion writes dated claims; the cell
quotes them, worded as claims, aged against its own clock, and refuses to
improve them.

1. **Two verbs on the companion, each leaving evidence.**
   `vayucell-sync replicate` pulls the whole listing into a mirror folder
   — downloads for anything missing or changed by size or mtime, deletion
   of local ghosts only behind `--prune`, every write landed by flush and
   rename under a temporary name first. `vayucell-sync drill` downloads
   every listed file **afresh** and compares it against the mirror byte
   for byte; both reads are independent — wire and disk — which is the
   whole point. Either command finishes completely or writes nothing:
   `--receipt <FILE>` is required up front, and a cycle that dies halfway
   leaves the previous receipt standing, where it ages out and starts
   reading as *nobody measuring*.

2. **A receipt format too small to lie interestingly.** A JSON array of
   `{kind, completed_unix, bytes, ...}` records, ASCII only, no escapes,
   duplicate fields refused, trailing bytes refused, parsed byte-by-byte
   in `replica.rs`. The parser's refusals name themselves, because an
   evidence file the cell cannot read is not whitespace to skip around —
   it is the moment to stop trusting the chain.

3. **The cell renders, and says so.** `--replica-evidence <FILE>` feeds
   the receipt into the same staleness rules [ADR-0004] already pinned:
   inside five minutes a lag shows with its measurement time; past it,
   nothing is shown except that nobody is still measuring; a stamp ahead
   of this cell's clock is refused whole rather than clamped to a
   flattering zero; an unreadable file breaks both sentences openly. Every
   rendered line carries *"as claimed by the replica's own receipt"*, the
   section opens by naming the file, and the startup banner changes from
   "this phone is the only copy" to the dated claim — never to a
   verification. The flag is refused on commands with no storage section,
   because an argument that silently does nothing is worse than one that
   is rejected.

4. **Overwriting evidence is not adding to it.** `upsert` parses the
   previous receipt text before writing the new one and refuses to touch a
   file it cannot parse. A companion that bulldozed unreadable evidence
   with fresh-looking evidence would be destroying the record precisely
   when the record was trying to say something.

## Consequences

The P6 gate still does not close: everything above runs on a machine with
a Rust toolchain talking to another machine over a wire, and the gate ends
in a sentence about a handset. What closes is the gap the roadmap named —
the lag and the restore drill now describe something that exists, and the
panel can quote it without pretending to have measured it.

The drill compares two reads but restores nothing anywhere; calling it a
restore *drill* is precise and the docs keep it that way. A true restore
into a wiped folder is a future decision, and nothing in this one claims
it.

[ADR-0004]: ADR-0004-storage-durability.md
[ADR-0011]: ADR-0011-synchronising-a-folder-to-a-vault.md
[charter]: ../../CHARTER.md
[constitution]: ../../GOVERNANCE-CONSTITUTION.md
