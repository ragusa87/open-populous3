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
- **Confirmed 2026-10-21, instruction-level (pop3-rev `docs/functions.tsv` 0x4C0EC0/0x4BF860/0x4C0380, all three
  pixel-fill dispatch targets): the `bigf0-X.dat` read is a single, un-blended lookup at one flat index, always.**
  No row+1 read exists anywhere in any of the three functions. The row component of that index is a hard floor of
  the interpolated altitude (`(altitude_fixed >> 16) << 8`, the exe explicitly zeroes the fixed-point value's own
  sub-integer fraction byte with an `and e?x, 0xFFFF00FF` right before the `>> 8` that builds this term) — the
  same truncated integer already used for the `DISP_SENSITIVITY` lookup, with zero fractional precision surviving
  into the bigfade index. See "Our bake" below for what this settles.
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
- **Corrected 2026-10-21: every one of these was measured against a corrupted install, not the genuine original
  (TODO.md "Terrain" - the `AppData/Local/Bullfrog/Populous/data/` leftover from a badly-uninstalled texture-pack
  extension, since deleted).** Re-measured all 36 `disp0-X.dat` files against the genuine 1998-dated
  `Program Files (x86)/Bullfrog/Populous - A l'aube de la création/` install: **every single theme spans the full
  raw byte range 0..255** (one, `disp0-f.dat`, 0..254) - none is narrow, and none stays on one side of 128 either,
  so the "no file crosses 127/128" claim above and the per-theme "narrow, off-centre range IS the bias" reasoning
  below are both wrong, not just the specific numbers. What *does* differ per theme is the raw mean, 82.9 (`s`) to
  167.5 (`g`/`i`) - a real but much gentler distributional skew than a hard range restriction, not strong evidence
  of a deliberate per-theme bias either way. Several theme letters share one file byte-for-byte (not independently
  authored per slot): `0`/`q`/`v`/`w`/`x`/`y`/`z` (a shared placeholder for unused slots), `c`/`d`, `l`/`m`. No
  code change needed - `pop3_format::theme::disp_mean()` (the thing "our bake's own 'subtract the file mean'"
  below refers to) is already dead code, called only by its own unit test, not from any live bake/shader path -
  but see "Our bake" for what, if anything, downstream reasoning assumed the old narrow ranges.
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
- **Corrected 2026-10-21, instruction-level (pop3-rev `docs/functions.tsv` 0x4C0EC0/0x4BF860/0x4C0380): the row
  jitter term's own shift hid the same kind of masking-before-shift trick `bigfade`'s row-index masking did,
  changing its effective SCALE, not just whether it's a blend.** `pop3_format::theme::disp_sensitivity`'s
  `(sensitivity * disp) / 1024` reads correctly as the per-row QUOTIENT, but the exe never uses that quotient
  directly at raw scale: it's computed via `and ecx, 0xFFFFFC03; sar ecx, 0x2` right after the multiply, which
  (verified algebraically and numerically, signed 32-bit, both signs of the product) always equals exactly
  `(quotient_by_1024) << 8` before being summed into the flat `bigfade` index — i.e. it's already expressed in
  row units (a multiple of 256) at the point it's added, the same way the row index itself is, not a small raw
  value comparable to the brightness column. Net: "moves the row much more on high ground" above is accurate in
  direction, but the actual magnitude can be large — on high terrain where `disp_sensitivity` reaches its 1024
  ceiling and a theme's own `disp` byte sits near its extreme, this term alone can shift the picked row by dozens
  of rows, not a handful. `game_core::shading`/`terrain_texture`'s own jitter must be added at the SAME scale as
  `row`, not at the scale of `brightness` — see "Our bake" below and `pop3-rev-analysis.md` "Terrain land colour"
  for the corrected, explicitly-scaled formula.

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

