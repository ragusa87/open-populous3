"""Build target/sprites.html: every original sprite we have identified (docs/specs/sprites.md, the code's constants),
with its name, bank file and index, searchable; the unnamed sprites of the small banks can be shown too. The page
embeds the user's original art: never commit it.

uvx --with pillow python tools/sprite_catalog.py [install dir] [out.html]
"""

import base64
import io
import json
import os
import sys

POINT = "POINT0-0.DAT"
HFX = "hfx0-0.dat"
SPELLS_BY_MODEL = ["Blast", "Lightning", "Whirlwind", "Swarm", "Invisibility", "Hypnotism", "Firestorm", "Ghost Army",
                   "Erosion", "Swamp", "Land Bridge", "Angel of Death", "Earthquake", "Flatten", "Volcano", "Convert"]
CURSOR_SPELLS = ["Bloodlust", "Magical Shield", "Armageddon", "Blast", "Convert", "Ghost Army", "Whirlwind",
                 "Invisibility", "Swarm", "Hypnotism", "Land Bridge", "Lightning", "Erosion", "Flatten", "Earthquake",
                 "Swamp", "Firestorm", "Angel of Death", "Volcano", "Teleport"]
BUILDINGS = ["hut", "drum tower", "warrior training", "firewarrior training", "temple", "spy training", "boat hut",
             "airship hut", "camp fire"]


def labels(file, group, first, names, ref=""):
    """One entry per name, from index `first` on."""
    return [(file, first + k, group, name, ref) for k, name in enumerate(names)]


def frames(file, group, first, last, name, ref=""):
    """A run of frames sharing a name, numbered."""
    return [(file, i, group, f"{name} {i - first + 1}/{last - first + 1}", ref) for i in range(first, last + 1)]


CATALOG = [
    *labels(POINT, "pointers", 13, ["arrow + ?"]),
    *labels(POINT, "pointers", 14, ["arrow (standard pointer)"], "sprites::POINTER_ARROW"),
    *labels(POINT, "pointers", 15, [f"arrow + {n}" for n in range(2, 9)]),
    *labels(POINT, "pointers", 27, ["arrow + up/down", "move cross", "rotate"]),
    *frames(POINT, "pointers", 30, 33, "gold arrow", "sprites::POINTER_GOLD_ARROW"),
    *labels(POINT, "cursor spell icons", 38, CURSOR_SPELLS, "virtual_cursor::spell_sprite"),
    *labels(POINT, "cursor building icons", 58, BUILDINGS, "virtual_cursor::building_sprite"),
    *frames(POINT, "action pointers", 80, 155, "action pointer"),
    *frames(POINT, "small icons", 156, 161, "small icon"),
    *labels(HFX, "map things", 2, ["dry plant", "dry plant"]),
    *labels(HFX, "map things", 22, ["ground shadow", "wood pile"], "catalog::WOOD_PILE_SPRITE"),
    *labels(HFX, "tooltip buttons", 46, ["rebuild", "rebuild pressed", "rebuild hovered",
                                         "dismantle", "dismantle pressed", "dismantle hovered"],
            "catalog::DISMANTLE_BUTTON / REBUILD_BUTTON"),
    *labels(HFX, "markers", 53, ["selected arrow"], "catalog::SELECTED_ARROW_SPRITE"),
    *labels(HFX, "tooltip buttons", 60, ["unload", "unload hovered", "unload pressed"], "catalog::UNLOAD_BUTTON"),
    *labels(HFX, "unit figures", 75, ["brave", "warrior", "preacher", "spy", "firewarrior", "shaman"],
            "catalog::UNIT_FIGURES"),
    *frames(HFX, "map things", 90, 93, "fire"),
    *frames(HFX, "HUD", 107, 111, "spell name"),
    *frames(HFX, "HUD", 187, 210, "build/wood action icon"),
    *frames(HFX, "effects", 346, 349, "volcano rock"),
    *frames(HFX, "HUD", 639, 652, "gold state icon"),
    *labels(HFX, "HUD", 653, ["boat", "boat outlined"]),
    *labels(HFX, "stats rows", 655, ["in a boat (flat boat)", "flat boat outlined"], "catalog::STATS_ROW_ICONS"),
    *labels(HFX, "unit figures", 664, [f"{u} {style}" for u in ["shaman (probably)", "brave", "warrior", "firewarrior",
                                                                "preacher", "spy"] for style in ["plain", "outlined"]]),
    *labels(HFX, "panel tabs", 676, ["build tab", "build tab highlighted", "spells tab", "spells tab highlighted"]),
    *labels(HFX, "panel tabs", 680, ["stats tab (people)", "stats tab highlighted"], "catalog::PEOPLE_ICON"),
    *labels(HFX, "HUD", 682, ["Magical Shield icon"]),
    *frames(HFX, "panel buildings", 1030, 1036, "coloured building picture"),
    *labels(HFX, "panel buildings", 1037, [f"{b} shadow" for b in BUILDINGS], "hud::build::SHADOWS"),
    *labels(HFX, "panel buildings", 1046, [f"{b} highlight" for b in BUILDINGS], "hud::build::HIGHLIGHTS"),
    *labels(HFX, "HUD", 1055, ["? to discover", "? to discover (small)"], "hud::build::UNKNOWN_ICON"),
    *labels(HFX, "panel spells", 1057, ["Armageddon", *SPELLS_BY_MODEL, "Teleport"], "hud::spells::panel_glyph"),
    *labels(HFX, "panel buildings", 1075, BUILDINGS, "hud::build::ICONS"),
    *labels(HFX, "stats rows", 1084, ["selected", "idle", "in a house", "working", "in a balloon"],
            "catalog::STATS_ROW_ICONS"),
    *frames(HFX, "effects", 1160, 1179, "rock-debris dust burst"),
    *frames(HFX, "effects", 1181, 1193, "fire cloud"),
    *frames(HFX, "effects", 1194, 1209, "volcano rock explosion"),
    *frames(HFX, "effects", 1210, 1224, "grey dust cloud"),
    *frames(HFX, "effects", 1225, 1239, "cream dust cloud (teleport landing)", "units::dust ORIGINAL_CLOUD"),
    *frames(HFX, "effects", 1241, 1247, "flash"),
    *frames(HFX, "effects", 1265, 1279, "flash"),
    *frames(HFX, "effects", 1330, 1345, "smoke column"),
    *frames(HFX, "markers", 1478, 1487, "bloodlust bubbles", "catalog::BLOODLUST_SPRITES"),
    *frames(HFX, "markers", 1489, 1498, "low health sparkles", "catalog::LOW_HEALTH_SPRITES"),
    *frames(HFX, "markers", 1538, 1552, "shield ring", "catalog::SHIELD_SPRITES"),
    *frames(HFX, "markers", 1553, 1563, "hypnotised halo", "catalog::HYPNOSIS_SPRITES"),
]

