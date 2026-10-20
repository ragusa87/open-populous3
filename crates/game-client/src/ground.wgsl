// Ground colour, per fragment (docs/specs/terrain-textures.md "Our bake", "bigfade"/"disp"/"cliff"): the exact
// same formula the deleted CPU `pixel_rgb` evaluated per baked texel, now evaluated per output pixel instead -
// see `terrain_texture.rs`'s module doc for why. One deliberate deviation: `disp`/`water` (the detail-noise
// lookups, below) are bilinearly sampled rather than `textureLoad`-exact like `bigfade`/`cliff` - the old baked
// texture got equivalent smoothing for free from the GPU's own bilinear magnification of its coarse 8 px/cell
// bake, which an exact per-fragment lookup here would lose (visible as harsh, blocky light/dark patches on
// steep, high terrain where `disp_sensitivity` is near its max - see `GroundMaterial::disp`'s own doc comment).
#import bevy_pbr::forward_io::VertexOutput

// x: map size (cells), y: land_start row, z: grid overlay on/off, w: PX_PER_CELL.
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
// 256x256 R8Unorm: the sea's own noise (`watdisp.dat`), unsigned. Bilinear, repeat-wrapped, same reason.
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var water_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var water_samp: sampler;

const BIGFADE_WIDTH: i32 = 256;
const BIGFADE_ROWS: i32 = 1152;
const BIGFADE_LAND_ROW: i32 = 128;
const CLIFF_WIDTH: i32 = 128;
const CLIFF_ROWS: i32 = 64;
const DISP_SIZE: f32 = 256.0;
// Where walkers cross every cell not sea on all four corners (`coast_land`'s own threshold in the deleted CPU
// path): the `channels` texture's bilinearly-sampled A (walkable) channel reaching this is "ground".
const COAST_THRESHOLD: f32 = 0.2;
const ASH_COLOUR: vec3<f32> = vec3<f32>(38.0 / 255.0, 33.0 / 255.0, 30.0 / 255.0);
// Brightness (out of 256) the grid overlay darkens a cell's own first 1/8th (dev-only, our own invention, not
// original behaviour): matches the deleted CPU path's `GRID_SHADE`/`PX_PER_CELL`.
const GRID_SHADE: f32 = 190.0 / 255.0;

// `disp`'s own raw byte, read signed (pop3-rev-analysis.md "Terrain land colour": no re-centring, no per-theme
// constant - the game reads it straight as i8), bilinearly sampled at the continuous (pxf, pyf) pixel coordinate
// (`GroundMaterial::disp`'s own doc comment): the texture is R8Snorm, so the signed decode is free, and the
// result is a continuous float rather than the single discrete byte `textureLoad` would give.
fn disp_at(pxf: f32, pyf: f32) -> f32 {
    let uv = vec2<f32>(pxf + 0.5, pyf + 0.5) / DISP_SIZE;
    return textureSample(disp_tex, disp_samp, uv).r * 127.0;
}
fn water_at(pxf: f32, pyf: f32) -> f32 {
    let uv = vec2<f32>(pxf + 0.5, pyf + 0.5) / DISP_SIZE;
    return textureSample(water_tex, water_samp, uv).r * 255.0;
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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let size = params.x;
    let land_start = params.y;
    let grid_on = params.z;
    let px_per_cell = params.w;

    let x = in.uv.x * size;
    let z = in.uv.y * size;
    // Vertex-grid-point sampling, not texel-centre sampling: shift by half a texel so an integer cell coordinate
    // lands exactly on that cell's own texel centre, reproducing the deleted CPU path's hand-written
    // `bilinear(corner_fn, x, z)` exactly via ordinary hardware texture filtering (a linear, repeat-wrapped
    // sampler already does the same interpolation that function spelled out by hand).
    let cell_uv = vec2<f32>(x + 0.5, z + 0.5) / size;

    let pxf = x * px_per_cell;
    let pyf = z * px_per_cell;
    let d_here = disp_at(pxf, pyf);
    let d_below = disp_at(pxf, pyf + 1.0);

    let ch = textureSample(channels_tex, channels_samp, cell_uv);
    let light = ch.g * 255.0;
    let brightness = clamp(i32(round((d_below - d_here) / 4.0 + light)), 0, 255);

    let walk = ch.a;
    var colour: vec3<f32>;
    if (walk < COAST_THRESHOLD) {
        let row = i32(round(water_at(pxf, pyf) * f32(BIGFADE_LAND_ROW) / 256.0));
        let idx = bigfade_flat(row * BIGFADE_WIDTH + brightness);
        colour = palette_rgb(idx);
    } else {
        // The bigfade/cliff/palette lookup is categorical (a `textureLoad`, not filtered): an exact per-fragment
        // row pick shows a hard edge wherever `row` changes between neighbouring fragments, which on steep
        // terrain (many rows crossed over a short distance) reads as harsh, blocky patches - the old baked
        // 8 px/cell texture never showed this because the GPU's own bilinear magnification blended neighbouring
        // *already-computed* baked colours. Reproduced here directly: blend the two colours the continuous
        // altitude's own fractional part sits between (`row`/`row + 1`), the same thing hardware bilinear
        // filtering would do if bigfade were addressable as a filterable texture along its row axis.
        let altitude = textureSample(altitude_tex, altitude_samp, cell_uv).r;
        let altitude_floored = max(altitude, land_start);
        let row = i32(floor(altitude_floored));
        let frac = fract(altitude_floored);
        let row_jitter = i32(round(disp_sensitivity(row) * d_here / 1024.0));
        let blend = i32(round(ch.r * 255.0));
        let cliff_row = blend / 4;
        let colour_lo = palette_rgb(cliff_remap(cliff_row, bigfade_flat(row * BIGFADE_WIDTH + brightness + row_jitter)));
        let colour_hi = palette_rgb(cliff_remap(cliff_row, bigfade_flat((row + 1) * BIGFADE_WIDTH + brightness + row_jitter)));
        colour = mix(colour_lo, colour_hi, frac);
        let ash_k = ch.b;
        colour = mix(colour, ASH_COLOUR, ash_k);
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

    return vec4<f32>(colour, 1.0);
}
