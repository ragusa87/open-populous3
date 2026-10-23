// Ground colour, per fragment (docs/specs/terrain-textures.md "Our bake", "bigfade"/"disp"/"cliff"): the exact
// same formula the deleted CPU `pixel_rgb` evaluated per baked texel, now evaluated per output pixel instead -
// see `terrain_texture.rs`'s module doc for why. One deliberate deviation: `disp`/`water` (the detail-noise
// lookups, below) are bilinearly sampled rather than `textureLoad`-exact like `bigfade`/`cliff` - the old baked
// texture got equivalent smoothing for free from the GPU's own bilinear magnification of its coarse 8 px/cell
// bake, which an exact per-fragment lookup here would lose (visible as harsh, blocky light/dark patches on
// steep, high terrain where `disp_sensitivity` is near its max - see `GroundMaterial::disp`'s own doc comment).
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    view_transformations::position_world_to_clip,
}

// x: map size (cells), y: grid overlay on/off, z: PX_PER_CELL, w: turn % 256 (the sea's own scroll phase,
// `ground_material::set_turn`).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>;
// R blend, G light, B ash coverage, A walkable (0/1): bilinear, repeat-wrapped, grid-vertex-aligned.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var channels_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var channels_samp: sampler;
// Precombined height+blend altitude index (terrain-textures.md "bigfade"), stored as a half-float: bilinear,
// repeat-wrapped, grid-vertex-aligned, same as `channels_tex`.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var altitude_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var altitude_samp: sampler;
// 256x1 RGBA8: palette[index].rgb. Read with textureLoad (exact index, no interpolation).
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var palette_tex: texture_2d<f32>;
// 256x1152 R8Uint: bigfade[row][column] -> palette index.
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var bigfade_tex: texture_2d<u32>;
// 128x64 R8Uint: cliff[row][land colour] -> remapped palette index.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var cliff_tex: texture_2d<u32>;
// 256x256 R8Snorm: the signed detail noise (terrain-textures.md "disp"). Bilinear, repeat-wrapped: see its own
// doc comment on `GroundMaterial::disp` for why this one isn't `textureLoad`-exact like `bigfade`/`cliff`.
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var disp_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var disp_samp: sampler;
// 256x256 R8Unorm: the sea's own precomputed ripple pattern (`watdisp.dat`), unsigned. Bilinear, repeat-wrapped,
// same reason as `disp_tex`.
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var water_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var water_samp: sampler;
// The dev-mode grid's hovered cell (`cell_debug.rs`, `GroundMaterial::highlight`): x, y its own coordinates, z
// 1.0/0.0 on or off, w unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var<uniform> highlight: vec4<f32>;
// x: terrain_mesh::CurveParams::height_scale (`ground_material::set_height_scale`), the sea's vertex-stage ripple
// displacement's own render-unit conversion. y, z, w: unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var<uniform> water_params: vec4<f32>;

const BIGFADE_WIDTH: i32 = 256;
const BIGFADE_ROWS: i32 = 1152;
const CLIFF_WIDTH: i32 = 128;
const CLIFF_ROWS: i32 = 64;
const DISP_SIZE: f32 = 256.0;
// Where walkers cross every cell not sea on all four corners (`coast_land`'s own threshold in the deleted CPU
// path): the `channels` texture's bilinearly-sampled A (walkable) channel reaching this is "ground".
const COAST_THRESHOLD: f32 = 0.2;
// `fragment`'s own coast colour blend: half-width (in `walk` units) and how much the blend's centre wobbles from
// `disp` noise - **our own invention, not original behaviour**, see `fragment`'s own doc comment on why.
const COAST_BLEND_WIDTH: f32 = 0.16;
const COAST_BLEND_JITTER: f32 = 0.12;
const ASH_COLOUR: vec3<f32> = vec3<f32>(38.0 / 255.0, 33.0 / 255.0, 30.0 / 255.0);
// Brightness (out of 256) the grid overlay darkens a cell's own first 1/8th (dev-only, our own invention, not
// original behaviour): matches the deleted CPU path's `GRID_SHADE`/`PX_PER_CELL`.
const GRID_SHADE: f32 = 190.0 / 255.0;
// The hovered-cell tint (dev-only, our own invention): a warm highlight mixed in at this strength.
const HIGHLIGHT_COLOUR: vec3<f32> = vec3<f32>(1.0, 0.95, 0.4);
const HIGHLIGHT_STRENGTH: f32 = 0.35;

