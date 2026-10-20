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

Themes used by the 41 shipped levels: 1, 4-6, 8-13, 16-19, 21-25, 28-30 (never 0) - numeric theme ids, which map
to the file-name chars as `0-9` then `a-z` for 10-35, i.e. chars `1, 4-6, 8, 9, a-d, g-j, l-p, s-u`. Some files
are shared between themes (22 distinct disp files, 32 bigfade, 32 sky; e.g. `v`/`w`/`x`/`y`/`z` are identical
bigfade/palette data).

**Rough colour-family catalogue, all 36 `pal0-X.dat`/`bigf0-X.dat` files, this session** (sampling a handful of
mid-brightness land rows at low/mid height and near the 1152-row ceiling; a first pass, not exhaustive - static
data-file analysis only, no exe trace, see "bigfade" above for how these rows are actually reached in play):

| Theme | Low/mid land colour | Near the row ceiling |
|---|---|---|
| `0` | desert orange | same, desert orange |
| `1` | desert orange/tan | same |
| `2` | desert orange/tan | same, a little muddier |
| `3` | desert orange/tan | same |
| `4` | desert orange | same |
| `5` | sand/orange (already known) | same, deeper orange |
| `6` | desert tan, one green patch | desert tan |
| `7` | desert/olive mix | blue-grey to olive-green (damp, jungle-ish) |
| `8` | desert/olive/green mix | green to grey-green (jungle/rock) |
| `9` | grey-green, muddy | grey, some green (rocky upland) |
| `a` | desert tan/olive | tan fading to grey-green (rock) |
| `b` | desert orange, one olive patch | desert orange/brown |
| `c` | blue-grey water patch then olive/green (already known: "water-sand-olive-green-snow") | muddy orange/grey mix |
| `d` | desert orange, some pink/mixed | green to grey (rock), one mixed |
| `e` | desert orange/tan | desert orange |
| `f` | desert/olive/green mix | desert orange, one mixed |
| `g` | desert tan, one pale patch | pale pinkish-grey/haze, all "mixed" - possibly a hazy highland, not clearly snow |
| `h` | mixed pale tan, one pale blue | **pale blue trending to near-white near the very top rows - the clearest snow/ice-style theme found** |
| `i` | blue/lavender throughout | pale blue/lavender, brighter near the top - a distinct blue-toned theme, not snow-white |
| `j` | desert tan to green | pale green (savanna-like), consistently |
| `k` | desert/olive/grey mix | pale blue-green near the top (icy lake highland?) |
| `l` | desert orange/tan | desert orange, deeper |
| `m` | desert orange/tan | desert orange |
| `n` | desert orange/tan | desert orange |
| `o` | pale desert/sand | pale desert/sand, one mixed |
| `p` | pale desert/sand, one green | green (highland) then back to orange at the very top |
| `q` | desert/olive | desert orange |
| `r` | green throughout | blue-green to green |
| `s` | desert orange/tan | desert/mixed brown |
| `t` | muddy grey/mixed | grey (rock/mountain theme) |
| `u` | pale pink/tan, mixed | mixed pink/grey/blue, one orange |
| `v`,`w`,`x`,`y`,`z` | desert tan to green (identical files) | desert orange/brown (identical files) |

Caveat: only one column (64, a medium brightness) was sampled per row here, so a theme can have a near-white
colour hiding at a brighter column on a row this pass called "mixed"/"desert" - confirmed for `c` itself:
column 0 (brightest), row ~200 (low ground) is near-white `(242, 232, 223)`, which is presumably that theme's
"snow" in its existing qualitative description, not a row-ceiling effect - brightness, not just height, matters
to whether a cell reads as snow-like. Treat this table as a rough first pass (one column, a handful of rows),
not a verified per-theme palette audit.

