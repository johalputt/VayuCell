<!-- SPDX-License-Identifier: Apache-2.0 -->

# Changelog

Every entry says what changed and, where it matters, what it means for someone
running this on hardware in their home. Entries that only a maintainer could
care about are still listed, but they are marked as such.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions are patch-only until the battery governor ships — see
[`CHARTER.md`](CHARTER.md) Article III.1, which forbids anything that serves
traffic before it.

## [Unreleased]

## [0.0.13] — unreleased

The heading says *unreleased* rather than carrying a date. The date is written
when the tag is cut.

### Added

- **The fleet contract** ([ADR-0014](docs/adr/ADR-0014-the-fleet-contract.md)).
  `core/src/fleet.rs` defines the four roles as declared promises, quorum
  as a computed majority where witnesses vote only to break exact ties —
  and can never promote a minority — an upgrade state machine that holds
  one node in flight and drains one that overstays, and jail verdicts
  sealed with HMAC-SHA-256 implemented in-tree and pinned against NIST
  and RFC 4231 vectors. `--fleet-role` on report, status and all renders
  the declaration, its consequence, and the honest ceiling in the same
  section.
- **Relay ingress — declaration without management**
  ([ADR-0013](docs/adr/ADR-0013-relay-ingress-a-rented-dependency.md)).
  `vayucell all --relay-via <HOST>` declares ADR-0003's rented-tunnel mode:
  the hostname validated against DNS's real rules at typing time, the
  supplier disclosure and forwarding instruction printed before anything
  binds, unverified standing until an outside request is observed, and the
  battery panel never announced through it. No dialing code, no tunnel
  abstraction, no claims about the far end — the dependency is named in
  every start instead.
- **The replicator with its verified restore — replication by receipt**
  ([ADR-0012](docs/adr/ADR-0012-replication-by-receipt.md)). `vayucell-sync`
  gains `replicate` (pull the vault into a mirror folder, durable writes,
  `--prune`) and `drill` (re-download every listed file and compare it
  against the mirror byte for byte); both require `--receipt`, write it
  only on complete success, and refuse to overwrite receipt text they
  cannot parse. The cell gains `--replica-evidence` on `vault`, `all`,
  `report` and `status`: the startup banner and the storage section quote
  the receipts through the staleness rules ADR-0004 already pinned, every
  line worded as a claim from the replica rather than as something the
  phone measured.
- **`core/src/replica.rs`: a receipt format too small to lie
  interestingly** — strict byte-level parser (ASCII only, duplicate fields
  refused, trailing bytes refused, every malformation named), conversion
  into `RecoveryPoint`/`BackupState` under the standing five-minute
  window, clock-skew refusal instead of clamping, and an upsert that will
  not bulldoze unreadable evidence.
- **`vayucell-sync`: the companion that keeps one folder of yours in step with
  one vault** — a new `sync/` crate building its own binary, because the dialing
  half of sync cannot live in a program whose charter forbids it from dialing
  (`vayucell-sync plan --dir <DIR> host:port`, then
  `VAYUCELL_TOKEN=… vayucell-sync push …`). `plan` prints the difference and
  moves nothing; `push` uploads what differs by size or mtime, and deletes the
  remote copies of files you removed locally **only when you pass `--prune`,
  and only after every upload succeeded**. The cell is dialed only while the
  command runs: no watcher, no schedule, nothing resident. It speaks exactly
  the vault's plain-HTTP dialect — over Tor the onion path already encrypts,
  which is why there is no TLS here to trust instead — and it refuses chunked
  answers by name rather than parsing them halfway. Names the protocol could
  not read back are skipped with a warning rather than stored blind. Decision
  in [`docs/adr/ADR-0011-synchronising-a-folder-to-a-vault.md`](docs/adr/ADR-0011-synchronising-a-folder-to-a-vault.md),
  governance in
  [`docs/vcip/VCIP-0002-the-folder-companion-and-the-vault-listing.md`](docs/vcip/VCIP-0002-the-folder-companion-and-the-vault-listing.md).

- **The vault answers `GET /` with what it stores** — one authenticated
  request returning every file's name, size and last write as JSON, sorted.
  A folder being kept in sync against a cell cannot diff what it cannot list,
  and until now the only way to learn what was stored was to guess names. The
  listing obeys ADR-0009 §2's read column — a derated cell still answers, a
  protected one does not — and it is **all or nothing**: a directory that
  cannot be walked fails the request rather than answering with whatever
  happened to enumerate, because a client pruning against a short listing
  would call the entries it never saw remote extras. Things the API could
  never have stored — subdirectories, dotfiles, links — are not listed at all.

- **Onion ingress: the site and the vault are now published through your
  system's tor daemon**, which closes the largest single gap between what this
  project is and what its headline says — as far as a binary that never dials
  out can close it. `vayucell all --onion-dir <DIR>` writes the daemon's
  configuration into that directory, starts `tor` from `PATH` as a child,
  reads the `.onion` address it publishes, and keeps it up: restarted on a
  crash with a doubling delay capped at sixteen seconds, shed **first** when
  the governor derates the cell (ADR-0003 §5 — it is the load making the
  device hot), stopped outright at PROTECT, and stopped again by hand before
  every halt or outage exit, because an orphaned publisher answering after the
  governor has died is precisely what this mode must never produce. The panel
  is never published: it reports whether the battery in somebody's home is
  safe, and handing that to the world is not this mode's purpose. The identity
  key is generated and held by the daemon inside the hidden-service directory;
  nothing here reads, copies or prints it, and the custody story — rotation
  breaks every link, backups belong in encrypted storage, a stolen key has no
  revocation — is printed once, before anything publishes. Decision recorded
  in [`docs/vcip/VCIP-0001-onion-ingress-via-system-tor.md`](docs/vcip/VCIP-0001-onion-ingress-via-system-tor.md).

  What this deliberately does **not** claim: reachability. ADR-0003 §4 defines
  verified as a request from outside traversing the path and being served, and
  no such request has ever been observed — no handset has run this binary.
  The address line prints **unverified** beside itself, and will until the
  observation exists.

  The contract half lives in `core/src/onion.rs` — the plan, the generated
  configuration pinned byte for byte by tests, hostname validation that names
  which rule failed (length, base32 alphabet, the v3 version character), and
  `should_run`, which delegates to `shed_for` rather than keeping a second
  opinion about when high-thermal ingress stops. The checksum inside an onion
  address is honestly *not* verified: doing so needs crypto code ADR-0005 §5.1
  forbids, so validation claims shape and nothing more. Eight new mutations pin
  the guards, each re-broken against its named test.

### Fixed

- **The charter gate now refuses to pass when it read nothing.** Its
  outbound-connection scan strips test items with python3 first, and on a
  machine where that interpreter is missing or broken every file stripped to
  empty, the pattern matched nothing, and Article V.2 passed while reading no
  source at all — which is exactly how this release's own gate sat red in CI
  while passing on the machine that wrote it. The interpreter is now proven
  usable before any verdict downstream of it means anything. *(Maintainer-facing.)*
- **`vayucell enrol` no longer prints an example naming a client the charter's
  own gate refuses.** The upload example described the request in one HTTP
  client's syntax, which tripped the outbound-connection scan on production
  source — the gate was right about the sentence and wrong about nothing else:
  this program listens and never dials, so the example now names the method,
  the URL and the header in the protocol's own words and leaves the choice of
  client where it belongs, on the machine holding the file.

## [0.0.12] — 2026-08-09

### Added

- **P6's observable half: the storage posture is now produced and shown.**
  `core/src/durability.rs` held the honesty machinery for ADR-0004 — a lag that
  goes stale, a restore drill that expires, no variant meaning *durable*, a
  `Default` at every field's least reassuring value — and **nothing ever built
  one**. `Posture::unconfigured` had no caller outside its own tests. Three of
  this session's fixes were in types that reached no operator at all.

  So somebody storing files in the vault was never told the thing ADR-0004 exists
  to tell them. `vayucell vault` now says it at the moment it matters:

  > no off-device copy is configured, so this phone is the only copy — which is
  > the one thing ADR-0004 says a phone must never be

  and `vayucell report` gains a `STORAGE` section carrying the flash posture, the
  wear estimate and every standing concern.

  The producer guesses in no direction. There is no replicator, so the recovery
  point is `NoReplica` — not "unknown", not "behind by zero". Nothing records a
  clean shutdown, so the ladder reads `NeverObserved`; reporting `Verified`
  because no failure was seen is absence taken as evidence, which Article IV.3
  forbids.

- **A wear probe, which is the one storage property a device can answer about
  itself.** eMMC and UFS report life used as a coarse step (`0x01` = 0–10%,
  `0x0B` = past rated life), and `core/src/wear.rs` reads it from any of four
  sysfs paths.

  Three decisions in it are the whole point. A range is reported at its **worse**
  end — `0x02` is 20%, not 15 and not 10, because rounding toward less wear is
  rounding in the reassuring direction on the one figure whose purpose is to stop
  being reassuring. The **worse of the two cell types** is the answer, since the
  device fails when either does. And `0x00` means *the device declines to say*,
  which is reported as unreliable and **never as new** — treating it as zero
  would make the least forthcoming flash look like the healthiest.

  A device that exposes nothing reads `ABSENT`, printed rather than omitted,
  because a missing line cannot be told apart from a node nobody looked for.

