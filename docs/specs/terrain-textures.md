# Terrain textures (original theme files)

The original has no tiled ground textures. Ground colour comes from per-theme lookup files
in `data/` (theme char `X` = `0-9a-z`, chosen by the level header byte 96, see level-format.md):

| File | Size | Content |
|---|---|---|
| `pal0-X.dat` | 1024 | 256 x RGBA palette (A unused) |
| `bigf0-X.dat` | 294 912 | "big fade": 1152 rows x 256 columns of palette indices. Row = height (+ noise), column = brightness (0 bright -> 255 dark). Rows 0..127 are water/shore, land starts at row 128 |
| `disp0-X.dat` | 65 536 | 256x256 detail noise, tiles over the map |
| `sky0-X.dat` | 262 144 (theme 0: 307 200 = 640x480) | 512x512 palettised image. For theme `c` it looks like a planet surface (space view?), not a sky. Not used yet |
| `fade0-X.dat`, `cliff0-X.dat`, `plscv/plsft/plspl0-X.dat` | | not decoded (objects shading, cliffs, globe view) |

Themes seen: `0` desert orange, `5` sand/orange, `c` water-sand-olive-green-snow.

## Our bake (`game-client/src/terrain_texture.rs`)
Per pixel (8 px per cell, texture tiles with a repeat sampler, UV = absolute cell / 128):
- `h` = bilinear height; `d` = `disp` texel.
- row = water (`h < 1`): `d / 2`; land: `max(128, 128 + h + (d - 128) / 2)`.
- brightness column from baked Lambert lighting of the heightmap slope (`SUN`), material is unlit.
- colour = `palette[bigfade[row][column]]`.

Unverified guesses to tune against the real game: the height -> row scale (`POP3_ROW_SCALE`, default 1.0;
x3-x4 turns level 1 olive/green but washes level 5 peaks out; level 1, all
below height 100, comes out sandy), how `disp` is applied, the brightness curve.
Fallback (no data, or `--no-original`): `procedural_theme::generate(seed)` builds a palette
(8 bands x 32 brightness), a jittered bigfade and tileable value-noise disp with the same
shapes as the original files, so the same bake is used in both modes.

## Next
- Re-bake only the dirty rect after edits (now the whole 1024² texture is re-baked).
- Mipmaps (distant aliasing), theme sky colour / backdrop, lava and swamp layers.
- Free theme packs for redistribution (the original files must not be shipped).
