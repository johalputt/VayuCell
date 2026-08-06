# VayuCell

**Turn a retired phone into a server you own.**

A five-year-old flagship has eight 64-bit cores, several gigabytes of RAM, fast
onboard storage, Wi-Fi *and* a cellular modem, an integrated battery that works
as an uninterruptible power supply — and it idles at one to three watts. Billions
of them are in drawers.

VayuCell turns one into a server that hosts your website, your mail, your files
and your backups. It is free, open source, and designed so that the project
disappearing would not stop your device working.

## Status

**Founding documents.** No code yet. Start with:

| Document | What it is |
|---|---|
| [`CHARTER.md`](CHARTER.md) | The constitution. CC0. Read this first |
| [`PLAN.md`](PLAN.md) | The full project plan |
| [`ADR-0001`](docs/adr/ADR-0001-tier-model-and-capability-registry.md) | Tier model and capability registry |
| [`ADR-0002`](docs/adr/ADR-0002-battery-safety-governor.md) | The Battery Safety Governor |
| [`hardware/`](hardware/) | Device compatibility database (CC0) |

## Read this before you plug anything in

VayuCell asks you to leave a lithium battery energised and warm, for years, in a
building where you sleep. That is the condition under which cells age fastest, and
a swollen cell is a fire hazard.

**Not every phone can limit its own charging.** On an unrooted stock phone, none
can. VayuCell will tell you which case yours is, on the first screen, before you
rely on it — and the safety row for a device that cannot limit charge stays red
forever, because it is.

**Put your phone face-down on a flat table now and then.** If it rocks, or the
screen or back is lifting at any edge, stop using it and take it to
hazardous-waste handling. Software cannot see that. You can.

## What it will never claim

1. That every phone can be a server.
2. That the battery is safe — risk is governed, never eliminated.
3. That an abandoned vendor kernel is secure.
4. That one phone is datacentre reliability.
5. That a rented relay is independence.

## Licence

| Artefact | Licence |
|---|---|
| Charter and specifications | CC0-1.0 |
| Source code | Apache-2.0 |
| Hardware database | CC0-1.0 |
| Documentation | CC-BY-4.0 |

Apache-2.0 rather than MIT is deliberate: this project touches charging circuits,
power management and virtualisation — patent-dense territory — and Apache-2.0
carries an express patent grant that MIT does not. See `CHARTER.md` Article VI.

No token. No treasury. No fee. No account.
