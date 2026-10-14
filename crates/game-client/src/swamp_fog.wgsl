#import bevy_pbr::forward_io::VertexOutput

// x: time (seconds, since the swamp started), y: density, z: this layer's height fade, w: unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> time_density: vec4<f32>;
// xy: this layer's noise seed, zw: its drift direction.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> seed_drift: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> fog_color: vec4<f32>;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash(i);
    let b = hash(i + vec2(1.0, 0.0));
    let c = hash(i + vec2(0.0, 1.0));
    let d = hash(i + vec2(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}

// Four octaves of value noise: cloudy shapes with both big blobs and fine detail.
fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var k = 0; k < 4; k = k + 1) {
        v = v + amp * noise(p * freq);
        freq = freq * 2.0;
        amp = amp * 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = time_density.x;
    let density = time_density.y;
    let layer_fade = time_density.z;
    let seed = seed_drift.xy;
    let drift = seed_drift.zw;
    let uv = in.uv;

    // Domain warping (fbm(p + fbm(p))): what makes the fog curl and swirl instead of sliding sideways.
    let p = uv * vec2(6.0, 5.0) + seed;
    let f = fbm(p + time * drift + fbm(p * 0.8 - time * 0.04));

    // Soft edges so the fog fades out past the swamp's own footprint, not stopping on a hard line.
    let edge = smoothstep(0.0, 0.16, uv.x) * smoothstep(1.0, 0.84, uv.x)
             * smoothstep(0.0, 0.18, uv.y) * smoothstep(1.0, 0.82, uv.y);

    // A minimum of 35% opacity even in the noise's gaps, so the fog never fully opens up.
    let alpha = edge * mix(0.35, 1.0, f) * density * layer_fade;
    return vec4(fog_color.rgb, alpha);
}
