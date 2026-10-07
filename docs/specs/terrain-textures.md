# Terrain textures (original theme files)

The original has no tiled ground textures. Ground colour comes from per-theme lookup files
in `data/` (theme char `X` = `0-9a-z`, chosen by the level header byte 96, see level-format.md):

Layouts cross-checked with PopResourceEditor (Toksisitee, MIT, `140e389`), verified on all 36 themes.
Every file is raw, no header, row-major.

| File | Size | Content |
|---|---|---|
| `pal0-X.dat` | 1024 | 256 x (R, G, B, pad). Pad always 0, components 8-bit (not VGA 6-bit) |
| `bigf0-X.dat` | 294 912 | "big fade": 1152 rows x 256 columns of palette indices **0..111**. Row = height (+ noise), column = brightness (0 bright -> 255 dark) |
| `sky0-X.dat` | 262 144 | 512 x 512, values 0..15, colour = `pal[112 + v]` (theme 0: 640 x 480, raw indices 100..127) |
| `disp0-X.dat` | 65 536 | 256 x 256 detail noise, tiles over the map (see below) |
| `fade0-X.dat` | 16 384 | 64 shade levels x 256 colours -> palette index |
| `cliff0-X.dat` | 8 192 | 64 levels x 128 land colours -> palette index |
| `ghost0-X.dat`, `al0-X.dat` | 65 536 | 256 x 256 blend tables, see sprites.md |
| `watdisp.dat` | 65 536 | one file for all themes: 256 x 256 water displacement, unsigned (6..248) |
| `plscv0-X` / `plsft0-X` / `plspl0-X.dat` | 24 576 / 16 384 / 1024 | not decoded (globe view?) |

Themes seen: `0` desert orange, `5` sand/orange, `c` water-sand-olive-green-snow.
Themes used by the 41 shipped levels: 1, 4-6, 8-13, 16-19, 21-25, 28-30 (never 0). Some files are shared
between themes (22 distinct disp files, 32 bigfade, 32 sky).

## Palette regions
- `0..111`: land colours, per theme. bigfade uses only these (themes 1-z use all 112; theme 0 1..100).
- `112..127`: sky colours, per theme. 112 and 127 are magenta (255, 0, 255) placeholders; sky data mostly
  uses 1..14.
- `128..255`: sprites, UI, objects, identical in every theme except `128-129, 131-139, 148-157`
  (theme rock colours, used by cliff).
- Theme 0 is an older set: its `0..127` and sky differ in format, no level uses it.

## Sky
`pal[112 + v]`, not `pal[v]`: read as raw indices it hits the land colours (the "planet surface" look of
theme `c`). Theme `c` is a light blue-grey cloud layer, flat across rows: a tiling cloud texture, not a
vertical gradient. Theme `m` (level 5) is a night sky: `pal[112]` is black there, not magenta.

`pop3_format::theme::Sky` decodes it (theme 0: 640 x 480 raw indices). The client (`sky.rs`) draws it on a
dome around the eye, projected like a flat ceiling of clouds (`dome_uv`: tiles shrink towards the horizon,
mirrored below it), faded out with the plain sky colour when zooming out to the planet (`camera::space_fade`).
How the original maps it (scrolling backdrop?) is not checked. Generated maps keep the plain colour.

## bigfade
- Columns: brightness, 0 bright -> 255 dark (theme 1, row 600: luminance 186 -> 40).
- Rows: 1152 = 1024 (max height) + 128. Rows 0..127 water/shore, land from 128 (fits, not proven).
- PopResourceEditor's level preview (its own approximation, not the game) uses row `height + 140`,
  column 64 as neutral light, +-16 for slope lighting, -16 on cliffs (|dh| > 10), and draws sea cells
  (height 0) as `bigfade[watdisp[x, y]]` (row 0).
- Level `.dat` sunlight block (ShadeStart 28, ShadeRange 15, Inclination 32 or 64, see level-format.md)
  is probably the game's lighting input: unverified. The ALACN world editor doesn't use it: it colours land from
  its own height ramp and zeroes the block when saving.

## disp
- PopResourceEditor treats it as signed (shows `v + 128`). No file crosses 127/128, so the files can't
  tell signed from unsigned.
- The ranges are narrow and differ per theme: theme 1 45..127 (mean 87), 5 130..193 (mean 161),
  7 150..170, c 73..106, 0/w/x/y/z 0..255. So the values are **not centred on 128**: `(d - 128) / 2`
  shifts every row by a theme constant (about -20 rows for theme 1, +17 for theme 5). Our bake subtracts the file
  mean to keep only the detail; how the game uses it is unknown.

## fade
`fade[level * 256 + colour]`. Row 32 is the identity, rows below darken (0: about 0.27x luminance,
16: 0.52x), rows above brighten (48: 1.6x, 63: 2x). A palette-space light table, for shading objects /
sprites and fog of war (PopResourceEditor: "shadows or Fog of War").

## cliff
`cliff[level * 128 + colour]`, colour = a land colour (bigfade output, < 128). Row 0 is the identity,
rows 1..16 stay about 75% identity, from about row 20 colours turn to rock (theme range 129..157 and a few
dark land colours at row 63). PopResourceEditor: "damaged ground, lava and similar". What picks the row
(slope? damage?) is unknown.

## Our bake (`game-client/src/terrain_texture.rs`)
Per pixel (8 px per cell, texture tiles with a repeat sampler, UV = absolute cell / 128):
- `h` = bilinear height; `d` = `disp` texel.
- row = water (`h < 1`): `watdisp * 128 / 256` (`sea_row`, the whole water band; `disp` when the file is
  missing, generated noise in the procedural theme); land: `land_row` =
  `max(start, start + h + (d - mean(disp)) / 2)`. `start` = `Theme::land_start_row`, the first row from 128 up
  where most columns are not water colours (colours of rows 0..127), at most 160: rows 128..~145 are still blue
  in most themes (theme 1: 146, 5: 136, m: 137; theme g reuses water colours up to row 475, hence the cap).
  So low ground (the simulation's land, height >= 1) never looks like the sea (level 5's reincarnation site
  used to), and centring `disp` on the file's mean keeps the detail without shifting the bands by a theme
  constant.
- brightness column from baked Lambert lighting of the heightmap slope (`SUN`), material is unlit.
- colour = `palette[bigfade[row][column]]`.

Unverified guesses to tune against the real game (see disp above): the height -> row scale (`POP3_ROW_SCALE`, default 1.0;
x3-x4 turns level 1 olive/green but washes level 5 peaks out; level 1, all
below height 100, comes out sandy), how `disp` is applied, the brightness curve.
Fallback (no data, or `--no-original`): `procedural_theme::generate(seed)` builds a palette
(8 bands x 32 brightness), a jittered bigfade and tileable value-noise disp with the same
shapes as the original files, so the same bake is used in both modes.

## Next
See TODO.md ("Terrain"): fade and cliff loading (`pop3_format::theme` loads pal, bigf, disp, watdisp and the
sky).
- Re-bake only the dirty rect after edits (now the whole 1024² texture is re-baked).
- Mipmaps (distant aliasing), lava and swamp layers.
- Free theme packs for redistribution (the original files must not be shipped). The community packs on
  thebeginning.uk/textures have no licence (site: "All rights reserved") and keep Bullfrog data (palette
  128..255, some ship the original `BL320`): fine to load from the user's `data/`, not to bundle.