# Banks small enough to list whole (unnamed sprites hidden by default on the page).
WHOLE_BANKS = [POINT, HFX]
ALPHA_SPRITES = [range(1090, 1500), range(1538, 1593)]


def is_alpha(file, index):
    return file == HFX and any(index in r for r in ALPHA_SPRITES)


def alpha_colours(table, pal):
    """RGBA of each alpha pixel `tint << 4 | strength`: the tint's average full-strength colour, opacity strength / 15
    (pop3_format::blend::AlphaTable::rgba)."""
    tints = []
    for tint in range(16):
        row = (tint * 16 + 15) * 256
        sums = [sum(pal[table[row + bg]][k] for bg in range(256)) // 256 for k in range(3)]
        tints.append(tuple(sums))
    return [(*tints[p // 16], p % 16 * 255 // 15) for p in range(256)]


def rgba(pixels, colours):
    return [colours[c] if c is not None else (0, 0, 0, 0) for row in pixels for c in row]


def png(sprite, colours):
    from PIL import Image

    w, h, pixels = sprite
    if w == 0 or h == 0:
        return None
    img = Image.new("RGBA", (w, h))
    img.putdata(rgba(pixels, colours))
    buf = io.BytesIO()
    img.save(buf, "PNG")
    return "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()


PAGE = """<!doctype html><meta charset="utf-8"><title>Sprite catalog</title>
<style>
:root{--bg:#1d1a16;--fg:#eee3cf;--card:#2c2722;--mute:#a59783;--tile:#cc8f33}
body{font-family:sans-serif;background:var(--bg);color:var(--fg);margin:16px}
header{position:sticky;top:0;background:var(--bg);padding:8px 0;display:flex;flex-wrap:wrap;gap:10px;align-items:center;z-index:1}
input,select{background:var(--card);color:var(--fg);border:1px solid #555;padding:4px}
#grid{display:flex;flex-wrap:wrap;gap:6px}
.s{background:var(--card);width:150px;padding:6px;box-sizing:border-box;font-size:11px;cursor:pointer;display:flex;flex-direction:column;gap:3px}
.s.unnamed{opacity:.7}
.pic{display:flex;align-items:center;justify-content:center;min-height:60px;background:var(--bg)}
body.tile .pic{background:var(--tile)}
.pic img{image-rendering:pixelated}
.name{font-weight:bold} .meta,.ref{color:var(--mute)} .ref{font-family:monospace;font-size:10px;word-break:break-all}
</style>
<header>
<input id="q" placeholder="search name, file:index, group, ref" size="34">
<select id="file"><option value="">all files</option></select>
<select id="group"><option value="">all groups</option></select>
<label><input type="checkbox" id="unnamed"> unnamed</label>
<label><input type="checkbox" id="tile"> panel background</label>
<label>scale <input type="range" id="scale" min="1" max="6" value="3"></label>
<span id="count"></span>
</header>
<div id="grid"></div>
<script>
const SPRITES = __DATA__;
const $ = id => document.getElementById(id);
for (const [sel, key] of [[$('file'), 'file'], [$('group'), 'group']])
  for (const v of [...new Set(SPRITES.map(s => s[key]).filter(Boolean))].sort()) sel.add(new Option(v, v));
function render() {
  const q = $('q').value.toLowerCase(), file = $('file').value, group = $('group').value, scale = +$('scale').value;
  const shown = SPRITES.filter(s => (s.name || $('unnamed').checked) && (!file || s.file === file) && (!group || s.group === group)
    && (!q || [s.name, s.group, s.ref, `${s.file}:${s.index}`].join(' ').toLowerCase().includes(q)));
  $('grid').innerHTML = shown.map(s => `<div class="s${s.name ? '' : ' unnamed'}" data-id="${s.file}:${s.index}" title="click to copy">
    <div class="pic">${s.src ? `<img src="${s.src}" width="${s.w * scale}" height="${s.h * scale}">` : 'empty'}</div>
    <div class="name">${s.name || '(unnamed)'}</div><div class="meta">${s.file}:${s.index} · ${s.w}x${s.h}${s.alpha ? ' · alpha' : ''}</div>
    <div class="meta">${s.group || ''}</div><div class="ref">${s.ref || ''}</div></div>`).join('');
  $('count').textContent = `${shown.length} / ${SPRITES.length}`;
}
for (const id of ['q', 'file', 'group', 'unnamed', 'scale']) $(id).addEventListener('input', render);
$('tile').addEventListener('input', e => document.body.classList.toggle('tile', e.target.checked));
$('grid').addEventListener('click', e => { const c = e.target.closest('.s'); if (c) navigator.clipboard?.writeText(c.dataset.id); });
render();
</script>"""


def page(entries):
    return PAGE.replace("__DATA__", json.dumps(entries))


def main():
    from model_mapping import find_install, read
    from spell_icons import sprites

    install = sys.argv[1] if len(sys.argv) > 1 else find_install()
    out = sys.argv[2] if len(sys.argv) > 2 else "target/sprites.html"
    data_dir = next((os.path.join(install, d) for d in os.listdir(install) if d.lower() == "data"), install)
    raw = read(data_dir, "pal0-0.dat")
    pal = [tuple(raw[i * 4:i * 4 + 3]) for i in range(256)]
    opaque = [(*c, 255) for c in pal]
    alpha = alpha_colours(read(data_dir, "al0-0.dat"), pal)
    named = {(f, i): (g, n, r) for f, i, g, n, r in CATALOG}
    entries = []
    for file in WHOLE_BANKS:
        bank = sprites(read(data_dir, file))
        for i, sprite in enumerate(bank):
            group, name, ref = named.get((file, i), ("", "", ""))
            entries.append({"file": file, "index": i, "group": group, "name": name, "ref": ref, "w": sprite[0],
                            "h": sprite[1], "alpha": is_alpha(file, i),
                            "src": png(sprite, alpha if is_alpha(file, i) else opaque)})
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w") as fh:
        fh.write(page(entries))
    print(f"{out}: {sum(1 for e in entries if e['name'])} named sprites, {len(entries)} in all")


if __name__ == "__main__":
    sys.path.insert(0, os.path.dirname(__file__))
    main()
