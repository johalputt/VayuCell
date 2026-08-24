# VCIP-0001: Onion ingress via the system's tor daemon

- **Status:** Accepted
- **Date:** 2026-08-24
- **Relates to:** ADR-0003 (sovereign ingress), ADR-0005 (implementation
  language), ADR-0002 (battery governor), Charter Articles III, IV, V and VIII;
  PLAN.md phase P3

## The problem

PLAN.md §11 names the largest gap between what this project is and what its
headline says: **an onion service is described, governed and disclosed in code,
but not implemented**, so nothing here is reachable from outside your own
network. P3's gate — reachable from outside with no port forward — cannot be
met until something publishes.

Implementing it runs into three standing constraints that do not bend:

1. **The binary never dials.** Charter Article V.2 is enforced by a gate on
   production source: nothing in it may open an outbound connection. The claim
   printed to operators — *it binds, and it never connects* — is load-bearing.
2. **Zero third-party runtime dependencies** (ADR-0005 §5.1). There is no arti
   crate to adopt without an ADR admitting a dependency class this project was
   founded to refuse, and no crypto crate to generate or verify onion keys
   with.
3. **Honest verification.** ADR-0003 §4 defines *verified* as a request from
   outside traversing the path and being served, observed by the cell. Nothing
   short of that may read as "working".

## The proposed change

VayuCell supervises **the system's own `tor` daemon** as one more surface of
`vayucell all`:

- The operator passes `--onion-dir <DIR>`; only `all` accepts it, because only
  `all` runs the governor whose authority over high-thermal ingress is the
  whole point (ADR-0003 §5).
- VayuCell writes the daemon's configuration into that directory — generated,
  deterministic, byte-pinned by tests — and starts `tor` as a child process,
  logging beside it.
- It reads the `.onion` address from the file the daemon publishes, validating
  shape (56 base32 characters, alphabet, trailing version character) before
  showing it to anybody. The embedded checksum is deliberately not verified;
  doing so requires crypto code constraint 2 forbids, so validation claims
  shape and says so.
- The identity key never leaves the daemon's own store. This process neither
  generates, reads, copies nor prints key material — the strongest custody
  available to a program with no crypto, and the arrangement ADR-0003 §6
  describes. Custody consequences (rotation breaks every link; backups are
  encrypted or worthless; theft has no revocation) print once, at startup,
  before anything publishes.
- `SocksPort 0`: the daemon publishes one cell and proxies nothing, so no
  other program on the device becomes a Tor user by accident.
- The introduction-point rate limit is requested by default (ADR-0003 §10 open
  decision 2). The proof-of-work defence is **never requested and never
  claimed**: whether a given daemon build compiled it in cannot be read back
  from here, and Article IV.3 forbids reporting what was not checked.
- Supervision: crash → restart with doubling delay capped at sixteen seconds;
  governor DERATED → shed first, before serving or storage; PROTECT/HALT →
  stopped; every halt/outage exit path stops the child **before**
  `std::process::exit`, because a publisher outliving its governor is the one
  orphan this mode must never leave behind.
- The panel is excluded from publication on purpose: it reports whether the
  battery in somebody's home is safe, which is not what this mode exists to
  hand the world.

What is *not* changed: reachability semantics. Until an outside request has
been observed traversing the path, every surface says **unverified**, in those
words.

## What it costs operationally

- **One external program dependency, itemised per Charter VIII.4:** a `tor`
  binary on `PATH`, operated by whoever maintains the device. Its absence
  degrades loudly to local-only rather than pretending otherwise; its failures
  are restarts with backoff, narrated as they happen, with its log kept beside
  the key directory.
- **Sustained CPU while published**, which is heat, which is battery ageing —
  already declared as `ThermalClass::High` in ADR-0003 §2 and now enforced in
  practice: the mode sheds first.
- **Restart churn after crashes** re-establishes introduction points each
  time; the capped delay bounds the cost without ever giving up.
- **Test and gate weight:** eight new mutations pin the guards (proxy refusal,
  rate-limit default, three hostname rules, absent/unreadable distinction,
  shed delegation, backoff cap).

## What it forecloses

- **Embedding a Tor client** (arti or any library) for as long as ADR-0005
  §5.1 stands — adopting one would need its own ADR and would end the
  zero-dependency headline this project is named by.
- **Speaking the daemon's control port**: it would require dialling
  (`TcpStream::connect`) from production source, breaking Charter V.2's
  enforced claim. Configuration flows through files; state flows through
  files the daemon writes.
- **Any default other than local-only.** Publishing stays an explicit flag,
  per ADR-0003 §3 and Charter VIII.5.
- **First-party relays** remain refused under ADR-0003 §10.3; this proposal
  does not touch them, and relay ingress remains unbuilt.

## Safety analysis (Charter Article III)

- **Thermal:** onion ingress is the highest-thermal ingress mode declared, and
  the first shed at DERATED. The delegation is structural — `should_run` calls
  `shed_for` — so a future edit cannot leave the onion burning circuits on a
  device the governor has already derated; a mutation pins exactly this.
- **Unattended failure states:** a dead daemon degrades to local-only with the
  reason on screen and in the log; it never takes the panel down with it, so
  the person who most needs to read the cell keeps being able to.
- **No new physical risk:** the mode adds no writes to sysfs, no charging
  behaviour changes, and no interaction with the halt record beyond obeying
  it (`all` already refuses to start halted).
- **Disclosure timing:** the seven property disclosures for `Mode::Onion`
  print before anything binds or publishes, satisfying §5.4's "before the
  choice" for a choice made on the command line.
