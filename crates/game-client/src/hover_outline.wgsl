#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sprite: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sprite_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> width: vec4<f32>;

fn alpha(uv: vec2<f32>) -> f32 {
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
        return 0.0;
    }
    return textureSampleLevel(sprite, sprite_sampler, uv, 0.0).a;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if alpha(in.uv) > 0.5 {
        discard;
    }
    let step = width.x / vec2<f32>(textureDimensions(sprite));
    for (var i = 0; i < 16; i++) {
        let a = f32(i) * 0.3926991;
        if alpha(in.uv + vec2(cos(a), sin(a)) * step) > 0.5 {
            return color;
        }
    }
    discard;
    return color;
}
