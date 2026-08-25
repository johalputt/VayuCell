# VayuCell — founding plan

**Status:** Proposed · **Date:** 2026-08-06 · **Licence:** charter CC0, code Apache-2.0 (§10)

---

## The claim, worded to be defensible

> **VayuCell turns a retired phone into a server whose capabilities are enumerated
> rather than assumed, whose battery risk is actively governed rather than
> ignored, and which never reports a capability it has not verified on the
> device actually in front of it.**

Read what that does *not* say. It does not say every phone can be a server —
most cannot reach the higher tiers, and §3 says which. It does not say a phone
matches a datacentre. It does not say the battery is safe; it says the risk is
**measured, bounded and reported**, which is a smaller claim and an achievable
one.

This follows the house rule established by `anonaudit`, `shieldaudit` and
`veilaudit`: a report that goes green on configuration rather than verification
is worse than no report, because it spends trust it did not earn.

---

## §0. The check that goes first

ADR-0150 in the VayuPress repository records an expensive authoring failure: a
subsystem was designed from the *threat* rather than from the *product*, and
promised six phases a headless Go binary could never execute. The correction
produced a one-sentence rule, and it goes at the top of this document because
VayuCell is far more exposed to it:

> **What can this actually execute, on the device in front of us?**

A phone-server project that ignores this ships a roadmap of things that work on
one Pixel and on nothing else. So every capability in this plan is bound to a
**tier** (§3), every tier declares what it cannot do, and the panel reports the
tier it actually detected — never the tier the user hoped for.

---

## §1. Why a retired phone is genuinely a good server

This is not a novelty. Measured against a single-board computer at the same
price point, a five-year-old flagship wins on nearly every axis:

| | Retired flagship phone | Typical SBC |
| --- | --- | --- |
| CPU | 8-core ARM64, big.LITTLE | 4-core ARM64 |
| RAM | 4–12 GB LPDDR | 2–8 GB |
| Storage | 64–512 GB UFS (fast, onboard) | microSD (slow, fails) |
| Power draw | **1–3 W idle** | 3–7 W idle |
| Networking | Wi-Fi 5/6 **+ LTE failover** | Ethernet / Wi-Fi |
| Integrated UPS | **Yes — the battery** | No |
| Screen + input | Yes, for local status | No |
| Sensors, camera, GPS | Yes | No |
| Marginal cost | **Zero — you own it** | $35–120 |

Three of those rows are decisive. **The battery is an uninterruptible power
supply that ships in the box** — a phone server rides through a power cut and
shuts down cleanly, which a bare SBC cannot. **The modem is a second network
path**, so a cell can fail over to LTE when the household link dies. And
**the marginal cost is zero**, which changes who can participate: a person with
a drawer full of old handsets can run real infrastructure without buying
anything.

The scale argument is the one that matters most. Billions of smartphones have
been retired, the overwhelming majority still functional. Every one converted is
one not entering the waste stream, and one household less dependent on rented
infrastructure. **That is the project's reason to exist**, and it is a better
reason than novelty.

---

## §2. The blockers, named before the features

A plan that lists capabilities before constraints is a marketing document. These
are the eight things that actually decide whether a given phone can serve.

| # | Blocker | Severity | Addressed by |
| --- | --- | --- | --- |
| **B1** | **Battery degradation and swelling** — a lithium cell held at 100% and warm, for years, degrades and can swell. Swelling is treated as damage and a fire hazard by waste authorities | **Critical — safety of persons** | §4, the Battery Safety Governor |
| **B2** | **No reachable address** — carrier CGNAT and most home ISPs give no inbound path | Blocks all serving | §5, Sovereign Ingress |
| **B3** | **Doze and background limits** — Android aggressively kills long-running work | Blocks reliability | §3 tiers; VM/native tiers escape it |
| **B4** | **Vendor kernel end-of-life** — old devices run kernels with years of unpatched CVEs | **Critical — security** | §3; mainline tier is the only real fix |
| **B5** | **Storage endurance and honesty** — onboard flash wears, and cheap controllers lie about `fsync` | Silent data loss | §6 |
| **B6** | **Thermal throttling** — phones are built for bursts, not sustained load | Performance ceiling | §4 governor, §7 fleet |
| **B7** | **No root on stock firmware** | Caps capability at Tier 0 | §3 |
| **B8** | **Locked bootloader** — some carrier devices can never be unlocked | Permanently caps the device | §3, reported honestly |

