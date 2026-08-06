<!-- SPDX-License-Identifier: CC0-1.0 -->

# The VayuCell mark

The logo set is **generated, not hand-edited**:

```bash
python3 scripts/make-logo.py
```

Regenerating produces byte-identical files. That is the same determinism
argument the release build makes, applied to the artwork: if you cannot rebuild
it, you cannot check it.

## What it is

A calligraphic **V** — one tapering brush stroke, a fine point at the entry,
swelling through the body, thinning again through the turn — with a three-unit
**server rack** standing in the crook of it.

The stroke is *vayu*, wind. The rack is what the wind is carrying.

They **interlock**. The rack's stand comes down exactly where the rising arm
ends its travel, so neither element reads as having been pasted on top of the
other. This is the part that took the most iterations: at the first tip position
the arm swept underneath the rack and buried the one element that says *server*.

## Construction

Everything is an explicit path — including **every letter of the wordmark**.
Nothing depends on a font being installed. That is not purism: most of the
machines this project runs on are phones, none of the build runners carry a
display typeface, and a logo that renders differently depending on what happens
to be installed is a logo nobody can verify.

The swash is a single closed contour. Both edges begin at the same point, which
is what makes the entry a *point* rather than a cut, and the body swells because
the inner edge bulges right while the outer edge holds its line.

The rack is **outlined, not filled**. An outline keeps its shape at the size a
favicon is actually seen at, where a solid block would close up into a
rectangle.

## Colour

**Monochrome.** One colour, inherited from `color` on the root element, so a
single SVG serves a light ground and a dark one.

| Role | Value |
| --- | --- |
| Ink | `#0A0A0A` |
| Paper | `#FFFFFF` |
| Tile ground | `#0A0A0A` |

There is no accent and no gradient. The mark carries its meaning in its shape.

## The files

| File | Use |
| --- | --- |
| `vayucell-logo.svg` / `.png` | Full lockup, light grounds |
| `vayucell-logo-dark.svg` / `.png` | Full lockup, dark grounds |
| `vayucell-mark.svg` / `.png` | Mark only, no wordmark |
| `vayucell-mark-dark.svg` / `.png` | Mark only, reversed |
| `vayucell-tile.svg`, `favicon-*.png` | Square, with its own dark ground |

The tile carries a background on purpose. At 32px a transparent icon competes
with whatever the browser puts behind it, and having a ground of your own is the
difference between a recognisable favicon and a smudge. The mark is fitted to
**66% of the tile** from its measured bounding box rather than a guessed scale
factor — launchers mask the corners and clip the outer few percent, and an
eyeballed factor had it sitting small and off-centre.

## Rules

1. **Do not retype the wordmark.** It is drawn geometry, not a font, and no
   installed typeface will match it.
2. **Do not separate the rack from the stroke.** They interlock; moved apart
   they are two clip-art elements sharing a canvas.
3. **Do not mirror the mark.** The stroke travels one way.
4. **Do not add colour, gradient, or shadow.** If a surface needs the mark to
   stand out, change the surface.
5. **Clear space:** at least the cap height of the wordmark on every side.
6. **Minimum size:** the full lockup stops being legible below 160px wide. Use
   the mark, or the tile, below that.

## Licence

CC0-1.0, like the charter and the hardware database. The *name* VayuCell is
covered by [`TRADEMARK.md`](../TRADEMARK.md) — copy the artwork freely, but do
not use the name to imply that a modified build is the official one.