- **A charter check that nothing in production source opens an outbound
  connection**, which is the mechanism behind the sentence the report prints. The
  existing V.5 check looks for a project-operated hostname, which only catches a
  call-home somebody wrote a URL for; this is the general form. `bind` is
  deliberately still allowed — listening is what the surfaces do.

  It strips `#[cfg(test)]` items by **matching braces** rather than cutting at
  the first marker, because cutting there would silently skip production code
  that followed one. Two plants were added to the gate self-test: an outbound
  connection in the binary crate, and a second in a different non-`core` file —
  the second because a check that flagged this repo's own pool tests would get
  loosened until it caught nothing, so the discrimination is proven rather than
  assumed. Verified by planting a `connect` in production code (gate goes red)
  and the identical call inside `#[cfg(test)]` (gate stays green).

- **A status per phase in the roadmap, written honestly.** `PLAN.md` §11 listed
  ten phases with no status column, so "the supervisor loop is in
  `core/src/runtime.rs`" and "P0 — nothing yet" sat in the same table and a
  reader had to reconstruct what was actually built. Every phase now carries ✅
  (code exists *and* its gate is met), ◐ (code exists, gate unmet) or ⬜ (not
  started), with a sentence on what is missing.

  The honest total: **two phases have their code, three are part-built, five are
  untouched** — and four gates (P2, P3, P4, P6) end in a sentence about a device
  and cannot be closed by writing anything in this repository.

- **A documentation check that every source path a document names exists.** The
  roadmap and the ADRs point at files to say where a decision is implemented, and
  that is the sentence a reader follows to check whether a claim is real. A
  rename falsifies it without making it look wrong — a claim that reads as
  verified and points at nothing.

  The failure names the path **and the document that claims it**, because the
  point is to go and correct the claim. Planted in the gate self-test as a
  rename rather than a deletion, since that is how it actually happens.

- **A check that the README's module count is the number of modules.** *"Sixteen
  modules in `core/src`"* sat in the README while the crate had twenty. It is a
  small number in a sentence nobody re-reads, which is exactly why it drifts:
  adding a module is the moment nobody thinks about prose.

  Found by getting it wrong in the same way — writing *eighteen* while adding the
  twentieth. This project already pins its other counts mechanically (the
  constitution's rules against Appendix A, the doctests exactly in both
  directions) and this one had no check at all. Planted in the self-test as an
  added module rather than an edited sentence, since that is the direction it
  drifts.

- **The four open Scorecard findings are recorded, with which of them we intend
  to close.** `SECURITY.md` now names each one. None is closed by changing code,
  and an alert nobody explains is the same defect this project refuses
  everywhere else: a red row reading as *"checked and failing"* when it means
  something narrower.

  **Branch-Protection**: `main` is protected. The branch reports
  `protected: true`; it is classic branch protection rather than a ruleset, which
  is why `/rulesets` returns an empty list and says nothing about it. This entry
  first claimed the branch was unprotected on the strength of that empty list —
  **a check is only enforcing where it looks**, which is the same lesson the
  charter gate had just been fixed for, applied to the verification of it.

  `.github/rulesets/main.json` remains as the equivalent expressed as a ruleset,
  for anyone who prefers to manage it that way. The alert stays open regardless
  until a `SCORECARD_TOKEN` with `Administration: read` exists, because the check
  scores zero whatever the branch is set to when it cannot read the setting.

  **Code-Review** scores zero because every commit is pushed straight to `main`,
  which is the stated model rather than an oversight. The trade is written down
  instead of hidden: one maintainer means a self-approved pull request is review
  theatre, so what replaces it is mechanical — twenty gates, the mutation gate,
  the gate self-test. **A second reviewer would be better than all of it**, and
  until there is one the honest statement is that this code is not peer-reviewed.

  **Maintained** grades ninety days of activity against a three-day-old
  repository and resolves itself. **CII-Best-Practices** needs a registration
  nobody has submitted.

### Fixed

- **A node on battery announced its own shutdown and went on running.** The shed
  ladder's last rung is `ShuttingDown` — *"shut down cleanly with charge
  remaining"* — and both supervisor loops **printed it and kept ticking**.
  `Stage::ShuttingDown` had no reference anywhere in the CLI outside tests.

  Verified against the built binary on a cell at 8%, below the 10% reserve, with
  mains lost: it walked the whole ladder, printed the last rung, and had to be
  killed after forty-five seconds. That line is an obligation the caller was
  meant to discharge, and the caller printed it instead — so a node runs to zero
  and dies the ungraceful way ADR-0002 §8 exists to prevent. The claim that a
  governed phone is a server with an integrated UPS rests entirely on it
  stopping *while there is charge left*.

  It stops now, and says why: *"stopping now, with charge remaining. Start it
  again when mains is back — this is an outage, not a halt, and nothing needs to
  be inspected."*

  **No halt record is written.** An outage is not a governor halt: mains
  returning is the whole remedy, and writing a record would earn a power cut a
  hard stop that requires somebody to inspect the phone — and would make the halt
  record mean two different things.

  The decision is a free function rather than two copies inline in `main`, where
  nothing could reach it: no test, and therefore no mutation that could turn one
  red. It is asked of **the rungs entered this tick**, because `ShuttingDown` is
  terminal — a later tick reports no rungs at all, so a check reading only the
  current stage would never fire again. A late tick walking four rungs at once
  has its own test, since reading only the first would leave the node running on
  a cell at the reserve.

- **A halted phone went on serving a website and accepting uploads.** `run` and
  `all` refused to start while a halt record stood. `site` and `vault` did not:
  they started, answered `200`, and said nothing about it. So a cell that crossed
  a hard threshold served content and **took files** as soon as somebody
  restarted the binary into a different subcommand — while its own halt message
  says *"no restart clears it"*.

  Verified on the built binary before and after: a phone halted at 61.5 °C and
  cooled to 25 °C, with nobody having looked at it, served `GET / -> 200` and
  accepted `PUT -> 200`.

  Nothing was wrong with the check. It was written once per command, and the two
  commands added after it did not get one. It now lives in the dispatch and is
  keyed by `Command::serves_traffic()`, so a variant added later **does not
  compile** until that match answers for it.

  `serve` is deliberately exempt and is the only interesting entry: it renders
  the panel, which is what a person needs to read at exactly the moment the
  device has halted, and it already reports the halt rather than serving through
  it. Taking it away would be the shed ladder's mistake made at the terminal.
  `inspect`, `status` and `report` stay ungated too — a halted phone must still
  be able to say what is wrong with it and let somebody record that they looked.

- **The device report claimed something about this binary that is not true.** It
  printed *"this program has no network code"*. The same binary runs three HTTP
  listeners — that is most of what it is for — so an operator who has ever run
  `vayucell site` reads that sentence, knows it is wrong, and has been handed a
  reason to discount every other assurance in the block it heads.

  **An overstated reassurance is worse than a narrow one, because the reader can
  check it.** What is true is stronger and narrower: *nothing in this binary
  dials out.* It binds when you ask it to serve, and it never connects. There is
  no `connect` anywhere outside the test module, and a test now pins that the
  report claims listening-not-connecting rather than an absence of sockets.

- **The charter gate scanned `core/src` only, so Article V was enforced on the
  one crate that cannot reach anything.** The forbidden-concept scan (telemetry,
  call-home, kill switch) and the V.5 project-operated-host check both ran over
  `core/src` and never looked at `cli/src` — the only crate that opens a socket.

  The V.5 *dependency* check in that same file already carries the note that a
  gate naming one manifest by hand "goes on passing while a dependency lands in
  the crate beside it". That half had learned the lesson; the source-scanning
  half had not. Crates are now found rather than listed.

## [0.0.11] — 2026-08-09

### Fixed

- **A refused upload answered with the vault's absolute filesystem path.**
  `VaultIo`'s error was a `String`, documented as *"what went wrong, for the
  operator's log"* — and `route_vault` put it straight into the response body.
  The operator's log went to the caller. Found by running the published v0.0.10
  binary and reading the 500 it gave back, not by reading the code.

  ADR-0008 had already settled this for reads, in as many words: a file that
  resolved and could not be read answers exactly like a typo, because *"the
  operator gets the reason in the log on the device they own; the wire gets the
  same 404 either way."* The write path did the opposite of its sibling.

  `StorageFailure` replaces the string. `told()` is what reaches the wire and
  **never carries a filesystem path** — a test asserts no separator appears in
  either variant. The path, the errno and which of the four ordering steps failed
  go to the log; the wire is not told them apart, because distinguishing them
  would leak the shape of the write ordering to anything that can send traffic.

- **A conflict with something already stored was reported as the server
  breaking.** Every write failure answered `500`, including the one added in
  0.0.10 for a symbolic link stored under the requested name. That is not the
  server having broken: the request was well formed, the device is fine, and
  something in the vault needs a person.

  Those now answer **409**, and the distinction is the point — a caller told 500
  retries, and a caller told 409 stops and tells somebody, which is the only
  thing that will ever clear a link sitting in the vault. A delete that genuinely
  fails, and a write whose filesystem step genuinely fails, still answer 500.

- **The site's 404 bodies mapped the directory its statuses were unified to
  hide.** ADR-0008 §3 makes every site refusal a 404 because the differences
  between them are *"a directory listing delivered one status code at a time"*.
  The bodies were left discriminating: `/folder` answered *"is a directory with
  no index.html in it"* and `/nodir` answered *"nothing is published"*, so the
  same listing was delivered one **body** at a time. A stranger enumerates the
  document root by reading instead of by counting.

  Every refusal now says `nothing is published at <path>` — a real directory, an
  absent name, a hidden name that exists and one that does not are word for word
  the same answer.

- **A traversal attempt answered 403** — the exact status §3 names as the
  tempting design it rejects. The status was decided in two places: `status_for`
  for a resolved refusal, and `refuse` for a request that never got that far.
  Two authorities on one question is how they came to disagree, so `refuse` goes
  through `status_for` like everything else.

