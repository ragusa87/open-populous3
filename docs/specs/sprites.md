# Original sprite banks ("PSFB")

`pop3_format::sprites`, example `sprite_info <file>`. Read-only, never shipped. Cross-checked with
PopSpriteEditor 1.4.0 (Toksisitee, GPLv3), PopResourceEditor (MIT) and a scan of `HSPR0-0.DAT`,
`hfx0-0.dat` and `POINT0-0.DAT`.

| Off | Type | Content |
|---|---|---|
| 0 | 4 bytes | magic `PSFB` |
| 4 | u32 | sprite count |
| 8 | count x (u16 width, u16 height, u32 offset) | offset is absolute in the file |

- Little-endian. The pixel data usually starts right after the table (`8 + 8 * count`); `POINT0-0.DAT`
  has 2 extra bytes (`a3 00`) before it.
- Offsets do not always increase. Empty entries (0 x 0) all point to the first data offset:
  `HSPR0-0.DAT` 71, 167, 2130, 4484, 5182, 5184, 5186, 5188, 5190, 5192, 5767, 6320, 6876, 6878;
  `hfx0-0.dat` 1089, 1449, 1593. Treat them as empty sprites, not errors.

## Pixel data
`height` rows, each a list of commands read as signed bytes:
- `0`: end of the row, the rest of the row (up to `width`) is transparent.
- `n > 0`: `n` palette indices follow (opaque pixels).
- `n < 0`: skip `-n` transparent pixels.

Observed in the game files:
- Rows mostly stop early: the trailing transparent part is not encoded (87% of `HSPR0-0` rows).
  `HSPR0-0` and `POINT0-0` never end a row with a skip, `hfx0-0` does it 351 times.
- No row goes past `width` (decoders clip anyway). Runs and skips are at most 127 (an encoder splits
  longer ones). Longest seen: runs 39 / 104 / 40, skips 39 / 127 / 33 (HSPR / hfx / POINT).
- Each sprite is followed by one extra `0x7F` byte after its last row (every sprite in the three banks,
  the last one in the file too). Always seek to the table offset, never read sprites back to back.
  PopSpriteEditor does not write this byte and its banks still load in the game.

## Transparency and palette
- Index 255 is the colour key: skipped pixels are 255, and 255 never appears as a literal (literals seen:
  0..253).
- Palette: `pal0-0.dat` (UI, 1024 bytes, 256 x R, G, B, pad). Normal sprites only use index 0 (black,
  outlines) and 128..254: no literal in 1..127 in `HSPR0-0` or `POINT0-0`.

## Banks in `data/`

| File | Sprites | Content |
|---|---|---|
| `HSPR0-0.DAT` | 7953 | people and units |
| `hfx0-0.dat` | 1615 | effects, map things, HUD art (below) |
| `POINT0-0.DAT` | 162 | mouse pointers (below) |
| `plspanel.spr`, `plsspace.spr` | 29, 14 | panel artwork |
| `FONT0..2-0..2.DAT`, `font3..7-0.dat` | | fonts |
| `MSHAD0-0.DAT`, `EDIT0-0.DAT`, `f0{0,1,2}t{0..11}-0.dat` | | not analysed |

Not PSFB: `HSPR0-0.TAB`, `HSPR0-1.DAT`/`.TAB`, `HFX20-0.DAT`/`.TAB` (look like the older Bullfrog
`.DAT` + `.TAB` sprite pair), `vspr-0.inf`.

## `data/POINT0-0.DAT` (162 sprites)
13 arrow + "?", 14 arrow (the standard pointer, tip at 0,0), 15-21 arrow + 2..8 (how many units are selected), 27 arrow + up/down,
28 move cross, 29 rotate, 30-33 gold arrow: the frames of an animated cursor, looping (0.12 s each, timing
guessed): the menus' cursor and, with the spell's icon, the cursor while aiming a spell,
38-57 gold spell icons, 58-66 teal building icons, 80-155 action pointers, animated:
84-89 fight, 90-93 send into a house, 94-99 cut wood, 100-105 a spy setting a building on fire, 106-111 build (with
wood), 112-115 board a balloon, 116-119 board a boat, 120-125 dismantle, 126-131 pray, 132-137 a spy's disguise,
138-143 guard (patrol, for a camp fire and guarding an area), 144-147 go into a building (a pyramid?), 148-151 pick up
cut wood, 152-155 grab; 156-161 small icons, maybe unused: 156 and 158 bridge variants, 157 and 159 lowered ground
variants, 160 a person, 161 a tree.