**B1 is the one that makes this project different from every weekend tutorial.**
Everything else is an engineering inconvenience; a swollen cell in a cupboard is
a fire in someone's home. A project that invites millions of people to leave old
phones plugged in forever, and does not govern that risk, is not enterprise-grade
— it is negligent. So the battery governor is not a feature. It is the
precondition.

---

## §3. The tier model — the core architecture

Devices differ enormously in what they permit. VayuCell therefore refuses to
present a single feature list. It **detects a tier**, declares that tier's
ceiling, and reports every capability against it.

| Tier | Environment | Root | Reachable? | Charge control | Kernel security |
| --- | --- | --- | --- | --- | --- |
| **T0** | Stock Android, userspace runtime | No | Tunnel/onion only, ports ≥1024 | **No — permanently red** | Vendor, often EOL |
| **T1** | Stock Android, rooted | Yes | Any port, wakelocks held | Yes — kernel sysfs node | Vendor, often EOL |
| **T2** | **Virtualised Linux** (pKVM VM on Android 16+) | In-guest | Host-mediated | Host-mediated | **Android-maintained host** |
| **T3** | **Mainline Linux** (postmarketOS-class) | Yes | Full | Yes — device tree / sysfs | **Mainline, maintained** |

**Tier 2 is the strategic bet.** Android now ships a real Linux virtual machine
— a Debian guest under protected KVM, with its own kernel and memory space,
mutually distrusted from the Android host. That is a genuine server environment
on an *unrooted, still-updated* phone, which was impossible until recently. It
escapes Doze, it gets a maintained host kernel, and it needs no bootloader
unlock.

Its limit must be stated as loudly: **AVF is not universal.** It is present on
Pixel-class and some Android One devices and absent on much of the market,
including major vendors. VayuCell detects it and says so; it never assumes it.

**Tier 3 is the endgame for security.** A mainline kernel is the only real answer
to B4 — a device whose vendor stopped shipping patches in 2021 is not made safe
by anything in userspace. Tier 3 also unlocks the cleanest battery control,
because the charge ceiling can be set in the device tree.

**The registry, and why it is a type.** Following `vayushield/rule.go`,
`vayuveil` and `vayuflow`, every capability is a registered contract whose zero
values are invalid:

```go
// A capability that does not answer every obligation does not compile
// past the test. Nothing lands undeclared.
type Capability struct {
    ID        CapID        // charge-ceiling, port-below-1024, wakelock, fsync-honest, …
    Tier      TierFloor    // the LOWEST tier that can provide it — tierUnset invalid
    Detect    DetectFn     // how presence is VERIFIED on this device
    Verify    VerifyFn     // read the result BACK; nil is invalid
    OnAbsent  Disposition   // degrade | refuse — dispositionUnset invalid
    Rationale string        // why, in prose, shown in the panel
}
```

Two properties, enforced by test rather than review. **A capability with no
`Verify` cannot be registered** — a control that cannot be read back is
indistinguishable from one that silently stopped working, which is the lesson
`veilaudit` paid for. And **a device probe that finds an unregistered capability
fails the build**, so the surface cannot grow silently.

---

## §4. The Battery Safety Governor — the flagship subsystem

The single most important thing VayuCell contains, and the part no existing
project does properly.

**The physics.** A lithium cell ages fastest when held at high state of charge
and elevated temperature — exactly the condition of a phone plugged in
permanently and running a workload. Sustained 100% charge plus heat produces
swelling, and swelling is a fire risk in someone's home.

**The fix, per tier.** The mechanism differs and the honesty must too:

| Tier | Mechanism | Ceiling |
| --- | --- | --- |
| T0 | **None available.** Reported permanently red, with the mitigation in plain language | — |
| T1 | Kernel charge-control sysfs node (device-specific) | 40–60% |
| T2 | Requested from the Android host; verified from the guest | 40–60% |
| T3 | Device-tree maximum charge voltage (≈3.8 V) | ≈40–50% |

