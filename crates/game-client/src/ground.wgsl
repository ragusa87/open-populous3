// Ground colour, per fragment (docs/specs/terrain-textures.md "Our bake", "bigfade"/"disp"/"cliff"): the exact
// same formula the deleted CPU `pixel_rgb` evaluated per baked texel, now evaluated per output pixel instead -
// see `terrain_texture.rs`'s module doc for why.
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
// 256x256 Rg8Uint: R disp, G water (same size, always read together).
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var dispwater_tex: texture_2d<u32>;

const BIGFADE_WIDTH: i32 = 256;
const BIGFADE_ROWS: i32 = 1152;
const BIGFADE_LAND_ROW: i32 = 128;
const CLIFF_WIDTH: i32 = 128;
const CLIFF_ROWS: i32 = 64;
const DISP_SIZE: i32 = 256;
// Where walkers cross every cell not sea on all four corners (`coast_land`'s own threshold in the deleted CPU
// path): the `channels` texture's bilinearly-sampled A (walkable) channel reaching this is "ground".
const COAST_THRESHOLD: f32 = 0.2;
const ASH_COLOUR: vec3<f32> = vec3<f32>(38.0 / 255.0, 33.0 / 255.0, 30.0 / 255.0);
// Brightness (out of 256) the grid overlay darkens a cell's own first 1/8th (dev-only, our own invention, not
// original behaviour): matches the deleted CPU path's `GRID_SHADE`/`PX_PER_CELL`.
const GRID_SHADE: f32 = 190.0 / 255.0;

fn wrap256(v: i32) -> i32 {
    return ((v % DISP_SIZE) + DISP_SIZE) % DISP_SIZE;
}

// `disp`/`water`'s own raw byte, read signed (pop3-rev-analysis.md "Terrain land colour": no re-centring, no
// per-theme constant - the game reads it straight as i8).
fn disp_at(px: i32, py: i32) -> i32 {
    let v = i32(textureLoad(dispwater_tex, vec2<i32>(wrap256(px), wrap256(py)), 0).r);
    return select(v, v - 256, v >= 128);
}
fn water_at(px: i32, py: i32) -> i32 {
    return i32(textureLoad(dispwater_tex, vec2<i32>(wrap256(px), wrap256(py)), 0).g);
}

// How much `disp` jitters a bigfade row at that altitude (`pop3_format::theme::disp_sensitivity`): flat 320
// below row 129, a +3/row ramp to 1024 at row 362, flat above.
fn disp_sensitivity(row: i32) -> i32 {
    return clamp(3 * row - 64, 320, 1024);
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
    let land_start = i32(params.y);
    let grid_on = params.z;
    let px_per_cell = params.w;

    let x = in.uv.x * size;
    let z = in.uv.y * size;
    // Vertex-grid-point sampling, not texel-centre sampling: shift by half a texel so an integer cell coordinate
    // lands exactly on that cell's own texel centre, reproducing the deleted CPU path's hand-written
    // `bilinear(corner_fn, x, z)` exactly via ordinary hardware texture filtering (a linear, repeat-wrapped
    // sampler already does the same interpolation that function spelled out by hand).
    let cell_uv = vec2<f32>(x + 0.5, z + 0.5) / size;

    let px = i32(floor(x * px_per_cell));
    let py = i32(floor(z * px_per_cell));
    let d_here = disp_at(px, py);
    let d_below = disp_at(px, py + 1);

    let ch = textureSample(channels_tex, channels_samp, cell_uv);
    let light = ch.g * 255.0;
    let brightness = clamp(((d_below - d_here) >> 2u) + i32(round(light)), 0, 255);

    let walk = ch.a;
    var colour: vec3<f32>;
    if (walk < COAST_THRESHOLD) {
        let row = water_at(px, py) * BIGFADE_LAND_ROW / 256;
        let idx = bigfade_flat(row * BIGFADE_WIDTH + brightness);
        colour = palette_rgb(idx);
    } else {
        let altitude = textureSample(altitude_tex, altitude_samp, cell_uv).r;
        let row = max(i32(altitude), land_start);
        let row_jitter = (disp_sensitivity(row) * d_here) >> 10u;
        let land_colour = bigfade_flat(row * BIGFADE_WIDTH + brightness + row_jitter);
        let blend = i32(round(ch.r * 255.0));
        let remapped = cliff_remap(blend / 4, land_colour);
        colour = palette_rgb(remapped);
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
