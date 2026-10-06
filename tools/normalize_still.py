"""Normalise a single unit picture into a still for the game (see docs/specs/unit-art.md, "Still").

Scales it to 4 image pixels per base pixel, puts the feet anchor at (128, 232) of a 256 x 256 canvas,
makes the edges hard (alpha 0 or 255) and turns the tribe-coloured hue (red by default) into magenta,
which the game swaps for each tribe's colour.

  uvx --with pillow python tools/normalize_still.py SRC OUT --feet 206,700 --px-per-base 17
"""

import argparse
import colorsys

from PIL import Image

SCALE = 4
CANVAS = 256
FEET = (128, 232)
MAGENTA_HUE = 300 / 360


def is_tribe_colour(r, g, b, hue, spread):
    h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
    d = min(abs(h - hue), 1 - abs(h - hue))
    return d <= spread and s > 0.35 and v > 0.25


def main():
    p = argparse.ArgumentParser()
    p.add_argument("src")
    p.add_argument("out")
    p.add_argument("--feet", required=True, help="x,y of the ground point between the feet, in SRC pixels")
    p.add_argument("--px-per-base", type=float, required=True, help="SRC pixels per base pixel (a standing adult is 34 base px feet to head)")
    p.add_argument("--tribe-hue", type=float, default=0, help="hue (degrees) of the tribe-coloured parts in SRC")
    p.add_argument("--hue-spread", type=float, default=22, help="degrees around --tribe-hue still counted as tribe colour")
    a = p.parse_args()

    fx, fy = (float(v) for v in a.feet.split(","))
    src = Image.open(a.src).convert("RGBA")
    k = SCALE / a.px_per_base
    size = (round(src.width * k), round(src.height * k))
    small = src.convert("RGBa").resize(size, Image.LANCZOS).convert("RGBA")
    hue, spread = a.tribe_hue / 360, a.hue_spread / 360
    px = small.load()
    for y in range(small.height):
        for x in range(small.width):
            r, g, b, al = px[x, y]
            if al < 128:
                px[x, y] = (0, 0, 0, 0)
                continue
            if is_tribe_colour(r, g, b, hue, spread):
                _, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
                r, g, b = (round(c * 255) for c in colorsys.hsv_to_rgb(MAGENTA_HUE, s, v))
            px[x, y] = (r, g, b, 255)
    canvas = Image.new("RGBA", (CANVAS, CANVAS))
    canvas.alpha_composite(small, (round(FEET[0] - fx * k), round(FEET[1] - fy * k)))
    canvas.save(a.out)


if __name__ == "__main__":
    main()
