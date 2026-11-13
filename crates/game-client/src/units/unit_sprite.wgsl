#import bevy_pbr::forward_io::VertexOutput

const TABLE_WIDTH: u32 = 256u;
const UPSCALE: i32 = #{UPSCALE};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var index_tex: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var colours: texture_2d<f32>;
// x: this tribe's first row in `colours`, y: alpha multiplier, z: outline width in texels (0: the sprite itself),
// w: 1 for pixel art (drawn `UPSCALE` times larger, rounded by Scale2x), 0 for detailed art.
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

// Scale2x (EPX), as `art::scale2x`: the quarter `sub` of pixel `p`, given its neighbours above, right, left and below.
fn epx(p: u32, a: u32, b: u32, c: u32, d: u32, sub: vec2<i32>) -> u32 {
    if sub.y == 0 {
        if sub.x == 0 && c == a && c != d && a != b {
            return c;
        }
        if sub.x == 1 && a == b && a != c && b != d {
            return a;
        }
    } else {
        if sub.x == 0 && d == c && d != b && c != a {
            return d;
        }
        if sub.x == 1 && b == d && b != a && d != c {
            return b;
        }
    }
    return p;
}

fn index_x2(q: vec2<i32>) -> u32 {
    let p = q >> vec2(1u);
    return epx(index_at(p), index_at(p + vec2(0, -1)), index_at(p + vec2(1, 0)), index_at(p + vec2(-1, 0)), index_at(p + vec2(0, 1)), q & vec2(1));
}

fn index_x4(q: vec2<i32>) -> u32 {
    let p = q >> vec2(1u);
    return epx(index_x2(p), index_x2(p + vec2(0, -1)), index_x2(p + vec2(1, 0)), index_x2(p + vec2(-1, 0)), index_x2(p + vec2(0, 1)), q & vec2(1));
}

fn scale() -> i32 {
    if params.w > 0.5 {
        return UPSCALE;
    }
    return 1;
}

// Alpha-weighted bilinear blend of 4 pixels, so see-through ones don't darken the edge.
fn blend4(c00: vec4<f32>, c10: vec4<f32>, c01: vec4<f32>, c11: vec4<f32>, f: vec2<f32>) -> vec4<f32> {
    let w = vec4((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y) * vec4(c00.a, c10.a, c01.a, c11.a);
    let a = w.x + w.y + w.z + w.w;
    let rgb = (c00.rgb * w.x + c10.rgb * w.y + c01.rgb * w.z + c11.rgb * w.w) / max(a, 1e-5);
    return vec4(rgb, a);
}

// The texels around `uv` drawn `s` times larger: the first one's coordinates and where `uv` falls between them.
fn taps(uv: vec2<f32>, s: i32) -> vec4<f32> {
    let pos = uv * vec2<f32>(textureDimensions(index_tex) * u32(s)) - 0.5;
    return vec4(floor(pos), fract(pos));
}

fn plain(uv: vec2<f32>) -> vec4<f32> {
    let t = taps(uv, 1);
    let q = vec2<i32>(t.xy);
    return blend4(colour_of(index_at(q)), colour_of(index_at(q + vec2(1, 0))), colour_of(index_at(q + vec2(0, 1))), colour_of(index_at(q + vec2(1, 1))), t.zw);
}

// The sprite drawn `scale()` times larger, filtered.
fn filtered(uv: vec2<f32>) -> vec4<f32> {
    let s = scale();
    if s == 1 {
        return plain(uv);
    }
    let t = taps(uv, s);
    let q = vec2<i32>(t.xy);
    var k = array<u32, 4>();
    let offsets = array<vec2<i32>, 4>(vec2(0, 0), vec2(1, 0), vec2(0, 1), vec2(1, 1));
    for (var i = 0; i < 4; i++) {
        if s == 4 {
            k[i] = index_x4(q + offsets[i]);
        } else {
            k[i] = index_x2(q + offsets[i]);
        }
    }
    return blend4(colour_of(k[0]), colour_of(k[1]), colour_of(k[2]), colour_of(k[3]), t.zw);
}

// Inside the outlined frame's silhouette (plain texels: enough for a halo, and cheap).
fn solid(uv: vec2<f32>) -> bool {
    if any(uv < frame_rect.xy) || any(uv > frame_rect.zw) {
        return false;
    }
    return plain(uv).a > 0.5;
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