`just spell-icons` (`tools/spell_icons.py`) writes `target/spell-icons.html` (original art: never committed): for
each spell of the panel, these cursor icons, the `hfx0-0.dat` glyphs 1030-1083 and spell names 107-111, and the
`plspanel.spr` / `plsspace.spr` artwork as candidates for its tile's icon, the choices shown as JSON.

`just sprites` (`tools/sprite_catalog.py`) writes `target/sprites.html` (original art: never committed): every sprite
of `POINT0-0.DAT` and `hfx0-0.dat`, searchable by name, file:index, group or code constant; the names are those of
this page, keep its `CATALOG` in step when one is identified.

Spell icons, named by someone who knows the game (`virtual_cursor::spell_sprite`), shown right of the gold arrow
while aiming: 38 bloodlust (a skull, not on
the player's panel), 39 magical shield (fist), 40 armageddon (scales, probably), 41 blast (flame), 42 convert
(wildmen), 43 ghost army (a man with a shield: the only spell and icon left, unconfirmed), 44 whirlwind, 45
invisibility (eye), 46 swarm (bees), 47 hypnotism (spiral, converts enemies), 48 land bridge, 49 lightning,
50 erosion, 51 flatten, 52 earthquake, 53 swamp, 54 firestorm, 55 angel of death, 56 volcano, 57 teleport.

Building icons, named the same way: 58 hut, 59 drum tower, 60 warrior training, 61 firewarrior training,
62 temple (preachers), 63 spy training, 64 boat hut, 65 airship (balloon) hut, 66 camp fire. Like the spell icons
they belong to the cursor, but added to it: while a blueprint is out the usual arrow stays and the building's icon,
at the same scale, sits right of it (`virtual_cursor::building_sprite`, `with_badge`);
the camp fire's (66) while placing one (`plan_sprite`).

## `data/hfx0-0.dat` (1615 sprites: effects, map things, HUD art)
Same palette. Identified: 2-3 dry plants, 53 the yellow down arrow over the player's selected people (9 x 7) [exe], 22 a flat dark smear, the generic ground shadow under sprite things (draw kind 15, pop3-rev-analysis.md "Volcano rocks") [exe], 23 a pile of logs (17 x 11, a piece of
wood on the ground: `catalog::WOOD_PILE_SPRITE`), 90-93 fire, 107-111 spell names, 112-114 the selection
count 18, 19 and 20, then the aerial view's map icons up to 150 (by eye): 115 an empty hut, 116 warrior training, 117
firewarrior training, 118 temple, 119 spy training, 120 drum tower, 121 pyramid, 122-124 totems (123 with the stone
animation, 124 with the moving head), 125-128 a boat and 129-132 a balloon, each blue, red, yellow, green, 133 a drum
tower (probably), huts by rooms and people inside: 134-137 3 rooms with 0 to 3, 138-142 4 rooms with 0 to 4, 143-148
5 rooms with 0 to 5, 149 the prison, 150 the winged death's totem; 163-170 building icons: drum tower, warrior
training, firewarrior training, temple, spy training, boat hut, airship hut, camp fire; 171 a book (the help icon),
172 a coloured praying figure, 173 a bubble with "!" and 174 an "i" (info), 175 a blue flag, 178 a coloured
firewarrior, 179 a coloured preacher; 187-210 build/wood action icons,
40 a piece of wood in the tooltip (16 x 15, `catalog::TOOLTIP_WOOD_SPRITE`), 46-51 the tooltip's dismantle toggle (49, 51 hovered, 50 pressed) and its undo (46, 48 hovered, 47 pressed), 60-62 the
unload button of a vehicle (60, 61 hovered, 62 pressed), tooltips.md; 59 the combat icon of the aerial view's map,
69-74 ground shadows of various shapes (under a balloon or a flying unit), 81 a RIP tombstone, the icon of a swamp
area; 639-656 gold state icons, each plain then outlined (653 a boat, 655 the same boat flat), 664-675 six unit figures (plain / outlined
pairs: 666 brave, 668 warrior, 670 firewarrior, 672 preacher, 674 spy, 664 probably the shaman; 75-80 the same in
teal, in unit model order: brave, warrior, preacher, spy, firewarrior, shaman, `catalog::UNIT_FIGURES`, the tooltip's
people), 676-681 the panel's tab icons, each then highlighted: 676 build (a house), 678 spells, 680 stats (three
people, also `catalog::PEOPLE_ICON`), 682 the Magical Shield's icon, 346-349 the volcano's rock, a boulder in 4 orientations (17 x 13) [exe], 1030-1083 spell and building glyphs (seen in the game: 1030-1036 coloured building pictures, 1037-1045 the building
icons' shadows and 1046-1054 their highlights, 1055 the "?" of something to discover (1056 the same, smaller),
1057-1074 the panel's spell icons, white, drawn dark: Armageddon 1057, models 2-17 in order from 1058 (Blast; Angel
of Death 1069), Teleport 1074; 1075-1083 the panel's building icons, white, drawn dark, in the cursor's order: hut,
drum tower 1076, warrior, firewarrior, temple, spy, boat hut, airship hut, camp fire), 1084-1088 the Stats
tab's row labels (selected, idle, in a house, working, in a balloon; in a boat: 655), 1090-1600 spell effects (dust, explosions, swarm...).
The 3D objects have no wood pile: objects 0 and 12 are a camp fire (logs and a flame board), see objects.md.

