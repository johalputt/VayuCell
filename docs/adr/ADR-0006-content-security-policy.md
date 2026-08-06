# ADR-0006 — Content Security Policy: the browser as the last enforcement point

**Status:** accepted
**Supersedes:** nothing
**Related:** [ADR-0001](ADR-0001-tier-model-and-capability-registry.md) (the
registry pattern this reuses), [ADR-0003](ADR-0003-sovereign-ingress.md) (what is
reachable, and from where), [CHARTER.md](../../CHARTER.md) Articles IV and V

---

## §0. The framing that would have been wrong

The obvious way to write this ADR is "VayuCell will set a strict CSP header",
followed by the header. That framing has a defect worth naming before the
decision, because it is the defect that makes most CSPs decorative.

A CSP written as a string constant is edited by whoever is trying to ship
something on a Friday. `'unsafe-inline'` is one word. It makes the immediate
problem disappear, the page keeps working, nothing in the diff announces that the
policy's central guarantee has just been removed, and the header still *reads*
strict in every audit that only greps for `Content-Security-Policy`. The failure
has no symptom. It is Article IV's exact shape — a report that means "configured"
while its reader hears "verified" — relocated into a security header.

So the decision below is not primarily about which directives to send. It is
about making the weakening edit **impossible to write casually**.

There is a second thing this ADR must not claim. A CSP is enforced by the
browser, which is software this project does not ship, on a device that may be
running a vendor kernel from 2019. It is the last enforcement point, not the
first, and it is not a substitute for output encoding. Where a policy is the only
thing standing between an injection and execution, the injection is already the
bug.

---

## §1. Context

VayuCell serves a control surface from the phone: the screen where an operator
sees which tier their device reached, whether the charge ceiling actually held,
and what the governor is doing. Later it serves hosted sites.

Three properties of that setting shape the policy.

1. **The control surface is the most dangerous page in the system.** It is the
   one authenticated to change power settings on a lithium cell. An injection
   there is not a defacement.
2. **There is no third-party origin to admit.** Everything the control surface
   needs is on the device. A project that must federate with an ad network or a
   font host has to make compromises here; this one does not, and should not
   quietly acquire the compromise anyway.
3. **Article V.5 applies to the reporting endpoint.** Violation reports describe
   what the operator's own pages tried to do. Sending them to a collector this
   project runs would be telemetry arriving through a side door, however useful
   the aggregate would be to us.

---

## §2. Decision

### §2.1 The unsafe keywords are unrepresentable

The policy is built from a `Source` enum in `core/src/csp.rs`, and that enum has
**no variant for `'unsafe-inline'`, `'unsafe-eval'` or `'unsafe-hashes'`.**

Restoring one is not a one-word edit to a string literal. It is an addition to a
public enum, in a file whose module documentation explains why the variant is
absent, accompanied by a match arm — a diff nobody merges without noticing. The
capability registry makes `verify` non-optional for the same reason and by the
same technique: *the guarantee lives in the type, where forgetting it does not
compile.*

Three `compile_fail` doctests hold that proof, on the **public** module, because
rustdoc collects doctests nowhere else — on a private item they run zero tests
and print `test result: ok`.

The mutation gate re-adds the variant, with its match arm, and requires the
doctest to go red. Without that step the proof is only a claim about a claim.

### §2.2 The baseline denies

`default-src 'none'`, not `'self'`.

With `'self'`, a directive nobody enumerated silently inherits same-origin
permission, and the policy's coverage becomes a question of what its author
happened to remember. With `'none'`, a forgotten directive fails closed: it shows
up as a broken page during development instead of an open door in production.

Four directives are set to deny even though no script directive covers them,
because each turns a read-only injection into a working attack:

| Directive | Attack it closes |
|---|---|
| `frame-ancestors 'none'` | Clickjacking. The control surface is never legitimately embedded |
| `base-uri 'none'` | An injected `<base>` rewrites every relative URL, including the ones nonce'd scripts load from |
| `form-action 'self'` | An injected form posting the operator's session to another origin |
| `object-src 'none'` | Plugin content, which predates and ignores most of the modern policy |

### §2.3 Script executes only with a per-response nonce

`script-src 'nonce-…'`, and **not** `'self'`.

`'self'` permits any file on the device's own origin to execute. On a server
whose entire purpose is hosting operator-supplied content, that set is not small
and not fully under our control.

The `Nonce` type enforces what is enforceable and says so about the rest:

- At least 22 base64url characters, which is 128 bits.
- Base64url alphabet only. A nonce carrying `'` or `;` would terminate the
  directive and append attacker-chosen policy to the rest of the header.
- **Not `Clone`**, and `Policy::render` consumes it. Reusing a nonce requires
  generating another one. A repeated nonce makes `'nonce-…'` exactly as strong as
  `'unsafe-inline'` while continuing to read as a strict policy.

What the type cannot check is that the value is *random*. It cannot tell a
CSPRNG from a counter. That obligation is documented on the type rather than
implied by its existence — the same rule the hardware database applies to a
charge ceiling that was never actually read back.

### §2.4 Passive sources may not touch executable directives

`data:` and `https:` are permitted on `img-src`. They are refused, at
construction, on `default-src`, `script-src`, `script-src-elem`, `worker-src` and
`object-src`.

`script-src data:` reads like a small concession for an inline asset and permits
any script an injection can spell.

### §2.5 The origin allowlist is closed, and currently empty

`allowed_origin` holds a fixed list. A caller passing an arbitrary origin is a
caller who can widen the policy from a configuration file, a hosted theme, or a
bug — none of which are reviewed the way this file is.

