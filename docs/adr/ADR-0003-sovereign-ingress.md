# ADR-0003 — Sovereign ingress: reaching a server that has no address

- **Status:** Proposed
- **Date:** 2026-08-06
- **Relates to:** ADR-0001 (tiers, capability registry), ADR-0002 (battery governor),
  CHARTER Articles II, IV and V.5

## §0. What the first draft got wrong

This ADR was drafted with four ingress modes "ranked by external dependency",
with the onion service ranked as having **none**, and made the default on that
basis. An adversarial pass took the ranking apart, and the corrections are
recorded here rather than quietly applied, because two of them invalidate the
draft's organising claim.

**First: an onion service is not dependency-free.** It depends on reaching the
Tor network — a small, hardcoded set of directory authorities to bootstrap a
consensus, then guards, introduction points, HSDirs and a rendezvous point. That
is a *different* dependency from a rented relay, and arguably a far better one:
it is a public commons, not a supplier, and no single operator can evict you from
it. But it is not nothing, and ranking it as nothing was a ruler chosen to
flatter the default.

**Second: the default was unexecutable on the only universal tier.** T0 is an
unprivileged process on stock Android — the tier most retired phones are
permanently stuck at. Making the default an ingress mode that tier cannot
reliably sustain meant the recommended path was the one most users could not
take.

**Third, and the one no reviewer was supposed to find:** the onion default
maximises sustained CPU, sustained CPU is heat, and heat is the primary
accelerant of the battery ageing that ADR-0002 exists to suppress. **Neither ADR
mentioned the other.** The project's flagship safety subsystem and its default
ingress mode were in direct conflict and nothing in the design noticed.

That third finding is the important one, and it is a category of error this
project has now hit twice: a decision that is locally correct and globally wrong
because two documents never referenced each other. The mitigation is structural
and appears in §5.

## §1. The problem

A phone on a mobile network is behind carrier-grade NAT. A phone on a home
network is usually behind ISP NAT, often with no usable inbound IPv6. In neither
case is there an address the world can connect to, and no amount of software on
the device creates one.

This defeats most home-server projects before they begin, and it is the reason
"just port-forward" is not an answer for the hardware VayuCell targets.

## §2. The four modes, with an honest field set

The draft's fields were insufficient — they had no slot for the things that
turned out to matter most. Each mode now declares **seven** properties, and the
registry (ADR-0001) rejects a mode that leaves any unanswered.

| | **Onion** | **Relay tunnel** | **Direct** | **Local-only** |
| --- | --- | --- | --- | --- |
| **Depends on** | The Tor network (a commons) | A rented VPS (a supplier) | Your ISP granting inbound | Nothing |
| **Can be evicted by** | No single party | The VPS provider, at will | The ISP changing policy | — |
| **Reachable by ordinary browsers** | **No** — needs Tor-capable client | Yes | Yes | On-LAN only |
| **Sustained CPU / heat cost** | **High** — crypto + circuit churn | Low | Lowest | Lowest |
| **Survives IP change / roaming** | **Yes** — all legs outbound | Yes — tunnel re-dials | No — needs dynamic DNS | N/A |
| **What it can see** | Relays see ciphertext; RP sees no identity | **The VPS sees everything it terminates** | Nobody in the middle | — |
| **Compromise story** | **Worst** — identity key theft is silent and permanent | Rotate the VPS, re-point DNS | Re-issue certs | — |

Three rows in that table did not exist in the draft, and each one changes a
decision.

**"Reachable by ordinary browsers: No."** `.onion` is a reserved special-use
name (RFC 7686). It is not in DNS and never resolves. A visitor needs a
Tor-capable client. So the headline claim "you can serve a real site from a
drawer with no rented infrastructure" is **true about the transport and
overstated about the audience** — it serves the visitors who can reach onions,
which is not the general public. That sentence is corrected everywhere it
appears, including in published articles.

**"Sustained CPU / heat cost: High."** See §5.

**"Compromise story: Worst."** The mode ranked most sovereign has the worst
recovery: the ed25519 identity key *is* the address. Steal it and an attacker
impersonates the service permanently, with no revocation mechanism, no
certificate authority to appeal to, and no way for a visitor to notice. §6
addresses this because the draft simply did not mention it.

## §3. The default

**The default is local-only.** Not onion, not a relay.

A newly installed cell serves its owner on their own network and nothing else.
Publishing to the world is an explicit, informed choice — one where the operator
is shown the dependency, the heat cost and the audience limit of each mode before
choosing.

This is a retreat from the draft and a better answer. A default that publishes is
a default that makes an irreversible disclosure decision on the user's behalf,
and Charter Article VIII.5 forbids irreversible actions without explicit
confirmation. It is also the only default that is executable on every tier.

**Recommended, not default,** once the user chooses to publish:

- **Onion** where the audience is technical, censorship resistance matters, or no
  money should be spent — and where the thermal budget allows it.
- **Relay** where ordinary visitors must reach a normal domain. The panel names
  the provider as a dependency and states plainly that **a relay terminating TLS
  can read everything passing through it** — so the recommended configuration
  passes TLS through to the cell rather than terminating at the relay.

## §4. Verification — what ADR-0001's obligation means here

ADR-0001 makes `Verify` unwaivable: a capability that cannot be read back may not
be reported. For ingress that is not a sysfs read, and the draft never said what
it was. It is this:

