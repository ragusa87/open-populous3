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
sprites and fog of war (PopResourceEditor: "shadows or Fog of War") - **confirmed unrelated to ground/terrain
rendering from the exe itself (2026-10-20)**: all three of `set_texture_landscape_main`'s own pixel-fill
dispatch targets (`landscape_texture_32`/`16`/`16_fade`, pop3-rev/docs/functions.tsv) were checked for any
reference to a distinct 64x256 table and found none - all three read only the already-documented
`bigf0-X.dat`/cliff/`DISP_SENSITIVITY` tables; `landscape_texture_16_fade`'s own "_fade" name is a 16-bit-surface
colour-dithering step, not a use of this file. So implementing `fade0-X.dat` consumption would not touch ground
colour at all, faithful or otherwise - it would only matter for whatever does read it on objects/sprites/fog of
war (not yet traced), a separate, unrelated TODO item from the ground brightness/contrast issue below.

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

## Our bake, now a shader (`game-client/src/ground_material.rs`, `ground.wgsl`, `terrain_texture.rs`)
Matches the traced exe formula (see "bigfade"/"disp"/"cliff" above) **and**, since 2026-10-20, its architecture:
the exe has no baked ground texture at all - `set_texture_landscape_main` recomputes the formula below fresh, per
rendered screen pixel, every frame, at whatever resolution the quad is currently drawn at
(`landscape_texture_32`/`16`/`16_fade`, pure CPU rasterization, no D3D texture involved). A previous version of
this bake instead pre-baked the formula into one fixed-resolution (8 px/cell) RGBA texture and let the GPU
filter/minify it like an ordinary texture-mapped surface — cheaper to write, but it needed its own mip chain to
avoid minification aliasing at distance (TODO.md "Terrain", the mip-chain entry, now fixed) that the exe's own
architecture never needed. `ground.wgsl` instead evaluates the formula per fragment, at the engine's own output
resolution, same as the exe: `terrain_texture.rs` only prepares small per-cell scalar buffers (`bake_cells`/
`rebake_cells`), `ground_material.rs` uploads them as GPU textures the shader bilinear/`textureLoad`-samples.
UV = absolute cell / map size, same mapping the old baked texture used; the shader reads an integer cell
coordinate as a half-texel-offset UV (`ground.wgsl`'s own doc comment) so ordinary hardware bilinear filtering of
a grid-vertex-aligned, repeat-wrapped texture reproduces the old hand-written corner bilinear exactly.
- `game_core::shading::CellShading` (`GameMap::shading`) holds the per-cell `blend`/`light`/`downhill` the
  formula below reads, recomputed after every height edit (`GameMap::recompute_shading`, hooked into `apply` and
  `tick` centrally) rather than derived from the rendered mesh at bake time; `terrain_texture::rebake_cells`
  copies `blend`/`light` (and ash coverage, and the walkable flag) into the shader's `channels` texture (RGBA8)
  one texel per cell, no pixel-level work at all any more.
- row = water (outside the coast, below): `watdisp * 128 / 256` (`sea_row` in `ground.wgsl`, the whole water
  band; `disp` when the file is missing, generated noise in the procedural theme); land: each corner's own
  `altitude_index = clamp(height + 75 + (75 if blend != 0 else 0), 0, 1022)` (`terrain_texture::altitude_cell`,
  no averaging), bilinearly sampled from the shader's `altitude` texture (one half-float per cell), then floored
  at `land_start` (`Theme::land_start_row`, the first row from 128 up where most columns are neither water
  colours nor sea-like) — a renderer-side safety net, not in the traced formula itself, so a freshly flattened
  low platform (`blend` 0, no second +75) still reads as ground. **Checked exe-side 2026-10-20 (pop3-rev
  `0x4BE330` corner-computation trace, TODO.md "Terrain"): the original has no such floor at all** - its row
  formula only ever reads height/blend/light, clamped 0..1022, with no per-theme or per-shape minimum; a flat
  low platform reads sea-like in the original too, unguarded. `land_start` is a pure invention with no original
  basis, not a faithful reproduction of a real mechanism - see TODO.md for the open question on whether it
  should be removed outright. A pixel's own `disp` sample (signed) then
  jitters the row by `(disp_sensitivity(row) * disp) / 1024` (`pop3_format::theme::disp_sensitivity`), added
  straight into the flat `bigfade` index alongside the brightness column — a large jitter or brightness can
  legitimately roll into the next row, as in the original.
- Unlike the deleted CPU bake, the `bigfade`/`cliff`/`palette` chain here is a `textureLoad` every fragment at
  full render resolution (categorical, no filtering): an exact row pick shows a hard edge wherever `row` changes
  between neighbouring fragments, which the old baked 8 px/cell texture never did (the GPU's own bilinear
  *magnification* blended neighbouring already-baked colours for free). `ground.wgsl` reproduces that blend
  directly instead: the two colours the continuous altitude's own fractional part sits between (`row`/
  `row + 1`, same `brightness`/jitter) are each resolved through `cliff`/`palette` and linearly mixed by that
  fraction — the same thing hardware bilinear filtering would do if `bigfade` were addressable as a filterable
  texture along its row axis. (An earlier attempt instead tried folding the row's own fraction directly into the
  flat `bigfade` index as the exe's disassembly pseudocode seemed to describe; that reading turned out to be
  mis-derived — it made the colour cycle through a full brightness sweep every single height unit, a dense,
  clearly-wrong contour/zigzag pattern, reverted 2026-10-20.)
