# ADR-0013 — Relay ingress: a rented dependency, named as one

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by the `--relay-via <HOST>` flag on `all`, the
`cli/src/relay.rs` module, and the `Mode::Relay` profile that has been
declared in `core/src/ingress.rs` since ADR-0003.

## Context

ADR-0003's table of ingress modes has had a relay row since it was
written — a tunnel to a rented host, for clearnet domains and ordinary
visitors — and its profile was fully declared: dependency **Supplier**,
ordinary browsers yes, thermal cost Low, compromise Recoverable, costs
money. The onion half of the table was built (VCIP-0001); the relay half
was not, and PLAN.md said so in exactly those words.

Two facts shaped what building it could mean:

1. **The cell never dials.** Whatever points visitors at this device,
   the forwarding process runs on the rented machine and dials *in* to
   the phone. There is no daemon on this device to supervise, no
   configuration to write, no key to hold: the entire rented
   infrastructure is somebody else's server, administered by somebody
   else's hands.
2. **§12.5 of the plan is permanent**: never independence while using a
   rented relay. That sentence is not a roadmap aspiration; it is a
   standing property of any deployment using this mode, and the code's
   job is to keep saying it out loud.

The temptation to avoid was a "tunnel" abstraction: some structure that
manages SSH keys, writes remote config, health-checks the far end. Every
part of that either dials (forbidden) or hides the dependency inside
convenience (forbidden by honesty, not by charter).

## Decision

**Declaration without management.** The cell accepts a name and tells the
truth about it; everything else stays where it belongs.

1. **`--relay-via <HOST>` on `all`, nowhere else**, for the identical
   reason `--onion-dir` is: a published path is load the governor sheds,
   and only `all` runs a governor. The value is validated at the moment
   it is typed — lowercased ASCII letters, digits, hyphens, dots; no
   scheme, port, spaces or user parts; length capped at 253; every
   refusal naming its rule — because a banner that mangles a hostname
   promises reachability at an address nobody can type.

2. **Three startup sentences before anything binds** (ADR-0003 §5.4),
   generated from the declared profile rather than hand-written prose:
   the supplier disclosure (`who can evict you`, `what the middle sees`,
   `costs money`), the forwarding instruction — which two addresses on
   this device the rented side must forward to, so an operator can
   configure their VPS in one sitting — and the standing: **unverified
   until a request arrives from outside**, the same word the onion uses,
   because neither mode has any other honest starting point.

3. **The panel is never published through it.** The battery report of
   somebody's home is not what this mode exists to hand the world; the
   instruction line names the site and vault addresses and nothing else,
   and a test pins that no panel address appears in what a relay
   deployment announces.

4. **Nothing is claimed about the far end.** No health check, no
   reachability shortcut, no "tunnel up" state. If the relay stops
   forwarding, this cell notices exactly when its visitors do.

## Consequences

PLAN.md's P3 row now reads *both halves written* with the same device
gate as before: reachable-from-outside still needs a handset and an
outside vantage point. What changed is that an operator with a $4 VPS
and a DNS name has commands and sentences instead of a gap — and the
dependency they just bought is counted, in the banner and in the
profile, every single start.
