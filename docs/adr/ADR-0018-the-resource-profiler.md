# ADR-0018 — The resource profiler: four figures, and one counted unsafe call

Date: 2026 (see `git log` for the exact day)

## Status

Accepted. Implemented by `cli/src/profile.rs` and `cli/src/statvfs.rs`, with
the policy in `core/src/thermal.rs`. The device gate is still open — see
*Consequences*.

## Context

ADR-0017 built the catalogue and the picker that filters it, and left the
filter's input as two lines inside the picker: the tier, and the memory read
out of `/proc/meminfo`. PLAN §9 §2 asks for a **profiling pass** — one place
where a device's figures are established, and where the safety state is read
alongside them. Two lines in a rendering function is not a pass. It is a
number, produced where a number is needed, by code that nobody would look for
when asking what the device actually is.

Three of the four figures the catalogue needs are cheap. The tier comes from
the tier detection, which already refuses to guess. The memory comes from the
kernel's own accounting. The **thermal class** needs the cell's temperature,
which the sysfs layer already reads. The fourth — the free space on the
filesystem — has no such source, and that is where the whole shape of this
decision was set.

### The free space is not written down anywhere

The kernel does not publish the free space of a filesystem in any file this
project can read. It is available through `statvfs(2)`, and through
`/proc/self/mountstats` — but only for filesystems that implement
`show_stats`. `f2fs`, which is what a great many Android phones put their
data partition on, does not implement it. On the product's real target, the
mountstats route answers *cannot be told* exactly when it matters most.

So the measurement has to be a call into libc. In Rust, that is `unsafe`.

### What the alternatives were

1. **Read the free space from a file.** Rejected above: there is no such file
   on the filesystems this product runs on, and a profiler that answers
   *cannot be told* on a phone with 200 GB free is not a profiler.
2. **Refuse to measure it at all, and let the catalogue say *cannot be told*
   about storage.** Honest, and it makes the storage requirement permanently
   unavailable: every storage-hungry service is greyed out on every device,
   which is the same as not having the requirement.
3. **One named, counted exception.** The call goes in the command, in one
   file, and a gate counts the unsafe blocks in the tree.

The third was chosen, and the *counting* is the part that matters. The
alternative to a counted exception is not "no unsafe" — it is a promise in a
README, which is a claim, and a claim is not a control.

## Decision

1. **The pass lives in one module and is a command.** `vayucell profile`
   prints the figures and says which of them were measured. The picker uses
   the same pass, so the numbers the catalogue is judged against and the
   numbers a person reads are the same numbers, from the same code.

2. **Four figures, each optional.** The tier, the memory, the storage and
   the thermal class. A figure that could not be measured is absent, and it
   is *named* as absent in the report — never printed as a zero, and never
   printed as a class. "Cannot be told" and "an answer that permits
   nothing" are different answers, and the report keeps them apart.

3. **The one unsafe call is in `cli/src/statvfs.rs`, and there is exactly
   one.** The core keeps `#![forbid(unsafe_code)]`; both manifests keep
   `unsafe_code = "deny"`. The call reads `f_bavail` and `f_frsize` only —
   the free space an unprivileged user may actually use, in the unit the
   filesystem counts its blocks in — and the struct is declared with glibc's
   layout, which is the largest of the three that exist, so the C library can
   never be asked to write past the end of the buffer it was handed.

   A CI step counts the `#[allow(unsafe_code)]` attributes in the tree, fails
   if there is a second, and fails if the one is in a file other than
   `cli/src/statvfs.rs`. Constitution rule 6.16 states it.

4. **The free space is measured where the command was run, and the path is
   named.** A phone has several filesystems and the figure is about a mount.
   A report that says "41230 MB free" without saying which filesystem it is
   about is a report about no filesystem in particular, so the path is in the
   output.

5. **The thermal class is a ceiling, and it follows the governor.** The class
   is the heaviest work the cell may now do, so the catalogue's comparison
   reads the right way round: a service that needs heavy work is refused on a
   cell that is only good for light work. The thresholds are the governor's
   own — 45 °C, 52 °C, 60 °C — so the project keeps one ladder and not two.

6. **The class follows the worst reading, not the latest.** The governor
   raises its level and never lowers it, and a profiler that reported the
   most recent temperature would hand out a lighter ceiling the moment the
   cell cooled for a minute. The history keeps the worst reading it has seen
   and the number of readings it has taken.

7. **A cell that was never observed has no class at all.** Not the lowest
   class: no class. The difference between "the cell is idle" and "nobody has
   read the cell" is the difference between a measurement and a question.

8. **The class is a policy in the core, and the measurement is in the
   command.** `core/src/thermal.rs` holds the class and the ladder, with no
   I/O in it; the command reads the cell and hands the reading over. The
   core keeps its forbid on unsafe, and the ladder is testable without a
   device.

## Consequences

`vayucell profile` exists, and it answers for a device: the tier, the
memory, the free space on the filesystem the command ran from, and the class
the cell is in — each one saying whether it was measured, and the cell's
temperature standing behind the class. The catalogue's picker now judges
storage and thermal class as well as memory and tier, and it can say *this
device has 41230 MB of free storage* instead of *cannot be told* about
storage forever.

The unsafe code in this repository is one call, in one file, counted by a
gate that fails on a second one and on the first one moving. That is a
narrower claim than "no unsafe code", and it is a claim with a mechanism
behind it.

**What none of this establishes:** no device has ever been profiled. Every
figure this project has produced came from a fake host describing handsets
nobody here is holding; the `statvfs` call has never run against a phone's
data partition, and the class has never been derived from a cell that was
actually warm. The profiler is written; it has not met a device.
