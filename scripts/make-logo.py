#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Generate the VayuCell logo set.

The mark is a calligraphic V — one tapering brush stroke, a fine point at the
entry, swelling through the body, thinning again through the turn — with a
three-unit server rack standing in the crook of it. The stroke is vayu, wind.
The rack is what the wind is carrying. They interlock: the rack's stand comes
down exactly where the rising arm ends its travel, so neither element reads as
having been pasted on top of the other.

Monochrome by design. One colour, inherited from `color` on the root element, so
a single file serves a light ground and a dark one.

Everything is drawn as explicit paths, including every letter of the wordmark, so
the logo renders identically on a machine with no fonts installed — which is most
of the machines this project runs on, and all of the phones.

Usage: python3 scripts/make-logo.py
"""

import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "docs" / "assets"

INK = "#0A0A0A"
PAPER = "#FFFFFF"

S = 1024                      # the mark is laid out on a square field
LOCKUP_H = 940                # the full lockup adds the wordmark below it

# ── The calligraphic V ────────────────────────────────────────────────────────
# One closed contour. Traced from the fine entry point at the top left: down the
# inner edge of the falling stroke to the notch, up the inner edge of the rising
# arm to its point, back down that arm's outer edge, round the bottom turn, and
# up the outer edge of the falling stroke to where it started.
#
# Both edges begin at the same point, which is what makes the entry a point
# rather than a cut. The body swells because the inner edge bulges right while
# the outer edge holds its line.
SWASH = (
    "M256 198"
    "C352 268 452 424 528 566"        # inner edge of the falling stroke
    "C560 522 612 486 676 468"        # up the inner edge of the rising arm
    "C650 528 614 590 580 630"        # back down its outer edge
    "C552 662 500 660 474 620"        # the bottom turn, rounded like a brush
    "C404 496 316 330 256 198"        # outer edge of the falling stroke
    "Z"
)

# ── The server rack ───────────────────────────────────────────────────────────
RACK_X, RACK_W = 600.0, 186.0
UNIT_H, UNIT_GAP, UNIT_R = 74.0, 14.0, 22.0
RACK_Y0 = 196.0
STROKE = 12.0

CX = RACK_X + RACK_W / 2
STEM_TOP = RACK_Y0 + 3 * UNIT_H + 2 * UNIT_GAP
STEM_BOTTOM = STEM_TOP + 40
BASE_HALF = 42.0


def rack():
    """Three outlined units, each with a status light and a drive bay, on a stand."""
    out = []
    for i in range(3):
        y = RACK_Y0 + i * (UNIT_H + UNIT_GAP)
        # Outlined rather than solid. The rack has to survive the size a favicon
        # is actually seen at, and an outline keeps its shape where a filled
        # block would just close up into a rectangle.
        out.append(
            f'<rect x="{RACK_X + STROKE / 2:.1f}" y="{y + STROKE / 2:.1f}" '
            f'width="{RACK_W - STROKE:.1f}" height="{UNIT_H - STROKE:.1f}" '
            f'rx="{UNIT_R - STROKE / 2:.1f}" fill="none" stroke="currentColor" '
            f'stroke-width="{STROKE}"/>'
        )
        cy = y + UNIT_H / 2
        out.append(
            f'<circle cx="{RACK_X + 38:.1f}" cy="{cy:.1f}" r="10" '
            f'fill="currentColor"/>'
        )
        out.append(
            f'<rect x="{RACK_X + 100:.1f}" y="{cy - 6:.1f}" width="64" '
            f'height="12" rx="6" fill="currentColor"/>'
        )
    out.append(
        f'<path d="M{CX:.0f} {STEM_TOP:.0f}L{CX:.0f} {STEM_BOTTOM:.0f}'
        f'M{CX - BASE_HALF:.0f} {STEM_BOTTOM:.0f}L{CX + BASE_HALF:.0f} '
        f'{STEM_BOTTOM:.0f}" fill="none" stroke="currentColor" '
        f'stroke-width="{STROKE}" stroke-linecap="round"/>'
    )
    return "\n    ".join(out)


# ── Letterforms ───────────────────────────────────────────────────────────────
# Light geometric sans, mixed case. Cap height 100, x-height 72, baseline 100,
# descender to 147. The stems are ~9 units, which is what makes this read light
# where an uppercase slab would read industrial.
GLYPHS = {
    "V": (80, "M0 0L10.5 0L40 86L69.5 0L80 0L45 100L35 100Z"),
    "a": (72, "M31 28C42 28 51 32 57 40L57 28L66 28L66 100L57 100L57 88"
              "C51 96 42 100 31 100C13 100 0 84 0 64C0 44 13 28 31 28Z"
              "M33 37C20 37 9 48 9 64C9 80 20 91 33 91C46 91 57 80 57 64"
              "C57 48 46 37 33 37Z"),
    "y": (68, "M0 28L10 28L34 88L58 28L68 28L27 132C22 143 15 148 5 148"
              "L0 148L0 139L4 139C11 139 16 135 19 128L23 118Z"),
    "u": (72, "M0 28L9 28L9 70C9 82 18 91 31 91C44 91 53 82 53 70L53 28"
              "L62 28L62 100L53 100L53 88C48 96 40 100 30 100"
              "C12 100 0 88 0 71Z"),
    "C": (94, "M94 22C84 8 69 0 52 0C23 0 4 21 4 50C4 79 23 100 52 100"
              "C69 100 84 92 94 78L86 72C78 84 66 91 52 91C29 91 13 74 13 50"
              "C13 26 29 9 52 9C66 9 78 16 86 28Z"),
    "e": (70, "M34 28C53 28 66 43 66 63L66 68L9 68C11 82 21 91 34 91"
              "C43 91 50 87 55 80L62 85C55 95 45 100 34 100"
              "C14 100 0 85 0 64C0 43 14 28 34 28Z"
              "M34 37C22 37 12 45 9 59L57 59C55 45 46 37 34 37Z"),
    "l": (28, "M9 0L18 0L18 100L9 100Z"),
}

WORD = "VayuCell"
TRACK = 5.0
CAP = 118.0
SCALE = CAP / 100.0
BASELINE = 838.0


def word_width():
    return (sum(GLYPHS[c][0] for c in WORD) + TRACK * (len(WORD) - 1)) * SCALE


def wordmark():
    x = (S - word_width()) / 2
    y = BASELINE - CAP
    parts = []
    for ch in WORD:
        adv, d = GLYPHS[ch]
        parts.append(
            f'<path d="{d}" fill="currentColor" fill-rule="evenodd" '
            f'transform="translate({x:.2f} {y:.2f}) scale({SCALE:.5f})"/>'
        )
        x += (adv + TRACK) * SCALE
    return "\n    ".join(parts)


def svg(vb, w, h, colour, body):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{w}" '
        f'height="{h}" color="{colour}" role="img" aria-label="VayuCell">\n'
        f"  <title>VayuCell</title>\n  {body}\n</svg>\n"
    )


# The mark's bounding box, derived from the geometry above rather than guessed:
# the swash spans x 256..~700 and the rack ends at RACK_X + RACK_W, while the
# stroke bottoms out around y 664.
MARK_X0, MARK_Y0_ = 256.0, 188.0
MARK_X1, MARK_Y1 = RACK_X + RACK_W, 664.0
MARK_W, MARK_H = MARK_X1 - MARK_X0, MARK_Y1 - MARK_Y0_
MARK_CX, MARK_CY = (MARK_X0 + MARK_X1) / 2, (MARK_Y0_ + MARK_Y1) / 2


def build(kind):
    """kind: light | dark | mark | mark-dark | tile"""
    dark = kind.endswith("dark") or kind == "tile"
    colour = PAPER if dark else INK
    mark = f'<path d="{SWASH}" fill="currentColor"/>\n    {rack()}'

    if kind == "tile":
        # The mark is fitted to the tile from its measured bounding box, not by
        # a guessed scale factor. 66% of the field leaves the margin launchers
        # need — they mask the corners and clip the outer few percent — and an
        # eyeballed factor had it sitting small and off-centre in a corner.
        k = (S * 0.66) / max(MARK_W, MARK_H)
        tx, ty = S / 2 - MARK_CX * k, S / 2 - MARK_CY * k
        return svg(
            f"0 0 {S} {S}", S, S, colour,
            f'<rect width="{S}" height="{S}" rx="216" fill="{INK}"/>\n  '
            f'<g transform="translate({tx:.2f} {ty:.2f}) scale({k:.5f})">'
            f"\n    {mark}\n  </g>",
        )

    if kind.startswith("mark"):
        # Tight crop, even optical margin on every side of the bounding box.
        m = 44
        return svg(
            f"{MARK_X0 - m:.0f} {MARK_Y0_ - m:.0f} {MARK_W + 2 * m:.0f} "
            f"{MARK_H + 2 * m:.0f}",
            int(MARK_W + 2 * m), int(MARK_H + 2 * m), colour, mark,
        )

    return svg(f"0 0 {S} {LOCKUP_H}", S, LOCKUP_H, colour,
               f"{mark}\n    {wordmark()}")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for kind, name in [
        ("light", "vayucell-logo.svg"),
        ("dark", "vayucell-logo-dark.svg"),
        ("mark", "vayucell-mark.svg"),
        ("mark-dark", "vayucell-mark-dark.svg"),
        ("tile", "vayucell-tile.svg"),
    ]:
        (OUT / name).write_text(build(kind))
        print("wrote", name)


if __name__ == "__main__":
    main()
