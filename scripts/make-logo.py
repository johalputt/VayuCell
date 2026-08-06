#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Generate the VayuCell logo set.

Everything here is drawn, not typeset. Every letterform is an explicit path, so
the logo renders identically on a machine with no fonts installed — which is
most of the machines this project runs on, and all of the phones.

It shares its construction with the VayuPress mark deliberately. The chevron and
the two wind ribbons are the family signature (vayu — wind). What changes per
product is the accent: VayuPress is blue, VayuCell is emerald going to mint,
because this product is about a cell with charge left in it.

The chevron geometry is derived rather than eyeballed. Both arms are bounded by
pairs of parallel lines offset 88px horizontally, and the vertices are the
intersections of those lines — which is what makes the left arm read heavier
than the right without either looking wrong.

Usage: python3 scripts/make-logo.py
"""

import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "docs" / "assets"

# ── Palette ───────────────────────────────────────────────────────────────────
INK = "#111C2B"          # near-black navy, the family wordmark colour
INK_LIGHT = "#EEF4F9"    # wordmark on a dark ground
DEEP = "#047857"         # emerald 700
MID = "#10B981"          # emerald 500
BRIGHT = "#5EEAD4"       # teal 300 — the lit tip of the leading ribbon
STEEL_EDGE = "#2A3340"

W, H = 920, 640

# ── The chevron ───────────────────────────────────────────────────────────────
# Left arm runs at (0.573, 0.819); right arm at (0.777, -0.629). The right arm
# is cut at 40% of the chevron's height, which is what leaves the ribbons a
# clear field instead of crossing the metal.
A1 = (196.0, 60.0)     # top-left, outer
A2 = (284.0, 60.0)     # top-left, inner
IV = (479.3, 339.1)    # inner vertex
B1 = (549.7, 282.0)    # top-right, inner
B2 = (637.7, 282.0)    # top-right, outer
OV = (454.9, 430.0)    # outer vertex

CHEVRON = "M{:.1f} {:.1f}L{:.1f} {:.1f}L{:.1f} {:.1f}L{:.1f} {:.1f}L{:.1f} {:.1f}L{:.1f} {:.1f}Z".format(
    *A1, *A2, *IV, *B1, *B2, *OV
)

# The right arm alone, drawn as a lighter facet on the metallic variant so the
# chevron reads as folded metal rather than a flat silhouette.
FACET = "M{:.1f} {:.1f}L{:.1f} {:.1f}L{:.1f} {:.1f}L{:.1f} {:.1f}Z".format(
    *IV, *B1, *B2, *OV
)

# ── The wind ribbons ──────────────────────────────────────────────────────────
# Two crescents sweeping up and right, each tapering to a point at BOTH ends.
# The pointed tails matter: a blunt tail lands on the chevron's right arm and
# turns the crossing into a smudge. Pointed, they sit in the notch between the
# arms and read as motion leaving the mark.
# The tails end inside the notch between the arms — the open wedge above the
# inner vertex at (479, 339). A tail even 20px lower lands on the right arm and
# the crossing turns into a smudge, which is what the first draft did.
RIBBON_TOP = (
    "M478 246"
    "C556 166 690 74 866 20"
    "C744 100 612 178 528 250"
    "Z"
)
RIBBON_BOTTOM = (
    "M486 300"
    "C556 240 668 172 826 118"
    "C712 186 604 246 516 300"
    "Z"
)

# ── Letterforms ───────────────────────────────────────────────────────────────
# Geometric, heavy, on a 100-unit cap height. `hole` paths are punched with
# fill-rule="evenodd".
GLYPHS = {
    "V": (78, "M0 0L21 0L39 70L57 0L78 0L48 100L30 100Z"),
    "A": (78, "M0 100L31 0L47 0L78 100L58 100L52 80L26 80L20 100Z"
              "M39 28L51 71L27 71Z"),
    "Y": (78, "M0 0L21 0L39 42L57 0L78 0L49 66L49 100L29 100L29 66Z"),
    "U": (78, "M0 0L20 0L20 60C20 74 28 82 39 82C50 82 58 74 58 60L58 0"
              "L78 0L78 60C78 86 61 101 39 101C17 101 0 86 0 60Z"),
    "C": (78, "M78 24C69 8 56 0 39 0C17 0 0 22 0 50C0 78 17 100 39 100"
              "C56 100 69 92 78 76L61 66C55 76 48 81 39 81C27 81 20 68 20 50"
              "C20 32 27 19 39 19C48 19 55 24 61 34Z"),
    "E": (72, "M0 0L72 0L72 19L20 19L20 40L66 40L66 59L20 59L20 81L72 81"
              "L72 100L0 100Z"),
    "L": (70, "M0 0L20 0L20 81L70 81L70 100L0 100Z"),
    "R": (78, "M0 0L48 0C66 0 78 13 78 31C78 45 70 55 58 59L80 100L57 100"
              "L38 64L20 64L20 100L0 100Z"
              "M20 19L46 19C54 19 58 24 58 31C58 39 54 45 46 45L20 45Z"),
    "I": (20, "M0 0L20 0L20 100L0 100Z"),
    "M": (86, "M0 0L23 0L43 55L63 0L86 0L86 100L67 100L67 40L52 82L34 82"
              "L19 40L19 100L0 100Z"),
    "G": (80, "M80 26C71 9 57 0 40 0C18 0 0 22 0 50C0 78 18 100 41 100"
              "C60 100 74 91 80 76L80 45L41 45L41 63L61 63C58 74 51 81 41 81"
              "C29 81 20 68 20 50C20 32 28 19 40 19C49 19 56 24 62 35Z"),
    "O": (80, "M40 0C62 0 80 22 80 50C80 78 62 100 40 100C18 100 0 78 0 50"
              "C0 22 18 0 40 0Z"
              "M40 19C29 19 20 32 20 50C20 68 29 81 40 81C51 81 60 68 60 50"
              "C60 32 51 19 40 19Z"),
    "N": (78, "M0 0L21 0L58 61L58 0L78 0L78 100L57 100L20 39L20 100L0 100Z"),
    "S": (74, "M66 24C60 9 51 0 36 0C15 0 2 12 2 29C2 44 12 53 30 58L40 61"
              "C50 64 54 68 54 74C54 81 47 85 37 85C26 85 18 79 12 69L0 81"
              "C8 94 21 100 37 100C58 100 74 89 74 71C74 55 64 46 45 41L35 38"
              "C25 35 21 31 21 26C21 19 27 15 36 15C45 15 51 20 56 29Z"),
    "·": (16, "M8 42C13 42 16 46 16 51C16 56 13 60 8 60C3 60 0 56 0 51"
              "C0 46 3 42 8 42Z"),
    " ": (44, ""),
}

WORD = "VAYUCELL"
SPLIT = 4               # VAYU in ink, CELL in the accent
TRACK = 28.0            # letter spacing, in glyph units
CAP = 108.0
SCALE = CAP / 100.0

TAGLINE = "RECLAIM · GOVERN · SERVE"
TAG_TRACK = 40.0
TAG_CAP = 26.0
TAG_SCALE = TAG_CAP / 100.0


def run_width(text, track, scale):
    adv = sum(GLYPHS[c][0] for c in text) + track * (len(text) - 1)
    return adv * scale


def lay_out(text, x, y, scale, track, fill_for):
    """Emit one path per glyph. fill_for(index) picks the paint."""
    parts, cx = [], x
    for i, ch in enumerate(text):
        adv, d = GLYPHS[ch]
        if d:
            parts.append(
                f'<path d="{d}" fill="{fill_for(i)}" fill-rule="evenodd" '
                f'transform="translate({cx:.2f} {y:.2f}) scale({scale:.5f})"/>'
            )
        cx += (adv + track) * scale
    return "\n  ".join(parts)


def defs(metallic, dx, dark):
    word_x0 = (W - run_width(WORD, TRACK, SCALE)) / 2
    word_x1 = word_x0 + run_width(WORD, TRACK, SCALE)
    # The accent gradient runs across the whole CELL span in user space, not
    # per glyph. Per-glyph object-bounding-box gradients make four letters that
    # each fade identically, which reads as flat.
    cell_x0 = word_x0 + run_width(WORD[:SPLIT], TRACK, SCALE) + TRACK * SCALE
    # On a dark ground the emerald end of the ramp closes up against the
    # background and the leading letters of CELL go muddy, so the whole accent
    # shifts one step brighter. Same hues, different footing — a palette that
    # ignores what it is sitting on is a palette that only works in one place.
    lead, tail = (MID, BRIGHT) if dark else (DEEP, MID)
    low_start, low_end = ("#046F55", MID) if dark else ("#02503E", MID)

    g = [
        f'<linearGradient id="wind" x1="{478+dx}" y1="300" x2="{866+dx}" y2="20" '
        f'gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="{lead}"/>'
        f'<stop offset="0.5" stop-color="{MID if not dark else "#34D8B0"}"/>'
        f'<stop offset="1" stop-color="{BRIGHT}"/></linearGradient>',

        f'<linearGradient id="windLow" x1="{486+dx}" y1="300" x2="{826+dx}" y2="118" '
        f'gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="{low_start}"/>'
        f'<stop offset="0.6" stop-color="{DEEP if not dark else MID}"/>'
        f'<stop offset="1" stop-color="{low_end}"/></linearGradient>',

        f'<linearGradient id="word" x1="{cell_x0:.1f}" y1="0" x2="{word_x1:.1f}" '
        f'y2="0" gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="{lead}"/>'
        f'<stop offset="1" stop-color="{tail}"/></linearGradient>',
    ]
    if metallic:
        g.append(
            f'<linearGradient id="steel" x1="{196+dx}" y1="60" x2="{638+dx}" y2="430" '
            'gradientUnits="userSpaceOnUse">'
            '<stop offset="0" stop-color="#FDFEFF"/>'
            '<stop offset="0.16" stop-color="#DDE3E9"/>'
            '<stop offset="0.34" stop-color="#B2BAC4"/>'
            '<stop offset="0.5" stop-color="#F5F8FA"/>'
            '<stop offset="0.7" stop-color="#BFC7D0"/>'
            '<stop offset="1" stop-color="#8B95A1"/></linearGradient>'
        )
        g.append(
            f'<linearGradient id="steelFacet" x1="{455+dx}" y1="430" x2="{638+dx}" y2="282" '
            'gradientUnits="userSpaceOnUse">'
            '<stop offset="0" stop-color="#98A2AD"/>'
            '<stop offset="0.55" stop-color="#E9EEF2"/>'
            '<stop offset="1" stop-color="#FBFCFD"/></linearGradient>'
        )
    return "\n    ".join(g)


def build(kind):
    """kind: light | dark | metallic | mark | mark-dark"""
    metallic = kind == "metallic"
    mark_only = kind.startswith(("mark", "icon", "tile"))
    tile = kind.startswith("tile")
    # A tile carries its own dark ground, so the mark on it is the light one.
    dark = kind.endswith("dark") or tile

    if kind.startswith("icon") or kind.startswith("tile"):
        # Square, for a favicon or an app tile. The mark's bounding box is
        # 670 x 410 centred on (531, 225), so a square crop leaves air above and
        # below rather than cropping the ribbons — an icon that clips its own
        # mark reads as a mistake at every size, and launchers mask the corners
        # anyway.
        side = 716
        vb = f"{531 - side/2:.0f} {225 - side/2:.0f} {side} {side}"
        w = h = side
    elif mark_only:
        vb, w, h = "170 0 720 450", 720, 450
    else:
        vb, w, h = f"0 0 {W} {H}", W, H

    if metallic:
        chevron = (
            f'<path d="{CHEVRON}" fill="url(#steel)" stroke="{STEEL_EDGE}" '
            f'stroke-width="4" stroke-linejoin="round"/>\n  '
            f'<path d="{FACET}" fill="url(#steelFacet)" stroke="{STEEL_EDGE}" '
            f'stroke-width="3" stroke-linejoin="round"/>'
        )
    else:
        fill = INK_LIGHT if dark else INK
        chevron = f'<path d="{CHEVRON}" fill="{fill}"/>'

    # The chevron begins at x=196 and the ribbons reach x=866, so the mark's
    # optical centre is right of the canvas centre. The lockup nudges it back;
    # the mark-only crop needs no shift because its viewBox frames it directly.
    dx = 0 if mark_only else -46
    body = []
    if tile:
        # A rounded square behind the mark. At 32px a transparent icon competes
        # with whatever the browser puts behind it; a ground of its own is the
        # difference between a recognisable favicon and a smudge.
        x0, y0 = 531 - 358, 225 - 358
        body.append(
            f'<rect x="{x0}" y="{y0}" width="716" height="716" rx="152" '
            f'fill="#0B1220"/>'
        )
    # Launchers mask icon corners and clip the outer few percent. The mark is
    # scaled about its own centre so it keeps a safe margin inside the tile
    # instead of running into the rounded edge.
    # One attribute, not two: a second transform="" on the same element is a
    # duplicate attribute and the browser silently keeps the first, so the
    # scale was being dropped while the file looked correct.
    inner = (
        " translate(531 225) scale(0.76) translate(-531 -225)" if tile else ""
    )
    body += [
        f'<g transform="translate({dx} 0){inner}">',
        f'  {chevron}',
        f'  <path d="{RIBBON_BOTTOM}" fill="url(#windLow)"/>',
        f'  <path d="{RIBBON_TOP}" fill="url(#wind)"/>',
        '</g>',
    ]

    if not mark_only:
        ink = INK_LIGHT if kind == "dark" else INK
        wx = (W - run_width(WORD, TRACK, SCALE)) / 2
        body.append(
            lay_out(WORD, wx, 476, SCALE, TRACK,
                    lambda i: ink if i < SPLIT else "url(#word)")
        )

        sub = "#9FB3C8" if kind == "dark" else "#33445C"
        tw = run_width(TAGLINE, TAG_TRACK, TAG_SCALE)
        tx = (W - tw) / 2
        ty = 604
        body.append(
            lay_out(TAGLINE, tx, ty, TAG_SCALE, TAG_TRACK, lambda i: sub)
        )
        # Rules flanking the tagline, set to the accent so the mark's colour
        # appears once more at the foot of the lockup.
        rule_y = ty + TAG_CAP / 2 - 1.25
        gap = 26
        body.append(
            f'<rect x="{tx - gap - 110:.1f}" y="{rule_y:.1f}" width="110" '
            f'height="2.5" fill="{MID}" opacity="0.9"/>'
            f'<rect x="{tx + tw + gap:.1f}" y="{rule_y:.1f}" width="110" '
            f'height="2.5" fill="{MID}" opacity="0.9"/>'
        )

    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{w}" '
        f'height="{h}" role="img" aria-label="VayuCell">\n'
        f'  <title>VayuCell</title>\n'
        f'  <defs>\n    {defs(metallic, dx, dark)}\n  </defs>\n  '
        + "\n  ".join(body)
        + "\n</svg>\n"
    )


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for kind, name in [
        ("light", "vayucell-logo.svg"),
        ("dark", "vayucell-logo-dark.svg"),
        ("metallic", "vayucell-logo-metallic.svg"),
        ("mark", "vayucell-mark.svg"),
        ("mark-dark", "vayucell-mark-dark.svg"),
        ("icon", "vayucell-icon.svg"),
        ("icon-dark", "vayucell-icon-dark.svg"),
        ("tile", "vayucell-tile.svg"),
    ]:
        (OUT / name).write_text(build(kind))
        print("wrote", (OUT / name).name)


if __name__ == "__main__":
    main()
