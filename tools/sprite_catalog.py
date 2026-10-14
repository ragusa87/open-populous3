"""Build target/sprites.html: every original sprite we have identified (docs/specs/sprites.md, the code's constants),
with its name, bank file and index, searchable; the unnamed sprites of the small banks can be shown too. Also every
person animation (docs/specs/animations.md, `pop3_format::anim`), composed from `VSTART`/`VFRA`/`VELE` over
`HSPR0-0.DAT`, one per kind (outfit) that actually looks different from the plain body. The page embeds the user's
original art: never commit it.

uvx --with pillow python tools/sprite_catalog.py [install dir] [out.html]
"""

import base64
import io
import json
import os
import re
import struct
import sys

POINT = "POINT0-0.DAT"
HFX = "hfx0-0.dat"
HSPR = "HSPR0-0.DAT"
VSTART = "VSTART-0.ANI"
VFRA = "VFRA-0.ANI"
VELE = "VELE-0.ANI"
PERSON_ANIM = "person animations"
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


def series(file, group, first, last, name, ref=""):
    """A run of separate sprites sharing a name, not an animation."""
    return [(file, i, group, f"{name} #{i - first + 1}", ref) for i in range(first, last + 1)]


# docs/specs/animations.md "Identified animations": tribesman body (braves, warriors, firewarriors, spies,
# preachers; `pop3_format::catalog::PersonAnim`), shared across outfits.
PERSON_ANIM_NAMES = {
    5: "walk", 6: "idle, standing breathing", 7: "firewarrior throwing fire (attack)",
    8: "kneeling, praying (worship)", 9: "walking with wood (brave only)",
    10: "standing holding wood (brave only)", 11: "cutting wood with an axe (brave only)",
    12: "flattening jump (brave only); stranded (frame 2, arms up)",
    13: "punch: trading blows", 14: "receiving a punch", 15: "punch, left foot forward (opening blow)",
    16: "tilted (struck by the opening blow, before flung)",
    17: "spy setting fire to a building",
    18: "sitting down, being converted by a preacher",
    19: "tumbling in the air, arms spread",
    20: "preaching: raises a held book", 21: "preaching: holds the book across the chest",
    22: "preaching: gestures with the free arm while reading", 23: "preaching: both arms up, book held high",
    24: "preaching: lowers the book, open arms",
    25: "kick",
    37: "falling down a slope, tumbling on the ground",
    38: "dying, falls onto its back",
    39: "lying dead",
    40: "spirit rising (drowning, or after the death fall)",
    48: "taunt: hiding its eyes", 49: "taunt: waving an arm", 50: "taunt: crouch, then punch",
    51: "pedalling a vehicle", 52: "running away (panic; bitten by a swarm)",
    89: "idle too long: scratching itself (brave)",
    90: "idle too long: push-ups (firewarrior gear)",
    91: "idle too long: push-ups (warrior headband)",
    92: "idle too long: juggling (spy)",
    97: "carbonized, struck by lightning (brave)", 98: "carbonized (shaman; probably unused)",
}
# Old, unused shaman body (blue headdress, staff): docs/specs/animations.md, below the identified table.
OLD_SHAMAN_ANIM_NAMES = {
    26: "old shaman: standing (unused)", 27: "old shaman: walk (unused)", 28: "old shaman: casting a spell (unused)",
    29: "old shaman: punch (unused)", 30: "old shaman: flying in a tornado (unused)",
    32: "old shaman: another spell (unused)", 33: "old shaman: staff gesture (unused)",
    34: "old shaman: staff gesture (unused)", 35: "old shaman: staff gesture (unused)",
    36: "old shaman: staff gesture (unused)", 41: "old shaman: staff gesture (unused)",
    42: "old shaman: kick (unused)", 43: "old shaman: falling, death (unused)",
    44: "old shaman: lying dead (unused)", 45: "old shaman: dying, burning (unused)",
    46: "old shaman: tumbling (unused)",
}
# docs/specs/animations.md's shaman table: 4 consecutive indices per action, one per tribe, not layered
# (`pop3_format::catalog::ShamanAnim` only wires up the ones the client actually plays, not `wand`).
SHAMAN_ANIM_BASES = {53: "shaman: idle", 57: "shaman: staff strike", 61: "shaman: flying (whirlwind)",
                     65: "shaman: casting a spell", 69: "shaman: kick", 73: "shaman: tumbling down a slope",
                     77: "shaman: walk", 81: "shaman: moving the magic wand", 85: "shaman: fall (death)",
                     93: "shaman: kneeling (praying)"}