That 3.8 V figure is not invented — it is the technique the postmarketOS
community demonstrated for exactly this use case, lowering the device tree's
maximum from 4.4 V so a permanently-powered handset sits mid-charge instead of
full.

**Four governor duties, each continuously verified:**

1. **Hold the ceiling.** Set it, then read it back every cycle. A ceiling that
   was set once and silently reverted is the failure mode this exists to catch.
2. **Watch the thermals.** Sustained high temperature is the accelerant. The
   governor throttles workload before it throttles silicon, and hard-stops at a
   declared limit.
3. **Estimate the risk.** Swelling cannot be measured directly, so it is
   *inferred* — cycle count, age, internal-resistance drift, temperature history
   and voltage-curve anomalies — and reported as an estimate that says it is an
   estimate. It then **prompts a physical inspection**, because the definitive
   check is a human looking at whether the phone still lies flat on a table.
4. **Support batteryless operation.** Where a device boots on USB power with the
   cell removed, that is the safest configuration and VayuCell should say so and
   detect it. Where the device refuses to boot without a battery — many do — it
   must say that instead of implying a choice the hardware does not offer.

**And then the inversion: the battery is also the best feature.** Once governed,
a phone server has a built-in UPS. On mains loss, the cell keeps the node alive,
VayuCell degrades gracefully — shed non-essential services, checkpoint state,
notify the fleet — and shuts down cleanly with charge to spare. A $35 SBC cannot
do that at any price.

---

## §5. Sovereign Ingress — reaching a server with no address

B2 defeats most home-server attempts. VayuCell treats reachability as a
first-class subsystem with four modes, ranked by how much they depend on someone
else:

| Mode | Dependency | Best for |
| --- | --- | --- |
| **Onion service** | **None** — no address, no port-forward, no relay | Censorship resistance, zero-infrastructure publishing |
| **Relay tunnel** | A cheap VPS you rent | Clearnet domains, normal visitors |
| **Direct + port forward** | A real public IP | The rare good ISP |
| **Local only** | Nothing | Personal cloud on your own network |

The onion path is the interesting one and the natural fit: it needs no public
address at all, which is precisely the constraint a phone on a mobile network
has. It pairs directly with VayuTor in the VayuPress stack, so a cell can serve
a real site from a drawer with no port forwarding, no static IP and no rented
relay.

The relay path must be honest about what it is: **a rented VPS is a dependency**,
and a project about sovereignty should say so rather than hiding it behind the
word "tunnel". The panel names the relay, and the posture report counts it as an
external dependency, not as infrastructure you own.

---

## §6. Storage: endurance, honesty, and getting data off the device

Two failure modes, both quiet.

**Flash wears out.** Phone storage is not rated for a database's write pattern.
VayuCell therefore batches and aligns writes, keeps write-amplifying workloads
off the internal cell where an external disk is attached, and **reports observed
wear indicators** rather than assuming health.

**Controllers lie about `fsync`.** A cheap flash controller can acknowledge a
flush it has not performed, which turns a power cut into a corrupted database.
The draft of this plan proposed *testing* that on-device. **It cannot be done:**
a sealed-battery phone cannot drop its own storage rail, and the ordinary reboot
paths flush the cache on the way out, so an honest device and a lying one produce
identical results (ADR-0004 §0). VayuCell therefore **assumes the flash may lie**
and is designed so that assumption costs nothing to be right about: the durability
guarantee is stated as a **live replication lag** rather than an adjective, and the
governed battery converts most power cuts into graceful shutdowns, which is the
real durability mechanism on a phone.

**And the rule that follows from both:** a phone is a *replica*, never the only
copy. VayuKeep in the VayuPress stack already provides encrypted, continuously
self-verifying backup that refuses a target inside its own data directory. A cell
ships pointed at off-device backup **by default**, because the person running
this has a drawer phone, not a storage array.

---

## §7. The Cell Fleet — how it gets big

One phone is one phone: a single point of failure, thermally limited, on one
household link. The answer to scale is not a bigger phone. It is **more of
them**, which is exactly the resource this project assumes you have.

A fleet assigns roles rather than cloning nodes:

