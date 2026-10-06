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
1-4, 13-18, 60-71 trees; 75 three capped stone pillars, 76 capped stone pillar (reincarnation site
"RS pillar", our choice), 77 short pillar; 78-80 standing stones; 81 stone arch; 82 stone head;
121-180 buildings (huts, towers...), 100-116 creatures/vehicles, 182 balloon.
The thing (kind, model) -> object index table lives in the game executable and is not decoded yet.