Of the themes checked at the row ceiling specifically, only `h` goes clearly near-white there (TODO.md
"Terrain", `procedural_theme.rs`'s universal top "snow" band at row 960 for every theme); `i` and `k` lean pale
blue near the top instead, everything else stays in its own desert/green/rock family all the way up - this still
confirms the TODO item's premise (a universal top-row "snow" band has no basis in most themes), while showing the
real picture is theme- and brightness-dependent, not simply "row 960 is snow" for the handful of themes that do
go pale up there.

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
- Rows: 1152 = 1024 (max height) + 128.
- PopResourceEditor's level preview (its own approximation, not the game) uses row `height + 140`,
  column 64 as neutral light, +-16 for slope lighting, -16 on cliffs (|dh| > 10), and draws sea cells
  (height 0) as `bigfade[watdisp[x, y]]` (row 0).
- **Row formula, traced from the exe this session** (pop3-rev-analysis.md "Terrain land colour"):
  `row = clamp(height + 75 + (75 if blend != 0 else 0), 0, 1022)`, the
  SAME per-cell `blend` byte as "cliff" below (so almost every non-dead-flat cell gets the second +75: blend is
  0 only when a cell and all 8 neighbours share the same height) — no cell averaging, each of a quad's 4 corners
  reads its own cell directly, then the 4 values are interpolated across the quad the same way as blend (see
  "cliff"). This explains, rather than hardcodes, the old "rows 0..127 water/shore" guess: a flat sea cell
  (height 0, blend 0) lands at row 75, squarely in that band, while almost any land cell (blend != 0 the moment
  it isn't perfectly flat) jumps straight past 127 even at height 1 (75+75+1=151) — the split falls out of the
  formula, it isn't a separate rule. `disp` noise (below) then jitters this per-pixel; the final flat
  `bigf0-X.dat` index also folds in the brightness column (a large brightness value can legitimately roll over
  into the next row — not guarded against in the original).
- Level `.dat` sunlight block (ShadeStart 28, ShadeRange 15, Inclination 32 or 64, see level-format.md) does
  **not** feed the row: traced this session into the brightness column instead (pop3-rev-analysis.md "Terrain
  land colour"). The ALACN world editor doesn't use it either: it colours land from its own height ramp and
  zeroes the block when saving.
- No elevation-vs-theme override exists anywhere in the traced exe path (see "cliff" below): a row is picked
  purely from height (+ noise) or from the per-cell `blend` byte, then looked up in whichever `bigf0-X.dat`/
  `cliff0-X.dat` the level's theme loaded once at level start. Nothing re-keys the colour by a global height
  band independent of that per-theme table. A theme's top rows look however that theme's own file draws them
  (near-white only for the few themes whose file actually goes pale up there, e.g. `h`/`i`/`k` - see "Themes
  seen" above) — it is theme data, not a rule.

## disp
- PopResourceEditor treats it as signed (shows `v + 128`). No file crosses 127/128, so the files can't
  tell signed from unsigned.
- The ranges are narrow and differ per theme: theme 1 45..127 (mean 87), 5 130..193 (mean 161),
  7 150..170, c 73..106, 0/w/x/y/z 0..255.
- **Confirmed from the exe this session** (pop3-rev-analysis.md "Terrain land colour"): the game reads the byte
  as SIGNED (no re-centring, no per-theme constant) and uses it twice. (1) As the bigfade row's jitter, scaled by
  a fixed altitude-dependent sensitivity curve baked once at level load (one value per row, `clamp(3 * row_index -
  64, 320, 1024)`: flat 320 below row index 129, a +3/index ramp to 1022 at 362, flat 1024 above — see
  pop3-rev-analysis.md "Terrain land colour" for the table itself) — so the SAME raw disp byte moves the row much
  more on high ground than on low/flat ground, not a flat scale. (2) As a small extra brightness perturbation,
  from the difference between two
  vertically-adjacent disp texel samples. So each theme's own narrow, off-centre byte range (e.g. file 1 always
  below 128 i.e. always a small positive signed value, file 5 always above 128 i.e. always a negative one) IS
  the per-theme bias once read as signed — there is no separate exe-side per-theme constant to find; our bake's
  own "subtract the file mean" is solving a different problem (keeping the width of the noise comparable across
  themes for `POP3_ROW_SCALE`), not reproducing this.

## fade
`fade[level * 256 + colour]`. Row 32 is the identity, rows below darken (0: about 0.27x luminance,
16: 0.52x), rows above brighten (48: 1.6x, 63: 2x). A palette-space light table, for shading objects /
sprites and fog of war (PopResourceEditor: "shadows or Fog of War").

## cliff
`cliff[level * 128 + colour]`, colour = a land colour (bigfade output, < 128). Row 0 is the identity,
rows 1..16 stay about 75% identity, from about row 20 colours turn to rock (theme range 129..157 and a few
dark land colours at row 63). PopResourceEditor: "damaged ground, lava and similar".

**What picks the row, confirmed from the exe this session** (pop3-rev-analysis.md "Building ashes"
"Drawn"): not slope and not a damage flag directly, but the SAME
per-map-cell "blend" byte (0..255) already traced for the simulation's own lava/ash state — `row = blend / 4`.
Blend is normally a slope measure (`clamp((max neighbour height − min) / 8, 1, 127)`, recomputed after every
height edit), which is why ordinary cliffs pick it up too; "Volcano lava flow" pushes it past that normal
range, up to 255 (row 63, "glowing"), and "Building ashes" forces it to 127 (row 31) on a building lost to fire
or attack, decaying 2/turn once its 6000-turn hold ends. Each quad's 4 corners sample their own cell's blend
directly (no neighbour-averaging at this step) and are then interpolated smoothly across the quad like any
other vertex colour, same as the brightness column below - this, not a separate glow effect, is why adjoining
hot cells read as linked, roughly round blobs rather than hard-edged tiles. Separately, a non-zero blend also
adds 75 to that corner's own index into a distinct, 1022-entry height-based colour gradient (also
corner-sampled and interpolated the same way) - two different lookups stacked, not one. The table itself is
loaded once at level load and never rewritten, so there is no separate time-based flicker: the "glow" rising
and fading over a lava cell's life is the blend byte's own value doing so, read straight into this lookup
every frame.

## Our bake (`game-client/src/terrain_texture.rs`)
Per pixel (8 px per cell, texture tiles with a repeat sampler, UV = absolute cell / 128):
- `h` = bilinear height; `d` = `disp` texel.
- row = water (outside the coast, below): `watdisp * 128 / 256` (`sea_row`, the whole water band; `disp` when the file is
  missing, generated noise in the procedural theme); land: `land_row` =
  `max(start, start + h + (d - mean(disp)) / 2)`. `start` = `Theme::land_start_row`, the first row from 128 up
  where most columns are neither water colours (colours of rows 0..127) nor sea-like (`sea_like`:
  blue > green >= red, the wet grey-blue of theme c's rows 134..136; purple land like theme i's stays land), at most 160: rows 128..~145 are still blue
  in most themes (theme 1: 146, 5: 136, m: 137; theme g reuses water colours up to row 475, hence the cap).
  So low ground (the simulation's land, height >= 1) never looks like the sea (level 5's reincarnation site
  used to), and centring `disp` on the file's mean keeps the detail without shifting the bands by a theme
  constant.
- coast (`coast_land`): walkers cross every cell not sea on all four corners (`is_sea`), so the coast is
  drawn round those cells, not where the bilinear height reaches 1 (that left most of a shore cell blue
  and units looked like they walked on water). Field = 1 at the centre of each walkable cell, 0 at sea,
  bilinear between centres; ground where it reaches 0.2 (`COAST_THRESHOLD`). Covers a lone walkable
  cell's corner (0.25 there), puts straight coasts 0.3 cell past the walkable edge and rounds the corners.
  Ground there, even at height 0, takes the lowest land row.
- brightness column from baked Lambert lighting of the heightmap slope (`SUN`), material is unlit. The original
  instead reads a precomputed per-cell brightness byte fed by the level's own sunlight block, recomputed
  live on every height edit rather than derived from the rendered/bilinear slope at bake time (pop3-rev-analysis.md
  "Terrain land colour") — close in spirit (both are a height-gradient dot a light direction) but a different
  input (raw per-cell neighbour heights, not the baked mesh normal) and update timing.
- colour = `palette[bigfade[row][column]]`.
- When the simulation changes heights (`GameMap::step`'s `DirtyRect`s, kept in `world::TerrainPatches`), only those
  cells and 2 around them (`REBAKE_MARGIN`: slope light and coast read up to a cell around) are baked again into the
  texture (`rebake`), with the theme loaded once per level: a full bake is ~0.4 s, a land bridge changes cells
  every tick.

Now traced against the real game (see "bigfade"/"disp" above): the original has no flat `POP3_ROW_SCALE`-like
height multiplier at all — row is `height + 75 (+75 again unless the cell is dead flat)`, clamped to 1022, and
`disp`'s effect on the row is scaled by a fixed altitude-dependent curve (320..1024), not a constant. Our single
global `POP3_ROW_SCALE` is the wrong shape of knob: it should be replaced by the `height + 75[+75]` formula
directly (no scale to tune), with `disp`'s multiplier following the same 320..1024 altitude curve instead of a
single factor. The brightness curve is also now traced (a per-cell precomputed byte from local height gradient
dotted against the level's sun vector, not a baked Lambert light of the mesh slope) — see the bullet above.
Fallback (no data, or `--no-original`): `procedural_theme::generate(seed)` builds a palette
(8 bands x 32 brightness), a jittered bigfade and tileable value-noise disp with the same
shapes as the original files, so the same bake is used in both modes. Its `BANDS` table hardcodes the same
8 bands, in the same order, for every theme/seed — including a top "snow" band starting at row 960 applied
unconditionally. That has no basis in the original (see "Themes seen" above: only a few themes, e.g. `h`, go
near-white that high up, and it's theme data, not an elevation rule) and is why the reimplementation shows snow
at high elevation on every theme when running without the original asset files. See TODO.md "Terrain".

## Next
See TODO.md ("Terrain"): fade and cliff loading (`pop3_format::theme` loads pal, bigf, disp, watdisp and the
sky).
- Re-bake only the dirty rect after edits (now the whole 1024² texture is re-baked).
- Mipmaps (distant aliasing), lava and swamp layers.
- Free theme packs for redistribution (the original files must not be shipped). The community packs on
  thebeginning.uk/textures have no licence (site: "All rights reserved") and keep Bullfrog data (palette
  128..255, some ship the original `BL320`): fine to load from the user's `data/`, not to bundle.
