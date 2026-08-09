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

## [0.0.6] — unreleased

The heading says *unreleased* rather than carrying a date. The date is written
when the tag is cut.

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