// `disp`'s own raw byte, read signed (pop3-rev-analysis.md "Terrain land colour": no re-centring, no per-theme
// constant - the game reads it straight as i8), bilinearly sampled at the continuous (pxf, pyf) pixel coordinate
// (`GroundMaterial::disp`'s own doc comment): the texture is R8Snorm, so the signed decode is free, and the
// result is a continuous float rather than the single discrete byte `textureLoad` would give.
fn disp_at(pxf: f32, pyf: f32) -> f32 {
    let uv = vec2<f32>(pxf + 0.5, pyf + 0.5) / DISP_SIZE;
    return textureSample(disp_tex, disp_samp, uv).r * 127.0;
}
// An explicit LOD, not an implicit (derivative-based) one: the only kind valid outside the fragment stage
// (`vertex`, below, needs it) - used in the fragment stage too, for one consistent height field on both sides.
fn water_at_level(pxf: f32, pyf: f32) -> f32 {
    let uv = vec2<f32>(pxf + 0.5, pyf + 0.5) / DISP_SIZE;
    return textureSampleLevel(water_tex, water_samp, uv, 0.0).r * 255.0;
}

// The sea's own ripple height (pop3-rev-analysis.md "Water rendering (watdisp.dat)", confirmed by
// [peterderivaz/populous](https://github.com/peterderivaz/populous), README "References"): two reads of
// `water_at_level`, scrolled `shift` texels in OPPOSITE directions, summed. Not a directional flow: a single
// scrolling sample alone would show a fixed wave pattern moving in a fixed direction (a visible prevailing wind);
// summing two counter-scrolling samples cancels that net drift while still producing lots of local swirls and
// seemingly non-uniform motion - the sea should read as animated water, never as a frozen static surface, but
// never as a simple flow in one direction either.
fn water_height_offset(pxf: f32, pyf: f32, shift: f32) -> f32 {
    let s1 = water_at_level(pxf - shift, pyf - shift);
    let s2 = water_at_level(pxf + shift, pyf + shift);
    return (s1 + s2) / 8.0;
}

// **Fixed 2026-10-23**, against a clean-room visual description of an original screenshot (not our own
// implementation notes - see TODO.md "Terrain" for the full critique): `water_at`/`water_height_offset` are
// sampled at `PX_PER_CELL`'s own per-cell granularity (8 texels/cell), tiling the 256-texel `watdisp` once every
// 32 cells - individual features read as smooth blobs 150-300 screen px across, 30-50x larger than a real
// screenshot's measured 3-8 px, reading as a contour map rather than choppy water. `WATER_FREQUENCY` shrinks the
// sampled pattern well below one cell, tiling every few cells instead of dozens.
const WATER_FREQUENCY: f32 = 24.0;

// The slope of the height field at `(pxf, pyf)`, as a normal (`-dx`/`-dz` tilt, `1.0` up - the standard height-
// field-to-normal construction): a CENTRED finite difference (`+e`/`-e` each axis), not the one-sided
// `h(p) - h(p + e)` this used before - the one-sided version is a cheaper but less accurate slope estimate;
// cheap enough here to afford the better one. `WATER_NORMAL_SCALE` tames how strongly the (otherwise raw-byte-
// scale) height delta tilts the normal.
const WATER_NORMAL_SCALE: f32 = 0.05;
fn water_normal(pxf: f32, pyf: f32, shift: f32) -> vec3<f32> {
    let e = 1.0;
    let dx = water_height_offset(pxf + e, pyf, shift) - water_height_offset(pxf - e, pyf, shift);
    let dz = water_height_offset(pxf, pyf + e, shift) - water_height_offset(pxf, pyf - e, shift);
    return normalize(vec3<f32>(-dx * WATER_NORMAL_SCALE, 1.0, -dz * WATER_NORMAL_SCALE));
}