> **An ingress mode is verified when a request originating outside the device has
> traversed the path and been served, and the cell has observed itself serving
> it.**

Not "the tunnel process is running". Not "the onion address was published". Not
"the daemon returned success". A loopback test proves nothing about a path whose
entire difficulty is external. Verification therefore requires a real
round-trip, and it re-runs on a schedule, because the failure that matters is the
path that worked for six weeks and then stopped.

Where a round-trip cannot be completed, the mode reports **unverified** — never
"up".

## §5. The thermal contract with ADR-0002

This section exists because its absence was the design's worst defect.

Running an onion service is sustained cryptographic work: circuit builds, relay
crypto, and — under load — introduction handling. On phone silicon that is real
heat, applied continuously, to a device whose battery is being held at a ceiling
precisely to slow heat-driven ageing.

**The governor wins. Always, and by construction:**

1. Every ingress mode declares a **thermal class**, and the registry rejects a
   mode that does not.
2. When the governor enters `DERATED`, high-thermal-class ingress is **shed
   first**, before storage or serving work.
3. In `PROTECT`, all outbound-facing ingress stops.
4. **The panel shows the interaction before the choice is made**, not after: *"On
   this device, onion ingress raises sustained temperature; the battery governor
   may shed it under thermal load."*
5. A cell that cannot hold a charge ceiling (T0, per ADR-0002 §1) and is asked to
   run high-thermal ingress **says so explicitly at the moment of choosing**,
   because that is the combination with no mitigation available.

The general rule this produces, and which now applies to every future ADR in this
project: **a subsystem that consumes sustained power must declare that fact to
the governor, and the governor's authority is absolute.** No feature outranks
Article III.

## §6. The onion identity key

The address is the public half of an ed25519 key. Its custody is therefore the
whole security of the mode, and the draft ignored it.

- **Where it lives** is stated in the panel, not left to be discovered.
- **It is sealed at rest**, not left readable beside the service config.
- **Backup is explicit and warned**: a backup of the identity key is a backup of
  the *ability to be this site*. It belongs in the encrypted off-device backup,
  never in a plaintext archive.
- **Rotation is a real operation with a real cost** — a new key is a new address,
  and every existing link breaks. The panel says that before rotation, not after.
- **There is no revocation.** If the key is compromised, the only remedy is
  rotating to a new address and telling your audience out of band. This is stated
  in §9 as a permanent limit rather than buried.

## §7. Ingress is not the whole story: egress

Both this ADR and ADR-0004 originally modelled **inbound only**, which the
completeness critic correctly called out. A cell also makes outbound
connections — update checks, backup uploads, relay dial-out, Tor bootstrap, mail
delivery — and those are where a supposedly private node leaks.

Egress is therefore a declared property too. Under a Tor-only posture, clearnet
egress is refused by a kill-switch, and the draft's sentence "clearnet egress is
refused entirely" was flagged as *meaningless or fatal* — because a Tor-only cell
must still reach the Tor network. The corrected statement:

> **Tor-only posture refuses clearnet egress to arbitrary destinations. It
> permits exactly the connections the Tor client itself needs to bootstrap and
> maintain circuits, and nothing else. Every other subsystem's egress is
> refused, and the refusal is logged per subsystem — because a layer that
> silently makes no requests looks identical to one that is working.**

## §8. The platform is not trusted

Also from the completeness pass, and worth stating once here for both ADRs:

- **Doze, App Standby and vendor task-killers** actively fight long-running work
  on stock Android, and some vendors reassert their killers after every OS
  update. This is a first-class ingress reliability problem at T0 and T1, not an
  edge case. T2 and T3 escape it; the tier table in ADR-0001 already reflects
  that, and the panel must set the expectation honestly at T0.
- **The device may be seized, borrowed or repaired.** There is no adversary with
  physical possession in either threat model, and there should be: an unlocked
  phone in someone's hand is a total compromise of the identity key, the backup
  passphrase if cached, and everything else. Full-disk encryption and a
  boot-time secret are the mitigations; both are out of scope for this ADR and
  belong in a future one, named here so the gap is on the record.

## §9. What sovereign ingress will never claim

Each becomes a permanent failing row in the posture report:

1. **Not** that an onion service has no dependency. It depends on a public
   commons rather than a supplier, which is better and is not nothing.
2. **Not** that an onion address reaches ordinary visitors. It does not.
3. **Not** that a relayed cell is independent. A rented relay is a supplier who
   can evict you, and one that terminates TLS can read your traffic.
4. **Not** that a compromised onion identity key can be revoked. It cannot.
5. **Not** that ingress survives an unattended stock-Android device
   indefinitely. Below T2 the platform is actively hostile to it.
6. **Not** that any of this protects a device someone else is holding.

## §10. Open decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | Default mode | **Local-only.** Publishing is an explicit choice, per Article VIII.5 |
| 2 | Onion denial-of-service defences | Enable the introduction-point rate limit by default; **verify the proof-of-work capability is actually compiled in** before reporting it — a build lacking it silently has no such defence, and reporting a defence that is absent is the §8-class lie |
| 3 | Whether to ship a first-party relay | **No.** Article V.5 — a service this project controls would become a dependency it created |
| 4 | Relay TLS | **Pass-through by default.** Terminating at the relay hands the operator's traffic to their supplier |
| 5 | Thermal class enforcement | **Registry-enforced**, per §5. A mode without one does not compile |