- **Edge** — terminates ingress, runs the shield, holds the tunnel or onion
- **Store** — attached external storage, holds replicas, serves the personal cloud
- **Compute** — batch work, media transcoding, indexing, model inference
- **Witness** — a tiny node that exists only to break quorum ties

Three properties make the fleet worth having. **Redundancy**: content is replicated
across nodes so one dying phone is an inconvenience rather than an incident.
**Rolling upgrades**: nodes update one at a time, and a node that fails to come
back is automatically drained. And **shared defence**: following ADR-0148's
verdict-sharing design, an attacker jailed on one cell is jailed across the
fleet, sealed under a derived key — so a swarm gets one run at the estate, not
one per phone.

The honest ceiling, stated where it is configured: **N phones multiply capacity
and availability linearly.** That is real and worth having. It is not a
datacentre, not anycast, and no defence against an adversary with more bandwidth
than your household link.

---

## §8. What a cell actually runs

One-click means a catalogue, not a shell prompt.

- **The full VayuPress stack** — website, blog, private mail with PGP, encrypted
  chat, cookieless analytics, scoped API, the bot shield, the onion, the AI
  connectors. One binary, which is exactly the right shape for a phone.
- **Personal cloud storage** — file sync, photo backup off the phones in your
  pocket, versioned and encrypted. The single most-wanted self-hosted service,
  and the natural companion to storage-role nodes.
- **Backup target** — a cell in a relative's house is off-site backup that costs
  nothing per month.
- **Media and household services** — library serving, ad-blocking DNS, home
  automation hub.
- **Private model inference** — modern phone silicon includes neural
  accelerators, which makes local, private AI assistance genuinely viable on
  hardware you already own.

Every catalogue entry declares its **tier floor** and its **resource envelope**,
so the panel can grey out what this device cannot run and say why — rather than
letting a user install something that will thrash a four-year-old handset.

---

## §9. What "one click" has to mean

The bar is a person with no terminal experience and a phone in a drawer.

1. **Install the app** and plug the phone in.
2. **It profiles the device** — SoC, RAM, storage class, root, AVF, bootloader,
   charge-control node, battery health, network path — and names the tier it
   found, with the reason.
3. **It states the safety position** before anything else: whether it can hold a
   charge ceiling on this device, and if not, what the user should do instead.
4. **Pick what to run** from the catalogue, filtered to the tier.
5. **Pick how to be reached** — onion, relay, or local only.
6. **It provisions, then verifies**, and shows a posture report that is green
   only where it has read the result back.

Everything under that is reproducible and signed: images build deterministically,
releases are signed, and the installer verifies before it writes. Rollback is a
first-class operation, because the recovery story on a phone with no console must
be *hold the button*, not *find a serial cable*.

---

## §10. Licence and governance — future-proofed, in the VayuWeb sense

VayuWeb separates a **CC0 charter** from **permissively licensed code**, and that
separation is right. VayuCell adopts it with one deliberate upgrade.

| Artefact | Licence | Why |
| --- | --- | --- |
| **Charter & specifications** | **CC0-1.0** | The rules of the commons belong to nobody. Anyone may implement, fork or standardise them |
| **Code** | **Apache-2.0** | Permissive *plus an express patent grant* |
| **Hardware compatibility database** | **CC0-1.0** | The device facts are a public good |
| **Documentation** | **CC-BY-4.0** | Free to reuse, attribution kept |
| **Name and logo** | Trademark policy, separate | Protects users from hostile forks claiming to be official |

**Why Apache-2.0 rather than MIT, and this is the future-proofing decision.**
MIT is admirably short but grants no patent rights. VayuCell touches charging
circuits, power management, virtualisation and radio behaviour — the most
patent-dense area in consumer electronics. Apache-2.0 gives every user an express
patent licence from every contributor, and terminates that licence for anyone who
initiates patent litigation over the work. For a project inviting people to
repurpose hardware built by large patent holders, that is not bureaucratic
caution; it is the clause that keeps the project usable in ten years.

**Four more provisions that make capture hard:**

- **DCO, never a CLA.** Contributors sign off; they do not assign copyright. With
  no single entity holding the rights, **no one can unilaterally relicense the
  project** — the standard mechanism by which open projects are taken private.
