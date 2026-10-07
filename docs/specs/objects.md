# Original 3D objects (Populous: The Beginning)

Reverse-engineered from the files; `pop3_format::objects`, example `objects_info <objects dir> [bank]`.
Read-only, never shipped (see assets.md).

## Banks: `objects/objs0-N.dat`, `pnts0-N.dat`, `facs0-N.dat`
The level header byte 97 (`ObjectsBankNum`, see level-format.md) picks the bank:
- 0 in 34 shipped levels;
- 6 in levels 3, 5, 16, 22 and 2120;
- 7 in 2110;
- 2 in 2127.

The ALACN world editor offers banks 0 and 2-7, and ties the bank to a tree style. Its previews:
- 0, 2, 7: cone pine and weeping trees;
- 3: twisted "bonsai";
- 4: mushroom-shaped;
- 5: thin and tall;
- 6: round, tall cone, palm.

The client loads only bank 0 for now (`original_models.rs`). File names mix case (`OBJS0-0.DAT`, `objs0-1.dat`):
match case-insensitively.

### Bank layouts (compared by identical points and faces, verified)
| Bank | Objects | Layout |
|---|---|---|
| 0 | 194 | the one catalogued below |
| 1 | 51 | fragments of bank 0 (winged death, trees, stones) |
| 2-7 | 158 each | **one common layout, different from bank 0**; the banks differ only in their trees (objects 13-18), and bank 7 has 0-5 empty |
| 8 | 170 | older set, mostly unmatched (its 13 is bank 0's 16) |

Banks 2-7 index -> bank 0 index:

| Banks 2-7 | Bank 0 |
|---|---|
| 0-5 | 7-12 (winged death) |
| 7-11 | 0-4 (camp fire, totems) |
| 13-18 | trees, see below |
| 19-21 | 19-21 |
| 30-32 | 30-32 |
| 39-44 | 75-80 |
| 45-47 | 82-84 (stone head) |
| 51, 52 | 90, 89 |
| 53-56 | 91-94 |
| 62-144 | **100-182 (+38)**: winged death back/wings, buildings 117-144, villager huts 145-180, boat 181, airship 182 |
| 149-155 | 187-193 (totem poles, pyramid of knowledge and doors) |
| 156 | 94 (prison) |
| 157 | 19 |

Empty in banks 2-7: 6, 12, 22-29, 33-38, 48-50, 57-61, 145-148.

**Trees of scenery models 1-6**:
- **Banks 2-7**: objects 13-18, where **16-18 are copies of 13-15** (tree 4 = 1, 5 = 2, 6 = 3).
- **Bank 0's 60-71 are the trees of banks 3-6**, three per bank:

  | Bank | Its 13-15 (and 16-18) = bank 0 objects |
  |---|---|
  | 2, 7 | 13, 14, 15 |
  | 3 | 60, 61, 62 |
  | 4 | 63, 64, 65 |
  | 5 | 66, 67, 68 |
  | 6 | 69, 70, 71 |

- **Bank 0** itself: 13-15 like banks 2 and 7, 17-18 copies of 14-15, but 16 is a different pine.

So the tree look follows the object bank, not the landscape theme. Which file the game loads for a bank-N level,
and how it indexes it, is not verified (it needs the game). Either load bank N with the table above, or stay on
bank 0 and pick the trees by bank.

### Records
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
256 x 1024 palette indices (theme `pal0-X.dat`): 8 x 32 tiles of 32x32, tile `i` at
`(i % 8 * 32, i / 8 * 32)` (same as PopResourceEditor). Tribe-coloured variants (blue, red, yellow, green)
sit side by side.
- 35 of the 36 files are byte-identical, only `BL320-G` differs: the theme look comes from the palette.
- 83 248 texels use indices < 128, i.e. theme land colours (0..111, none in the sky range): draw the atlas
  with the level's theme palette, never a fixed one.
- `BL160-0..2.DAT` (45 056 B) also exist, not analysed (low-res atlas?).

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
| 60-71 | trees, 0.9-1.8 cells tall: twisted bonsai-like (60-65), thin, round, tall cone, palm (71, fronds a cut-out texture); the trees of object banks 3-6 (see above); drawn for generated maps' tree types 6-17 |
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

### Unplaced buildings and leftover objects
The level editor cannot place reconversion (9), wall piece (10), gate (11), curr OE slot (12) nor guard post (17),
and no level has one: nothing ties them to an object, they keep the stand-in box. Bank 0 and the old bank 8 were
looked through (thumbnails from `tools/model_mapping.py`'s renderer, which shows them from below), and checked by
someone who knows the game; none of them is needed:
- Bank 0 91 (92, 93 untextured copies, = bank 8's 70-72), a pit with a ring and a wooden frame with a lever: used
  by the game when it draws a building under construction. Not used: our wooden structure (buildings.md) is
  kept instead.
- Bank 0 89: the spy hut, untextured. Bank 0 90: a wind propeller (windmill sail).
- Bank 8 133: a water tower on four stilts; no known use. Bank 8 25: probably a piece of wood.
- Bank 8 84: a stone arch with a portal in it (like an arc de triomphe).
- Bank 8 101: a jail (old prison). Bank 8 139: a training hut (probably warriors); 140 and 141 training huts too,
  yellow and blue.
- Bank 8 100 (and the sets 106, 112, 118, 124 after it): parts of a pyramid of knowledge, probably for its
  animation.
- Bank 8 also holds older versions of known objects: huts in fenced yards in three sizes (130-132, 144-146, 162),
  drum tower (30 = bank 0's 117), balloon (38), boat and airship huts (42, 43), pyramids of knowledge (73-75),
  stone heads (1-5, 21-24), trees, and a winged creature (151-157, 169).

Atlas texels of palette index 0 are see-through in the game (e.g. the palm's fronds): `atlas_rgba` gives them
alpha 0 and materials that need it cut them out (`AlphaMode::Mask`, trees); flat palette colours stay opaque.
38 579 texels are 0, none are 255; PopResourceEditor also takes the last texel's index (0) as the key.

### Tribe colours
Objects are stored in blue. Tribe-coloured atlas tiles have their red, yellow and green versions right after
them (`tile + tribe`). Blue tiles found by diffing the tribe series face by face:
16, 24, 32, 40, 44, 104, 112, 154, 186, 226, plus 194 (site stone glyph). A few faces differ otherwise
(e.g. 214 -> 25..27), ignored for now.

## Mapping tool
`just model-mapping` writes `target/model-mapping.html`: every bank 0 model as a textured thumbnail with a
select of game items (prefilled with guesses, each item usable once). The JSON it outputs is the input for the
(kind, model) -> object table. The page embeds original art: never commit or publish it.
