<!-- SPDX-License-Identifier: CC0-1.0 -->

# The VayuCell mark

## The logo is not generated

It is a designed artefact. The two originals live in
[`docs/assets/source/`](assets/source/) and are the authority:

| File | What it is |
| --- | --- |
| `vayucell-logo-light.png` | The lockup, dark artwork on a white ground |
| `vayucell-logo-dark.png` | The lockup, light artwork on a black ground |

Everything else in `docs/assets/` is **derived** from those two by
[`scripts/make-logo.py`](../scripts/make-logo.py):

```bash
python3 scripts/make-logo.py     # needs Pillow
```

Derivation, not redrawing — the script invents no geometry. Regenerating
produces byte-identical files, so any derived asset can be checked against its
source rather than taken on trust.

## What it is

A calligraphic **V** — one tapering brush stroke, broad through the fall,
thinning through the turn — with a three-unit **server rack** standing in the
crook of it. The stroke is *vayu*, wind. The rack is what the wind is carrying.
The rack's stand comes down where the rising arm ends, so the two read as one
mark rather than two elements sharing a canvas.

Monochrome. There is no accent colour and no gradient.

## How the derived files are made

**Transparency** comes from luminance, not a threshold. Both sources are
monochrome line art on a flat ground, so on the light source alpha is
`255 − luminance` and on the dark source alpha is `luminance`. That keeps every
antialiased edge intact where a hard threshold would leave the curves of the V
ragged.

Alpha at or below **8** is floored to zero. The dark source carries roughly
115,000 background pixels at alpha 1–8 — invisible to the eye, and enough to
make `getbbox()` return the entire canvas. That silently turned the tile crop
into a no-op and scaled the mark down to a quarter of its frame before it was
caught.

**The mark/wordmark split is found, not hard-coded.** The script scans for rows
containing ink and cuts at the largest vertical gap — the space the designer
left between the mark and the word. A hard-coded row would crop the wrong place
the first time a source is re-exported at a different size, and would do it
quietly.

## The files

| File | Use |
| --- | --- |
| `vayucell-logo.png` | Full lockup, light grounds |
| `vayucell-logo-dark.png` | Full lockup, dark grounds |
| `vayucell-logo-transparent.png` | Full lockup, dark artwork, no ground |
| `vayucell-logo-transparent-dark.png` | Full lockup, light artwork, no ground |
| `vayucell-mark.png` | Mark only, dark artwork, no ground |
| `vayucell-mark-dark.png` | Mark only, light artwork, no ground |
| `vayucell-tile.png`, `favicon-*.png` | Square, with its own dark ground |

The tile carries a ground on purpose. At 32px a transparent icon competes with
whatever the browser puts behind it, and having a ground of your own is the
difference between a recognisable favicon and a smudge. The mark is inset 13% —
launchers mask the corners and clip the outer few percent.

## Rules

1. **Edit the source, never a derived file.** A derived file edited by hand is
   overwritten the next time anyone runs the script, and the change is lost
   without a trace.
2. **Do not separate the rack from the stroke.** They interlock; moved apart
   they are two pieces of clip art.
3. **Do not mirror the mark.** The stroke travels one way.
4. **Do not add colour, gradient, or shadow.** If a surface needs the mark to
   stand out, change the surface.
5. **Clear space:** at least the cap height of the wordmark on every side.
6. **Minimum size:** the full lockup stops being legible below 160px wide. Use
   the mark, or the tile, below that.

## Licence

The artwork is CC0-1.0, like the charter and the hardware database. The *name*
VayuCell is covered by [`TRADEMARK.md`](../TRADEMARK.md) — copy the artwork
freely, but do not use the name to imply that a modified build is the official
one.
