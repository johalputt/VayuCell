# VCIP-0003: The replicator with a verified restore

- **Status:** Implemented (device gate still open)
- **Date:** 2026 (see `git log` for the exact day)
- **Relates to:** ADR-0004 (storage durability), ADR-0009 (accepting a
  file), ADR-0011 (synchronising a folder to a vault), ADR-0012
  (replication by receipt); Charter Articles II, IV, V, VIII; PLAN.md
  phase P6

## The problem

P6 promised "replication lag as the stated guarantee" and
"verified-restore reporting". What the repository had after P4 was the
honesty machinery for both — `durability.rs` will not let a lag pretend to
be live or a drill pretend to be recent — and nothing producing either
number. A panel that renders a guarantee nobody generates is prose with
types.

The gap had three parts, and they are different kinds of thing:

1. A **maker of copies**: something that turns a vault into a mirror and
   can say when it last did.
2. A **prover**: something that checks the mirror restores — not by
   trusting the mirror's own bytes, which is the archive grading its own
   homework, but by pulling every file down the wire again and comparing.
3. A **boundary**: a rule for what the cell may say about any of it,
   given that the cell never dials and therefore never measured.

## The shape of the answer

The companion gains two verbs. `replicate` mirrors the listing into a
folder — size/mtime diffing, durable writes via temporary-plus-rename,
`--prune` as the only deletion. `drill` re-downloads every listed file and
compares byte for byte; a mismatch names the file, a missing mirror copy
names itself, and neither writes a receipt. Both require `--receipt`
before they run, both write only on complete success, and both fold their
claim into the receipt file through an upsert that refuses to overwrite
evidence it cannot parse.

The cell gains `--replica-evidence`. It reads the receipts through a
strict parser, pushes them through exactly the staleness rules ADR-0004
pinned, and renders every line as a claim from the replica — the section
says so in its first line, and no number appears without its measurement
time or without having aged out into *"nothing is still measuring"*.

The boundary is the decision, and it is written down because it would be
easy to erode one flag at a time: **the cell quotes, it never measures.**
There is no code path by which the phone tests its own backup, and the
wording on every surface keeps the provenance attached.

## What is deliberately not here

No schedule, no daemon, no watching. Replication happens when a person
runs a command, which is why a lapsed receipt reads honestly as nobody
measuring rather than as an error to retry around. No restore *into* a
wiped folder; the drill proves the mirror matches the wire, and the docs
keep calling that a comparison. And no closure of P6's device gate:
somebody still has to run this against a handset and let thirty days pass.