- **The operator's log that §3 depends on did not exist.** *"The operator's
  diagnosis is not lost — it goes to the log on the device they own"* was true
  only for an unreadable file. Nothing was written for a hidden name, a directory
  with no index, a traversal attempt or a plain miss, so the diagnosis survived
  **only** in the response body — the one place §3 says it must not be. That is
  why the bodies stayed informative: taking them away without a log first would
  have left the operator with nothing.

  `Response` now carries a **private** `log` field that `render` never reads and
  a caller cannot reach; the binary prints it to its own stderr. A test asserts
  the line never appears in the rendered bytes.

  All three were found by probing the running binary. The tests here had covered
  the half of §3 that was implemented — they asserted `resolve` and `status_for`,
  and never what a visitor is handed. **A property about the wire needs a test
  that reads the wire.**

## [0.0.10] — 2026-08-09

### Fixed

- **A replication lag nobody was still measuring read as a lag that was fine.**
  ADR-0004 §1.1 is the section that argues a number beats an adjective, because a
  number can be checked — and it promises the panel shows the lag *"continuously,
  as a live figure"*. `RecoveryPoint::Behind(Duration)` carried the lag and
  nothing else, and implemented `Display`.

  A figure with no measurement time renders identically whether it was taken a
  second ago or the morning the replicator died. So the number §1.1 prefers over
  an adjective was, structurally, an adjective wearing a number's clothes: `47`
  said nothing about whether anybody was still counting, and a replicator that
  stopped an hour ago would have gone on reporting its last good reading — inside
  target, no concern raised — for as long as the process lived. That is the exact
  reading Charter Article IV.3 forbids.

  The type now carries `Behind { lag, measured_at }` with a monotonic stamp, and
  **the `Display` impl is gone** — `Display` was the hole, because
  `format!("{rp}")` renders with no clock in scope and no way for the type to
  object. A `compile_fail` doctest proves it no longer compiles. `describe(now)`
  and `needs_attention(target, now)` require the clock's reading; a measurement
  older than `MEASUREMENT_STANDS_FOR` (five minutes, against the 60-second
  default target) says so in the sentence the operator reads rather than going
  quiet; a stamp ahead of the clock cannot be aged and is not live either; and
  `Posture::concerns` takes `now`, because a rule enforced only on the type the
  panel wraps is a rule the panel can route around — which is how the governor
  row went wrong twice.

  Replication is not implemented, so nothing shipped was showing a stale figure.
  What shipped was a type that could not have known, in the module whose entire
  argument is that a checkable number beats an unfalsifiable word. Same defect as
  the ingress verification in 0.0.9, found by the same question, and repaired
  before the subsystem exists rather than after.

- **A restore drill that ran once proved the backup forever.** ADR-0004 §4 is the
  section that refuses to call an uploaded archive a backup — *"a hope with a
  filename"* — and it says the system restores an archive **on a schedule** and
  reports the time of the last verified restore. `BackupState::Restored` carried
  `when: String`, a free-form date nothing in the crate ever compared to
  anything, and `is_proven()` returned `true` for it permanently.

  So the discipline was half-built: an unrestored backup read as unverified,
  correctly and forever, while a backup restored once in March read as proven in
  December. The failure §4 exists to catch is a chain that breaks **silently** —
  the upload keeps succeeding and the only thing that would notice is the restore
  nobody has run since — and a drill with no expiry is precisely the instrument
  that cannot notice it.

  `Restored { at_unix }` now carries a date, `is_proven(today)` and
  `describe(today)` require the current one, the `Display` impl is gone with a
  `compile_fail` doctest proving it, and a drill older than `DRILL_STANDS_FOR`
  (a month) reports how old it is instead of reading as proof.

  **The stamp is wall-clock, and that is the one difference from the other two
  fixes.** A replication lag is a duration inside one process, where
  `Clock::elapsed` is the only safe answer. A restore drill happened before this
  process started, and a monotonic clock that begins at zero on boot cannot date
  March. So `Clock` gains `wall_clock_unix()`, documented as unusable by the
  governor, the sampler and the shed ladder — a wall clock that steps backwards
  would hand them an outage that ran in reverse, which is the hazard `elapsed`
  was written to avoid.

  **It returns `Option`, and `None` is not recent.** A phone with no network and
  a dead RTC is an ordinary phone; reading an unknown date as a current one would
  make the least capable device the most confident. `Posture::concerns` now takes
  both readings as a single `Now`, so a caller cannot supply one and forget the
  other.

  Third instance of one defect — a fact whose honesty depends on time, stored
  without a time — after the ingress verification and the replication lag. All
  three were found by the same question, and none by the test suite.

- **A credential store could enrol one device name twice.** ADR-0010 says a name
  already present is "refused rather than duplicated". `enrol` refuses it — the
  path *this software* writes. `parse_store` did not, and the store is a text
  file the operator is told to edit by hand: the enrolment error itself says to
  remove the existing line first. The one path a duplicate actually arrives by
  had no check on it.

  §5 of that ADR already requires a malformed store to be **refused whole**,
  loudly, rather than partly loaded — and every other malformation was: a line
  that is not two fields, a bad name, a bad secret. This one loaded quietly.
  `parse_store` now refuses it and names **both** lines, because deciding which
  credential to keep means looking at two rows, and a message naming only the
  second sends somebody searching a file full of secrets for the first.

  **The reason the ADR gave for the rule was wrong, and that is what hid the
  gap.** It said a duplicate matters because revoking the name "leaves the other
  behind". `revoke` skips every line carrying the name, so it does not. Checking
  the stated reason against the code is what surfaced both the false rationale
  and the missing check.

  The real reason is identity: `verify` matches on the secret and answers with
  the name, so two rows sharing a name means two different credentials
  authenticate as one device — `Authenticated(name)` cannot say which of them
  presented anything, and revoking that name silently takes both. That is now
  what the ADR says and what the refusal message says.

- **An upload was the one operation not contained against symbolic links.**
  ADR-0008 §2 puts link containment in the binary, where real paths resolve, and
  `read_contained` canonicalises for a read. `remove` canonicalises too, and says
  why in its own comment: *"for the same reason a read is"*. That sentence is
  equally true of a write, and a write had no such check.

  The temporary is the dangerous half. `OpenOptions::open` follows links, so a
  link sitting at the `.partial` path is opened, truncated and filled with the
  uploaded bytes **wherever it points** — and the rename afterwards moves the
  link rather than the content, so the upload lands outside the vault and the
  vault looks empty. The destination is the quieter half: `rename` replaces a
  link instead of following it, so nothing escapes, but an operator's link is
  destroyed without a word by a vault that would have refused to *read* through
  it. Two operations disagreeing about the same file is its own defect.

  Both paths are now refused before a byte is written, checked with
  `symlink_metadata`, which reports the link rather than what it points at.
  Overwriting an ordinary stored file is unaffected and has its own test, because
  a vault that refused to replace a file it stored itself would break the case
  the surface exists for.

  **What this does not close, said rather than implied:** it is a check before an
  open, so a link created in the gap between them is not caught by it. Winning
  that race needs write access to the vault directory — the same user this
  process runs as — and ADR-0010 already states that the same user is not an
  adversary this design can hold off. The check earns its place because the
  ordinary way a link ends up in a served directory is that the operator put it
  there.

## [0.0.9] — 2026-08-09

### Fixed

- **A verified ingress path stayed verified for ever.** ADR-0003 §4 has said
  since it was written that the round trip "re-runs on a schedule, because the
  failure that matters is the path that worked for six weeks and then stopped",
  and `Reachability::Unverified` said in its own doc comment that it was *"also
  the state a mode returns to when its check is overdue… a verification that
  never expires cannot notice it"*.

  Nothing computed overdue. `Verified` carried a free-form string that no code
  in the crate ever compared to a clock, and `is_verified` took no time
  argument, so one completed round trip marked a path verified for the rest of
  the process's life. The type was, precisely, the thing its own comment named
  as the failure mode.

  A completed round trip now stands for `FRESH_FOR` — fifteen minutes — and
  **time is a mandatory argument**: `is_verified(now)` and `describe(now)`
  cannot be called without saying when you are asking, which is what stops the
  next caller doing what every previous caller did. A `compile_fail` doctest
  proves the no-argument form does not compile. The stamp is monotonic and
  taken from the supervisor's clock, so a restarted cell has verified nothing —
  the process that watched the round trip is gone. A stamp *ahead* of the clock
  cannot be aged and is therefore not evidence. A standing that has lapsed
  reports **unverified, not failed**: nothing failed, nobody looked, and sending
  an operator to debug a working path is its own defect. `due_in` answers "when
  is the next check due" out of the same arithmetic the panel reads, so a
  scheduler cannot hold an idea of overdue that disagrees with the row in front
  of the operator.

  The expiry tests are written against literal durations rather than against
  `FRESH_FOR`, because a test that pins a constant by referring to it stays
  green when the constant is widened to a century — which is the exact change
  that puts the defect back. Both directions are pinned by the mutation gate: a
  century-long `FRESH_FOR`, a zero-length one, a future stamp read as fresh, and
  a lapsed standing that reports itself as the round trip it used to be.

  No ingress mode is implemented, so nothing shipped was reporting a stale path
  as current. What shipped was a type that could not have noticed — found by
  reading a claim in a doc comment against the mechanism meant to keep it.