The list is empty today. The function exists so that the first proposal to add an
origin arrives as a change to it.

### §2.6 Violation reports do not leave the device

`report-uri` must be a same-origin absolute path. `Policy::report_to` refuses
anything else, including protocol-relative `//host/path`.

Aggregate violation data across every VayuCell install would be genuinely useful
to this project. That is precisely why Article V.2 forbids it: the useful thing
and the forbidden thing are the same thing.

---

## §3. The rest of the headers

A Content Security Policy is the loudest of the response headers and the least
complete. It does nothing about a MIME type the browser decides to sniff, a
referrer leaking a session path to another origin, a document sharing a browsing
context group with an attacker's, or a device permission the page never needed
and was granted anyway.

Each of those has its own header. Each is one line. **Each is forgotten
separately** — which is the actual failure mode, and it is why they are not
seven independent lines in a handler here. `SecurityHeaders` in
`core/src/headers.rs` emits the whole set or none of it.

### §3.1 The ones with no weaker legitimate value are not configurable

`X-Content-Type-Options: nosniff` is not a parameter. There is no case in this
project where letting the browser guess a content type is wanted, and a knob for
it would only ever be turned the wrong way.

`X-Frame-Options: DENY` is sent **in addition to** `frame-ancestors 'none'`, not
instead of it. `frame-ancestors` is the modern control and supersedes the legacy
header — but the WebView on an abandoned vendor Android build may predate it,
and the browser is the one component here that nobody gets to choose.

### §3.2 Referrer, closed by type

`Referrer` has no `unsafe-url` and no `no-referrer-when-downgrade` variant. Both
send a full URL cross-origin, both are still common defaults elsewhere, and
neither can be written down. A `compile_fail` doctest holds that proof and the
mutation gate re-adds the variant to confirm the doctest notices.

The production default is `no-referrer`.

### §3.3 Permissions denied by enumeration, never by omission

An unlisted feature is governed by the browser's default, and defaults change
without asking us. On a device that has a camera, a microphone and a location,
that is not a theoretical difference. Thirteen features are named and denied.

### §3.4 HSTS has a floor

`Hsts::MIN_MAX_AGE` is 180 days and a shorter value is refused rather than sent.
A token max-age reads as an HSTS deployment in every scan while leaving a window
in which a downgrade still works. **The header is a promise about the future, and
a promise measured in hours is not one.**

`Hsts::ONE_YEAR` is a `const`, and a compile-time assertion proves it satisfies
the floor. Lowering it below the minimum stops the crate building rather than
shipping a weaker promise.

Development sends no HSTS at all. Pinning HTTPS from a machine that is serving
plain HTTP locks the developer out of their own device, and the lockout outlives
the mistake that caused it.

### §3.5 Report-only, resolved

This was an open decision in the first draft and it is now closed.

`Content-Security-Policy-Report-Only` is genuinely useful while a policy is being
tightened, and it is the most dangerous value in the module: on a production
build it enforces **nothing** while looking identical to an enforcing header in
every log, every screenshot, and every audit that greps for the header name.

So `Mode::ReportOnly` carries a reason string, and `SecurityHeaders::production`
cannot produce it. Choosing it is possible; choosing it by accident is not.

---

## §4. What this will never claim

1. **That a CSP prevents cross-site scripting.** It reduces what a successful
   injection can do. The injection is still the bug, and output encoding is still
   the fix.
2. **That the policy is enforced.** It is enforced by a browser this project does
   not ship. On an old vendor Android build, the WebView may be years behind and
   may not implement every directive sent to it. §3.1 sends the legacy framing
   header alongside the modern one for exactly this reason, and that is
   mitigation, not a guarantee.
3. **That a nonce is unpredictable.** The type checks length and alphabet. It
   cannot check entropy, and does not pretend to.
4. **That `report-uri` is complete.** Browsers vary in what they report, and a
   policy violation that is never reported is indistinguishable from one that
   never happened. An empty report log is not evidence of a clean system — the
   permanent-failing-row rule of Article IV.4 applies to this reader too.
5. **That this covers hosted sites.** This ADR governs the VayuCell control
   surface. A hosted site's policy is that application's decision, and pretending
   otherwise would put a header in front of software whose behaviour we do not
   control.

---

## §5. Test gates

Thirteen unit tests in `core/src/csp_test.rs`, written as attacks, and three
`compile_fail` doctests with one positive control on the public module.

Ten of these are re-broken by `scripts/mutation-gate.sh`, which requires the
matching test to go red — including the mutation that adds an unsafe variant back
to `Source` and requires the `compile_fail` proof to notice.

That gate has already earned its place here. It found that
`allowing_a_source_clears_the_none_that_was_there` passed a single source, never
reached the branch that strips a stale `'none'`, and so asserted a property that
was true for the wrong reason. It would have gone on passing with the guard
deleted. A green test is not evidence that the test works.

---

## §6. Open decisions

1. **Trusted Types.** `require-trusted-types-for 'script'` is strictly stronger
   than a nonce and is not universally supported by the WebView versions this
   project targets. Deferred until the control surface exists and can be measured
   against real devices, rather than adopted now on the strength of the
   specification.
2. ~~**`Content-Security-Policy-Report-Only` during development.**~~ **Resolved
   in §3.5.** `Mode::ReportOnly` requires a stated reason and
   `SecurityHeaders::production` refuses it.
3. **Subresource Integrity.** Everything is same-origin today, so SRI adds
   nothing. It becomes relevant the first time §2.5's allowlist is non-empty, and
   should be decided in the same change.
4. **Hosted-site policy.** Out of scope here, deliberately. It needs its own ADR
   once there is a hosting surface to reason about.