// How far from any land a point must be before the sea's own ripple applies at full strength.
//
// **Fixed 2026-10-23, three times.** First version checked only the point's own immediate `walk` value (a
// bilinear average of up to 4 corners' own walkable flag, `terrain_texture::walkable_cells`) - correctly stopped a
// cell's own immediate coast neighbours from rippling (confirmed, level 8 cell 89/90), but a small pond or inlet a
// few cells across still has an INTERIOR that's just as "fully surrounded by sea" as the open ocean is, by that
// single-point test - reported, confirmed on a real level: a small pond kept changing elevation even well inside
// its own shore (level 1, cell 6/101).
//
// Second version sampled a sparse RING of 8 points at exactly `WATER_OPENNESS_RADIUS` cells out (plus the centre)
// and took the WORST (most land-like) of them. Fixed the pond case, but reported again (level 1, cell 9/102): its
// own edge traces a visibly different, facetted/octagonal "convex corner" next to the coastline's own smooth,
// naturally-rounded one (`fragment`'s `COAST_THRESHOLD` cutoff on a single bilinear `walk` sample) - the two no
// longer read as one coherent coast, each doing its own incompatible corner shape. Root cause: a `max` of any
// FIXED, finite set of `walk` samples is a facetted shape by construction (its level set has a corner everywhere
// the arg-max sample changes), no matter how many are checked or how they're arranged - swapping the sparse ring
// for a dense filled disc of ~29 samples (every cell offset within the radius, not just its boundary) was tried
// and still showed the same kind of straight-segment facets (confirmed visually, rendering `water_sea_factor`
// alone as greyscale and overlaying its edge against the coastline's own) - a different bug than a missed
// direction, inherent to combining samples with `max` at all.
//
// Fixed instead by computing an actual EUCLIDEAN DISTANCE to the nearest land-like sample within the radius (over
// that same filled-disc neighbourhood) and smoothstepping on THAT, not on a `max` of raw `walk` values. A true
// distance field's level sets are circular arcs around a convex corner (the defining property of a Minkowski
// dilation by a disc) - continuous and round, the same quality the coastline's own bilinear corner-rounding has,
// instead of a `max`-of-samples' inherent facets. Confirmed visually: `water_sea_factor` rendered alone no longer
// shows straight facet segments at level 1, cell 9/102, its edge now a smooth curve alongside the coastline's own.
const WATER_OPENNESS_RADIUS: f32 = 3.0;
const WATER_OPENNESS_RADIUS_CELLS: i32 = 3;
fn water_sea_factor(cell_uv: vec2<f32>, size: f32) -> f32 {
    // Past this, "nearest land" is simply "not found" - any distance beyond the radius reads the same as one
    // just past it, so the sentinel value itself never affects the smoothstep below.
    let far = WATER_OPENNESS_RADIUS + 1.0;
    var min_d2 = far * far;
    for (var dz = -WATER_OPENNESS_RADIUS_CELLS; dz <= WATER_OPENNESS_RADIUS_CELLS; dz = dz + 1) {
        for (var dx = -WATER_OPENNESS_RADIUS_CELLS; dx <= WATER_OPENNESS_RADIUS_CELLS; dx = dx + 1) {
            let d2 = f32(dx * dx + dz * dz);
            if (d2 < min_d2) {
                let offset = vec2<f32>(f32(dx), f32(dz)) / size;
                let walk_here = textureSampleLevel(channels_tex, channels_samp, cell_uv + offset, 0.0).a;
                if (walk_here > COAST_THRESHOLD) {
                    min_d2 = d2;
                }
            }
        }
    }
    // One cell of smoothstep width: wide enough to blend the quantised cell-offset distances (0, 1, sqrt(2), 2,
    // ...) this samples into a continuous fade, same spirit as `COAST_THRESHOLD`'s own `fragment`-side AA. No
    // `1.0 -` here, unlike the old `max_walk`-based versions: `dist` already runs the OPPOSITE way (it grows
    // FARTHER from land, `max_walk` grew CLOSER to it), so smoothstep on `dist` directly already gives 0 near
    // land and 1 in open water - negating it was a sign-flip bug (regression, 2026-10-23: ripple ran right up to
    // the coast and open water sat still, exactly backwards) carried over by analogy from the old formula's own
    // `1.0 -` without checking which way the new quantity actually runs.
    return smoothstep(WATER_OPENNESS_RADIUS - 1.0, WATER_OPENNESS_RADIUS, sqrt(min_d2));
}

