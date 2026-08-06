<!-- SPDX-License-Identifier: CC0-1.0 -->

# The VayuCell mark

The logo set is **generated**, not hand-edited:

```bash
python3 scripts/make-logo.py
```

Everything is drawn as explicit paths — including every letter of the wordmark
and the tagline. Nothing depends on a font being installed, which matters
because most of the machines this project runs on are phones, and none of the
build runners have a display typeface.

## Construction

It shares its construction with the VayuPress mark deliberately. The **chevron**
and the **two wind ribbons** are the family signature — *vayu*, wind. What
changes per product is the accent.

The chevron geometry is derived rather than eyeballed. Both arms are bounded by
pairs of parallel lines offset 88 units horizontally, and the two vertices are
the intersections of those lines. That is what makes the left arm read heavier
than the right without either looking like a mistake. The right arm is cut at
**40% of the chevron's height**, which is what leaves the ribbons a clear field
instead of crossing the metal.

The ribbons taper to a point at **both** ends. This is not decoration: a blunt
tail lands on the right arm and turns the crossing into a smudge. Pointed, the
tails sit in the notch between the arms and read as motion leaving the mark.

## Palette

| Role | Value | Note |
|---|---|---|
| Ink | `#111C2B` | Wordmark on a light ground. The family navy |
| Ink, reversed | `#EEF4F9` | Wordmark on a dark ground |
| Accent, deep | `#047857` | Emerald 700 |
| Accent, mid | `#10B981` | Emerald 500 |
| Accent, bright | `#5EEAD4` | Teal 300 — the lit tip of the leading ribbon |
| Tile ground | `#0B1220` | The favicon's own background |

VayuPress is blue. **VayuCell is emerald going to mint, because this product is
about a cell that still has charge left in it** — a phone somebody was told was
finished.

On a dark ground the whole accent shifts one step brighter. The emerald end of
the ramp closes up against a dark background and the leading letters of `CELL`
go muddy; same hues, different footing. A palette that ignores what it is
sitting on is a palette that only works in one place.

## The files

| File | Use |
|---|---|
| `vayucell-logo.svg` / `.png` | Full lockup, light backgrounds |
| `vayucell-logo-dark.svg` / `.png` | Full lockup, dark backgrounds |
| `vayucell-logo-metallic.svg` / `.png` | The premium lockup — chiselled steel chevron |
| `vayucell-mark.svg` / `.png` | Mark only, no wordmark |
| `vayucell-mark-dark.svg` | Mark only, reversed |
| `vayucell-icon.svg`, `vayucell-icon-dark.svg` | Square, transparent |
| `vayucell-tile.svg`, `favicon-*.png` | Square with its own dark ground |

The tile carries a background on purpose. At 32px a transparent icon competes
with whatever the browser puts behind it, and the difference between a
recognisable favicon and a smudge is having a ground of your own. The mark is
scaled to 76% inside it, because launchers mask the corners and clip the outer
few percent.

## Rules

1. **Do not retype the wordmark.** It is drawn geometry, not a font, and no
   installed typeface will match it.
2. **Do not recolour the accent** to another product's colour. The accent is how
   the family tells its members apart.
3. **Do not put the ribbons on the left**, or mirror the mark. It moves one way.
4. **Do not add a drop shadow to the flat variants.** The metallic variant is
   the one that carries dimension; the flat ones are flat on purpose.
5. **Clear space:** at least the height of the wordmark's cap on every side.
6. **Minimum size:** the full lockup stops being legible below 180px wide. Use
   the mark or the tile below that.

## Licence

CC0-1.0, like the charter and the hardware database. The *name* VayuCell is
covered by [`TRADEMARK.md`](../TRADEMARK.md) — you may copy the artwork freely,
but do not use the name to imply that a modified build is the official one.