- **An ADR named a mechanism that does not do the job it was credited with.**
  ADR-0003 §2 said the capability registry "rejects a mode that leaves any
  unanswered", and §5.1 said it "rejects a mode that does not" declare a thermal
  class. The registry validates `Capability` values; an ingress mode is not one,
  and `Capability` has no field for a thermal class or for any of the other six
  properties. Nothing was ever going to reject anything.

  The property itself holds, and by something stronger than the runtime check
  described: `Mode::profile` is an exhaustive match returning a struct with no
  `Option` and no `Default`, so a mode that leaves a property blank does not
  build. Both sentences now name that, §2 records the correction rather than
  quietly applying it, and a `compile_fail` doctest proves the claim instead of
  asserting it.

  **A claim that names the wrong enforcing mechanism is a defect even when the
  property it claims is true** — a reader auditing that sentence would find
  nothing at the registry, and the obvious repair would have been to add a
  runtime check and relax the struct, trading a compile-time guarantee for a
  weaker one.

## [0.0.8] — 2026-08-09

### Fixed

- **The installer continued unverified when it could not download the
  checksums.** `install.sh` verified a published build against
  `SHA256SUMS.txt`, and when that file would not download it printed
  `no checksum file published alongside this build; continuing unverified`
  and installed the binary anyway.

  That made every other check in the download path decorative. Anything able to
  fail one request — a proxy, a captive portal, a bad minute of connectivity —
  silently downgraded the install to no verification at all, behind a yellow
  mark that scrolls past on a phone screen. **Absence is never protection**, and
  this is the first thing a stranger runs.

  It now refuses, naming what to do. The comparison itself was sound and is
  unchanged: a tampered archive and a build the list does not mention were both
  already refused, and both are now tested rather than assumed.

- **The install gate only tested the happy path.** It ran the installer and
  grepped the output for `Checksum matches`, so it could only ever confirm that
  a good download passes. It could not have caught the defect above, and did
  not.

  `verify_download` is now a function defined before the installer does
  anything, and the gate **sources the shipped file and calls that function** —
  not a copy of it, because a copy in the gate drifts from the copy that ships
  and the one that drifts is the one nobody runs. Four cases are planted: a
  matching download, a tampered one, a build absent from the list, and no list
  at all. Verified by reintroducing both defects and watching the gate go red
  for each.

### Changed

- **The installer says what the checksum proves and what it does not.** It is
  not an independent signature check: the archive and the checksum list come
  from the same place over the same connection. The release publishes a cosign
  signature over the list, and the installer now points at it rather than
  letting a green tick imply it was used.

## [0.0.7] — 2026-08-09

### Added

- **`vayucell report`** — a device report somebody can paste, for a project that
  has never seen a phone.

  Every device-facing claim in this repository rests on a fake host, and the
  only thing that changes that is a person running this on real hardware and
  saying what happened. The issue template asked four free-text questions and
  never asked for the program's own output — so the half of a hardware database
  nobody can type from memory, which power-supply nodes the kernel exposes and
  which of the four charge mechanisms exist, was being discarded at the point of
  collection. The template now asks for it.

  **Absence is the most useful line in it.** A node the handset does not have is
  printed as `ABSENT` rather than omitted, because a missing line cannot be told
  apart from a node nobody looked for.

  **It prints and does not send.** There is no network code in it and there is
  not going to be: the README's headline is "No account. No telemetry.", and a
  device report that phoned home would make that false quietly, in the one
  direction nobody checks.

  **It says what it holds and what it leaves out, in what it prints.** A promise
  in a document nobody reads is not a control, so the claim travels with the text
  it describes and whoever is about to paste it can check one against the other.
  Nothing reads a serial number, an IMEI, a MAC address, a hostname, a username
  or any network configuration. The only path that can carry a name is a
  `--supply-dir` the operator chose, and the report flags that line as theirs.

  One exception to that claim was found by tracing it rather than trusting it:
  the tier probe quotes an unrecognised `VAYUCELL_HOST_ASSERTION` back verbatim,
  which is correct — somebody who set it wrongly needs to see what they set — and
  means whatever they typed lands in a report headed for a public issue. It is
  now named beside the claim, as `--supply-dir` already was. A claim with an
  exception nobody mentions is a false claim.

- **`sysfs::NODES`**, the published list of what a reading consults. The report
  is pinned to it rather than keeping a copy, because two lists drift and the one
  that drifts is the report — the only thing anybody would have to go on about a
  handset nobody here is holding. A test removes each node in turn and requires
  the reader to fail naming it.

## [0.0.6] — 2026-08-09

### Fixed

- **`vayucell status` reported a halted phone as fine, and I put that there.**
  0.0.5 taught `run` and `all` to refuse while a halt stands, and left the panel
  alone. The panel reads the cell, and by the time anybody looks at a phone that
  halted on temperature the cell has cooled — so `status` printed
  `VERIFIED  battery governor  governor at NORMAL; no threshold crossed` next to
  a record on the same disk saying a threshold had been crossed and nobody had
  been to look.

  That is the same defect 0.0.3 fixed, arriving from the other direction: then
  the level came from a literal, now it came from a reading that could not see
  the whole state. `Standing::floor` names the rule and `report::observed` takes
  the **maximum** of the reading and the floor, so a cell that is hot right now
  is still reported as hot rather than as halted-earlier, and one that has cooled
  is still reported as halted.

  The panel is deliberately still served on a halted device. `run` and `all`
  refuse to start, but the surface that reports whether the battery is safe is
  the last thing to take away from somebody whose battery is not — there is a
  mutation asserting exactly that, and it predates this.

  The record is re-read per request, like everything else on that surface, so
  clearing a halt is visible without a restart.

## [0.0.5] — 2026-08-09

### Added

- **A halt now survives a restart, which is what the binary has been claiming.**
  When the governor halts it prints *"This requires a person who has looked at
  the phone; no restart clears it."* Any restart cleared it completely.

  The mechanism was already there and unused. `Supervisor::new` takes a governor
  rather than building one, its documentation saying *"so a device that was
  halted before a restart comes back halted"*; `Governor::after_inspection` is
  the way back down the ladder; `core/src/runtime_test.rs` even asserts that a
  supervisor built around a halted governor comes back halted. Every caller in
  the binary passed `Governor::new(thresholds)` — fresh, at `NORMAL`. So a phone
  that halted on temperature came back serving the moment anything restarted it:
  the operator, Android reclaiming memory, a power cut, a boot script.

  `core/src/halt.rs` holds the decision and no I/O; `cli/src/halted.rs` holds the
  file. `run` and `all` refuse to start while a halt stands, before anything
  binds a socket, and record the halt before exiting — in that order, so a power
  cut between the two leaves the record rather than only the sentence claiming
  there is one. The write uses the vault's ordering, directory flush included,
  because a halt is written exactly when a device may lose power a second later.

  **An unreadable record is a halted device.** Three outcomes, not two: no
  record, a record, and a record that exists and cannot be read. `NotFound` is
  clear; anything else is not, because something wrote that file and the only
  thing that writes it is a hard stop. Collapsing the two would mean a
  permissions change or a half-mounted card silently returned a halted phone to
  service.

- **`vayucell inspect --lies-flat | --deformed`** — the only way down from
  `HALT`, and it takes a human observation rather than a reading. ADR-0002 §6:
  software cannot measure a millimetre of deformation.

  `--deformed` does not clear the halt and there is no flag that overrides it: a
  cell somebody has watched deform is not a cell to resume serving on, whatever
  the sensors say afterwards. Neither flag has a default, because the default
  would be this program deciding what somebody saw when they looked at their
  phone, and passing both is refused rather than resolved last-one-wins — one of
  the two answers ends with a phone going to hazardous waste.

- **`scripts/msrv-gate.sh`, run by `local-ci.sh`.** The crate declares
  `rust-version = "1.80"` and the gates were only ever run against whatever
  stable happened to be installed — years ahead of it. `Option::expect` in a
  const context is stable to call since 1.83 and a compile error at 1.80; it
  reached `main`, and CI's MSRV job was the only thing that noticed.

  The gate builds and tests against the declared version, clearing `RUSTFLAGS`
  as CI does, because an older toolchain emits lints the current one has since
  renamed and failing on those would be testing the compiler. When the toolchain
  is not installed it reports **NOT CHECKED** and fails, rather than passing for
  a check it never ran. Verified by reintroducing the defect and watching it go
  red.

### Fixed

- **One idle TCP connection silenced the safety panel.** Each surface ran a
  single accept loop, reading one connection to completion before looking at the
  next, so a caller who opened a socket and sent nothing held the whole surface
  until the read timeout expired. One new silent connection per second held it
  indefinitely.

  Measured against the running binary rather than reasoned about: **zero
  successful panel reads in thirty seconds**, from a caller sending no bytes,
  presenting no credential, and needing nothing but the ability to open a TCP
  connection from the same network. The panel is the surface that answers
  whether the battery in somebody's house is safe.

  Each surface now runs a small pool of workers accepting from the same
  listener, and the idle timeout drops from ten seconds to five. Both matter,
  and the arithmetic is why: a caller opening silent connections at `r` per
  second keeps about `r × timeout` of them stalled at once, and the surface
  answers while that stays under the worker count. Eight workers alone still
  left four requests in thirty seconds timing out; eight with a five-second
  timeout answered all sixty, median one millisecond.

  **This is not immunity and is not described as such.** Blocking I/O with a
  fixed number of workers cannot be made immune to a caller who opens
  connections faster than they time out; that needs an event loop, and an event
  loop without dependencies is a large amount of subtle code this project would
  then have to be right about. The limit is recorded next to the constant that
  bounds it.

  Three tests hold it, one of which had to be fixed before it held anything: the
  first version asserted the surface answered "faster than the read timeout",
  which a single-worker surface does — it answers the instant the stall ahead of
  it expires. The mutation reducing the pool to one worker survived it. The
  bound is now a quarter of the timeout, because a pool that is absorbing stalls
  answers in milliseconds and a bound has to be nowhere near the thing it
  distinguishes itself from.