- **A pale/near-white patch on a slope is not necessarily a bug**: verified directly against `data/cliff0-s.dat`/
  `pal0-s.dat` (theme 28/`s`, level 2) this session — land-colour 106 maps to palette index 129 (229, 220, 214,
  an intentional pale "weathered rock" theme colour) at every nonzero cliff row, so *any* sloped cell whose
  `bigfade` lookup happens to land on colour 106 shows this pale tone; the un-shader'd `main` renderer shows the
  same patches in the same places, just blurred by its own baked-texture magnification. Before treating a pale
  patch as a rendering bug, check the theme's own `cliff`/`palette` files for a legitimate near-white entry
  first.
  `disp`/`water` are bilinearly sampled (continuous `px`/`py`, not floored first) rather than `textureLoad`-exact
  like `bigfade`/`cliff`, matching their own noise being a smooth, continuously-tiling signal rather than a
  categorical table (`GroundMaterial::disp`'s own doc comment).
- coast: walkers cross every cell not sea on all four corners (`terrain_texture::walkable_cells`, `is_sea`), so
  the coast is drawn round those cells, not where the bilinear height reaches 1 (that left most of a shore cell
  blue and units looked like they walked on water). The `channels` texture's A channel is 1 at a walkable cell's
  own texel, 0 at sea, bilinearly sampled; ground where it reaches 0.2 (`COAST_THRESHOLD` in `ground.wgsl`).
  Covers a lone walkable cell's corner (0.25 there), puts straight coasts 0.3 cell past the walkable edge and
  rounds the corners. Ground there, even at height 0, takes the lowest land row.
- brightness column: `shading.light` (sunlight-derived, bilinearly sampled per corner like `blend`) plus a small
  extra perturbation from `disp`'s own row-to-row difference, clamped 0..255 — not a baked Lambert light of the
  rendered mesh slope any more. `game_core::shading::SunParams::default` is the sun vector: live-measured and
  disassembly-traced (pop3-rev-analysis.md "Terrain land colour"), `sunX`/`sunZ`/`sunBase` are a flat hardcoded
  `147`/`147`/`147` in normal play, independent of the level's own `ShadeStart`/`ShadeRange`/`Inclination`
  (`pop3_format::level::Sunlight`) entirely (confirmed identical across two levels with different on-disk
  `Inclination`), so the level's sunlight block is read and stored but no longer feeds `SunParams` at all.
  `shading.light`'s own neighbour reads were briefly a 2-real-cell baseline (`height(x, z) - height(x + 2, z)`,
  read off `compute_cell_light`'s raw argument literally) before a 2026-10-20 correction: that argument is double
  open-populous's real file-cell coordinate (proven against `load_level2`'s height-table load loop,
  pop3-rev-analysis.md "Terrain land colour"), so the real baseline is one real cell - `height(x + 1, z)` /
  `height(x, z + 1)`, as `game_core::shading::cell_light` now reads. A renderer-only `soften_light` inset
  (`terrain_texture.rs`, `LIGHT_SOFT_MIN`/`MAX` 32..223) still caps how close the stored byte gets to the hard
  0/255 clamp ends, kept as a deliberate visual safety net independent of that fix (TODO.md "Terrain").
