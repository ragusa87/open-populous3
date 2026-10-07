"""Surface details for the generated building kit, made without image inputs.

One shared sRGB modulation atlas: pigments stay in the glTF material factors so
Tribe cloth can be recoloured independently. PNG encoder uses only the standard library.
"""

import math
import struct
import zlib

# Material name -> sRGB pigment. Order is the atlas tile and the glTF material index.
PALETTE = {
    "Clay": "c89162", "Wood": "68452e", "Timber": "aa7847",
    "Thatch": "d7b66d", "ThatchShade": "b58b4e", "Stone": "8d9694",
    "Dark": "302b30", "Tribe": "2659f2", "Linen": "eee0b5", "Ember": "ef8135",
}
MATERIALS = tuple(PALETTE)
SIDE, TILE, GUTTER = 512, 128, 8
INNER = TILE - 2 * GUTTER


def noise(x, y, seed=0):
    n = ((x * 374761393 + y * 668265263 + seed * 982451653) ^ 0x59AC3) & 0xffffffff
    n = ((n ^ (n >> 13)) * 1274126177) & 0xffffffff
    return ((n ^ (n >> 16)) & 65535) / 65535


def shade(material, x, y):
    """Broad hand-painted-style details, with fine grain kept low contrast."""
    grit = (noise(x, y) - .5) * .055
    if material in ("Thatch", "ThatchShade"):
        course = y // 28
        strand = (x + course * 3 + int(1.5 * math.sin(y * .12))) % 9
        cut = y % 28
        value = .87 + .07 * noise(x // 9, course, 3)
        value -= .19 if strand < 2 else 0
        value += .045 if strand == 2 else 0
        value -= .19 if cut < 3 else .07 if cut < 6 else 0
    elif material in ("Wood", "Timber", "Dark"):
        grain = (x + 2.5 * math.sin(y * .045 + x * .06)) % 17
        knot = math.sqrt(((x - 70) * .65)**2 + ((y - 64) * .22)**2)
        value = .88 + .05 * math.sin(x * .13)
        value -= .22 if grain < 1.6 else .08 if grain < 3 else 0
        if knot < 13:
            value -= .16 * (.5 + .5 * math.sin(knot * 2.1))
        if material == "Timber" and x % 37 < 2:
            value -= .15  # sawn board joints
    elif material == "Stone":
        row = y // 28
        bx, by = (x + (row % 2) * 23) % 46, y % 28
        value = .84 + .1 * noise((x + (row % 2) * 23) // 46, row, 4)
        if bx < 3 or by < 3:
            value = .48
        elif bx < 5 or by < 5:
            value = .98
        elif bx > 43 or by > 25:
            value -= .1
        value += .035 * math.sin(x * .31 + y * .43)
    elif material == "Clay":
        value = .91 + .035 * math.sin(x * .11) * math.sin(y * .17)
        value -= .1 if noise(x, y, 9) > .98 else 0
        # Occasional short hairline cracks in sun-dried plaster.
        if 24 < y < 58 and abs(x - (37 + 2 * math.sin(y * .24))) < .8:
            value -= .19
    elif material in ("Tribe", "Linen"):
        value = .94 - (.06 if x % 4 == 0 else 0) - (.05 if y % 4 == 0 else 0)
        # Woven border rather than coloured paint: ownership tint stays pure.
        if material == "Tribe" and (y < 9 or y > INNER - 10):
            value -= .23 if (x // 5 + y // 3) % 2 == 0 else .09
    else:  # static ember surfaces, not a new fire animation
        value = .87 + .08 * math.sin(x * .13 + math.sin(y * .09))
    return round(max(.25, min(1, value + grit)) * 255)


def atlas_png():
    pixels = bytearray()
    for y in range(SIDE):
        pixels.append(0)  # PNG filter: None
        for x in range(SIDE):
            index = (y // TILE) * 4 + x // TILE
            if index < len(MATERIALS):
                # Extruded edge pixels prevent neighbouring tiles bleeding with linear filtering.
                tx = min(INNER - 1, max(0, x % TILE - GUTTER))
                ty = min(INNER - 1, max(0, y % TILE - GUTTER))
                v = shade(MATERIALS[index], tx, ty)
            else:
                v = 255
            pixels.extend((v, v, v, 255))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIDE, SIDE, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels, 9)) + chunk(b"IEND", b""))


def png_pixels(png):
    """Raw filtered scanlines of a PNG written by `atlas_png`, to compare content not encoding."""
    pos, idat = 8, b""
    while pos < len(png):
        length, kind = struct.unpack(">I4s", png[pos:pos + 8])
        if kind == b"IDAT":
            idat += png[pos + 8:pos + 8 + length]
        pos += 12 + length
    return png[8:33], zlib.decompress(idat)


def atlas_uv(material, u, v):
    tile = MATERIALS.index(material)
    return (((tile % 4) * TILE + GUTTER + .5 + u * (INNER - 1)) / SIDE,
            ((tile // 4) * TILE + GUTTER + .5 + v * (INNER - 1)) / SIDE)