## Water: visual reference (clean-room screenshot observation, 2026-10-23)
A description of what the original's sea looks like in one screenshot, from direct visual inspection only - no
code, memory addresses, or trace knowledge folded in (unlike "Water rendering" in pop3-rev-analysis.md, which
*is* exe-trace material, kept clearly on that "dirty" side instead of here, per AGENTS.md's clean-room split).
Treat every figure below as approximate: the screenshot itself was about 380x340 px, likely downscaled from a
higher native resolution (probably 640x480 - multiply by roughly 1.7 for native-resolution sizes), and a single
still frame can't show animation at all (speed, whether motion is directional, how long a swirl persists, the
height pattern's real repeat period, or whether objects bob with the waves) - only a video or frame sequence
could.

- **Shape/placement**: a winding inlet/bay, roughly the central 60% of the frame (y~110 near the horizon to
  y~300 at the bottom), enclosed by snow-capped and grassy land on the left, right and bottom. Camera looks down
  at roughly 30-45 deg; the "curved world" bend (terrain.md "Planet illusion") foreshortens the water toward the
  top of the frame.
- **Colour/shading**: base colour a deep navy, roughly #1C2A5C to #2E4482. Darker, near-black-blue troughs and
  lighter, desaturated blue-grey crests are scattered through it. No sky reflection, no mirrored objects, no
  transparency (no visible seabed), no strong specular sun highlight - brightness varies in soft, irregular
  patches, reading as diffuse lighting on a slightly bumpy surface rather than sharp glints. Distant water near
  the horizon is somewhat darker and less contrasty than the foreground.
- **Surface pattern**: mottled and cellular, small irregular blotches (choppy water/ripples), no wave fronts,
  streaks or rows pointing in any one direction (no sense of a prevailing wind). Foreground features are about
  3-8 px across at screenshot scale (so roughly 5-14 px at native resolution); shrinking to 1-3 px toward the
  horizon from perspective. No obvious tiling repeat visible within the frame. The shading is consistent with a
  mesh with gentle height variation rather than a flat textured plane, though a still image can't confirm that on
  its own.
- **Shoreline**: no foam band - the water meets the land directly along an irregular edge, fairly hard-edged,
  with little blending; the land side has a pale sandy or snowy rim.
- **Objects on the water** (sizes at screenshot scale, for context - not specifically about the water surface
  itself): a single boat (hull + blue triangular sail) about 28x30 px; a pair of boats together about 65x40 px
  (each roughly 30-35 px wide); a splash/impact effect (small white/pale particles ~2-4 px each, a few tiny
  figures ~6-10 px tall, vs. ~8-12 px for people on the shore). Boats show no visible reflection or wake in this
  frame.

**Matched against this session's own rendering attempts** (`ground.wgsl`'s `water_colour`/`water_normal`, TODO.md
"Terrain" has the full history): an earlier attempt rendered individual features 150-300 px across on a
comparably-framed shot - 30-50x larger than this reference's 3-8 px - with hard two-colour banding (concentric
ring/contour-map look) instead of soft diffuse shading, traced to two compounding bugs: the sampled noise pattern
was tiled far too coarsely (once every ~32 cells, `PX_PER_CELL`'s own per-cell granularity with no further
scaling), and colour came from a discrete, rounded-to-integer `bigfade`/`palette` lookup (categorical by
construction) rather than a continuous function of the surface slope. Fixed by shrinking the tiling frequency
well below one cell (`WATER_FREQUENCY`) and replacing the palette lookup with a continuous normal-from-slope,
lit by a fixed direction, mixed smoothly between two measured colours - see "Our bake" below for the
implementation.

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
- Sea (outside the coast, below): `ground.wgsl`'s `vertex`/`water_colour` (pop3-rev-analysis.md "Water rendering
  (watdisp.dat)"), confirmed against [peterderivaz/populous](https://github.com/peterderivaz/populous) (README
  "References"). Full history of what was tried and reverted along the way: TODO.md "Terrain".

  `vertex` (a real vertex shader, `Material::vertex_shader`, the same embedded module as the fragment shader)
  displaces each pure-sea vertex's own `y` by `water_height_offset`: two reads of `watdisp` (`disp` when the file
  is missing, generated noise in the procedural theme), scrolled `turn % 256` texels in OPPOSITE directions and
  summed (mirrors `pop3_format::theme::Theme::water_shimmer`'s `height_offset = sum / 8`; `turn` reaches the
  shader as `params.w`/`ground_material::set_turn`, driven every frame by `world.rs::animate_water` independently
  of `rebuild_terrain` so the sea keeps rippling on frames nothing else touches) - converted into render units via
  the live `terrain_mesh::CurveParams::height_scale` (`ground_material::set_height_scale`, respects a runtime
  relief change, `hud/view_menu.rs`). The two counter-scrolling samples are deliberate, not incidental: a single
  scrolling sample would show a fixed wave pattern moving in a fixed direction (an obvious prevailing wind);
  summing two scrolling in opposite directions cancels that net drift while still producing lots of local swirls
  and seemingly non-uniform motion (confirmed: cross-correlating far-apart video frames finds no net directional
  shift, while consecutive frames show real, moderate, ongoing local variation over a broad deep-water sample).
  The vertex normal is tilted by that same height field's own local slope.

  Faded by `sea` (`water_sea_factor`), a factor gating how much of the above actually applies - **not**
  `COAST_THRESHOLD` (0.2, tuned purely for the land/water COLOUR boundary's own rounded shape - still used,
  unrelated, by `fragment`'s own branch pick below). Matches the exe's own documented rule (pop3-rev-analysis.md
  "Water rendering": "land and shore vertices take the real heightmap value instead, no watdisp involved" - a
  hard per-vertex binary, not a graded fade), via two corrections: a single point's own `walk` value (a bilinear
  average of up to 4 corners' own walkable flag, only ever the exact values 0/0.25/0.5/0.75/1.0) first replaced
  `COAST_THRESHOLD` directly - tight enough to stop a cell's own immediate coast neighbours rippling, but a small
  pond or inlet a few cells across still has an INTERIOR that reads `walk == 0` too, same as the open ocean does,
  by that test (reported, confirmed on real levels: a small 2x2 pond, a known coastal cell, and later a pond's own
  interior well past its shore, all rippled when they shouldn't have). `water_sea_factor` instead samples a ring
  of 8 points `WATER_OPENNESS_RADIUS` (3) cells out, plus the centre, taking the WORST (most land-like) of them -
  only when every one of those is also still "sea" does the ripple reach full strength, so a body of water has to
  be open for several cells around a point, not just at that exact point. Approximate (a ring can miss a thin
  obstacle between two sampled directions, and this measures "no land at roughly this radius", not the water
  body's true size or shape) but much closer to "only open ocean ripples" than a single point ever could be.
  `water_colour` (below) takes the fragment's own `water_sea_factor` value (computed once, passed in) and gates
  the SAME way (mixing its own normal towards flat "up" where it's low) so the shading goes still exactly where
  the geometry already does - verified: confirmed near-shore water pixels show zero variation over a fixed-seed,
  20 s capture, while a confirmed-deep region (several cells from any island, found from an aerial shot) still
  shows substantial ongoing change over the same window - a short (under 2 s) capture can under-sample how fast a
  specific deep-water location's own local swirl happens to be changing at that moment, so check over several
  seconds, not less, if re-verifying this.

  **Colour**: no palette/row lookup driven by the noisy height field (that produced hard contour-ring banding in
  an earlier attempt, read against a clean-room description of a real screenshot - "Water: visual reference"
  above - which also caught the ripple pattern being tiled 30-50x too coarsely, `WATER_FREQUENCY` now shrinking it
  well below one cell). Instead, `water_normal` (a centred finite difference, more accurate than the one-sided
  difference the vertex displacement above uses for itself) feeds a continuous `dot(normal, light)`, mixed
  smoothly between two colours - nothing here is rounded to an integer index, so nothing can band. Those two
  colours come from the THEME's own `bigfade` row 0 at two fixed brightness columns (`WATER_DARK_BRIGHTNESS`/
  `WATER_LIGHT_BRIGHTNESS`) - only two FIXED lookups per fragment, not varying with the noisy field, so this
  doesn't reintroduce banding; sourcing them from the theme keeps the sea in the same colour family as that
  theme's own land, rather than a fixed hue that can clash with an arbitrary generated palette (reported on one
  such palette: teal/brown land next to a universal navy sea). A reported red/lava-looking sea on another level
  turned out to be `land_colour`'s own `cliff` remap on a misclassified near-zero-height cell, not this function
  at all - see "coast" below and TODO.md "Terrain" for that one. Verified: no shader errors; measured pixel
  colours land in the visual reference's own colour family and order of magnitude; a close shot shows fine
  mottled texture, no banding/contour rings. Not yet re-confirmed against the original's own files directly (no
  original install available in this environment) - checked against the procedural theme here, against
  peterderivaz/populous's written description, and against the clean-room screenshot description above. Land:
  each corner's own
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
  legitimately roll into the next row, as in the original. **Fixed 2026-10-21 ("disp" above, "bigfade" above):
  "alongside the brightness column" was the bug** - the exe's own masking makes this term always a multiple of
  256 before it's summed, i.e. it moves the *row* (by up to dozens on steep/high terrain), not the column.
  `ground.wgsl` now computes `row = floor(altitude) + row_jitter_quotient` and multiplies the whole thing by
  `BIGFADE_WIDTH` once, instead of adding the raw quotient next to `brightness` - this, together with dropping
  the row/row+1 blend just below, is what actually resolved the white/pale patch on steep slopes (TODO.md
  "Terrain", docs/archived-plans/terrain-white-patch.md).
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
  **Resolved 2026-10-21, instruction-level (pop3-rev `docs/functions.tsv` 0x4C0EC0/0x4BF860/0x4C0380 — the open
  question TODO.md "Terrain" left pending, asking whether `landscape_texture_16_fade`'s own dithering pass or the
  original's typical camera distance/pitch is what hides this seam in practice): neither — the real game shows no
  seam because it never blends row with row+1 at all, in any of the three pixel-fill dispatch targets.** The
  "fraction" the original attempt tried to fold into the column axis is explicitly discarded by the exe itself:
  right before the shift that builds the row's contribution to the flat `bigf0-X.dat` index, the exe does
  `and e?x, 0xFFFF00FF` on the interpolated altitude's 16.16 fixed-point value — zeroing bits 15:8, the one byte
  that holds the interpolation's own live, per-pixel sub-integer fraction — so that term always comes out to
  exactly `(altitude_fixed >> 16) << 8`: the same floored integer already used for the `DISP_SENSITIVITY`/
  `row_jitter` lookup, times 256, with zero fractional precision surviving. The earlier reverted attempt's zigzag
  bug was almost certainly keeping that fraction alive (real per-pixel entropy, changing every few pixels as the
  Gouraud interpolation carries) instead of masking it away the way the exe explicitly does. `landscape_texture_
  16_fade`'s own "_fade" naming is a red herring here too: the identical masking, and the identical single,
  un-blended `bigf0-X.dat` read, is present byte-for-byte in `landscape_texture_32` (a 32-bit, non-"_fade"
  surface) and both code paths of `landscape_texture_16` — this is shared row-index arithmetic, not a dithering
  effect specific to the 16-bit "_fade" variant. **Our bake's own fix, then, is not "pick a correct blend" but
  "do no blend at all"**: drop the `row`/`row + 1` bilinear mix entirely and use a single, hard-floored row index
  — `row = floor(altitude)`, the same integer `disp_sensitivity` already keys off, with no interpolation of the
  row axis beyond what the ordinary Gouraud/bilinear altitude sampling already gives it. See TODO.md "Terrain".
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
- coast: walkers cross every cell not sea on all four corners (`terrain_texture::walkable_cells`,
  `game_core::unit::is_sea`), so the coast is drawn round those cells, not where the bilinear height reaches 1
  (that left most of a shore cell blue and units looked like they walked on water). The `channels` texture's A
  channel is 1 at a walkable cell's own texel, 0 at sea, bilinearly sampled; ground where it reaches 0.2
  (`COAST_THRESHOLD` in `ground.wgsl`). Covers a lone walkable cell's corner (0.25 there), puts straight coasts
  0.3 cell past the walkable edge and rounds the corners. Ground there, even at height 0, takes the lowest land
  row. `walkable_cells`'s own render-only sea test (`terrain_texture::looks_like_sea`) has since diverged slightly
  from `game_core::unit::is_sea` itself - see TODO.md "Terrain" ("Sea level"): a cell at a very low but nonzero
  height now also reads as "sea" for this one purpose (height <= `VISUAL_SEA_HEIGHT`, a guess), so it renders as
  water instead of through `land_colour`'s own `cliff` remap, while staying exactly as walkable as before for
  everything gameplay (pathing, drowning, building placement all still read `is_sea` directly, untouched).
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
