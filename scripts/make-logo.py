#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Derive the VayuCell asset set from the two source logos.

The logo itself is **not generated**. It is a designed artefact, and the two
files in `docs/assets/source/` are the originals. This script only derives the
variants a repository needs from them — transparent versions, a mark-only crop,
and the square tile and favicons — so every derived file has one obvious
provenance and regenerating produces the same bytes.

Derivation, not redrawing. Nothing here invents geometry.

## How transparency is made

Both sources are monochrome line art on a flat ground, so alpha comes straight
from luminance: on the light source the artwork is dark and alpha is
`255 - luminance`; on the dark source the artwork is light and alpha is
`luminance`. That keeps every antialiased edge intact, where a threshold would
leave the curves of the V ragged.

## Where the mark ends and the wordmark begins

Found, not hard-coded. The script scans for rows containing ink and splits at
the largest vertical gap, which is the space the designer left between the mark
and the word. A hard-coded row would silently crop the wrong place the first
time a source is re-exported at a different size.

Usage: python3 scripts/make-logo.py
"""

import pathlib
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:
    sys.exit(
        "Pillow is required to derive the asset set:\n"
        "  python3 -m pip install Pillow\n"
        "The source logos in docs/assets/source/ are the originals and are "
        "committed; this script only rebuilds what is derived from them."
    )

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASSETS = ROOT / "docs" / "assets"
SOURCE = ASSETS / "source"

LIGHT = SOURCE / "vayucell-logo-light.png"   # dark artwork on a white ground
DARK = SOURCE / "vayucell-logo-dark.png"     # light artwork on a black ground

# Alpha at or below this is background haze, not artwork. The dark source
# carries ~115,000 pixels at alpha 1..8 — invisible to the eye and enough to
# make getbbox() return the whole canvas, which silently turned the tile crop
# into a no-op and scaled the mark down to a quarter of the frame. Real
# antialiased edges ramp through the full range; losing their faintest few
# percent is not visible, and keeping it breaks every bounding box.
ALPHA_FLOOR = 8

TILE_BG = (10, 10, 10)
TILE_RADIUS_RATIO = 0.21
TILE_INSET_RATIO = 0.13


def alpha_from_luminance(img, artwork_is_dark):
    """Flat ground to transparency, keeping antialiased edges."""
    grey = img.convert("L")
    alpha = grey.point(lambda v: 255 - v) if artwork_is_dark else grey
    alpha = alpha.point(lambda v: 0 if v <= ALPHA_FLOOR else v)
    out = Image.new("RGBA", img.size, (0, 0, 0, 0))
    solid = Image.new(
        "RGBA", img.size, (0, 0, 0, 255) if artwork_is_dark else (255, 255, 255, 255)
    )
    out.paste(solid, (0, 0), alpha)
    return out


def ink_rows(rgba, threshold=ALPHA_FLOOR):
    """Row indices that contain any artwork at all."""
    alpha = rgba.getchannel("A")
    width, height = rgba.size
    return [
        y
        for y in range(height)
        if alpha.crop((0, y, width, y + 1)).getextrema()[1] > threshold
    ]


def split_mark_from_wordmark(rgba):
    """The y at which the wordmark starts, from the largest vertical gap."""
    rows = ink_rows(rgba)
    if not rows:
        sys.exit("the source image appears to be blank")
    best_gap, best_at = 0, None
    for a, b in zip(rows, rows[1:]):
        if b - a > best_gap:
            best_gap, best_at = b - a, a
    if best_at is None or best_gap < 8:
        sys.exit(
            "no clear gap between the mark and the wordmark was found; the "
            "source layout has changed and this split needs revisiting"
        )
    return best_at + best_gap // 2, rows[0]


def trim(rgba, pad=0):
    """Crop to the artwork's bounding box, with optional padding.

    The bounding box is taken from a thresholded copy of the alpha channel.
    `getbbox()` counts any non-zero pixel, so a single unit of background haze
    anywhere makes it return the entire canvas.
    """
    box = (
        rgba.getchannel("A")
        .point(lambda v: 255 if v > ALPHA_FLOOR else 0)
        .getbbox()
    )
    if box is None:
        return rgba
    left, top, right, bottom = box
    width, height = rgba.size
    return rgba.crop(
        (
            max(0, left - pad),
            max(0, top - pad),
            min(width, right + pad),
            min(height, bottom + pad),
        )
    )


def rounded_tile(mark_rgba, size=1024):
    """The mark on its own dark ground, inset for the corners launchers mask."""
    tile = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    ground = Image.new("RGBA", (size, size), TILE_BG + (255,))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, size - 1, size - 1], int(size * TILE_RADIUS_RATIO), fill=255
    )
    tile.paste(ground, (0, 0), mask)

    inset = int(size * TILE_INSET_RATIO)
    avail = size - 2 * inset
    art = trim(mark_rgba)
    scale = min(avail / art.width, avail / art.height)
    art = art.resize(
        (max(1, round(art.width * scale)), max(1, round(art.height * scale))),
        Image.LANCZOS,
    )
    tile.paste(art, ((size - art.width) // 2, (size - art.height) // 2), art)
    return tile


def main():
    missing = [p.name for p in (LIGHT, DARK) if not p.exists()]
    if missing:
        sys.exit(
            "the source logos are missing from docs/assets/source/: "
            + ", ".join(missing)
            + "\nThey are the originals, not something this script can recreate."
        )

    light = Image.open(LIGHT).convert("RGB")
    dark = Image.open(DARK).convert("RGB")

    written = []

    def save(img, name):
        img.save(ASSETS / name, optimize=True)
        written.append(name)

    # The full lockups, exactly as designed.
    save(light, "vayucell-logo.png")
    save(dark, "vayucell-logo-dark.png")

    light_t = alpha_from_luminance(light, artwork_is_dark=True)
    dark_t = alpha_from_luminance(dark, artwork_is_dark=False)
    save(light_t, "vayucell-logo-transparent.png")
    save(dark_t, "vayucell-logo-transparent-dark.png")

    split, top = split_mark_from_wordmark(dark_t)
    print(f"  mark/wordmark split found at y={split} (artwork starts at y={top})")

    save(trim(light_t.crop((0, 0, light_t.width, split)), pad=12), "vayucell-mark.png")
    save(trim(dark_t.crop((0, 0, dark_t.width, split)), pad=12), "vayucell-mark-dark.png")

    tile = rounded_tile(dark_t.crop((0, 0, dark_t.width, split)))
    save(tile, "vayucell-tile.png")
    for px in (512, 180, 32):
        save(tile.resize((px, px), Image.LANCZOS), f"favicon-{px}.png")

    print(f"  derived {len(written)} file(s) from docs/assets/source/")
    for name in written:
        print("   ", name)


if __name__ == "__main__":
    main()
