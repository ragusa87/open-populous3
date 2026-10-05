# Original level format (Populous: The Beginning)

Files in `levels/`: `levlNNNN.dat` (map + objects), `levlNNNN.hdr` (616 B, settings + name),
`levlNNNN.inf`, `levlNNNN.ver`, plus `cpscr*`/`cpatr*` (AI scripts/attributes, not parsed).
Reverse-engineered from the files themselves; cross-check with community editors
(PopTB level editor, popre / "Populous Reincarnated" docs) before trusting "tentative" fields.

## `.dat` (192 137 bytes, little endian)

| Offset | Size | Content |
|---|---|---|
| 0 | 128*128*2 | `u16` ground height per cell, row-major `z*128+x`. 0 = sea, observed max 1024 |
| 32768 | 16384 | `u8` layer 2 (tentative: terrain/texture type) |
| 49152 | 16384 | `u8` layer 3 (tentative: same domain as layer 2) |
| 65536 | 16384 | `u8` "no access" squares (all 0 in level 1) |
| 81920 | 122 | misc block; starts with a 4x16 B per-tribe table (allies?), rest unknown |
| 82042 | 2000*55 | thing records, `kind == 0` means empty slot |
| 192042 | 95 | trailer, unknown |

Thing record (55 bytes), known part:

| Byte | Field |
|---|---|
| 0 | model (subtype, e.g. tree variant) |
| 1 | kind (observed: 5 = scenery, 6 = general/marker; 1 person, 2 building expected) |
| 2 | owner tribe (255 = neutral) |
| 3..5 | x, `u16`, world units (512 per cell, odd multiples of 256 = cell centre) |
| 5..7 | z, `u16` |
| 7..55 | unknown (kept raw in `Thing::raw`) |

Axis order (`x` vs `z`) is unverified; a mirrored map would still look right.

## `.hdr` (616 bytes)
- offset 56, 32 bytes: NUL-terminated level name ("Level 1").
- offset 88: number of tribes (2-4); 89..91: computer player script ids (tentative); 92..95: allies bitmasks.
- offset 96: landscape theme index, 0-35 -> files `*0-X.dat` with X in `0-9a-z` (see terrain-textures.md).
- rest: unknown (spells/buildings availability, tribe count, sky/palette...).

## Code
`crates/pop3-format/src/level.rs`, example: `just level-info path/to/levl2005.dat`.