// The sea's own vertex-stage ripple: a near-copy of the default mesh vertex shader (`bevy_pbr::render::mesh::
// vertex`) with one addition. Confirmed by peterderivaz/populous (see `water_height_offset`'s own doc comment):
// "the altitudes of the sea vertices... [are] adjusted based on the sea texture in watdisp.dat" - so this
// displaces the vertex itself, not just its colour. Faded out by `sea` (`water_sea_factor`, above) so the mesh
// doesn't pop at the boundary; the normal is tilted by the same height field's own local slope and blended in by
// the same `sea` factor, for `water_colour`'s matching lighting response (below) to read as a direct consequence
// of this same ripple, not a separate effect.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);

    let size = params.x;
    let px_per_cell = params.z;
    let shift = params.w;
    let height_scale = water_params.x;

    let x = vertex.uv.x * size;
    let z = vertex.uv.y * size;
    let cell_uv = vec2<f32>(x + 0.5, z + 0.5) / size;
    let sea = water_sea_factor(cell_uv, size);

    var position = vertex.position;
    var normal = vertex.normal;
    if (sea > 0.0) {
        let pxf = x * px_per_cell * WATER_FREQUENCY;
        let pyf = z * px_per_cell * WATER_FREQUENCY;
        let h_c = water_height_offset(pxf, pyf, shift);
        position.y = position.y + h_c * height_scale * sea;
        normal = normalize(mix(normal, water_normal(pxf, pyf, shift), sea));
    }

    out.world_normal = mesh_functions::mesh_normal_local_to_world(normal, vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.uv = vertex.uv;
    return out;
}

