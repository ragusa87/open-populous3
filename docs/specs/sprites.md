# Original sprite banks ("PSFB")

`pop3_format::sprites`, example `sprite_info <file>`. Read-only, never shipped.

| Off | Type | Content |
|---|---|---|
| 0 | 4 bytes | magic `PSFB` |
| 4 | u32 | sprite count |
| 8 | count x (u16 width, u16 height, u32 offset) | offset is absolute in the file |

Pixel data: per row, runs until a `0` byte. A positive byte `n` is followed by `n` palette indices,
a negative byte skips `-n` transparent pixels. Palette: `pal0-0.dat` (UI).

## `data/POINT0-0.DAT` (162 sprites)
13 arrow + "?", 14 arrow (the standard pointer, tip at 0,0), 15-21 arrow + 2..8, 27 arrow + up/down,
28 move cross, 29 rotate, 30 gold arrow, 31-33 gold arrow variants (spell targeting?),
38-66 spell / building icons, 80-155 action pointers (build, plant, guard...), 156-161 small icons.

## `data/hfx0-0.dat` (1615 sprites: effects, map things, HUD art)
Same palette. Identified: 2-3 dry plants, 22 a flat dark smear (shadow of 23?), 23 a pile of logs (17 x 11, a piece of
wood on the ground: `catalog::WOOD_PILE_SPRITE`), 90-93 fire, 107-111 spell names, 187-210 build/wood action icons,
1030-1089 spell and building glyphs, 1090-1600 spell effects (dust, explosions, swarm...).
The 3D objects have no wood pile: objects 0 and 12 are a camp fire (logs and a flame board), see objects.md.