TRIBES = 4
# Wildmen (no layers, no tribe variants): docs/specs/animations.md, below the identified table
# (`pop3_format::catalog::WildmanAnim` only wires up the ones the client actually plays).
WILDMAN_ANIM_NAMES = {0: "wildman: walk", 1: "wildman: standing", 2: "wildman: eating fruit at a tree",
                      3: "wildman: drinking water, still (used for praying)",
                      4: "wildman: drinking water, animated",
                      31: "wildman: flung, arms spread (also drowning)",
                      47: "wildman: falling down a slope (also dying)"}

# `pop3_format::anim::Outfit` over the shared tribesman body: braves wear none.
OUTFITS = {"brave (no outfit)": None, "firewarrior": (0x20, 1), "warrior": (0x20, 2), "spy": (0x20, 3),
           "preacher": (0x30, 1)}
FLAG_FLIP, FLAG_SHADOW, FLAG_TRIBE_LAYER, FLAG_OUTFIT = 0x0001, 0x0004, 0x0010, 0x0020
MAX_ANIM_FRAMES = 64


def anim_name(index):
    """The best name we have for a `VSTART` animation index, or None."""
    if index in PERSON_ANIM_NAMES:
        return PERSON_ANIM_NAMES[index]
    if index in OLD_SHAMAN_ANIM_NAMES:
        return OLD_SHAMAN_ANIM_NAMES[index]
    if index in WILDMAN_ANIM_NAMES:
        return WILDMAN_ANIM_NAMES[index]
    for base, name in SHAMAN_ANIM_BASES.items():
        if base <= index < base + TRIBES:
            tribe = index - base
            return name if tribe == 0 else f"{name} (tribe {tribe})"
    return None