- **Spec-first.** The charter and wire formats are CC0 and complete enough to
  build an independent implementation. If this codebase were ever mismanaged,
  the specification survives it.
- **Reproducible builds and signed releases**, so a user can verify the binary
  matches the source rather than trusting a download.
- **VCIP** — a public improvement-proposal process, in the shape of VayuWeb's
  VWIP: numbered, archived, discussed in the open. Changes to the charter require
  a proposal; changes to code require review.

**No token, no treasury, no fee.** Stated in the charter as a permanent
constraint, matching VayuWeb. A project whose purpose is reducing dependence must
not create a new one.

---

## §11. Roadmap

Each phase is independently useful and independently shippable. Nothing claims a
capability before it can verify it.

**Status is written per phase, and it is written honestly.** ✅ means the code
exists *and* its gate is met. ◐ means code exists and the gate is not met — which
is usually because the gate needs a handset, not because the code is unfinished.
⬜ means not started. A phase does not get to be ✅ because the interesting part
of it compiles.

| Phase | Content | What it unlocks | Gate that proves it | Status |
| --- | --- | --- | --- | --- |
| **P0** | Charter, licences, capability registry, device profiler, hardware DB | Nothing yet — but nothing can land undeclared | An unregistered capability fails the build | ✅ **met.** The registry, the tier probes and the charter gate are written and enforced in CI. The hardware database has **no entries**: it is a schema and a gate, waiting for reports from real handsets |
| **P1** | **Battery Safety Governor** + posture report | Safe unattended operation | Ceiling is set, read back, and reverting is detected | ◐ **written, unproven on hardware.** The state machine, the read-back, the reversion detection, the shed ladder, the halt record and the panel all exist and are mutation-tested. Every one of them has been exercised against a fake host and **has governed zero real cells** |
| **P2** | T0/T1 runtime, service supervisor, one-click installer | A phone that serves something | Survives 30 days unattended with Doze active | ◐ **needs a device.** The supervisor loop is in `core/src/runtime.rs`, the binary runs it, and thirty *simulated* days is a test. A service supervisor and Doze survival cannot be simulated |
| **P3** | **Sovereign Ingress** — onion, relay, local | A reachable server with no public IP | Reachable from outside with no port forward | ◐ **onion written, relay not.** The modes, their seven declared properties and the governor's authority over them are in `core/src/ingress.rs`, local-only is served, and `all --onion-dir` now supervises your system's tor daemon to publish the site and the vault — contract in `core/src/onion.rs`, supervision in `cli/src/onion.rs`, decision in [VCIP-0001](docs/vcip/VCIP-0001-onion-ingress-via-system-tor.md). The key stays in the daemon's own directory, the daemon is shed first at DERATED and stopped before every halt exit, and reachability reads **unverified** until a request from outside has been observed — which has not happened, because no handset has run this. **Relay is not implemented**, and the gate needs hardware plus an outside vantage point |
| **P4** | Catalogue: site + personal cloud | The two headline uses | A real site and real file sync, served from a phone | ◐ **code complete, unproven on hardware.** `vayucell site` serves a directory under the governor's authority (ADR-0008) and `vayucell vault` accepts authenticated files (ADR-0009), now with an authenticated listing so a client can decide what differs. The companion `vayucell-sync` keeps one folder of yours in step — plan/push, prune only on the flag, dials only while it runs (ADR-0011). What remains is the gate: a handset serving both halves, and sync run against it from another machine |
| **P5** | **T2 virtualised tier** (pKVM guest) | Server-grade on unrooted, updated phones | Guest survives host reboot; escapes Doze | ⬜ not started |
| **P6** | Replication lag as the stated guarantee, verified-restore reporting, wear observation | Data you can trust | Graceful-shutdown ladder verified; an unrestored backup reads *unverified* | ◐ **both code halves done, device gate open.** The honesty machinery (`core/src/durability.rs`: a lag that goes stale, a drill that expires, no variant meaning *durable*) now has both a renderer (`cli/src/storage.rs`, wear from `core/src/wear.rs`) and a subject: `vayucell-sync replicate` mirrors the vault and `drill` re-downloads every file to compare byte for byte, each leaving a dated receipt (ADR-0012). The cell quotes that receipt through `--replica-evidence` — worded as the replica's claim, aged against this clock, never measured here. **What remains is exactly what code cannot do**: somebody runs this against a real handset and lets the receipts age in the world |
| **P7** | **Fleet** — roles, replication, rolling upgrade, shared verdicts | Redundancy and scale | One node killed mid-write loses nothing | ⬜ not started |
| **P8** | **T3 mainline tier** (postmarketOS-class images) | A maintained kernel — the real fix for B4 | Verified images for the top device families | ⬜ not started |
| **P9** | Local model inference, household services | Private AI on hardware you own | Runs within the declared thermal envelope | ⬜ not started |