- **A red check named the wrong thing.** The gate self-test ran as a second step
  inside the job called *"Charter · Articles III–IX enforced"*. The self-test
  refuses to run whenever another gate is already failing on a clean tree —
  correctly, because a caught violation would prove nothing then — and that
  refusal reported as a charter failure while the charter was entirely fine.
  A build status that sends whoever reads it to the wrong file is the same
  defect this project refuses in its own output, so the self-test is now its own
  job, *"Gates · Self-test, every check must fire"*, and is in the required set.
- **A clippy lint that this machine could not see failed CI.**
  `clippy::byte_char_slices` landed after the toolchain the gates are run on,
  and flagged `[b'a', b'b']` in a test that had been passing locally for weeks.
  The lint is right; the local gate was not wrong, it simply could not see it.

  `scripts/local-ci.sh` now prints, on every green run, which clippy it used and
  that CI installs whatever `stable` is on the day — so a green local run stops
  reading as a promise about CI. It is the same rule the code follows: a check
  that could not be made must not render as one that was.

## [0.0.4] — 2026-08-09

### Fixed

- **`vayucell all` consulted the governor and never ran one.** The command the
  install guide tells somebody to leave running for months was the one command
  that never wrote a charge ceiling — while the panel it served told the
  operator to *"run `vayucell run` to write 60% and read it back"*, which the
  guide never asked them to do. A phone following the guide exactly charged to
  100% and sat there, which Step 3 of that same guide names as the condition
  that ages a cell fastest.

  `all` now runs a real `Supervisor` in its own thread: it holds the ceiling,
  samples on the cadence, walks the outage ladder, and owns the governor the
  other three surfaces read.

- **A hard stop was not hard in the command people are told to run.** The
  supervisor's governor escalates monotonically — `escalate` refuses to move
  down — which is what makes `run` able to say *"no restart clears it"*. The
  serving surfaces built a fresh governor per request, so a phone that reached
  `HALT` and then cooled resumed serving on its own. `all` now stops the
  process on `HALT` exactly as `run` does.

### Changed

- **The surfaces take the worse of two answers about the cell.** `Governed`
  serves `max(fresh reading, supervisor's latched level)`, because neither is
  sufficient alone: the supervisor's level is up to one sampling interval old,
  so a cell that spiked since the last tick would be served as though it had
  not; and a fresh reading cannot latch, so a halted device that cooled would
  quietly resume. The maximum cannot be wrong in the reassuring direction,
  which is the only direction that matters. Verified both ways against a
  running binary — a spike is refused before the supervisor has ticked, and a
  halt survives the cell cooling.
- **`site` and `vault` say that they govern nothing.** Both still consult the
  governor before every request, and neither runs one — no ceiling is held and
  a `HALT` is forgotten when the cell cools. That is right for a command
  somebody is trying out and wrong for one left running, and the difference is
  invisible from outside, so it is printed at startup rather than left to be
  discovered.

## [0.0.3] — 2026-08-09

### Added

- **`vayucell all`** — the panel, the site and the vault in one process, on
  three consecutive ports counted from `--bind`, under **one governor and one
  outage ladder**.

  This is the command to leave running on a phone, and it exists for a
  correctness reason rather than a convenience one. See the fix below.

  Each surface still gets its own port. That is not tidiness: the panel reports
  whether somebody's battery is safe and the site serves whatever they put in a
  folder, and the browser rule that stops one reading the other is the
  same-origin policy — which counts a differing port as a different origin and a
  differing path as the same one.

  `--site-dir` and `--vault-dir` name the two directories, because `--dir`
  cannot mean two folders at once. Omit either and that surface is simply not
  served, which the startup summary says out loud rather than leaving the
  operator to notice. Omit both and the command is refused, because with neither
  it is `vayucell serve` under a longer name.

  Ports are counted, not guessed: a `--bind` without a numeric port is refused
  rather than resolved through DNS at parse time, port 0 is refused because
  "the one after whichever the kernel picked" is not a thing, and a base port
  above 65533 is refused rather than wrapped — 65535 + 1 is not port 0, and
  binding port 0 would put a surface somewhere nobody chose.
- **`cli/src/cell.rs`** — one cell, one ladder, however many surfaces are
  serving from it. `site` and `vault` now borrow it rather than each building
  their own.
- **`DELETE`, `vayucell devices` and `vayucell revoke`** — the three things that
  turn the vault from a thing that accepts files into a thing somebody can
  actually run.

  `Method` gained `Delete`, and it arrived on its own rather than riding along
  with `Put`, because removing somebody's file is the one action here with no
  undo. It obeys the governor exactly as a write does — a device in trouble is
  not the place to be changing somebody's data — and deleting something already
  gone is a `404` rather than an error, so a retry after a dropped connection
  lands where the caller wanted. A **full disk never refuses a delete** —
  `Admission::for_removal` asks the governor and the outage ladder and does not
  look at the disk at all, because refusing the one request that would free
  space would have been perverse.

  `devices` lists what is enrolled and **never prints a secret**; `revoke`
  removes one. Revocation rewrites the store through the same
  temporary-flush-rename-flush sequence a vault write uses, because a credential
  store truncated by a power cut locks out every device at once, and it keeps the
  operator's comments line for line — somebody annotates a store with which
  laptop is which, and rewriting from parsed data would throw that away.
- **`cli/src/device.rs`** — `site` and `vault` each carried their own copy of
  "read the cell, ask the governor". Two copies of a safety decision is one more
  than can be kept in step, and the copy that drifts is the one nobody is
  looking at. It also moved that logic out of `main.rs`, which is the one file
  no test reaches.
- **The durable write and the bounded header reader are now tested.** They were
  the most important untested code in the binary: the four-step write that a
  power cut is supposed to survive, and the parser that decides how many bytes a
  stranger may make this device allocate. `read_headers_and_body` takes any
  `BufRead` rather than a socket, so the bounds are exercised without one — and
  the test that matters most is that a body **shorter** than its declared length
  is refused rather than stored truncated, because storing a short read as
  though it were whole is how a file becomes silently damaged.
- **`serve::VaultIo`**, replacing three loose closures. With two they were a pair
  of arguments a caller could transpose; with three it became a shape somebody
  would get wrong silently, and a reader and a remover swapped is not a mistake
  worth relying on review to catch.

- **Storage that works end to end.** `vayucell enrol --device <name>` mints a
  credential and prints it once; `vayucell vault --dir <DIR>` serves it. A
  `PUT` with a bearer credential stores a file, a `GET` reads it back. Verified
  against a running binary, not only in tests: an unauthenticated `PUT` answers
  `401`, an authenticated one stores the file and returns a receipt, a `60 °C`
  device answers `503` and stores nothing, and a `.env` or a `sub/dir.txt` is
  refused with a sentence that says which rule it broke.
- **`serve::Method` gained `Put`.** This is the widening both ADR-0008 and
  ADR-0009 said would have to arrive "in a diff somebody has to approve", and
  the test that guarded the boundary changed with it rather than being deleted.
  `Put` and only `Put`: it names one file and replaces it, so a retry after a
  dropped connection is safe. `Delete` destroys data and still deserves its own
  decision; `Post` has no meaning where nothing is appended to.
- **`serve::route_vault` checks the credential first** — before the path is
  parsed, before the governor is consulted, before the disk is measured. Every
  other order leaks something: the name check tells a stranger which filenames
  are acceptable, and the device check tells them the battery level of a phone
  that is none of their business. A test sends an unauthenticated `PUT` at a
  halted device with a full disk and an illegal path, and requires the answer to
  be `401` mentioning none of the three.
- **Header parsing**, bounded at every step: at most 64 header lines, each
  capped at the request-line limit, and a `Content-Length` over 64 MiB refused
  *before a byte of the body is read*. Only `Authorization` and `Content-Length`
  are retained — a map of arbitrary headers is a thing that ends up in a log. An
  `Authorization` scheme this does not implement reads as nothing presented,
  rather than as a different refusal that would tell a stranger which schemes
  exist.
- **`cli/src/enrol.rs`**: the store is created mode `0600` *at open time*, not
  chmod'ed afterwards — a file that was world-readable for an instant has been
  read by whatever was looking. A store others can read is refused with the
  `chmod` that fixes it. A missing store is the empty store, which accepts
  nobody. A name already enrolled is refused rather than duplicated, because two
  rows with one name means revoking it leaves the other behind.
- **`write_durably`** in the listener is the one place that acts on a
  `WritePlan`, and it performs all four steps — including the directory flush,
  the one whose absence is invisible until a real power cut.

- **The vault** (ADR-0009) — `core/src/vault.rs`, the decision layer for
  accepting a file. It is the first thing in this project that takes rather than
  gives, and the two failures that matter have no counterpart in a reader: a
  write that half-lands, and a write that lands when the device was in no
  condition to take it.

  It performs **no I/O**. It validates the name, checks the room, asks the
  governor, and returns the *ordering* a caller must follow — so every
  interesting case is reachable in a test with no filesystem.

  **A write is refused earlier than a read, and the asymmetry is the point.** The
  site keeps serving at `DERATED`; the vault refuses there. A refused upload
  costs one retry; a half-written file outlives the event that interrupted it.
  `Stage::Announced` refuses too, and that is not a new policy — that rung's own
  obligation is "stopped accepting new work", and an upload is new work. Exactly
  one of the twenty combinations of level and rung accepts anything, asserted
  exhaustively so a level added later cannot fall through to a default that takes
  files.

  `WritePlan::steps()` returns the only order that survives a power cut: write a
  temporary, flush the file, rename, **flush the directory** — the last being the
  step everybody forgets, whose absence is undetectable until a real power cut.
  The temporary sits beside the destination, because a rename across filesystems
  is a copy and a copy is not atomic, and it is hidden, so a partial upload can
  never be served by the site — which refuses hidden names as a class. Two
  modules, one property, neither relying on the other's discipline.

  `Admission::plan` returns `None` when the vault is refusing, so a caller cannot
  obtain a plan for a write the device declined. Splitting "may I" from "how" is
  how a check gets skipped by somebody in a hurry.

  **No receipt says the file is safe.** `Receipt` has no `Durable` variant and
  will not get one: ADR-0004 §0 established that nothing on a sealed phone can
  tell a flash that honoured a flush from one that acknowledged it and did
  nothing. The class is fixed rather than passed in — a caller able to choose it
  is a caller able to choose a flattering one. A test asserts the rendered text
  contains none of *saved*, *safe*, *durable*, *guaranteed* or *backed up*.

  There is **no upload route**: `serve::Method` still has only `Get` and `Head`.
  Adding one requires authentication, and shipping a network write surface
  without it would be worse than shipping neither.
