# ADR-0005 — Implementation language: Rust for the core, Kotlin for the shell

- **Status:** Accepted
- **Date:** 2026-08-06
- **Supersedes:** the Go implementation committed in `77af40e`, now discarded
- **Relates to:** ADR-0001 (capability registry), ADR-0002 (battery governor),
  CHARTER Articles III and VI

## §0. What is being discarded, and why it is recorded here

The first code in this repository was Go: `internal/capability`, roughly 450
lines with ten tests, committed about an hour before this ADR. **It is being
deleted.** The reasoning is recorded in the active voice rather than quietly
rewritten, per the practice ADR-0150 established in the sibling project.

Go was chosen by default rather than by decision — the neighbouring projects are
Go, so the first file was Go. That is a reason to *consider* a language, not a
reason to *choose* one, and the check that was skipped is the same one ADR-0150
had to learn: **what does this specific product actually need?**

Discarding 450 lines is cheap. Discarding 10,000 is not. This is the correct
moment to answer the question properly.

## §1. The question is narrower than it looks

VayuCell is not one program, and noticing that removes most of the argument.

| Part | Constraint | Language |
| --- | --- | --- |
| **Android shell** (T0/T1) | Must call platform APIs — foreground services, battery manager, boot receivers, notifications, installer UX | **Kotlin.** There is no realistic alternative |
| **Core engine** | Reads sysfs and procfs, runs the governor loop, serves the local panel, holds the capability registry | **The actual decision** |
| **T2 / T3** | A Linux guest or a mainline port — anything runs here | Follows the core |

Because the shell must be Kotlin regardless, **this project is multi-language no
matter what is chosen.** That substantially weakens the strongest argument for
Go, which was consistency with the sibling projects: there was never going to be
a single-language repository here.

## §2. Why Rust, and the argument that decided it

Three reasons, in order of weight. Only the first is decisive.

### 2.1 The registry's central pattern becomes a compiler guarantee

ADR-0001's design is *obligations expressed as types whose zero values are
invalid*. In Go that pattern needs a runtime `Complete()` method plus a test to
enforce it, because Go gives every struct field a zero value whether the author
decided one or not. The guarantee is real but it is **checked**.

In Rust the same design is **unrepresentable when violated**:

```rust
pub struct Capability {
    pub verify: VerifyFn,        // not Option — a missing Verify does not compile
    pub apply:  Option<ApplyFn>, // Option ONLY because observe-class may omit it
    pub floor:  Tier,            // no Unset variant exists at all
    // ...
}
```

Two of the six guards written in Go stop being guards and become facts:

- **`Verify` cannot be nil**, because the field is not an `Option`. The single
  most important rule in the project — *a control that cannot be read back may
  not be reported* — is enforced by the compiler instead of by a test that
  someone must remember to keep.
- **`TierUnset` does not exist.** An undetected tier is `Option<Tier>::None`,
  which cannot be compared against a floor at all. The Go version needed
  `AtLeast` to defend against its own zero value; here there is nothing to
  defend against.

A project whose whole thesis is *make the wrong thing unrepresentable rather
than merely checked* should use the language that lets it do exactly that. This
is not a preference for Rust. It is the observation that ADR-0001 was describing
Rust's type system and implementing a Go approximation of it.

### 2.2 The idle commitment is easier to keep

`PLAN.md` commits to *"idle install: no timers doing work"*, and ADR-0002 exists
because sustained work is heat and heat ages the battery this project asks people
to leave plugged in for years.

A garbage-collected runtime does periodic background work — scavenging, sweeping,
timer wheels — whether or not the program has anything to do. On a VPS this is
invisible. On a device where the safety argument is *minimise sustained work*, a
runtime that is never truly idle makes a commitment harder to prove. Rust has no
runtime to be busy.

This is a modest effect, not a dramatic one, and it is listed second for that
reason. But it points the same way as §2.1 rather than against it.

### 2.3 It is the better long-horizon bet for this layer

The charter promises durability measured in years. For systems software that
reads device nodes and runs supervised control loops, Rust is now in the Linux
kernel, is the default choice for new low-level projects, and has the strongest
trajectory of any candidate. T3 — mainline Linux — is territory where it is
first-class.

## §3. What was rejected, and honestly

**Go** would have worked. It is not a bad answer, and three real advantages are
being given up: faster compiles across four tier-specific build matrices, a lower
barrier for the many small contributions a hardware-compatibility project needs,
and existing proof inside this organisation that pure Go runs on Android. The
last of those is genuine — a sibling project ships a pure-Go Android application.

The honest summary is that **this is a judgment call, not a rout.** §2.1 is what
tips it, and without §2.1 the decision could reasonably have gone the other way.

**Zig** is rejected on a single ground: it is pre-1.0 and the language still
changes. For a charter that promises the specification and the code will still be
usable in a decade, adopting a language that may break its own syntax is the
wrong risk. Its cross-compilation story is the best of any candidate and that is
not enough.

**C** is rejected. A safety-critical control loop handling attacker-influenceable
input, written in a language without memory safety, is not defensible under
Article III.

**Kotlin for everything** is rejected for the core: a JVM on a memory-constrained
handset, for a process meant to idle at near-zero cost, is the wrong shape. It
remains correct for the shell.

## §4. Consequences

**Good.** The project's central invariant is compiler-enforced rather than
test-enforced. There is no runtime and no garbage collector on the path that
matters. Dependencies stay at zero for the core — the standard library covers
sysfs reads, and the charter's no-new-dependency instinct is easier to hold.

**Costly.** The Go implementation is deleted. Compile times are worse. The
contributor pool for the core is smaller than Go's, which matters for a project
that wants many small hardware contributions — mitigated by keeping the
**hardware database as plain CC0 JSON**, so the highest-volume contribution
requires no code at all.

**Accepted.** Two languages, two toolchains, one foreign-function boundary
between the Kotlin shell and the Rust core. That boundary is real work and is
named here rather than discovered later.

## §5. Standing rules that follow

1. **The core has no third-party runtime dependencies** unless an ADR admits one,
   matching the sibling project's instinct and keeping the audit surface small.
2. **`unsafe` requires a comment naming the invariant it upholds**, and a test.
   A safety-critical subsystem written in a safe language does not get to opt out
   of safety in the interesting places without saying why.
3. **The Kotlin shell holds no policy.** It renders what the core reports and
   forwards what the operator chooses. Policy lives in one place, in the language
   where the obligations are types.
4. **Everything ADR-0001 required still applies**, with the two obligations from
   §2.1 now enforced by the compiler rather than by `Complete()`.