def parse_starts(data):
    """`VSTART-0.ANI`: per animation, 8 directions x (u16 frame, u16 mirror source != 0 = mirrored)."""
    per_anim = 4 * 8
    return [[struct.unpack_from("<HH", data, i * per_anim + d * 4) for d in range(8)] for i in range(len(data) // per_anim)]


def parse_frames(data):
    """`VFRA-0.ANI`: 8 bytes, u16 first element, u8 w, u8 h (unused), u16 flags (unused), u16 next frame."""
    return [struct.unpack_from("<HBBHH", data, i * 8) for i in range(len(data) // 8)]


def parse_elements(data):
    """`VELE-0.ANI`: 10 bytes, u16 (sprite index + 1) x 6 (0 = none), i16 x, i16 y, u16 flags, u16 next element."""
    out = []
    for i in range(len(data) // 10):
        raw_sprite, x, y, flags, nxt = struct.unpack_from("<HhhHH", data, i * 10)
        out.append((raw_sprite // 6 - 1 if raw_sprite else None, x, y, flags, nxt))
    return out


def frame_loop(starts, frames, anim, direction):
    """Frame indices of one loop, following `next` until it comes back (or breaks)."""
    if anim >= len(starts):
        return []
    first = starts[anim][direction % 8][0]
    if first == 0:
        return []
    out, f = [first], first
    while f < len(frames):
        nxt = frames[f][4]
        if nxt == 0 or nxt == first or len(out) >= MAX_ANIM_FRAMES:
            break
        out.append(nxt)
        f = nxt
    return out


def frame_elements(frames, elements, frame):
    """Elements drawn for a frame, back to front."""
    out = []
    e = frames[frame][0] if frame < len(frames) else 0
    while e != 0 and e < len(elements) and len(out) < MAX_ANIM_FRAMES:
        el = elements[e]
        out.append(el)
        e = el[4]
    return out


def element_shown(flags, tribe, outfit):
    """Whether an element is drawn for a person of `tribe` wearing `outfit`: never shadows; outfit layers only for
    that outfit; tribe layers only for that tribe; everything else always."""
    bits = (flags >> 9) & 3
    if flags & FLAG_SHADOW:
        return False
    if flags & FLAG_OUTFIT:
        return outfit is not None and outfit == (flags & (FLAG_TRIBE_LAYER | FLAG_OUTFIT), bits)
    if flags & FLAG_TRIBE_LAYER:
        return bits == tribe
    return True


def compose_frame(starts, frames, elements, sprite_bank, anim, direction, tribe, outfit):
    """The person as a unit of `tribe` wearing `outfit`, one composed frame per `frame_loop` index: palette indices
    (rows of rows, `None` = transparent), the feet at `origin`. None if the loop or every element is empty."""
    mirrored = starts[anim][direction % 8][1] != 0 if anim < len(starts) else False
    out = []
    for frame in frame_loop(starts, frames, anim, direction):
        parts = []
        for sprite, x, y, flags, _next in frame_elements(frames, elements, frame):
            if sprite is None or not element_shown(flags, tribe, outfit) or sprite >= len(sprite_bank):
                continue
            sw, sh, pixels = sprite_bank[sprite]
            if sw and sh:
                parts.append((x, y, flags, sw, sh, pixels))
        x0 = min([0, *(x for x, *_ in parts)])
        y0 = min([0, *(y for _, y, *_ in parts)])
        x1 = max([0, *(x + sw for x, _, _, sw, _, _ in parts)])
        y1 = max([0, *(y + sh for _, y, _, _, sh, _ in parts)])
        w, h = max(x1 - x0, 1), max(y1 - y0, 1)
        pixels = [[None] * w for _ in range(h)]
        for x, y, flags, sw, sh, spixels in parts:
            flip = bool(flags & FLAG_FLIP)
            for sy in range(sh):
                for sx in range(sw):
                    c = spixels[sy][sw - 1 - sx if flip else sx]
                    if c is not None:
                        pixels[y - y0 + sy][x - x0 + sx] = c
        if mirrored:
            pixels = [row[::-1] for row in pixels]
        out.append((w, h, pixels))
    return out


CATALOG = [
    *labels(POINT, "pointers", 13, ["arrow + ?"]),
    *labels(POINT, "pointers", 14, ["arrow (standard pointer)"], "sprites::POINTER_ARROW"),
    *labels(POINT, "pointers", 15, [f"arrow + {n} (units selected)" for n in range(2, 9)]),
    *labels(POINT, "pointers", 27, ["arrow + up/down", "move cross", "rotate"]),
    *frames(POINT, "pointers", 30, 33, "gold arrow", "sprites::POINTER_GOLD_ARROW"),
    *labels(POINT, "cursor spell icons", 38, CURSOR_SPELLS, "virtual_cursor::spell_sprite"),
    *labels(POINT, "cursor building icons", 58, BUILDINGS, "virtual_cursor::building_sprite"),
    *frames(POINT, "action pointers", 80, 83, "action pointer"),
    *frames(POINT, "action pointers", 84, 89, "fight"),
    *frames(POINT, "action pointers", 90, 93, "send into a house"),
    *frames(POINT, "action pointers", 94, 99, "cut wood"),
    *frames(POINT, "action pointers", 100, 105, "spy sets a building on fire"),
    *frames(POINT, "action pointers", 106, 111, "build (with wood)"),
    *frames(POINT, "action pointers", 112, 115, "board a balloon"),
    *frames(POINT, "action pointers", 116, 119, "board a boat"),
    *frames(POINT, "action pointers", 120, 125, "dismantle"),
    *frames(POINT, "action pointers", 126, 131, "pray"),
    *frames(POINT, "action pointers", 132, 137, "spy disguise"),
    *frames(POINT, "action pointers", 138, 143, "guard (patrol)"),
    *frames(POINT, "action pointers", 144, 147, "go into a building (pyramid?)"),
    *frames(POINT, "action pointers", 148, 151, "pick up cut wood"),
    *frames(POINT, "action pointers", 152, 155, "grab"),
    *labels(POINT, "small icons", 156, ["bridge variant", "lowered ground variant", "bridge variant 2",
                                        "lowered ground variant 2", "person", "tree"]),
    *labels(HFX, "map things", 2, ["dry plant", "dry plant"]),
    *labels(HFX, "map things", 22, ["ground shadow", "wood pile"], "catalog::WOOD_PILE_SPRITE"),
    *labels(HFX, "tooltip", 40, ["piece of wood"], "catalog::TOOLTIP_WOOD_SPRITE"),
    *labels(HFX, "tooltip buttons", 46, ["rebuild", "rebuild pressed", "rebuild hovered",
                                         "dismantle", "dismantle pressed", "dismantle hovered"],
            "catalog::DISMANTLE_BUTTON / REBUILD_BUTTON"),
    *labels(HFX, "markers", 53, ["selected arrow"], "catalog::SELECTED_ARROW_SPRITE"),
    *labels(HFX, "aerial map", 59, ["combat"]),
    *labels(HFX, "tooltip buttons", 60, ["unload", "unload hovered", "unload pressed"], "catalog::UNLOAD_BUTTON"),
    *frames(HFX, "map things", 69, 74, "ground shadow (balloon, flying unit)"),
    *labels(HFX, "unit figures", 75, ["brave", "warrior", "preacher", "spy", "firewarrior", "shaman"],
            "catalog::UNIT_FIGURES"),
    *labels(HFX, "aerial map", 81, ["swamp (RIP tombstone)"]),
    *frames(HFX, "map things", 90, 93, "fire"),
    *series(HFX, "HUD", 107, 111, "spell name"),
    *labels(HFX, "HUD", 112, ["selection count 18", "selection count 19", "selection count 20"]),
    *labels(HFX, "aerial map", 115, ["hut (empty)", "warrior training", "firewarrior training", "temple",
                                     "spy training", "drum tower", "pyramid", "totem",
                                     "totem (stone animation)", "totem (moving head)"]),
    *labels(HFX, "aerial map", 125, [f"{t} boat" for t in ("blue", "red", "yellow", "green")]),
    *labels(HFX, "aerial map", 129, [f"{t} balloon" for t in ("blue", "red", "yellow", "green")]),
    *labels(HFX, "aerial map", 133, ["drum tower (probably)"]),
    *labels(HFX, "aerial map", 134, [f"3-room hut, {n} inside" for n in range(4)]),
    *labels(HFX, "aerial map", 138, [f"4-room hut, {n} inside" for n in range(5)]),
    *labels(HFX, "aerial map", 143, [f"5-room hut, {n} inside" for n in range(6)]),
    *labels(HFX, "aerial map", 149, ["prison", "winged death totem"]),
    *labels(HFX, "building icons", 163, ["drum tower", "warrior training", "firewarrior training", "temple",
                                         "spy training", "boat hut", "airship hut", "camp fire"]),
    *labels(HFX, "HUD", 171, ["help (book)", "prayer (coloured)", "info (bubble with !)", "info (i)", "blue flag"]),
    *labels(HFX, "HUD", 178, ["firewarrior (coloured)", "preacher (coloured)"]),
    *labels(HFX, "digits", 219, [f"digit {n} (6 x 9)" for n in range(10)]),
    *labels(HFX, "digits", 229, [f"digit {n} (7 x 9)" for n in range(10)]),
    *frames(HFX, "HUD", 187, 210, "build/wood action icon"),
    *frames(HFX, "effects", 346, 349, "volcano rock"),
    *series(HFX, "spell icons", 354, 371, "spell icon, purple (hovered?)"),
    *series(HFX, "spell icons", 372, 389, "spell icon"),
    *series(HFX, "spell icons", 390, 407, "spell icon, lighter"),
    *series(HFX, "spell icons", 408, 410, "shield fist, colour variant"),
    *series(HFX, "spell icons", 411, 413, "bloodlust skull"),
    *labels(HFX, "spell icons", 414, ["shield fist, colour variant"]),
    *labels(HFX, "spell icons", 415, ["bloodlust skull"]),
    *series(HFX, "spell icons", 416, 419, "shield fist, colour variant"),
    *series(HFX, "HUD", 639, 652, "gold state icon"),
    *labels(HFX, "HUD", 653, ["boat", "boat outlined"]),
    *labels(HFX, "stats rows", 655, ["in a boat (flat boat)", "flat boat outlined"], "catalog::STATS_ROW_ICONS"),
    *labels(HFX, "unit figures", 664, [f"{u} {style}" for u in ["shaman (probably)", "brave", "warrior", "firewarrior",
                                                                "preacher", "spy"] for style in ["plain", "outlined"]]),
    *labels(HFX, "panel tabs", 676, ["build tab", "build tab highlighted", "spells tab", "spells tab highlighted"]),
    *labels(HFX, "panel tabs", 680, ["stats tab (people)", "stats tab highlighted"], "catalog::PEOPLE_ICON"),
    *labels(HFX, "HUD", 682, ["Magical Shield icon"]),
    *series(HFX, "panel buildings", 1028, 1036, "coloured building picture (pinkish)"),
    *labels(HFX, "panel buildings", 1037, [f"{b} shadow" for b in BUILDINGS], "hud::build::SHADOWS"),
    *labels(HFX, "panel buildings", 1046, [f"{b} highlight" for b in BUILDINGS], "hud::build::HIGHLIGHTS"),
    *labels(HFX, "HUD", 1055, ["? to discover", "? to discover (small)"], "hud::build::UNKNOWN_ICON"),
    *labels(HFX, "panel spells", 1057, ["Armageddon", *SPELLS_BY_MODEL, "Teleport"], "hud::spells::panel_glyph"),
    *labels(HFX, "panel buildings", 1075, BUILDINGS, "hud::build::ICONS"),
    *labels(HFX, "stats rows", 1084, ["selected", "idle", "in a house", "working", "in a balloon"],
            "catalog::STATS_ROW_ICONS"),
    *frames(HFX, "effects", 1090, 1098, "building fire (probably)"),
    *frames(HFX, "effects", 1100, 1107, "fireball turning to smoke"),
    *frames(HFX, "effects", 1108, 1127, "fireball effect"),
    *frames(HFX, "effects", 1136, 1151, "whirlwind base"),
    *frames(HFX, "effects", 1160, 1179, "winged death totem animation (rock-debris dust burst [exe])"),
    *frames(HFX, "effects", 1180, 1187, "fire cloud"),
    *frames(HFX, "effects", 1188, 1192, "fire cloud fading"),
    *labels(HFX, "effects", 1193, ["explosion"]),
    *frames(HFX, "effects", 1194, 1209, "volcano fireball landing blast"),
    *frames(HFX, "effects", 1210, 1224, "grey dust cloud"),
    *frames(HFX, "effects", 1225, 1239, "cream dust cloud (teleport landing)", "units::dust ORIGINAL_CLOUD"),
    *frames(HFX, "effects", 1240, 1247, "flash (conversion halo?)"),
    *frames(HFX, "effects", 1265, 1279, "flash"),
    *frames(HFX, "effects", 1288, 1303, "sparks"),
    *frames(HFX, "effects", 1304, 1319, "water splash (drowning?)"),
    *frames(HFX, "effects", 1329, 1344, "smoke (spy setting a building on fire)"),
    *frames(HFX, "effects", 1345, 1360, "smoke (spy setting a building on fire) 2"),
    *frames(HFX, "effects", 1369, 1384, "smoke"),
    *frames(HFX, "effects", 1385, 1416, "smoke"),
    *frames(HFX, "effects", 1433, 1440, "sparkle"),
    *frames(HFX, "effects", 1443, 1448, "sparkle"),
    *series(HFX, "HUD", 1450, 1465, "cell border part"),
    *frames(HFX, "effects", 1466, 1477, "wind"),
    *frames(HFX, "markers", 1478, 1487, "bloodlust bubbles", "catalog::BLOODLUST_SPRITES"),
    *frames(HFX, "markers", 1489, 1498, "low health sparkles", "catalog::LOW_HEALTH_SPRITES"),
    *frames(HFX, "markers", 1538, 1552, "shield ring", "catalog::SHIELD_SPRITES"),
    *frames(HFX, "markers", 1553, 1563, "hypnotised halo", "catalog::HYPNOSIS_SPRITES"),
    *labels(HFX, "tooltip", 1594, ["1 brave", "3 braves", "4 braves", "5 braves"]),
    *labels(HFX, "tooltip", 1598, [f"{n} wood" for n in (3, 5, 6, 8, 11)]),
    *frames(HFX, "HUD", 1603, 1614, "progress bar part"),
    # docs/specs/animations.md "`HSPR0-0.DAT` by index": a unit's body pose and its outfit's gear, separate pieces.
    *series(HSPR, "firewarrior", 4510, 4529, "pose sprite"),
    *series(HSPR, "firewarrior", 4530, 4554, "decoration: held fire, horned helmet"),
    *series(HSPR, "firewarrior", 4665, 5079, "pose or decoration"),
    *series(HSPR, "warrior", 5193, 5217, "decoration: headband, vest, hip item"),
    *series(HSPR, "preacher", 6526, 6536, "sermon: raises a held book", "anim 20"),
    *series(HSPR, "preacher", 6537, 6547, "sermon: holds the book across the chest", "anim 21"),
    *series(HSPR, "preacher", 6548, 6558, "sermon: gestures with the free arm while reading", "anim 22"),
    *series(HSPR, "preacher", 6559, 6566, "sermon: both arms up, book held high", "anim 23"),
    *series(HSPR, "preacher", 6567, 6574, "sermon: lowers the book, open arms", "anim 24"),
]

# Banks small enough to list whole (unnamed sprites hidden by default on the page).
WHOLE_BANKS = [POINT, HFX, HSPR]
ALPHA_SPRITES = [range(1090, 1500), range(1538, 1593)]


def animation(file, index, name):
    """The animation a `frames` entry belongs to, named after its file and first index; None for a lone sprite."""
    m = re.fullmatch(r"(.*) (\d+)/(\d+)", name)
    return f"{file}:{index - int(m[2]) + 1}" if m else None


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


PAGE = r"""<!doctype html><meta charset="utf-8"><title>Sprite catalog</title>
<style>
:root{--bg:#1d1a16;--fg:#eee3cf;--card:#2c2722;--mute:#a59783;--tile:#cc8f33}
body{font-family:sans-serif;background:var(--bg);color:var(--fg);margin:16px}
header{position:sticky;top:0;background:var(--bg);padding:8px 0;display:flex;flex-wrap:wrap;gap:10px;align-items:center;z-index:1}
input,select{background:var(--card);color:var(--fg);border:1px solid #555;padding:4px}
#grid,#anims{display:flex;flex-wrap:wrap;gap:6px}
h2{font-size:15px;margin:14px 0 6px}
.s{background:var(--card);width:150px;padding:6px;box-sizing:border-box;font-size:11px;cursor:pointer;display:flex;flex-direction:column;gap:3px}
.s.unnamed{opacity:.7}
.pic{display:flex;align-items:center;justify-content:center;min-height:60px;background:var(--bg)}
body.tile .pic{background:var(--tile)}
.pic img{image-rendering:pixelated;max-width:100%;height:auto}
.name{font-weight:bold} .meta,.ref{color:var(--mute)} .ref{font-family:monospace;font-size:10px;word-break:break-all}
</style>
<header>
<input id="q" placeholder="search name, file:index, group, ref" size="34">
<select id="file"><option value="">all files</option></select>
<select id="group"><option value="">all groups</option></select>
<label><input type="checkbox" id="unnamed"> unnamed</label>
<label><input type="checkbox" id="tile"> panel background</label>
<label>scale <input type="range" id="scale" min="1" max="6" value="3"></label>
<label>frame ms <input type="number" id="speed" min="20" max="2000" step="10" value="120" style="width:60px"></label>
<span id="count"></span>
</header>
<h2>Animations</h2><div id="anims"></div>
<h2>Sprites</h2><div id="grid"></div>
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
  animFrames = Map.groupBy(shown.filter(s => s.anim && s.src), s => s.anim);
  $('anims').innerHTML = [...animFrames].map(([id, f]) => `<div class="s" data-id="${id}" title="click to copy">
    <div class="pic" style="height:${Math.max(...f.map(s => s.h)) * scale}px"><img data-anim="${id}"></div>
    <div class="name">${f[0].name.replace(/ \d+\/\d+$/, '')}</div><div class="meta">${f[0].file}:${f[0].index}-${f.at(-1).index} · ${f.length} frames</div>
    <div class="meta">${f[0].group}</div><div class="ref">${f[0].ref || ''}</div></div>`).join('');
}
let animFrames = new Map(), tick = 0;
function step() {
  const scale = +$('scale').value;
  for (const img of $('anims').querySelectorAll('img')) {
    const f = animFrames.get(img.dataset.anim), s = f[tick % f.length];
    img.src = s.src; img.width = s.w * scale; img.height = s.h * scale;
  }
  tick++;
  setTimeout(step, Math.max(20, +$('speed').value || 120));
}
$('anims').addEventListener('click', e => { const c = e.target.closest('.s'); if (c) navigator.clipboard?.writeText(c.dataset.id); });
for (const id of ['q', 'file', 'group', 'unnamed', 'scale']) $(id).addEventListener('input', render);
$('tile').addEventListener('input', e => document.body.classList.toggle('tile', e.target.checked));
$('grid').addEventListener('click', e => { const c = e.target.closest('.s'); if (c) navigator.clipboard?.writeText(c.dataset.id); });
render();
step();
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
    banks = {}
    for file in WHOLE_BANKS:
        banks[file] = sprites(read(data_dir, file))
        for i, sprite in enumerate(banks[file]):
            group, name, ref = named.get((file, i), ("", "", ""))
            entries.append({"file": file, "index": i, "group": group, "name": name, "ref": ref, "w": sprite[0],
                            "h": sprite[1], "alpha": is_alpha(file, i), "anim": animation(file, i, name),
                            "src": png(sprite, alpha if is_alpha(file, i) else opaque)})
    entries += person_animations(data_dir, read, banks[HSPR], opaque)
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w") as fh:
        fh.write(page(entries))
    print(f"{out}: {sum(1 for e in entries if e['name'])} named sprites, {len(entries)} in all")


def person_animations(data_dir, read, hspr, opaque):
    """One entry per frame of every animated loop that has one (`VSTART`/`VFRA`/`VELE`, direction 0, tribe 0): the
    plain body, plus any outfit (`OUTFITS`) whose composed frames actually differ from it."""
    starts = parse_starts(read(data_dir, VSTART))
    frames = parse_frames(read(data_dir, VFRA))
    elements = parse_elements(read(data_dir, VELE))
    entries = []
    for anim in range(len(starts)):
        name = anim_name(anim)
        baseline = None
        for outfit_name, outfit in OUTFITS.items():
            composed = compose_frame(starts, frames, elements, hspr, anim, 0, 0, outfit)
            is_baseline = outfit_name == "brave (no outfit)"
            if not composed or (not is_baseline and composed == baseline):
                continue
            if is_baseline:
                baseline = composed
            label = f"anim {anim}" + (f": {name}" if name else "") + ("" if is_baseline else f" ({outfit_name})")
            total = len(composed)
            for k, sprite in enumerate(composed):
                entries.append({"file": PERSON_ANIM, "index": anim * 1000 + k, "group": "person animations",
                                "name": f"{label} {k + 1}/{total}", "ref": "", "w": sprite[0], "h": sprite[1],
                                "alpha": False, "anim": f"{PERSON_ANIM}:{anim}:{outfit_name}", "src": png(sprite, opaque)})
    return entries


if __name__ == "__main__":
    sys.path.insert(0, os.path.dirname(__file__))
    main()
