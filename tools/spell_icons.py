"""Build target/spell-icons.html: for each spell of the panel, candidate icons from the original sprite banks
(the gold cursor icon first, then the HUD glyphs and the panel artwork), to pick the one its tile draws. The page
embeds the user's original art: never commit it.

uvx --with pillow python tools/spell_icons.py [install dir] [out.html]
"""

import base64
import html
import io
import json
import os
import struct
import sys

from PIL import Image

sys.path.insert(0, os.path.dirname(__file__))
from model_mapping import find_install, read  # noqa: E402

INSTALL = sys.argv[1] if len(sys.argv) > 1 else find_install()
OUT = sys.argv[2] if len(sys.argv) > 2 else "target/spell-icons.html"
SCALE = 3

# (name, cursor icon in POINT0-0.DAT: game-client virtual_cursor::spell_sprite, spell model), panel order
# (hud/spells.rs ROWS).
SPELLS = [
    ("Angel of Death", 55, 13), ("Volcano", 56, 16),
    ("Earthquake", 52, 14), ("Erosion", 50, 10), ("Firestorm", 54, 8),
    ("Whirlwind", 44, 4), ("Swamp", 53, 11), ("Flatten", 51, 15),
    ("Land Bridge", 48, 12), ("Lightning", 49, 3), ("Hypnotism", 47, 7),
    ("Swarm", 46, 5), ("Invisibility", 45, 6), ("Magical Shield", 39, 19),
    ("Ghost Army", 43, 9), ("Blast", 41, 2), ("Convert", 42, 17),
    ("Bloodlust", 38, 20), ("Teleport", 57, 21), ("Armageddon", 40, 18),
]

# The white hfx0-0.dat glyphs follow the spell models from 1058 (blast, model 2) on: a guess by eye, to confirm.
WHITE_GLYPH_BASE = 1056

# Candidate pictures: (bank file, first index, last index, what sprites.md says of them).
CANDIDATES = [
    ("POINT0-0.DAT", 38, 57, "gold cursor spell icons"),
    ("hfx0-0.dat", 1030, 1083, "spell and building glyphs"),
    ("hfx0-0.dat", 107, 111, "spell names"),
]


def palette(data_dir):
    raw = read(data_dir, "pal0-0.dat")
    return [tuple(raw[i * 4:i * 4 + 3]) for i in range(256)]


def sprites(data):
    """A PSFB bank (docs/specs/sprites.md): each sprite as (width, height, rows of palette indices or None)."""
    if data[:4] != b"PSFB":
        return []
    count = struct.unpack_from("<I", data, 4)[0]
    out = []
    for k in range(count):
        w, h, off = struct.unpack_from("<HHI", data, 8 + 8 * k)
        pixels = [[None] * w for _ in range(h)]
        p = off
        for y in range(h):
            x = 0
            while p < len(data):
                n = struct.unpack_from("<b", data, p)[0]
                p += 1
                if n == 0:
                    break
                if n < 0:
                    x -= n
                    continue
                for c in data[p:p + n]:
                    if x < w:
                        pixels[y][x] = c
                    x += 1
                p += n
        out.append((w, h, pixels))
    return out


def png(sprite, pal):
    w, h, pixels = sprite
    if w == 0 or h == 0:
        return None
    img = Image.new("RGBA", (w, h))
    img.putdata([(*pal[c], 255) if c is not None else (0, 0, 0, 0) for row in pixels for c in row])
    img = img.resize((w * SCALE, h * SCALE), Image.NEAREST)
    buf = io.BytesIO()
    img.save(buf, "PNG")
    return "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()


def main():
    data_dir = next((os.path.join(INSTALL, d) for d in os.listdir(INSTALL) if d.lower() == "data"), INSTALL)
    pal = palette(data_dir)
    banks = {}
    pictures = []
    for name, first, last, what in CANDIDATES:
        try:
            bank = banks.setdefault(name, sprites(read(data_dir, name)))
        except (FileNotFoundError, TypeError):
            continue
        for i in range(first, min(last, len(bank) - 1) + 1):
            src = png(bank[i], pal)
            if src:
                pictures.append({"id": f"{name}:{i}", "src": src, "what": what, "size": f"{bank[i][0]} x {bank[i][1]}"})
    spells = [{"name": s, "first": f"POINT0-0.DAT:{c}", "glyph": f"hfx0-0.dat:{WHITE_GLYPH_BASE + m}"} for s, c, m in SPELLS]
    page = f"""<!doctype html><meta charset="utf-8"><title>Spell icons</title>
<style>
body{{font-family:sans-serif;background:#2a1d0c;color:#f3dcae;margin:16px}}
h1{{font-size:20px}} h2{{font-size:15px;margin:14px 0 4px}}
.row{{display:flex;flex-wrap:wrap;gap:6px;padding-bottom:6px}}
.cand{{display:flex;flex-direction:column;align-items:center;justify-content:space-between;background:#cc8f33;color:#2d1a08;padding:4px;border:2px solid transparent;font-size:10px;cursor:pointer;flex:none;width:104px;height:124px;box-sizing:border-box}}
.cand.first{{outline:1px dashed #fff}} .cand.glyph{{outline:2px dotted #6cf}}
.cand input{{display:none}} .cand:has(input:checked){{border-color:#fff}}
img{{image-rendering:pixelated;display:block;max-width:96px;max-height:96px;object-fit:contain;margin:auto}}
textarea{{width:100%;height:160px;background:#1b1208;color:#f3dcae}}
</style>
<h1>Spell icons: pick one per spell (dashed: its cursor icon; dotted blue: the white glyph of its model, a guess)</h1>
<p>Candidates: {html.escape(", ".join(f"{n} {a}-{b} ({w})" for n, a, b, w in CANDIDATES))}. Embeds original art: never commit.</p>
<div id="spells"></div>
<h2>Choices</h2><textarea id="out" readonly></textarea>
<script>
const pictures = {json.dumps(pictures)};
const spells = {json.dumps(spells)};
const root = document.getElementById('spells');
for (const s of spells) {{
  const section = document.createElement('section');
  section.innerHTML = `<h2>${{s.name}}</h2>`;
  const row = document.createElement('div');
  row.className = 'row';
  const rank = p => p.id === s.first ? 0 : p.id === s.glyph ? 1 : 2;
  for (const p of [...pictures].sort((a, b) => rank(a) - rank(b))) {{
    const label = document.createElement('label');
    label.className = 'cand' + (p.id === s.first ? ' first' : p.id === s.glyph ? ' glyph' : '');
    label.title = `${{p.what}} (${{p.size}})`;
    label.innerHTML = `<input type="radio" name="${{s.name}}" value="${{p.id}}" ${{p.id === s.first ? 'checked' : ''}}><img src="${{p.src}}"><span>${{p.id}}</span>`;
    row.appendChild(label);
  }}
  section.appendChild(row);
  root.appendChild(section);
}}
function update() {{
  const picked = {{}};
  for (const s of spells) {{ const c = document.querySelector(`input[name="${{CSS.escape(s.name)}}"]:checked`); picked[s.name] = c ? c.value : null; }}
  document.getElementById('out').value = JSON.stringify(picked, null, 2);
}}
document.addEventListener('change', update); update();
</script>"""
    os.makedirs(os.path.dirname(OUT) or ".", exist_ok=True)
    with open(OUT, "w") as fh:
        fh.write(page)
    print(f"{OUT}: {len(SPELLS)} spells, {len(pictures)} candidates")


if __name__ == "__main__":
    main()