- `Name` refuses a filename that is really a path, a hidden name, control
  characters, a trailing space or dot — which several filesystems strip silently,
  so the file asked for and the file that exists differ — and anything over 255
  **bytes**, counted in bytes because a filesystem's limit is, and 255 emoji is
  about a kilobyte.

- **A published website** (ADR-0008). `vayucell site --dir <DIR>` serves a
  directory of files to the operator's own network — the first surface in this
  project that exists for somebody other than the device's owner.

  What makes it not simply a file server is that **the governor is consulted on
  every request**, not cached at startup. `PROTECT` and `HALT` withhold the site;
  so does the outage ladder from `Stage::Shed` down, whose obligation is
  literally "stopped non-essential services". `DERATED` keeps serving, and that
  is a decision rather than an oversight: deration answers heat, a static file
  read is not producing the heat, and shedding a negligible load to fix a thermal
  problem is theatre that costs the operator their site. A cell that cannot be
  read yields `PROTECT` — absence is never protection.

  Traversal is impossible by construction rather than by checking: `resolve`
  splits the path into segments, refuses every segment that is not a plain name,
  and joins the survivors, so no accepted sequence can leave the root. It is a
  second and independent check on top of the request parser, because a defence
  that relies on a caller's discipline is a convention. Hidden names are refused
  as a class, so the `.git` and `.env` beside somebody's site never leave the
  building. No directory listing is ever generated.

  **Every refusal is the same 404.** Hidden name, traversal, missing file,
  directory without an index, unreadable file, escaping symlink — one answer, so
  the difference between them cannot be used to map the directory one probe at a
  time. The operator gets the real reason in the log on the device they own.

  `--dir` has no default, because a `site` that published whatever folder the
  operator was standing in is the worst thing the command could do. `--bind`
  still defaults to loopback (ADR-0003 §3).
- **A second CSP profile**, `csp::published_site`. The site and the panel are
  separate origins on separate ports, so the operator's own stylesheet and their
  own script files can run without that permission reaching the screen that
  reports whether their battery is safe. The whole of the difference is
  `script-src 'self'` instead of a per-response nonce; inline script is still
  refused on both. `serve::Surface` is passed at every render rather than
  defaulted, so which policy a response carries is a decision at the call site.
- `Host::is_file`, and `FakeHost::with_dir` beside it — see below for why.

### Fixed

- **The safety panel asserted a governor level nobody had computed.** The
  governor row is the one row on the panel that renders as `Verified`, with the
  positively worded evidence *"governor at NORMAL; no threshold crossed"* — and
  `vayucell status` and `vayucell serve` both passed a literal `Level::Normal`
  into it. On a 60 °C phone, every other row did its job while that one printed
  green and said no threshold had been crossed, which was not a stale reading
  but a comparison nobody had made.

  `report::observed` now derives the level from the same cell the panel is
  about, and both callers go through it. `run` still passes its own level, and
  should: the supervisor's governor has latched across ticks, and replacing that
  with a fresh reading would throw the history away.

  Found by running the binary against a fake sysfs tree and heating it, not by
  reading the diff — which is now the third defect in this project found that
  way and the third that every unit test was happy with.
- **Two serving processes were two outage ladders.** `site` and `vault` each
  built their own `Shed`, each measuring from its own start instant. One process
  serving one surface is fine, and that is all there ever was — until
  `docs/INSTALL.md` started telling beginners to run both, at which point one
  phone with one battery had two ladders latching independently and able to
  disagree about which rung the node had reached. The one that disagrees in the
  reassuring direction is the one still serving after the other has shed.

  The ladder now lives in `Cell` and the surfaces borrow it. The governor
  reading is deliberately **not** shared: it is re-read per request through a
  fresh governor that cannot latch, because the rung is history and the cell's
  temperature is not.
- **The vault quota was a number, not a limit.** It was built once at startup as
  `Quota::new(0, limit)` — usage fixed at zero, for the life of the process — so
  the only upload it could ever refuse was a single file larger than the whole
  quota. Two hundred uploads of half a gigabyte each fitted inside a one-gigabyte
  vault without a word.

  What the directory already holds is now read **before every upload**, the same
  way the governor is asked before every request and for the same reason: a
  figure taken once is a figure that is wrong from the second request onward, and
  wrong in the admitting direction.

  Measuring is I/O, and I/O fails. A directory that cannot be read now **refuses
  the write** rather than counting as empty: an unreadable usage figure is
  indistinguishable from free space, and a limit that quietly stops being one on
  the first permission change is worse than no limit at all.
  `Admission::of` takes an `Option<Quota>` so that case cannot be forgotten, and
  `Refused::Unmeasured` keeps it distinct from `Refused::Full` — "full" names a
  shortfall, which is a measurement, and this refusal is the absence of one. It
  answers `503`, never `507`.

  A **delete is unaffected either way**, through `Admission::for_removal`. A
  vault that cannot be measured is a vault somebody needs to be able to empty.
- **The vault handed stored files out at `PROTECT` and `HALT`.**
  [ADR-0009](docs/adr/ADR-0009-accepting-a-file.md) §2's table has always said
  the vault refuses reads wherever the website does, but only the write path ever
  consulted the device — a cell in enough trouble to stop serving a web page was
  still spinning storage up for anybody enrolled. Reads now sit on
  `site::Availability`, the same thresholds as the site: `DERATED` and
  `Stage::Announced` still answer, `PROTECT`, `HALT` and `Stage::Shed` and below
  do not. The refusal is decided **before** the disk is touched, not filtered
  afterwards.
- **A `507` said `Insufficient Storage` in its status code and
  `Service Unavailable` in its reason phrase.** Two different answers in one
  line, and the wrong one is the one most clients parse.
- Two messages that outlived the commands they described: `enrol` told the
  operator to revoke a device by editing the store by hand, and the usage text
  listed `--bind` as belonging to `serve` and `site` only.

- **Every `PUT` response had an empty body.** `Response::render` suppressed the
  body for any method that was not `GET`, which was correct when `Get` and
  `Head` were the only verbs and silently wrong the moment `Put` existed: an
  upload confirmed nothing and a `400` explained nothing. The condition now asks
  which verb *omits* a body, which is the form that stays correct when a verb is
  added. Found by running an upload, not by reading the diff.
- **Two mutations had gone stale** against the renamed verb test and the line
  above, and the mutation gate reported one as `SURVIVED` rather than passing
  quietly — a test name that no longer exists matches nothing and passes
  vacuously, which is the exact failure the gate exists to catch.
- **Per-device credentials** (ADR-0010) — `core/src/auth.rs`, the answer to
  *whose* file it is. ADR-0009 settled whether the device is fit to take a file
  and said outright that shipping that without this would be worse than shipping
  neither.
  **The human never picks the secret, and that decision is the whole design.**
  Charter V.5 forbids third-party runtime dependencies, so there is no `argon2`
  and no `bcrypt` here — and hand-rolling a memory-hard derivation under a rule
  that exists to keep unreviewed code out of the build would be the worst
  possible use of that rule. A chosen password needs one; 256 bits of kernel
  randomness needs nothing. `Secret::new("hunter2")` is a `WrongLength` error and
  a test asserts it, so no cryptography is implemented in this file.
  **An empty store refuses everything.** The most dangerous thing this module
  could do is treat "nobody enrolled" as "authentication off" — the state every
  installation begins in. `StoreEmpty` is also kept distinct from
  `NotRecognised`, so an operator who has enrolled nothing is told that rather
  than sent looking for a typo that is not there.
  **Every entry is compared, every time.** Returning on the first match would
  answer sooner for a device enrolled early than one enrolled late.
  `constant_time_eq` accumulates a difference instead of returning at the first
  mismatching byte, because `==` on a secret leaks how many leading bytes were
  right — enough to recover it one byte at a time. It is checked exhaustively
  against the language's own `==` over every input up to three bytes.
  **A secret never prints.** `Secret` does not derive `Debug`; a derived one
  reaches every `{:?}`, every `unwrap` panic and every log line that formats a
  structure containing it, and none of those call sites reads like a disclosure.
  Tests assert the value is absent from the debug output of the secret, of the
  credential, of the whole store, and of the parse error raised on a line that
  contained one.
  **The store holds secrets rather than hashes**, deliberately: anyone who can
  read it is already the same user, on the same filesystem, as the vault it
  guards, so hashing would defend the credential and lose the files in the same
  breath. That makes the file's mode the whole of the protection, so
  `readable_by_others` checks it rather than assuming it.
  A malformed line refuses the whole store by line number rather than loading
  what it can — a partially loaded store is a device that stopped working for a
  reason nobody connects to the edit.
  No route consults this yet, and enrolment is not written. `serve::Method` still
  has only `Get` and `Head`.

