"""Build target/model-mapping.html: every original 3D object (bank 0) rendered as a thumbnail,
with a select to map it to a game item. The page embeds the user's original art: never commit it.

uvx --with pillow python tools/model_mapping.py [install dir] [out.html]
"""

import base64
import io
import json
import math
import os
import struct
import sys

from PIL import Image, ImageDraw

INSTALL = sys.argv[1] if len(sys.argv) > 1 else os.environ.get(
    "POP3_INSTALL",
    os.path.expanduser("~/.wine/drive_c/Program Files (x86)/Bullfrog/Populous - A l'aube de la création"),
)
OUT = sys.argv[2] if len(sys.argv) > 2 else "target/model-mapping.html"
THUMB = 160

# (kind, model, name): PopTB editor naming, unverified.
ITEMS = [
    *[(5, m, n) for m, n in enumerate(
        ["Tree 1", "Tree 2", "Tree 3", "Tree 4", "Tree 5", "Tree 6", "Plant 1", "Plant 2", "Stone head",
         "Fire", "Wood pile", "RS pillar", "Rock", "Portal", "Island", "Bridge", "Dormant tree",
         "Top level scenery", "Sub level scenery"], start=1)],
    *[(2, m, n) for m, n in enumerate(
        ["Hut small", "Hut medium", "Hut large", "Drum tower", "Temple", "Spy training hut",
         "Warrior training hut", "Firewarrior training hut", "Reconversion", "Wall piece", "Gate",
         "Curr OE slot", "Boat hut 1", "Boat hut 2", "Airship hut 1", "Airship hut 2", "Guard post",
         "Vault of knowledge", "Prison"], start=1)],
    *[(3, m, n) for m, n in enumerate(["Bear", "Buffalo", "Wolf", "Eagle", "Rabbit", "Beaver", "Fish"], start=1)],
    *[(4, m, n) for m, n in enumerate(["Boat 1", "Boat 2", "Airship 1", "Airship 2"], start=1)],
    (0, 1, "Reincarnation site platform"),
    (0, 2, "Reincarnation site totem"),
]
KINDS = {0: "Special (not a level thing)", 2: "Building", 3: "Creature", 4: "Vehicle", 5: "Scenery"}

# object index -> (kind, model): Claude's visual guesses, to be corrected by hand.
GUESSES = {
    13: (5, 1), 14: (5, 2), 15: (5, 3), 16: (5, 4), 60: (5, 5), 63: (5, 6),
    0: (5, 11), 76: (5, 12), 82: (5, 9), 79: (5, 13), 81: (5, 14),
    129: (2, 1), 133: (2, 3), 117: (2, 4), 141: (2, 5), 137: (2, 7), 190: (2, 17),
    121: (2, 13), 125: (2, 15),
    181: (4, 1), 182: (4, 3), 100: (3, 4),
    32: (0, 1), 187: (0, 2),
}


def read(folder, name):
    for f in os.listdir(folder):
        if f.lower() == name.lower():
            with open(os.path.join(folder, f), "rb") as fh:
                return fh.read()
    raise FileNotFoundError(name)


