# Original 3D objects (Populous: The Beginning)

Reverse-engineered from the files; `pop3_format::objects`, example `objects_info <objects dir> [bank]`.
Read-only, never shipped (see assets.md).

## Banks: `objects/objs0-N.dat`, `pnts0-N.dat`, `facs0-N.dat`
Bank 0 is the main one (194 slots). Banks 1-8 exist (2-7 look like variants), unused for now.
File names mix case (`OBJS0-0.DAT`, `objs0-1.dat`): match case-insensitively.

`objs` record, 54 bytes, little endian:

| Off | Type | Content |
|---|---|---|
| 0 | u16 | flags (unknown) |
| 2 | u16 | face count |
| 4 | u16 | point count |
| 6..16 | | unknown |
| 16, 20 | i32 | faces `[start, end)`, **1-based** into `facs` |
| 24, 28 | i32 | points `[start, end)`, **1-based** into `pnts` |
| 32 | 6 x i16 | bounding box min xyz / max xyz, about 1.25x the points (selection margin?) |
| 44..54 | | unknown |

`pnts`: 6 bytes, `i16` x, y, z. **y is up**, 0 = ground; units are world units (512 per cell).

`facs` record, 60 bytes:

| Off | Type | Content |
|---|---|---|
| 0 | u8 | palette colour (flat faces) |
| 2 | u16 | atlas tile, `0xffff` = untextured |
| 6 | u8 | point count (3 or 4; others skipped) |
| 8 | 4 x (i32 u, i32 v) | texel inside the tile, 16.16 fixed point, 0..32 |
| 40 | 4 x u16 | point indices, 0-based within the object |
| 48..60 | | per-point shade (?) and flags, unknown |

## Atlas: `data/bl320-X.dat` (theme char X, upper case on disk)
256 x 1024 palette indices (theme `pal0-X.dat`): 8 x 32 tiles of 32x32. Tribe-coloured variants
(blue, red, yellow, green) sit side by side.

## Identified objects (bank 0)
Identified by hand from the mapping page; encoded in `pop3_format::catalog`.

| Objects | What |
|---|---|
| 0 | camp fire (probably: wood at the base, a board for the flames); 12 looks the same, smaller |
| 1 | totem; 2, 4 untextured copies; 3 animation frame (rotating rocks) |
| 7, 100, 107 | "mort ailée" (winged death, probably Angel of Death): front, back, wings rotated; 8-11, 101-106, 108-116 untextured |
| 19, 21 | totem of the winged death, and the bird perched on it |
| 20 | stone prayer totem |
| 30 | reincarnation site stone (tribe-coloured) |
| 31, 32 | book, shield |
| 13-18 | trees of scenery models 1-6 (`catalog::tree_object`): 13 cone pine, 14 weeping tree, 15 big weeping tree, 16 pine, 17-18 copies of 14-15 (level 19 confirmed 1 = cone pine, 2 = weeping tree) |
| 60-71 | trees, 0.9-1.8 cells tall: twisted bonsai-like (60-65), thin, round, tall cone, palm (71, fronds a cut-out texture); probably other landscapes' trees (which theme uses them is unknown); drawn for generated maps' tree types 6-17 |
| 82 | stone head; 83, 84 untextured |
| 94 | prison (shaman locked until freed, some levels) |
| 117-120 | drum tower, blue / red / yellow / green |
| 121-124 | boat hut, per tribe |
| 125-128 | airship hut, per tribe |
| 129-132 | spy hut, per tribe |
| 133-136 | prayer hut, per tribe |
| 137-140 | firewarrior training hut, per tribe |
| 141-144 | warrior training hut, per tribe |
| 145-180 | villager huts: 3 styles x 4 tribes x 3 sizes, `145 + style*12 + tribe*3 + size-1` |
| 181, 182 | boat (blue), airship (balloon) |
| 187, 188, 189 | totem poles |
| 191 | pyramid of knowledge (unlocks a spell or building); 192, 193 door animation frames |

Unidentified: 75-81 (stone pillars, standing stones, arch), 89-93,
190 (looks like the pyramid of knowledge).

Atlas texels of palette index 0 are see-through in the game (e.g. the palm's fronds): `atlas_rgba` gives them
alpha 0 and materials that need it cut them out (`AlphaMode::Mask`, trees); flat palette colours stay opaque.

### Tribe colours
Objects are stored in blue. Tribe-coloured atlas tiles have their red, yellow and green versions right after
them (`tile + tribe`). Blue tiles found by diffing the tribe series face by face:
16, 24, 32, 40, 44, 104, 112, 154, 186, 226, plus 194 (site stone glyph). A few faces differ otherwise
(e.g. 214 -> 25..27), ignored for now.

## Mapping tool
`just model-mapping` writes `target/model-mapping.html`: every bank 0 model as a textured thumbnail with a
select of game items (prefilled with guesses, each item usable once). The JSON it outputs is the input for the
(kind, model) -> object table. The page embeds original art: never commit or publish it.