- **A directory resolved as though it were a page.** `resolve` asked
  `Host::exists`, which cannot tell a folder from a file, so `/blog` resolved to
  the *directory* and the read failed — a server error for a page that was there
  the whole time. Every unit test passed, because the fake host had no
  directories: every path in it was a file, so the case did not exist in the test
  world. `Host` now has `is_file`, `FakeHost` has `with_dir`, and the regression
  is pinned. Found by running the thing, not by reading it.
- **An unreadable file answered differently from a missing one.** A 500 against a
  404 is more accurate and is also a directory listing delivered one status code
  at a time: a stranger could tell "this exists and I cannot have it" from "this
  does not exist". Both are now 404 on the wire, with the reason logged where the
  operator — and only the operator — can read it.

- **A failing gate did not fail CI.** The install job was added to `ci.yml` and
  left out of the aggregating required-checks list. It then failed on its very
  first run — correctly, on a real installer bug — and CI reported *all required
  checks passed*. The aggregator carries a comment warning about exactly this
  hazard; the hazard was written down and nothing enforced it. The actions gate
  now refuses any `ci.yml` job that is not in the required list, and a self-test
  plant proves it fires.
- **The installer said Rust was available when Rust could not run.** It asked
  `command -v cargo`, which answers "is there something on PATH called cargo" —
  not the question. On a machine carrying a rustup shim with no default
  toolchain the name resolves, the check passes, and the build dies with
  "rustup could not choose a version of cargo to run". Presence is not
  verification, which is this project's whole argument, and the installer was
  doing the thing the charter forbids everywhere else. It now runs
  `cargo --version`, and where rustup is present but unconfigured it says so and
  gives the one command that fixes it.
- **The build failure told people to free up 2 GB regardless of the cause.** It
  guessed, and its guess sent someone to fix something that was never wrong. The
  build's own output is now kept and its last lines are shown, because the real
  error already says why.

### Changed

- `v0.0.2` is published, so `install.sh` now downloads a signed, checksummed
  build in seconds instead of compiling on the phone. README and
  `docs/INSTALL.md` said the opposite — they were written before a release
  existed and were left saying "no published release yet", which stopped being
  true the moment one was cut. Both now describe the download-and-verify path,
  and the install gate prints which path it actually took rather than letting a
  green tick stand for either.

## [0.0.2] — 2026-08-08

The first release with something a person can actually download and run.

### Fixed

- **The release published library files, not a program.** Every tagged release
  would have collected `libvayucell-<target>.rlib` — a Rust static library,
  which nobody can run — so `install.sh` would never have found a usable build
  and *every* install would have silently fallen back to compiling from source
  on the phone. Twenty minutes, on a device chosen for being old, for a download
  that should have taken seconds. The build was green the entire time, because
  nothing connected the name the release writes to the name the installer asks
  for. The release now cross-links a real binary for all five targets — the
  Android ones against the NDK at API 24, which is the same Android 7 floor
  `docs/INSTALL.md` promises — and publishes `vayucell-<target>.tar.gz` with a
  fixed mtime and sorted entries so the tarball stays reproducible.
- **`scripts/install-gate.sh` now checks the two names agree**, in both
  directions: every target the installer downloads must be one the release
  matrix builds, and the release must publish a runnable binary under the name
  the installer asks for. This is the check that would have caught the above.
- **The release gate checked one manifest out of two.** It compared
  `core/Cargo.toml` against `.release-version` and never looked at
  `cli/Cargo.toml` or at the `vayucell-core = { version = "…" }` pin between
  them — so a version bump left the CLI behind, pinning a version of the core
  that no longer existed. That is a release which fails at dependency
  resolution *after* the tag is public. Manifests are now discovered rather than
  listed, so a crate added later is not exempt by never having been named.
- **Two self-test plants had gone stale by hardcoding `0.0.1`.** They were
  scored `STALE` and `MISSED` on the first release the project ever cut — the
  moment they mattered most. Both are now version-agnostic. The harness caught
  this itself; that is what the fingerprint check is for.

### Added

- The installer now resolves a full Rust target triple rather than a bare
  processor name, because the triple is the string the release names its
  artefacts with and a friendly name is one translation step where drift hides.
- **A one-command installer for a phone** (`install.sh`) and
  [`docs/INSTALL.md`](docs/INSTALL.md), written for somebody who has never
  opened a terminal. It names the battery risk and waits for an explicit `yes`
  **before writing anything**, installs what is missing, verifies the checksum
  of a published build or falls back to building from source, and refuses to
  claim success until the program it installed has actually run. Every failure
  path says what to do next rather than printing an error code. It never asks
  for root and writes nothing outside `~/.vayucell`, so removing it is one
  `rm -rf`. The guide states plainly that no release has been installed on a
  physical phone, that `UNSAFE` is the expected and correct verdict on an
  ordinary handset, and that hosting a website or storing files is not built
  yet — the safety layer had to come first.
- **An install gate** (`scripts/install-gate.sh`), because `install.sh` is the
  only file here that runs on a stranger's device and the only one the test
  suite cannot reach — which made it the least-tested and most exposed file in
  the repository. It requires every failure path to name both what happened and
  what to do, requires the battery warning to precede the first write to disk,
  requires the physical-inspection instruction to be present, refuses an
  installer that escalates privileges, and installs from a clean `HOME` twice
  over, running the result. It prints that Termux itself is not exercised
  rather than letting green ticks imply a device was involved. Four plants in
  the gate self-test prove it fires; the count there is now 52.

- **The Battery Safety Governor** (ADR-0002) — the subsystem Charter Article
  III.1 required before anything may serve traffic. State machine, verification
  loop, thresholds, and a recovery path that requires a person who looked at the
  phone.
- Response security headers as a set (ADR-0006 §3), with the posture committed
  to `docs/security-posture.txt` so weakening it is a visible diff.
- The **power-supply sysfs layer** (ADR-0002 §2–3): battery readings that refuse
  to be assembled from whatever happened to be readable, mechanism detection that
  records which node answered, and a charge ceiling that reads back from the
  hardware rather than from what this process remembers writing.
- The **sampling cadence** (ADR-0002 §3): thirty seconds when nothing is close
  to happening, five when the cell is charging or within five degrees of the
  lowest threshold. It is a function of the reading rather than a loop that owns
  a clock, so a cell warming over an hour is a handful of assertions instead of
  an hour of waiting — and so the device is not kept awake by the monitor that
  exists to protect its battery.
- **A governor that has gone blind now says so.** Three consecutive failed reads
  derate the device and name the reason. Before this, a phone whose power-supply
  nodes vanished — a kernel update, a permission change — produced no readings,
  no transitions, and a panel that still said `NORMAL`; a monitor that has
  stopped measuring and stays quiet is reporting a healthy device. A reading
  that actually arrived is the only thing that clears the counter. Unreadability
  also tightens the sampling cadence rather than backing it off, which is the
  direction a retry timer would naturally have taken it.
- **The mains-loss shed ladder** (ADR-0002 §8) — the inversion. A governed
  phone is a server with an integrated uninterruptible power supply, which no
  single-board computer can say without buying a UPS costing more than the
  board. On mains loss the node announces, sheds non-essential services at 60
  seconds, checkpoints and quiesces its database at 180, and shuts down while
  it still holds a reserve. Reaching that reserve shuts the node down whatever
  the clock says, and time alone never does: a node an hour into an outage
  still holding 70% is doing exactly what it was built to do.
- **The UPS claim is computed rather than written down.** A handset running
  with its pack removed has no cell to ride an outage on, so mains loss stops
  it immediately — and it reports that it cannot make the claim, instead of
  presenting three minutes of ladder it has no energy to run.
- **The safety panel** (ADR-0002 §5–6) — the one screen anybody actually
  reads, and so the one place where being wrong is guaranteed to reach them.
  Every row cites what it saw, including the rows that admit they could not
  check; there is no way to write "verified" without saying what verified it.
  The headline is computed from the rows rather than set beside them, and a
  single unchecked row is enough to take it off `PROTECTED` — four green rows
  and one nobody could read is not a protected device.
- **Swelling is estimated and never claimed.** The confidence attached to that
  estimate has no `High` setting and cannot be given one without editing the
  source, because software has no instrument for a millimetre of deformation.
  The panel renders it as an estimate and then asks for the check that does
  settle it: the phone face-down on a flat table, at every risk level rather
  than only the alarming ones — an estimate reading nominal is not evidence of
  a flat cell.
- **What the panel says is committed to `docs/panel-snapshot.txt`**, alongside
  the response security posture. Both the reassuring panel and the alarming one
  are rendered there, so softening the alarming one — the way status displays
  actually drift — produces a plain-text diff rather than an innocuous-looking
  edit to a Rust file.
- **The fuzzer found a real bug within seconds of first running.**
  `charge_full * 100` overflowed `i64` in the state-of-health calculation —
  `charge_full` is whatever a vendor kernel wrote into the node, parsed with no
  upper bound, so a device reporting nonsense there panicked under debug
  assertions and silently wrapped to a negative without them. It sat inside the
  reading the governor uses to decide whether to keep charging a cell. Now a
  `checked_mul` whose overflow reports `Unknown`, because a capacity that cannot
  be scaled is unverified rather than a number.
- *Maintainers only:* the fuzz target for the request line asserted that an
  accepted path contained no `..` anywhere, and the fuzzer produced `/a..b`
  within seconds — an ordinary filename. The oracle was wrong, not the parser,
  and it now checks per segment, which is what the parser actually guarantees.
  An over-strict fuzz oracle costs exactly as much attention as a real bug.
