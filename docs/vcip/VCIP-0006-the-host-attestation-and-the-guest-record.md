# VCIP-0006: The host attestation and the guest's own record

- **Status:** Implemented (device gate still open)
- **Date:** 2026 (see `git log` for the exact day)
- **Relates to:** ADR-0015 (the tier-2 guest contract), ADR-0001 (the tier
  model), CHARTER Article IV; PLAN.md phase P5

## The problem

A tier-2 guest runs on an Android handset and owns none of the three
controls that matter to it: the charge ceiling on the real cell, the host's
Doze policy towards the guest, and whether the host publishes the guest's
ingress. The host knows all three and the guest cannot observe any of
them.

Two things therefore have to travel: a statement from the host, and the
guest's own memory of what it was told before — because a host reboot
destroys the guest along with everything the host was doing for it.

## The shape of the answer

### The host's statement

One environment variable, `VAYUCELL_HOST_ATTESTATION`, holding a
semicolon-separated list of `key=value` pairs:

```text
epoch=8;ceiling=60;doze=exempt;ingress=published
```

| key | values | meaning |
| --- | --- | --- |
| `epoch` | a boot counter above zero | which boot of the host this is |
| `ceiling` | a percentage, or `none` | the ceiling the host holds on the real cell |
| `doze` | `exempt` or `applies` | the host's Doze policy towards this guest |
| `ingress` | `published` or `withheld` | whether the host publishes this guest's ports |

`epoch` is **required**: a statement that cannot be dated is a statement
whose claim can never be invalidated. The other three are not, because a
host that has said nothing about its Doze policy has not promised to doze
the guest.

Everything else is refused rather than partly believed: an unknown key, a
repeated key, two claims about one control, a ceiling above a hundred, a
boot counter of zero. A host that said two things about one control has
said nothing, and choosing one of them would be this program deciding.

### The guest's own record

A file, at `$HOME/.vayucell/guest` by default (`--guest-record` moves it):

```text
vayucell guest ledger 1
observed 8
fact 8 holding 60
fact 8 doze-exempt
```

`observed` is the boot the guest last heard from; `fact` lines are what the
host said, and the boot it said it in. A record that exists and will not
parse is **corrupt**, not empty: something wrote it, and reading it as
"the guest has established nothing" would turn a lost file into a clean
bill of health.

The file is written the way the halt record is written — a temporary, a
flush, a rename, and a flush of the directory — because a guest that
forgets what it was told is exactly as bad as one that never heard.

## Held by tests, held by wording

Twenty-four gate mutations pin the load-bearing lines: a counter that
moves backwards, a reboot that is not noticed, a claim carried forward
across one, a statement from a boot the guest has left, a key this build
does not know, a ceiling clamped instead of refused, a claim filed under
the wrong control, a record read as empty when it is corrupt. The rest is
held by wording that refuses to overclaim: the standing of a control is
`ATTESTED`, `VOID` or `UNKNOWN` and there is deliberately no `VERIFIED`,
proved by a `compile_fail` doctest; every claim travels with the caveat
that makes it honest; and the panel says the charge ceiling is the host's
to hold rather than that the device has no charge control at all.

## The channel is a stand-in

The environment variable is **not** the pKVM host channel; it is a local,
severable stand-in for the contract in ADR-0015, which does not know where
a statement came from. A real host replaces the variable without changing
a rule. What cannot be faked by any of this is the gate itself: a guest on
a real handset, a real host reboot underneath it, and a real Doze policy.