def load_objects(folder, bank=0):
    objs, pnts, facs = (read(folder, f"{p}0-{bank}.dat") for p in ("objs", "pnts", "facs"))
    points = [struct.unpack_from("<hhh", pnts, i * 6) for i in range(len(pnts) // 6)]
    out = []
    for i in range(len(objs) // 54):
        fs, fe, ps, pe = struct.unpack_from("<iiii", objs, i * 54 + 16)
        pts = points[ps - 1:pe - 1] if ps > 0 else []
        faces = []
        for k in range(max(fs - 1, 0), max(fe - 1, 0)):
            r = facs[k * 60:k * 60 + 60]
            n = r[6]
            if n not in (3, 4):
                continue
            idx = struct.unpack_from("<4H", r, 40)[:n]
            if max(idx) >= len(pts):
                continue
            uv = [struct.unpack_from("<ii", r, 8 + j * 8) for j in range(n)]
            faces.append((r[0], struct.unpack_from("<H", r, 2)[0], idx, uv))
        out.append((pts, faces))
    return out


def render(pts, faces, atlas, palette, size):
    big = size * 2
    img = Image.new("RGB", (big, big), (34, 36, 48))
    draw = ImageDraw.Draw(img)
    ya, xa = math.radians(35), math.radians(28)
    light = (0.4, 0.8, -0.45)

    def proj(p):
        x, y, z = p
        x, z = x * math.cos(ya) - z * math.sin(ya), x * math.sin(ya) + z * math.cos(ya)
        y, z = y * math.cos(xa) - z * math.sin(xa), y * math.sin(xa) + z * math.cos(xa)
        return x, -y, z

    pp = [proj(p) for p in pts]
    xs, ys = [p[0] for p in pp], [p[1] for p in pp]
    s = (big - 16) / max(1, max(xs) - min(xs), max(ys) - min(ys))
    ox = (big - (max(xs) - min(xs)) * s) / 2 - min(xs) * s
    oy = (big - (max(ys) - min(ys)) * s) / 2 - min(ys) * s
    polys = []
    for colour, tile, idx, uv in faces:
        q = [pp[i] for i in idx]
        a, b, c = (pts[i] for i in idx[:3])
        u = [b[k] - a[k] for k in range(3)]
        v = [c[k] - a[k] for k in range(3)]
        n = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
        ln = math.sqrt(sum(x * x for x in n)) or 1
        shade = 0.45 + 0.55 * abs(sum(n[k] / ln * light[k] for k in range(3)))
        dst = [(ox + p[0] * s, oy + p[1] * s) for p in q]
        polys.append((sum(p[2] for p in q) / len(q), dst, colour, tile, uv, shade))
    for _, dst, colour, tile, uv, shade in sorted(polys, key=lambda t: -t[0]):
        if tile == 0xFFFF:
            col = tuple(int(c * shade) for c in palette[colour])
            draw.polygon(dst, fill=col)
            continue
        tris = [(0, 1, 2)] + ([(0, 2, 3)] if len(dst) == 4 else [])
        for t in tris:
            textured_triangle(img, atlas, [dst[k] for k in t], [uv[k] for k in t], tile, shade)
    return img.resize((size, size), Image.LANCZOS)


def textured_triangle(img, atlas, dst, uv, tile, shade):
    tx, ty = (tile % 8) * 32, (tile // 8) * 32
    (x0, y0), (x1, y1), (x2, y2) = dst
    src = [(tx + min(max(u / 65536, 0), 31.99), ty + min(max(v / 65536, 0), 31.99)) for u, v in uv]
    det = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
    if abs(det) < 1e-6:
        return

    def solve(a0, a1, a2):
        a = ((a1 - a0) * (y2 - y0) - (a2 - a0) * (y1 - y0)) / det
        b = ((x1 - x0) * (a2 - a0) - (x2 - x0) * (a1 - a0)) / det
        return a, b, a0 - a * x0 - b * y0

    au, av = solve(*(p[0] for p in src)), solve(*(p[1] for p in src))
    xs, ys = [p[0] for p in dst], [p[1] for p in dst]
    box = (int(min(xs)), int(min(ys)), int(max(xs)) + 2, int(max(ys)) + 2)
    w, h = box[2] - box[0], box[3] - box[1]
    if w <= 0 or h <= 0:
        return
    coeffs = (au[0], au[1], au[2] + au[0] * box[0] + au[1] * box[1],
              av[0], av[1], av[2] + av[0] * box[0] + av[1] * box[1])
    patch = atlas.transform((w, h), Image.AFFINE, coeffs, resample=Image.NEAREST)
    patch = patch.point(lambda c: int(c * shade))
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).polygon([(x - box[0], y - box[1]) for x, y in dst], fill=255)
    img.paste(patch, box[:2], mask)


def main():
    data = os.path.join(INSTALL, "data")
    pal = read(data, "pal0-0.dat")
    palette = [tuple(pal[i * 4:i * 4 + 3]) for i in range(256)]
    atlas = Image.frombytes("P", (256, 1024), read(data, "bl320-0.dat")[:256 * 1024])
    atlas.putpalette([c for p in palette for c in p])
    atlas = atlas.convert("RGB")
    models = []
    for i, (pts, faces) in enumerate(load_objects(os.path.join(INSTALL, "objects"))):
        if not faces:
            continue
        buf = io.BytesIO()
        render(pts, faces, atlas, palette, THUMB).save(buf, "PNG", optimize=True)
        guess = GUESSES.get(i)
        models.append({
            "object": i,
            "faces": len(faces),
            "png": base64.b64encode(buf.getvalue()).decode(),
            "guess": f"{guess[0]}:{guess[1]}" if guess else "",
        })
    items = [{"key": f"{k}:{m}", "kind": k, "model": m, "name": n, "group": KINDS[k]} for k, m, n in ITEMS]
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "model_mapping.html")) as fh:
        page = fh.read()
    page = page.replace("/*DATA*/", f"const MODELS = {json.dumps(models)};\nconst ITEMS = {json.dumps(items)};")
    os.makedirs(os.path.dirname(OUT) or ".", exist_ok=True)
    with open(OUT, "w") as fh:
        fh.write(page)
    print(f"{OUT}: {len(models)} models, {len(items)} items, {len(page) // 1024} KiB")


if __name__ == "__main__":
    main()