- **The schema validator is pinned by hash, not just by name.** Three workflows
  ran `pip install jsonschema`, which resolves to whatever the index serves at
  that moment — the same moving-reference problem the action tags had, in the
  job that decides whether a device profile is valid. `requirements/schema.txt`
  now pins every package and every published artefact hash, installed with
  `--require-hashes` so pip refuses when anything in the resolved set lacks one.
  All 116 of rpds-py's per-platform wheels are listed, because a hash set
  covering only the machine that generated it fails on every other runner.
- **`SECURITY.md` says how to report and what to expect back.** It described two
  kinds of defect and never named a route or a timeframe. It now points at
  private vulnerability reporting and commits to acknowledgement in 7 days,
  assessment in 14, and a fix or a stated refusal within 90 — with an escalation
  path if those pass in silence, because a disclosure process nobody answers is
  worse than none: it persuades a reporter to stay quiet.
- **Every GitHub Action is pinned to a commit SHA.** Sixty-three references
  across the workflows were tags — and a tag is whatever its owner repoints it
  at tomorrow, with no diff appearing in this repository. That is the
  supply-chain attack a project asking people to run a binary unattended in
  their home has no other defence against. The actions gate now requires the
  pin rather than merely resolving the reference, and requires the commit to be
  fetchable, because a typo in a SHA looks exactly like a legitimate one.
- **The auto-merge workflow no longer grants write at the top level.** It runs
  on `pull_request`, where the branch is proposed by whoever opened it, and a
  workflow-wide `contents: write` hands that token to every job the file will
  ever gain — including one added later by somebody who did not read the header.
  Now `permissions: {}` at the top and the two scopes it needs on the one job
  that needs them.
- **Static analysis by a tool that did not write this code.** A CodeQL workflow
  on push, on pull request, and weekly — because new queries ship after the code
  does, so a repository that only scans on push stops learning the day the last
  commit lands. Everything already here was written by somebody who believed the
  code was correct, which is precisely the belief a second analyser does not
  share.
- **Fuzzing, on the three places a string this project did not write becomes a
  decision it acts on:** the HTTP request line, a battery reading from a vendor
  kernel, and a CSP nonce. The harness carries `libfuzzer-sys` and is therefore
  excluded from the workspace, so nothing it touches can reach the binary — and
  the charter gate checks that exclusion rather than trusting the comment
  explaining it, because an exemption nobody verifies is a hole.
- **A local-only listener** — the first thing in this project a browser has
  ever spoken to, which makes the CSP and the response headers real rather than
  rendered into a snapshot. `vayucell serve` binds loopback by default; reaching
  the rest of your network is a flag you type. Every response carries the full
  posture including the errors, because a 404 without a CSP is still a page a
  browser will execute script in. The nonce is minted per response and consumed
  by the render. Traversal is refused rather than normalised, and
  percent-encoding refused rather than decoded, because both bypasses work
  precisely when the check runs against a different string from the one that
  arrived. Parsing and routing own no socket, so a malformed request is a unit
  test.
- *Maintainers only:* the first version of the nonce minter called
  `std::fs::read("/dev/urandom")`. That device has no end, so the read never
  returned — it allocated until the process was killed, and the listener died on
  its first request. Replaced with a `read_exact` into a fixed buffer. The good
  outcome was that it failed immediately and visibly; the same mistake against a
  file that is merely large would have shipped as a slow leak.
- **Sovereign ingress** (ADR-0003) — four modes, each declaring seven
  properties with none of them optional, because the three fields the ADR's
  draft lacked each changed a decision. An onion depends on a commons rather
  than on nothing; it is not reachable by an ordinary browser, since `.onion` is
  a reserved name that is not in DNS; and it has the worst compromise story of
  the four, because the identity key is the address and there is no revocation.
  The default is local-only: publishing is irreversible disclosure, which
  Charter Article VIII.5 forbids without explicit confirmation, and it is the
  only default executable on T0.
- **The governor now outranks ingress, by construction.** The worst defect
  ADR-0003 records is that its draft made the highest-heat mode the default
  while the battery governor existed to suppress heat-driven ageing — and
  neither document mentioned the other. `shed_for` takes a governor level and
  has no parameter that overrides it: `DERATED` sheds high-thermal ingress
  first, `PROTECT` and `HALT` stop everything outward-facing, and local-only
  survives because stopping it would take the panel away from the person who
  most needs to read it. The heat cost, the audience limit and the permanent
  compromise are disclosed *before* the mode is chosen, and a device that
  cannot hold a charge ceiling is told that this combination has no mitigation
  available at all.
- **"The tunnel is up" is not expressible.** `Reachability` has no variant for a
  running process; verified means a request originating outside the device
  traversed the path and was served. A loopback test proves nothing about a path
  whose entire difficulty is external.
- **Storage durability** (ADR-0004) — the guarantee is a number rather than an
  adjective. `RecoveryPoint` has no variant meaning durable, and a
  `compile_fail` doctest keeps it that way: a phone is a replica, and that is a
  guarantee only for data older than the replication lag. The closest thing to
  good news the type can express is how far behind the off-device copy is, which
  still names the window in which data exists on one device only.
- **A backup nobody has restored can never read as proven.** Everything anybody
  checks on a written backup — its size, its checksum, that it appeared — is a
  property of the file rather than of the restore, and writing more backups is
  what people do instead of restoring one, so it never moves that row. Of the
  four things ADR-0004 records, the one that can read as verified is the shed
  ladder completing, because it measures this software's behaviour rather than
  the flash controller's honesty.
- **Assuming the flash lies is never itself reported as a fault.** It is the
  correct posture toward all consumer flash and true of every device; rendered
  as a warning it would appear on every panel forever, and a warning that is
  always on is one nobody reads. `lab_verified` cannot be claimed without naming
  the method, the fixture and the date, so it cannot be set by somebody who
  rebooted a phone and watched the database survive — which is the test ADR-0004
  withdrew.
- **`vayucell`, the binary** — the thing that owns the loop. `status` reads the
  device once, prints the panel and exits with the verdict: 0 protected, 1 not
  fully verified, 2 unsafe, 64 unusable arguments. A monitor gets the answer
  without parsing prose, and unmeasured stays a different number from failed,
  because collapsing them loses the distinction Article IV exists to keep.
  `run` holds the ceiling and stops when the governor halts. A `--ceiling` of
  200 is refused rather than clamped — 100 holds no ceiling at all, so clamping
  would make the unsafe reading the silent one on the single setting that
  governs a cell in somebody's home. Zero third-party dependencies here too;
  argument parsing is thirty lines of `std`.
- *Maintainers only:* **the mutation gate was corrupting a crate it did not know
  about.** It snapshotted `core/src` by name, so mutations naming files in the
  new `cli/src` were applied and never restored — five accumulated on disk.
  Nothing in the mutation output said so; the gate's own closing check that the
  suite must be green *after* the last restore is what caught it. It now
  enumerates every crate rather than naming one, the charter gate's
  no-dependencies rule was widened from `core/Cargo.toml` to every manifest, and
  the gate self-test plants a dependency in the CLI crate to prove that widening
  works.
- **The supervisor loop** — the piece that makes the rest a running thing. One
  tick reads the cell, enforces the ceiling, shows the reading to the governor,
  advances the shed ladder and returns how long to wait. The clock is a trait,
  so thirty simulated days — 86,400 ticks — is a unit test that finishes in
  milliseconds. That test says the composition does not drift or stop
  escalating over a long run; it says nothing about a real kernel or a real
  cell, and it is not the roadmap's P2 gate. The unreadable case is not an
  early return: it feeds the blind counter, tightens the cadence and fills in
  the same outcome as any other tick, because a loop whose error path is
  shorter than its success path goes quiet exactly when something is wrong.
  A governor that halted before a restart comes back halted, because the
  supervisor is handed one rather than building a fresh one.
- **ADR-0007** records the panel's design decisions and, more usefully, the
  alternatives that were rejected: a numeric risk score, a stored headline, a
  conditional inspection prompt, and dropping the charge-mechanism row on
  devices that have no charge mechanism. Each of those is the obvious design,
  and each fails in the reassuring direction.

### Changed

- Charter Article III.1 is now **satisfied**: the governor exists, so serving
  capabilities are permitted. The gate stays live in the other direction.
- *Maintainers only:* the doctest gate now asserts the **exact** number of
  compile-time proofs rather than a floor of one. Those proofs are collected
  only from public items, so a proof moved onto a private one runs nothing and
  still reports success — and under a floor of one, fifteen of sixteen could
  disappear without the gate noticing. The gate against silent passes was
  passing silently. Both directions are now covered by the gate self-test.

## [0.0.1] — 2026-08-06

The founding release. **Nothing here serves traffic**, and by charter nothing
will until the battery governor exists.

### Added

- **The charter** and a subordinate governance constitution — 93 rules, each
  marked with whether a machine or a human enforces it.
- **The capability registry** (ADR-0001). Obligations have no valid zero value:
  a capability that sets something without reading it back does not compile.
- **Tier detection** (ADR-0001 §2) from positive evidence only. A machine
  nothing recognises is `Unknown`, and `Unknown` satisfies no capability floor.
- **A Content Security Policy as a type** (ADR-0006). `Source` has no variant
  for `'unsafe-inline'` or `'unsafe-eval'`, so weakening it is an addition to a
  public enum rather than a one-word edit to a string.
- **The hardware compatibility database** (CC0) with a schema that refuses a
  verified charge ceiling which names no sysfs node.
- **CI that enforces the charter**, and gates that are themselves tested: 34
  planted violations must each be caught, and 20 mutations must each turn a test
  red.

### Known limits

- No end-to-end test on real hardware. Every device-facing behaviour is
  exercised through a fake host describing handsets nobody here is holding.
- Four charter articles are human review only — III.2, III.4, IV.4 and V.4. The
  charter gate prints them on every run rather than omitting them.