- colour = `palette[cliff[blend / 4][bigfade[row][column]]]`: the bigfade lookup, then remapped through the
  cliff table (bilinearly-sampled `blend` picks the row) before the palette lookup - identity at `blend < 4`.
  Ash charring (a flat RGB darken towards a fixed dark colour, by the same bilinearly-sampled per-cell coverage
  the old `char_ashes` used) happens after, in the same fragment; `game-client/src/lava.rs`'s `LavaView` overlay
  mesh stays wired alongside this for now, pending further visual verification against more original levels and
  effects before it's removed (TODO.md "Terrain").
- When the simulation changes heights (`GameMap::step`'s `DirtyRect`s, kept in `world::TerrainPatches`), only
  those cells and 2 around them (`REBAKE_MARGIN`: `CellShading::recompute`'s own neighbour reach) get their
  texel rewritten (`rebake_cells`), with the theme's LUT textures uploaded once per level. Height gliding
  mid-tick (`world::TerrainGlide`) now lerps the one `altitude` scalar that feeds the shader, not the finished
  RGB colour the old pixel-level glide used to blend — same documented simplification (`blend`/`light` are not
  themselves glided), moved to the input instead of the output.
- **Colour-space bug, fixed (2026-10-20):** the rendered ground read uniformly too bright/washed-out next to the
  original, with the shoreline's darker, rockier "cracked" look (the `cliff` table's normal effect wherever
  `blend` is high, which a coastline's own big land/seabed height delta makes true almost everywhere along a
  shore, see "cliff" above) barely visible, and a stray near-white patch on ground next to a shore. Bevy's
  tonemapping was ruled out as the cause: the game camera (`camera::spawn_camera`) never adds the `Hdr` marker
  component, and `bevy_core_pipeline`'s tonemapping node itself no-ops without it (`if !camera.hdr { return; }`),
  confirmed by reading the installed `bevy_core_pipeline`/`bevy_render` 0.19 source directly, not assumed. Actual
  cause: `ground_material.rs::palette_image` stored the palette LUT as `TextureFormat::Rgba8Unorm` - every
  *other* texture in the codebase built from the original's own palette-derived pixel data (`sky.rs`,
  `original_models.rs`, `units/mod.rs`, `lightning.rs`, `sparkles.rs`, `flame.rs`, `lava.rs`'s overlay,
  `tooltip.rs`) uses `Rgba8UnormSrgb` instead, so this was the one outlier, with no comment explaining why.
  `ground.wgsl`'s `palette_rgb` then `textureLoad`s that LUT and returns the raw byte/255 straight as the
  fragment's output colour; since the swapchain target is itself sRGB-formatted and GPUs encode a shader's output
  through the sRGB curve on store to an `Rgba8UnormSrgb` target, a palette byte that is already gamma-encoded
  (meant to be shown as-is) got a second sRGB encode applied on top - which lifts darker values much more than
  bright ones (compounding, not additive), exactly matching "too bright everywhere" and lost shadow/cliff
  contrast, and turned a moderately dark cliff/rock colour pale enough to read as a white patch against grass.
  Confirmed by a `just shot` before/after on level 1: storing `palette_image` (and its placeholder) as
  `Rgba8UnormSrgb` instead visibly restores ground contrast and the shoreline's darker, rockier look, replacing
  the near-uniform pale sand.

Fallback (no data, or `--no-original`): `procedural_theme::generate(seed)` builds a palette
(8 bands x 32 brightness), a jittered bigfade and tileable value-noise disp with the same
shapes as the original files, plus a synthetic `cliff`/identity `fade` (`synthetic_cliff`/`identity_fade`), so
the same bake is used in both modes. Its `BANDS` table hardcodes the same 8 bands, in the same order, for every
theme/seed — including a top "snow" band starting at row 960 applied unconditionally, and row boundaries that
were never recalibrated for how much further the real, unscaled height + blend formula now reaches into the
table (TODO.md "Terrain": both need their own pass).

## Next
See TODO.md ("Terrain") for the open follow-ups: removing the ash-charring shader step/`LavaView` once visually
verified, switching `lava::downhill` to the precomputed field, the procedural `BANDS`/`synthetic_cliff`
recalibration, the theme-aware snow band, and consuming `fade0-X.dat` (loaded, nothing reads it yet).
- Curvature in a vertex shader over a static height texture (TODO.md "Performance"/"Terrain") - geometry, not
  colour: still unaddressed, `terrain_mesh.rs` still computes curved vertex positions CPU-side every rebuild.
- Free theme packs for redistribution (the original files must not be shipped). The community packs on
  thebeginning.uk/textures have no licence (site: "All rights reserved") and keep Bullfrog data (palette
  128..255, some ship the original `BL320`): fine to load from the user's `data/`, not to bundle.