**Two phases have their code, three are part-built, five are untouched — and
that undercounts what is left.** Everything written so far is the layer
*underneath* the product: twelve core modules and twenty gates, serving a directory,
storing files on your own network, keeping a folder of yours in step with that
store when you tell it to, and — as of ADR-0012 — holding a replica whose
restore has been compared byte for byte, on evidence the phone can quote but
never claims to have measured. The three things that would make this the thing
the README describes were **an onion service** (P3), **file sync** (P4), and
**a replicator with verified restore** (P6). The first two have their code, and
the third's does too — companion-side `replicate`/`drill` leaving dated
receipts, cell-side quoting that refuses to improve them — none of it yet
exercised by an outside visitor or a real handset. What remains unbuilt is
**the relay half of P3**.

**Four gates cannot be closed by writing code.** P2, P3, P4 and P6 all end in a
sentence about a device: thirty days unattended, reachable from outside, served
from a phone, a ladder verified on hardware. No amount of work in this repository
closes them. Somebody has to put a retired phone on a bench.

**P1 precedes everything that serves traffic, deliberately.** Shipping a
convenient server before the safety governor would put hardware in a risky state
in people's homes to hit a demo. That ordering is not negotiable.

---

## §12. What VayuCell will never claim

Written down so no future copy can quietly upgrade it. Each appears in the
posture report as a permanent red row that no configuration clears.

1. **Not** that every phone can be a server. Locked bootloaders, absent
   virtualisation and dead charge control are real, permanent ceilings on real
   devices.
2. **Not** that the battery is safe. Risk is governed, bounded and reported —
   never eliminated. Physical inspection remains the definitive check, and the
   product will keep saying so.
3. **Not** that a vendor kernel is secure. Below Tier 3, the device runs code its
   manufacturer abandoned, and no userspace feature changes that.
4. **Not** datacentre reliability. One phone is one phone; a fleet raises the
   ceiling without removing it.
5. **Not** independence while using a rented relay. That is a dependency, and the
   report counts it as one.
6. **Not** protection against an attacker with physical possession of the device.

A report in which everything eventually turns green teaches its reader to stop
reading it. The permanent red rows are what make the green ones worth believing.

---

## §13. Open decisions

| # | Decision | Recommendation |
| --- | --- | --- |
| 1 | Primary tier to target first | **T0/T1 for reach, T2 as the flagship.** T2 is where this stops being a hobby |
| 2 | Ship our own mainline images, or defer to an existing distribution | **Defer and contribute upstream.** Maintaining device trees is a decade-long commitment |
| 3 | Default ingress | **Onion.** It is the only mode with no external dependency |
| 4 | Guest OS for T2 | Whatever the platform ships by default, plus a declarative option for reproducibility |
| 5 | Fleet membership trust | Derived key from a shared secret, following ADR-0148 — never the raw secret |
| 6 | Batteryless as default recommendation | **No.** Recommend it where the device supports it, but the UPS is a genuine benefit worth governing for |

---

## §14. Prior art, credited

VayuCell should stand on existing work rather than pretend to originate it: the
Android userspace runtimes that made phone servers possible at all, the mainline
porting community that keeps abandoned hardware alive, the platform
virtualisation work that made a real Linux VM possible on a stock phone, and the
sync, tunnel and backup projects that will fill the catalogue. **The novel
contributions here are narrow and specific**: the tiered capability registry, the
Battery Safety Governor, the verified posture report, and the fleet model. Those
are worth building. Everything else is worth adopting, and the charter should say
so.
