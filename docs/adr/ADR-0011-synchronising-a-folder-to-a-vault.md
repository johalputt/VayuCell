# ADR-0011 — Synchronising a folder to a vault

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by the `vayucell-sync` crate and the vault listing
route it reads.

## Context

The vault accepts files one at a time: `PUT` a name, read a name, delete a
name ([ADR-0009]). That is honest storage, but keeping twenty photographs in
step by hand means typing twenty commands and hoping you remembered which ones
changed. Every real user of a folder wants one verb that says "make that
match this".

The obvious design — a `vayucell sync` subcommand where the phone reaches out
to wherever the files live — is the one thing this architecture cannot do.
The cell is a device that listens ([charter] Article V): it has no business
dialing out to a laptop's address, learning that address, or carrying code
that knows how. The whole security story of running a server on a telephone
rests on the phone being a thing people talk *to*.

So the dialing belongs to the other end. The machine that holds the folder is
the one with somewhere to be; the phone stays where it is.

## Decision

**A companion binary, its own crate, whose only purpose is dialing.**
`sync/` builds `vayucell-sync`. It is not part of the cell binary and shares
no socket with it; the workspace gains a member and nothing else. The
charter's outbound-connection scan keeps scanning every production source of
the cell — `core/` and `cli/` in full — and skips exactly one directory,
guarded by a self-test plant that fails the gate if `sync/src` ever vanishes
while the exclusion remains. A carve-out that survives what it carves out is
a hole.

**The listing contract: all of it or none of it.** The client decides what to
upload by comparing what it has against what the vault holds, so the vault
must answer "what do you hold" completely. `GET /vault-port /` returns every
stored file with size and mtime, sorted, as JSON. A partial listing would be
indistinguishable from an empty one, and a pruning client told the vault is
empty will empty it for real — so any entry that cannot be described fails
the whole listing rather than shipping short. Names needing percent-encoding
are not addressable over this protocol today; the walk skips them loudly on
the client side rather than storing something it could never read back.

**Difference is size-or-mtime, deliberately dumb.** No hash handshake, no
content fingerprinting: if either figure differs, the file goes up again.
Flash on the far end is cheap; a clever differ that decides two different
files are the same is not. Re-uploading an unchanged file costs seconds;
trusting a heuristic with the only copy costs the file.

**Deletion is never implied.** `plan` cannot delete anything, by parse error
if necessary. `push` removes a remote copy only when `--prune` was passed,
and only after every upload succeeded — clearing remote copies of files that
failed to re-upload is how data gets lost twice in one afternoon. Without
the flag the run ends by naming the count and the flag, so "the vault still
holds three things your folder does not" is said out loud rather than
discovered later.

**Plain HTTP, and no apology.** The client speaks exactly the dialect the
vault serves — Content-Length bodies, no chunked encoding, no TLS. This is
not an oversight to fix later; over Tor the onion path already encrypts and
authenticates both directions, and adding a TLS layer under that would trade
one honest sentence ("the path is the circuit") for a certificate authority
in the trust chain of a sovereign device. A URL with a scheme is refused at
parse time with that explanation.

**Token from the environment, named per invocation.** `--token-env` names the
variable; `VAYUCELL_TOKEN` is the default. A secret on the command line lands
in shell history and process listings; this project has been down that road
with minted credentials [ADR-0010] and is not walking back up it.

## Consequences

- The cell gains one read route (`GET /` → listing) and loses nothing else;
  availability rules apply to the listing exactly as to a file read, so a
  `PROTECT`ed node answers 503 before it answers with inventory.
- The workspace now builds a third crate. The release gate reads versions
  across all of them; the internal pin on `vayucell-core` moves with every
  release like the others.
- Sync is invoked, not resident. Nothing watches the filesystem, nothing
  retries on a timer, nothing runs that a person did not start. A folder is
  in step because somebody ran the command, and the output says what it did.
- What is NOT solved: live watching, incremental resume of a partial upload,
  conflict resolution when both ends changed (size/mtime diffing re-uploads
  and the newer local copy wins by fiat), and names outside the HTTP-pathable
  set. These are recorded here so their absence is a decision, not a gap
  somebody discovers at 2 a.m.

[ADR-0009]: ADR-0009-accepting-a-file.md
[ADR-0010]: ADR-0010-per-device-credentials.md
[charter]: ../../LICENSE-CHARTER.md
