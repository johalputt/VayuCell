# ADR-0010 — Per-device credentials: deciding whose file it is

**Status:** accepted — implemented in `core/src/auth.rs`, consulted by
`serve::route_vault`, with enrolment in `cli/src/enrol.rs`. See §7 for what
changed after this was first written.
**Supersedes:** nothing
**Related:** [ADR-0009](ADR-0009-accepting-a-file.md) (the decision this
completes), [ADR-0003](ADR-0003-sovereign-ingress.md) §3 (local-only, and why
that is not a substitute for this), [CHARTER.md](../../CHARTER.md) Article V.5
(no runtime dependencies — the constraint that shaped every decision below)

---

## §0. The constraint that decided the design

ADR-0009 settled whether the device is in a condition to take a file. It does not
settle whose file it is, and it said so: shipping a write surface without an
answer would be worse than shipping neither.

The obvious answer is a password. **It is not available here**, and the reason is
not fashion:

Charter V.5 forbids third-party runtime dependencies, so there is no `argon2` and
no `bcrypt` in this tree. A password a human chose is drawn from a small,
skewed distribution, and the only thing standing between that distribution and an
attacker with the store is a deliberately slow, memory-hard derivation. Writing
one of those by hand, under a rule that exists to keep unreviewed code out of the
build, would be the single worst thing this project could do with its own
constraint.

**So the human never picks the secret.** `Secret` is 256 bits of kernel
randomness, encoded as 43 base64url characters, and there is no constructor that
accepts a memorable value — `Secret::new("hunter2")` is a `WrongLength` error and
a test asserts it. With that distribution there is nothing to guess: an attacker
holding the entire store is doing arithmetic against the search space, not
running a word list.

This is the whole of the argument, and it is why no cryptography is implemented
in this file.

## §1. The store holds secrets, not hashes, and here is why that is not laziness

A credential store is conventionally hashed at rest. This one is not, and the
reason is specific rather than general:

**An attacker who can read the store is already the same user, on the same
filesystem, as the vault it protects.** Hashing at rest would defend the
credential while losing every file it guards, in the same breath. It buys nothing
that matters here.

What follows from that is a real obligation, not a shrug: **the file's mode is
the whole of the protection**, so it is checked rather than assumed.
`readable_by_others` is a pure function of the mode, testable without a
filesystem, and the binary refuses to start on a store any other user can read.
Absence is never protection, and that applies to permissions as much as to
battery readings.

This is recorded here so that "why isn't this hashed" has an answer in the
repository rather than in somebody's memory — and so that the day the threat
model changes (a multi-user device, a store on shared storage), the reasoning
that has to be revisited is written down.

## §2. An empty store refuses everything

The most dangerous thing this module could do is treat *no devices enrolled* as
*no authentication required*. That is the state **every installation begins in**,
so it is the state worth being loudest about.

`Credentials::verify` returns `Refusal::StoreEmpty`, `Credentials::default()` is
the empty store, and both are asserted. `StoreEmpty` is deliberately distinct
from `NotRecognised`: an operator who has enrolled nothing needs to hear that,
not that their secret is wrong — otherwise they go looking for a typo that is not
there.

## §3. Every entry is compared, every time

`verify` does not return on the first match. It walks the whole store and records
the match.

Returning early would make the answer arrive sooner for a device enrolled first
than for one enrolled last, which is a usable signal about the store's contents.
The comparison itself is `constant_time_eq`, which accumulates a difference
rather than returning at the first mismatching byte — a `==` on a secret leaks
how many leading bytes were right, which is enough to recover it one byte at a
time.

Length is compared first and *is* allowed to short-circuit. Every secret here is
exactly 43 characters by construction, so the length carries nothing an attacker
did not already know; and without that check `zip` would stop at the shorter
input and a one-character secret would match everything. A mutation removes the
length check and the prefix test turns red.

Timing is not assertable in a unit test, so what the suite asserts is the
observable consequence — the last entry authenticates exactly as the first does —
and `constant_time_eq` is checked exhaustively against the language's own `==`
over every input up to three bytes.

## §4. A secret never prints

`Secret` does not derive `Debug`. A derived one puts the value into every `{:?}`,
every `unwrap` panic and every log line that ever formats a structure containing
one — and not one of those call sites reads like a disclosure.

The manual implementation prints `Secret(hidden)`. Tests assert the secret is
absent from the debug output of the `Secret`, of the `Credential` that holds it,
of the whole `Credentials` store, and of the parse error raised on a line that
contained one.

There is no `Display` and no `as_str`; the only accessor is
`expose_for_comparison`, named so that a call site which is not a comparison
looks wrong in review.

## §5. A malformed store is refused whole

`parse_store` fails on the first bad line and names the line number. It does not
load what it could and skip the rest.

A partially loaded store is a device that silently stopped working, for a reason
nobody connects to the edit that caused it. Refusing the file is loud, immediate
and fixable.

## §6. What this is not, yet

**No route consults this.** `serve::Method` still has only `Get` and `Head`, and
nothing parses an `Authorization` header. Enrolment — minting a secret from
`/dev/urandom` and writing it to the store — is not written either.

Both are deliberate. This ADR settles *how a credential is judged*; the surface
that presents one and the command that issues one are separate changes, and each
should arrive able to be reviewed on its own.

**Local-only is not a substitute for this and never was.** ADR-0003 §3 keeps the
device off the public internet; it says nothing about the other things on the
operator's Wi-Fi, which is precisely the population a home network's threat model
is about.

It has not run on a phone.

## §7. Since this was accepted

§6 said no route consulted this and enrolment was not written. Both now exist.

**`route_vault` checks the credential first** — before the path is parsed, before
the governor is consulted, before the disk is measured. Every other order leaks
something to a stranger: the name check tells them which filenames are
acceptable, and the device check tells them the battery level of a phone that is
none of their business. A test sends an unauthenticated `PUT` at a halted device
with a full disk and an illegal path, and asserts the answer is `401` and that
the body mentions none of the three.

**`vayucell enrol --device <name>`** mints from `/dev/urandom`, appends to the
store, and prints the secret once. The store is created with mode `0600` *at
open time* rather than chmod'ed afterwards — a file that was world-readable for
an instant has been read by anything that was looking. A name already present is
refused rather than duplicated.

**The reason first given for that rule was wrong, and it hid where the rule was
missing.** This section said a duplicate matters because "revoking it leaves the
other behind and the operator cannot see that". `revoke` skips *every* line
carrying the name, so it does not leave the other behind — the failure named here
was not one this code had.

The real reason is identity. `verify` matches on the secret and answers with the
name, so two rows sharing a name means **two different credentials authenticate
as one device**: `Authenticated(name)` no longer says which of them presented
anything, and revoking that name takes both, including the one the operator had
forgotten was there.

And stating the wrong reason concealed a real gap. The rule was enforced only in
`enrol` — the path *this software* writes. The store is a text file the operator
is told to edit by hand, and the enrolment error itself says to remove the
existing line first, so the one path a duplicate actually arrives by had no check
on it. `parse_store` now refuses a repeated name, naming **both** lines, which is
what §5 already required of every other malformation: refuse the file whole,
loudly, rather than load something nobody asked for.

There is no command that prints a secret back. A credential a program will
re-display is one that leaks through a scrollback or a screen share, and enrolling
again takes five seconds.

**A missing store is the empty store**, which accepts nobody — the path that
turns "no file" into "no authentication" does not exist.
