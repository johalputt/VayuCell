# VCIP-0002: The folder companion and the vault listing

- **Status:** Implemented
- **Date:** 2026 (see `git log` for the exact day)
- **Relates to:** ADR-0009 (accepting a file), ADR-0010 (per-device
  credentials), ADR-0011 (synchronising a folder to a vault), Charter
  Articles II, IV, V, VIII; PLAN.md phase P4

## The problem

P4 promised file sync. What the repository had was a vault that stores one
name at a time and a README that said, in effect, bring your own `curl`
loop. Three things were missing, and they are three different kinds of
thing:

1. A **read** the cell does not have: a client cannot decide what to upload
   without knowing what the vault already holds, and asking file-by-file
   means probing names — exactly the enumeration the unified-refusal design
   exists to prevent.
2. A **program** the workspace does not have: something must compare, decide,
   upload, and delete on the operator's behalf.
3. A **governance answer**: this project's charter forbids the binary from
   dialing out, and "sync" sounds like it needs dialing from somewhere.

## The proposed change

Adopted as described in [ADR-0011]; the short form:

- The cell gains `GET /vault-port /` → a complete JSON listing of stored
  files (name, bytes, mtime), sorted server-side, all-or-nothing, behind the
  same authentication and availability ladder as every read. Probing is
  replaced by disclosure to somebody who already authenticated; the
  availability rules mean a protected node answers 503 before it answers
  with inventory.
- The workspace gains `sync/`, building `vayucell-sync`: `plan` (prints the
  difference; sends nothing), `push` (uploads size/mtime differences;
  deletes only with `--prune`, only after uploads succeed). It dials one
  address for the length of one invocation and then it is gone.
- The charter gate's outbound scan keeps full jurisdiction over `core/` and
  `cli/` and skips exactly `./sync/*`, with a self-test plant that fails the
  moment `sync/src` stops existing while the exclusion remains.

## Why a separate crate rather than a flag on the cell

The prohibition in Article V.2 is about what runs on the phone. Putting a
dialer behind `--sync-mode` would make the *claim* ("it binds, and it never
connects") false for the very binary operators are told to trust, and would
put the code that knows how to connect onto the device whose safety depends
on it not knowing. Two binaries keep both sentences true: the cell listens,
and the companion dials.

The cost is real and accepted: two things to build, two versions to keep in
lockstep through the release gate, and a user who must run the sync command
themselves. That last one is not a cost — nothing should be syncing a
sovereign device's storage except its owner.

## Verification posture

- The listing route is pinned by unit tests over the fake store (shape,
  sort order, empty-vault body, HEAD without body, auth refusal, availability
  refusal, escaped-quote names, wrong-method refusals) and by mutation:
  dropping the availability check, dropping the sort, inverting the path
  guard, unescaping quotes, listing directories, and swallowing entry errors
  each turn their named test red.
- The companion is pinned end-to-end against scripted TCP servers: wire shape
  (method, percent-encoded path, Authorization header, exact bytes), plan
  purity (a refused upload stops the run before any prune), prune gating
  (`--prune` or nothing is deleted), and the chunked-response refusal.
- Five further mutations cover the client guards themselves: the token
  leaving the request, unchanged files re-uploading, hidden files entering
  the walk, directories being treated as storable, and deletion proceeding
  without the flag.

## What is deliberately not here

Live watching, resume of partial uploads, conflict resolution beyond
"local wins by re-upload", TLS, and any name needing percent-encoding to
travel an HTTP path. Each absence is recorded in ADR-0011 §Consequences so
that finding one later reads as reading, not discovering.