// The sea branch's full per-fragment colour (pop3-rev-analysis.md "Water rendering (watdisp.dat)").
//
// **Fixed 2026-10-23**: previously looked up a discrete `bigfade`/`palette` entry from a rounded brightness -
// exactly the "2-entry palette", "height quantised before shading" failure a clean-room critique of a real
// screenshot described, producing hard-edged contour rings instead of the original's soft, irregular diffuse
// shading ("crests catch light and troughs go dark, with soft gradients in between"). Fixed: `water_normal`'s own
// continuous tilt is lit by a fixed direction and the result mixes SMOOTHLY between two colours - a continuous
// `vec3<f32>` blend has no discretisation to band at all, unlike indexing a table by a rounded brightness.
//
// **Fixed again, same day**: those two endpoint colours were first a pair of universal constants (measured off
// one reference screenshot, #1C2A5C..#2E4482) - reported as visibly clashing with a generated theme whose own
// land colours (this being procedural, not the original's own data) happened to be teal/brown, not blue-adjacent.
// Tried sourcing both endpoints from the THEME's own `bigfade` row 0 instead (the "sea row" convention
// terrain-textures.md's own PopResourceEditor note describes), with a "fall back unless it's actually
// blue-dominant" guard against a theme whose own row 0 isn't water-coloured - reported in turn as the wrong fix:
// level 38's own water rendered red, more likely because that water is actually going through `land_colour`'s
// own `cliff` remap (TODO.md "Terrain": a documented, open class of bug where very-low-but-nonzero-elevation
// ground - not exactly `is_sea`'s own height-0 test - reads as land, and a steep drop to the seabed there pushes
// `blend` high enough to hit the cliff table's own "damaged ground, lava" rows), not through this function at
// all - no guard in `water_colour` could fix a cell that never reaches it. Left as the theme's own `bigfade` row
// 0 for now, not reverted further, pending that separate classification question - see TODO.md "Terrain".
// **Fixed 2026-10-23, alongside `vertex`'s own fix above**: this used to light every water fragment from the
// full ripple normal regardless of how close to the coast (or how small the body of water) was - so even where
// `vertex` no longer displaces a near-coast/small-pond cell's geometry, its colour still shimmered as if it had.
// Both need to agree: `sea` (`water_sea_factor`, computed once in `fragment` and passed in rather than
// re-sampled) gates the SAME way, mixing towards a flat "up" normal where it's low so the shading goes still
// exactly where the geometry already does.
const WATER_LIGHT_DIR: vec3<f32> = vec3<f32>(0.5, 0.35, 0.35);
const WATER_ROW: i32 = 0;
const WATER_DARK_BRIGHTNESS: i32 = 200;
const WATER_LIGHT_BRIGHTNESS: i32 = 60;
fn water_colour(x: f32, z: f32, px_per_cell: f32, shift: f32, sea: f32) -> vec3<f32> {
    let pxf = x * px_per_cell * WATER_FREQUENCY;
    let pyf = z * px_per_cell * WATER_FREQUENCY;
    let n = mix(vec3<f32>(0.0, 1.0, 0.0), water_normal(pxf, pyf, shift), sea);
    let lit = clamp(dot(n, normalize(WATER_LIGHT_DIR)), 0.0, 1.0);
    let deep = palette_rgb(bigfade_flat(WATER_ROW * BIGFADE_WIDTH + WATER_DARK_BRIGHTNESS));
    let crest = palette_rgb(bigfade_flat(WATER_ROW * BIGFADE_WIDTH + WATER_LIGHT_BRIGHTNESS));
    return mix(deep, crest, lit);
}

// How much `disp` jitters a bigfade row at that altitude (`pop3_format::theme::disp_sensitivity`): flat 320
// below row 129, a +3/row ramp to 1024 at row 362, flat above.
fn disp_sensitivity(row: i32) -> f32 {
    return clamp(3.0 * f32(row) - 64.0, 320.0, 1024.0);
}

// `bigfade` read as one flat array (terrain-textures.md "bigfade"/"Our bake"): a large jitter or brightness can
// legitimately roll the index into the next row, as in the original - not guarded against, only clamped to the
// table's own bounds.
fn bigfade_flat(index: i32) -> u32 {
    let max_index = BIGFADE_WIDTH * BIGFADE_ROWS - 1;
    let i = clamp(index, 0, max_index);
    return textureLoad(bigfade_tex, vec2<i32>(i % BIGFADE_WIDTH, i / BIGFADE_WIDTH), 0).r;
}

// The cliff/lava/ash remap (terrain-textures.md "cliff"): anything that isn't a land colour (>= CLIFF_WIDTH, a
// water colour) passes through unchanged, the table has no entry for it.
fn cliff_remap(row: i32, colour: u32) -> u32 {
    if (colour >= u32(CLIFF_WIDTH)) {
        return colour;
    }
    let r = clamp(row, 0, CLIFF_ROWS - 1);
    return textureLoad(cliff_tex, vec2<i32>(i32(colour), r), 0).r;
}

