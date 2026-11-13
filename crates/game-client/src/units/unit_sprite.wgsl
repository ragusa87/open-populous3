#import bevy_pbr::forward_io::VertexOutput

const TABLE_WIDTH: u32 = 256u;

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var index_tex: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var colours: texture_2d<f32>;
// x: this tribe's first row in `colours`, y: alpha multiplier, z: outline width in texels (0: the sprite itself).
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> outline_colour: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> frame_rect: vec4<f32>;

fn index_at(p: vec2<i32>) -> u32 {
    let size = vec2<i32>(textureDimensions(index_tex));
    if any(p < vec2(0)) || any(p >= size) {
        return 0u;
    }
    return textureLoad(index_tex, p, 0).r;
}

fn colour_of(i: u32) -> vec4<f32> {
    let row = u32(params.x) + i / TABLE_WIDTH;
    return textureLoad(colours, vec2<i32>(i32(i % TABLE_WIDTH), i32(row)), 0);
}

fn texel(p: vec2<i32>) -> vec4<f32> {
    return colour_of(index_at(p));
}

// Bilinear over the 4 nearest texels, colours weighted by their alpha so see-through texels don't darken the edge.
fn filtered(uv: vec2<f32>) -> vec4<f32> {
    let pos = uv * vec2<f32>(textureDimensions(index_tex)) - 0.5;
    let base = vec2<i32>(floor(pos));
    let f = fract(pos);
    let c00 = texel(base);
    let c10 = texel(base + vec2(1, 0));
    let c01 = texel(base + vec2(0, 1));
    let c11 = texel(base + vec2(1, 1));
    let w = vec4((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y) * vec4(c00.a, c10.a, c01.a, c11.a);
    let a = w.x + w.y + w.z + w.w;
    let rgb = (c00.rgb * w.x + c10.rgb * w.y + c01.rgb * w.z + c11.rgb * w.w) / max(a, 1e-5);
    return vec4(rgb, a);
}

fn solid(uv: vec2<f32>) -> bool {
    if any(uv < frame_rect.xy) || any(uv > frame_rect.zw) {
        return false;
    }
    return filtered(uv).a > 0.5;
}

fn outline(uv: vec2<f32>) -> vec4<f32> {
    if solid(uv) {
        discard;
    }
    let step = params.z / vec2<f32>(textureDimensions(index_tex));
    for (var i = 0; i < 16; i++) {
        let a = f32(i) * 0.3926991;
        if solid(uv + vec2(cos(a), sin(a)) * step) {
            return outline_colour;
        }
    }
    discard;
    return outline_colour;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if params.z > 0.0 {
        return outline(in.uv);
    }
    var c = filtered(in.uv);
    c.a *= params.y;
#ifdef MAY_DISCARD
    if c.a < 0.5 {
        discard;
    }
#endif
    return c;
}
