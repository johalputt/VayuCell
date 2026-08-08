# ADR-0008 — Publishing a site: serving strangers from a governed phone

**Status:** accepted — implemented in `core/src/site.rs`, `core/src/serve.rs`
(`route_site`, `Surface`) and `cli/src/listen.rs` (`serve_site`,
`read_contained`)
**Supersedes:** nothing
**Related:** [ADR-0002](ADR-0002-battery-safety-governor.md) (the governor this
is subordinate to), [ADR-0003](ADR-0003-sovereign-ingress.md) §3 and §5 (the
default that does not publish, and the rule that the governor wins),
[ADR-0006](ADR-0006-content-security-policy.md) (the policy this needed a second
profile of), [ADR-0007](ADR-0007-the-safety-panel.md) (the surface this must not
weaken), [CHARTER.md](../../CHARTER.md) Articles III.1 and IV

---

## §0. What changes about the project when this exists

Everything before this served the owner. The panel is what somebody can already
see by picking their phone up, which is why it needs no authentication and why
its policy can be as strict as the browser will allow.

A site is different in kind. It is content the owner wrote for other people, and
the requests arrive from people who are not the owner and cannot see the device.
Three things follow, and each of them is a decision this document exists to
record rather than a detail of the implementation.

## §1. The site does not share the panel's origin

A person's own website needs their own stylesheet and their own script to run.
The panel's policy permits script only with a per-response nonce, and this
program does not rewrite somebody's HTML to inject one — so serving a site under
the control surface's policy would mean their site is broken, and serving it
under a policy loose enough to work would mean **the panel is now under that
looser policy too**, because same-origin script can read same-origin pages.

So there are two surfaces, on two listeners, with two policies:

| | Control surface | Published site |
| --- | --- | --- |
| Command | `vayucell serve` | `vayucell site --dir <DIR>` |
| `script-src` | `'nonce-…'`, per response | `'self'` |
| Everything else | locked down | locked down |
| `report-uri` | `/csp-report` | none — there is nowhere to collect |

`script-src 'self'` is the whole of the weakening, stated plainly here so that
nobody has to diff two functions to discover it. Inline `<script>`, inline
`on*=` handlers and script from any other origin remain refused on both.

`serve::Surface` is passed at every render rather than defaulted, so which
policy a response carries is a decision somebody made at the call site rather
than something it inherited.

## §2. Traversal is prevented by construction, and the gap is named

`site::resolve` never concatenates a request path onto the root. It splits into
segments, refuses any segment that is `.` or `..`, refuses any segment beginning
with a dot, refuses separators and NULs, and joins the survivors. No sequence of
accepted segments can produce a path outside the root, so there is no ordering
of checks to get wrong.

This is a second, independent check: `serve::parse_request_line` already refuses
percent-encoding, backslashes and `..`. The duplication is deliberate. A later
change making the parser more permissive would otherwise silently make the site
unsafe, and a defence that depends on a caller's discipline is a convention.

**Hidden names are refused as a class, not by blocklist.** `.git`, `.env`,
`.ssh` and `.aws` are how "I served a folder" becomes a credential disclosure,
and a blocklist is a list of the ones somebody thought of.

**No directory listing is ever generated.** A directory with no `index.html` is
a refusal. A listing publishes everything the operator happened to leave in a
folder — a disclosure they never asked for and would never see in testing,
because they test URLs they already know.

**What the core cannot check, it says.** A symbolic link inside the site
directory pointing outside it is invisible to the `Host` interface, which reads
and tests existence; a link is transparent to both. Containment against links is
enforced in `cli/src/listen.rs::read_contained`, which canonicalises the root and
the file and compares real paths. Both halves are tested — the core with a fake
host, the binary against a real filesystem with a real symlink.

## §3. Every refusal is the same refusal

Hidden name, traversal attempt, missing file, directory without an index, file
that could not be read, link that escaped: all 404.

The tempting design gives each its own status, and each is individually
defensible — a 403 is more accurate for a forbidden path, a 500 tells the
operator their file is unreadable. Together they are a directory listing
delivered one status code at a time: a stranger probing the site learns which
paths exist from the *difference* between the answers.

The operator's diagnosis is not lost. It goes to the log on the device they own,
which is where it belongs and where a visitor cannot read it.

This was got wrong first: an unreadable file answered 500 while a missing one
answered 404, and the difference was found by running the thing rather than by
reading it.

## §4. The governor outranks the site, per request

`site::Availability::of` takes a `governor::Level` and a `shed::Stage` and
answers whether the site serves at all. There is no parameter that overrides it.

| Condition | Site | Why |
| --- | --- | --- |
| `NORMAL`, mains | serves | — |
| `DERATED` | **serves** | Deration answers heat. A static file read on a home network is not producing the heat, and shedding a negligible load to fix a thermal problem is theatre that costs the operator their site. The load worth shedding is high-thermal ingress, and `ingress::shed_for` already sheds exactly that, first |
| `PROTECT`, `HALT` | withheld | The device is in trouble; every watt is the owner's, not a visitor's |
| `Stage::Announced` | serves | Nothing has been torn down yet |
| `Stage::Shed` and below | withheld | That rung's obligation is literally "stopped non-essential services", and a website served from a phone during a power cut is the definition of one |

The verdict is recomputed **per request**, not cached at startup. That costs a
few small sysfs reads per request, which on a home network is nothing, and it
buys the property the project turns on: a cached verdict goes stale, and stale
always fails in the reassuring direction.

A cell that cannot be read yields `PROTECT`, not "assume fine". Absence is never
protection.

The withheld response is a 503 that says which of the two withheld it and why,
phrased for somebody who cannot see the device — so it reads as a deliberate
refusal rather than a fault to retry into. It is returned **before** resolution,
so a withheld site cannot be probed for which paths exist.

## §5. `--dir` has no default

A `site` command that defaulted to the working directory would publish whatever
folder the operator happened to be standing in. That is the worst thing this
command could do, so it is refused with a message that says why.

`--bind` still defaults to loopback, exactly as `serve` does. ADR-0003 §3: the
default must not make a disclosure decision on the owner's behalf. A website
command is precisely where a helpful default of `0.0.0.0` would feel natural and
would be doing that.

## §6. What this is not

It is **not** file storage, and not a sync client. It serves files; it accepts
none. `serve::Method` has no `Post`, `Put` or `Delete` variant, so a route that
accepted an upload could not be written without first widening a public enum in
a reviewed diff.

It is **not** reachable from outside the operator's network. ADR-0003's onion
and relay modes remain unimplemented. `Reachability::Verified` still means a
request from outside was served, and nothing here has produced one.

It has **not** run on a phone.