fn palette_rgb(index: u32) -> vec3<f32> {
    return textureLoad(palette_tex, vec2<i32>(i32(index), 0), 0).rgb;
}

// `d`'s shortest-path signed distance on a torus of circumference `size` (same idea as `grounded.rs`'s own
// `wrap`): the disc's cell coordinates are absolute and unwrapped (`terrain_mesh::build`'s own doc comment), so a
// cell picked by its wrapped coordinates (`cell_debug.rs`) still matches every fragment that draws it.
fn wrap_signed(d: f32, size: f32) -> f32 {
    var w = (d + size * 0.5) % size;
    if (w < 0.0) {
        w = w + size;
    }
    return w - size * 0.5;
}

// The land branch's full per-fragment formula (pop3-rev-analysis.md "Terrain land colour": a single hard-floored
// row, `row_jitter` folded in at row scale, no row+1 blend - see `fragment`'s own doc comment on why there's no
// blend). Factored out so `fragment` can evaluate it at several jittered sub-pixel positions and average the
// results: the `bigfade`/`cliff`/`palette` chain is categorical (`textureLoad`, not filtered), so an exact
// per-fragment row pick shows a hard edge wherever `row` changes between neighbouring fragments - harsher now
// that it's a true single row instead of the old (unfaithful) row/row+1 blend that incidentally smoothed it.
fn land_colour(x: f32, z: f32, size: f32, px_per_cell: f32) -> vec3<f32> {
    let cell_uv = vec2<f32>(x + 0.5, z + 0.5) / size;
    let pxf = x * px_per_cell;
    let pyf = z * px_per_cell;
    let d_here = disp_at(pxf, pyf);
    let d_below = disp_at(pxf, pyf + 1.0);
    let ch = textureSample(channels_tex, channels_samp, cell_uv);
    let light = ch.g * 255.0;
    let brightness = clamp(i32(round((d_below - d_here) / 4.0 + light)), 0, 255);
    let altitude = textureSample(altitude_tex, altitude_samp, cell_uv).r;
    let altitude_int = i32(floor(altitude));
    let row_jitter = i32(round(disp_sensitivity(altitude_int) * d_here / 1024.0));
    let row = altitude_int + row_jitter;
    let blend = i32(round(ch.r * 255.0));
    let cliff_row = blend / 4;
    var colour = palette_rgb(cliff_remap(cliff_row, bigfade_flat(row * BIGFADE_WIDTH + brightness)));
    colour = mix(colour, ASH_COLOUR, ch.b);
    return colour;
}