Alpha sprites: 1090..1499 and 1538..1592 are blended (same ranges in PopResourceEditor,
`blend::is_alpha_sprite`). Their indices (including 1..127, 633k pixels) are rows of the alpha table, not colours.
`pop3_format::sprites` returns the raw indices; `blend::AlphaTable::rgba` turns them into RGBA (tint colour,
opacity strength / 15) for GPU blending (`art::alpha_picture_frame`, uploaded with `AlphaMode::Blend`).
Identified: 1225-1239 a cream dust cloud growing and fading (the teleport landing puff), 1210-1224 the same in
grey, 1180-1187 a fire cloud and 1188-1192 its fading, 1193 an explosion, 1194-1209 the rock explosion of a big
volcano fireball's landing [exe] (by eye two explosions, 1193-1197 and 1198-1202), 1160-1179 the rock-debris dust burst [exe], seen in the game as the animation on the winged death's totem, status markers over people [exe] (pop3-rev-analysis.md "Status markers over people"): 1478-1487 bloodlust
(red bubbles), 1489-1498 low health (white sparkles), 1538-1552 shield (3 rings x 5 sizes), 1553-1563 the hypnotised
halo (a yellow ring turning over 11 frames), 1240-1247 a flash (the conversion's halo, a guess) and 1265-1279 a
similar one. By eye, unconfirmed: 1090-1098 a building's fire, 1100-1107 a fireball turning to smoke and up to 1127
other fireball effects, 1136-1151 the whirlwind's base, 1288-1303 sparks, 1304-1319 a water splash (drowning?),
1329-1344 and 1345-1360 the smoke of a spy setting a building on fire (two animations), 1369-1384 and 1385-1416 smoke, 1433-1440 and 1443-1448
sparkles, 1450-1465 parts of a HUD cell border, 1466-1477 wind.
Opaque again after 1592: the tooltip's people and wood, one sprite per count: 1594-1597 braves (1, 3, 4 and 5),
1598-1602 wood (3, 5 as a row of 4 and a row of 1, 6, 8 and 11), 1603-1614 parts of a progress bar (probably).

## Blend tables (`data/`, one per theme, 65 536 bytes = 256 x 256 palette indices)
Layouts from PopResourceEditor (Toksisitee, MIT, `140e389`), checked against the files.

### Alpha, `al0-X.dat`
`al[row * 256 + background]`, `row = tint * 16 + strength`, so an alpha sprite pixel is
`tint << 4 | strength`:
- strength 0 = background unchanged (the first row of each block is 98% identity), 15 = about the tint.
  In between the blend is linear: fitted over all backgrounds, the mix is strength / 15 within a few percent
  (themes 0, 1, c), so an alpha pixel is the RGBA (tint, strength * 17).
- tints (theme 1, row 15): 0 red, 1 orange, 2 cream, 3 green, 4 blue, 5 yellow, 6 brown, 7 pale cyan,
  8..15 black (darkening, shadows).
- The table shape matches the guessed in-game lookup `al[idx * 256 + background]`; the game code itself was
  not checked. PopSpriteEditor's preview `al[idx * 256 + 0]` is the blend over black.
- `alpha.dat` (65 536), `alavaa.dat` and `awat.dat` (131 072) also exist, not analysed.

### Ghost, `ghost0-X.dat`
`ghost[a * 256 + b]` = nearest palette colour to `pal[a] + (pal[b] - pal[a]) * 0.66` (fitted on themes 0,
1, 5, c, z: 0.66-0.67 everywhere; PopResourceEditor's generator also defaults to 66%). Diagonal is the
identity, not symmetric. Which operand is the sprite and which the background is unknown. Used for
transparency (ghost / see-through units).
