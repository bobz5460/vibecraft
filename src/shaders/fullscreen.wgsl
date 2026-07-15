@group(0) @binding(0) var<uniform> ortho: mat4x4<f32>;
@group(0) @binding(1) var scene_tex: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;

struct BlurUniform {
    intensity: f32,
    _pad: f32,
    _pad2: f32,
    _pad3: f32,
}
@group(0) @binding(3) var<uniform> blur: BlurUniform;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let pos = array(
        vec4(-1.0, -1.0, 0.0, 1.0),
        vec4( 3.0, -1.0, 0.0, 1.0),
        vec4(-1.0,  3.0, 0.0, 1.0),
    );
    let uv = array(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );
    return VOut(pos[vi], uv[vi]);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let blur_intensity = blur.intensity;
    if blur_intensity <= 0.0 {
        return textureSample(scene_tex, scene_sampler, in.uv);
    }
    let texel_size = vec2(1.0 / f32(textureDimensions(scene_tex).x), 1.0 / f32(textureDimensions(scene_tex).y));
    let samples = u32(blur_intensity * 2.0 + 1.0);
    let half_samples = i32(samples) / 2;
    var color = vec4(0.0);
    var total: f32 = 0.0;
    for (var dy = -half_samples; dy <= half_samples; dy = dy + 1) {
        for (var dx = -half_samples; dx <= half_samples; dx = dx + 1) {
            let offset = vec2(f32(dx), f32(dy)) * texel_size * blur_intensity * 0.3;
            color += textureSample(scene_tex, scene_sampler, in.uv + offset);
            total += 1.0;
        }
    }
    return color / total;
}