// `land_colour`'s own 2x2 jittered supersample (`fragment`'s doc comment on why), factored out so the coast
// blend below can call it too without duplicating the sample pattern.
fn land_colour_aa(x: f32, z: f32, size: f32, px_per_cell: f32) -> vec3<f32> {
    let dx = fwidth(x) * 0.25;
    let dz = fwidth(z) * 0.25;
    let c0 = land_colour(x - dx, z - dz, size, px_per_cell);
    let c1 = land_colour(x + dx, z - dz, size, px_per_cell);
    let c2 = land_colour(x - dx, z + dz, size, px_per_cell);
    let c3 = land_colour(x + dx, z + dz, size, px_per_cell);
    return (c0 + c1 + c2 + c3) * 0.25;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let size = params.x;
    let grid_on = params.y;
    let px_per_cell = params.z;

    let x = in.uv.x * size;
    let z = in.uv.y * size;
    // Vertex-grid-point sampling, not texel-centre sampling: shift by half a texel so an integer cell coordinate
    // lands exactly on that cell's own texel centre, reproducing the deleted CPU path's hand-written
    // `bilinear(corner_fn, x, z)` exactly via ordinary hardware texture filtering (a linear, repeat-wrapped
    // sampler already does the same interpolation that function spelled out by hand).
    let cell_uv = vec2<f32>(x + 0.5, z + 0.5) / size;

    // `disp`/`light`/`brightness` are no longer needed here: `land_colour`/`water_colour` (below) each compute
    // their own - only `walk` (the branch pick) is shared.
    let walk = textureSample(channels_tex, channels_samp, cell_uv).a;
    // A wide, organic colour blend between `land_colour` and `water_colour` (2026-10-23; terrain-textures.md
    // "coast"). `COAST_BLEND_WIDTH` is the blend's own half-width in `walk` units; `COAST_BLEND_JITTER` perturbs
    // the blend's own centre with the SAME `disp` noise `land_colour` already reads, so the line wobbles/frays
    // rather than tracing a uniform smooth curve.
    let pxf = x * px_per_cell;
    let pyf = z * px_per_cell;
    let jitter = (disp_at(pxf, pyf) / 127.0) * COAST_BLEND_JITTER;
    let centre = COAST_THRESHOLD + jitter;
    let lo = centre - COAST_BLEND_WIDTH;
    let hi = centre + COAST_BLEND_WIDTH;
    var colour: vec3<f32>;
    if (walk < lo) {
        // No supersampling needed here, unlike `land_colour`: `water_colour`'s own row is fixed and its
        // brightness is a smooth, continuous function of the height field, with no per-pixel categorical jump to
        // alias against.
        colour = water_colour(x, z, px_per_cell, params.w, water_sea_factor(cell_uv, size));
    } else if (walk > hi) {
        // `land_colour` (above) is a single, hard-floored, categorical row pick with no row+1 blend
        // (pop3-rev-analysis.md "Terrain land colour": the exe never blends two rows either - see its own doc
        // comment). Faithful, but a `textureLoad`, not filtered: on steep ground `row` (and `row_jitter`, now
        // correctly row-scale - TODO.md "Terrain", 2026-10-21) can change between neighbouring fragments, which
        // reads as aliasing - harsher than before fixing the white/pale patch, since the old row/row+1 blend
        // (wrong, and the actual cause of that patch) incidentally smoothed this for free. Supersampled here
        // instead: a small jittered 2x2 grid within this fragment's own footprint (`fwidth`), averaged - a
        // renderer-only anti-aliasing pass, same spirit as `soften_light`/the `disp` bilinear sampling above, not
        // a formula change (every sample runs the exact same per-pixel formula, just at a slightly offset
        // position each time).
        colour = land_colour_aa(x, z, size, px_per_cell);
    } else {
        let water = water_colour(x, z, px_per_cell, params.w, water_sea_factor(cell_uv, size));
        let land = land_colour_aa(x, z, size, px_per_cell);
        let t = smoothstep(lo, hi, walk);
        colour = mix(water, land, t);
    }

    if (grid_on > 0.5) {
        let gx = fract(x);
        let gz = fract(z);
        let wx = max(fwidth(x), 0.0001);
        let wz = max(fwidth(z), 0.0001);
        let ex = 1.0 - smoothstep(0.125 - wx, 0.125 + wx, gx);
        let ez = 1.0 - smoothstep(0.125 - wz, 0.125 + wz, gz);
        colour = colour * (1.0 - max(ex, ez) * (1.0 - GRID_SHADE));
    }

    if (highlight.z > 0.5) {
        let lx = wrap_signed(x - highlight.x, size);
        let lz = wrap_signed(z - highlight.y, size);
        let wx = max(fwidth(x), 0.0001);
        let wz = max(fwidth(z), 0.0001);
        let inx = smoothstep(-wx, wx, lx) * (1.0 - smoothstep(1.0 - wx, 1.0 + wx, lx));
        let inz = smoothstep(-wz, wz, lz) * (1.0 - smoothstep(1.0 - wz, 1.0 + wz, lz));
        colour = mix(colour, HIGHLIGHT_COLOUR, inx * inz * HIGHLIGHT_STRENGTH);
    }

    return vec4<f32>(colour, 1.0);
}
