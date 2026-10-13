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

# (name, cursor icon in POINT0-0.DAT: game-client virtual_cursor::spell_sprite), panel order (hud/spells.rs ROWS).
SPELLS = [
    ("Angel of Death", 55), ("Volcano", 56),
    ("Earthquake", 52), ("Erosion", 50), ("Firestorm", 54),
    ("Whirlwind", 44), ("Swamp", 53), ("Flatten", 51),
    ("Land Bridge", 48), ("Lightning", 49), ("Hypnotism", 47),
    ("Swarm", 46), ("Invisibility", 45), ("Magical Shield", 39),
    ("Ghost Army", 43), ("Blast", 41), ("Convert", 42),
    ("Bloodlust", 38), ("Teleport", 57), ("Armageddon", 40),
]

# Candidate pictures: (bank file, first index, last index, what sprites.md says of them).
CANDIDATES = [
    ("POINT0-0.DAT", 38, 57, "gold cursor spell icons"),
    ("hfx0-0.dat", 1030, 1083, "spell and building glyphs"),
    ("hfx0-0.dat", 107, 111, "spell names"),
    ("plspanel.spr", 0, 28, "panel artwork"),
    ("plsspace.spr", 0, 13, "panel artwork"),
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
                pictures.append({"id": f"{name}:{i}", "src": src, "what": what})
    spells = [{"name": s, "first": f"POINT0-0.DAT:{c}"} for s, c in SPELLS]
    page = f"""<!doctype html><meta charset="utf-8"><title>Spell icons</title>
<style>
body{{font-family:sans-serif;background:#2a1d0c;color:#f3dcae;margin:16px}}
h1{{font-size:20px}} h2{{font-size:15px;margin:14px 0 4px}}
.row{{display:flex;gap:6px;overflow-x:auto;padding-bottom:6px}}
.cand{{display:flex;flex-direction:column;align-items:center;background:#cc8f33;color:#2d1a08;padding:4px;border:2px solid transparent;font-size:10px;cursor:pointer;flex:none}}
.cand.first{{outline:1px dashed #fff}}
.cand input{{display:none}} .cand:has(input:checked){{border-color:#fff}}
img{{image-rendering:pixelated;display:block}}
textarea{{width:100%;height:160px;background:#1b1208;color:#f3dcae}}
</style>
<h1>Spell icons: pick one per spell (the dashed one is its cursor icon)</h1>
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
  for (const p of [...pictures].sort((a, b) => (b.id === s.first) - (a.id === s.first))) {{
    const label = document.createElement('label');
    label.className = 'cand' + (p.id === s.first ? ' first' : '');
    label.title = p.what;
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
