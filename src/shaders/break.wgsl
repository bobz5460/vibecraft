struct Uniforms {
    vp_matrix: mat4x4<f32>,
    _camera_pos: vec4<f32>,
    _light_direction: vec4<f32>,
    break_progress: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var destroy_atlas: texture_2d<f32>;
@group(0) @binding(2) var destroy_sampler: sampler;

const DESTROY_FRAMES: f32 = 10.0;
const TEX_SIZE: f32 = 16.0;
const STRIP_WIDTH: f32 = 160.0;

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_pos = uniforms.vp_matrix * vec4<f32>(input.pos, 1.0);
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let progress = uniforms.break_progress.x;
    if progress <= 0.0 { discard; }

    // Select frame based on break progress (0..1 → 0..9)
    let frame = min(u32(progress * DESTROY_FRAMES), u32(DESTROY_FRAMES - 1.0));
    let frame_f32 = f32(frame);
    // UV within the strip: x offset = frame * 16 / 160 = frame / 10
    let atlas_uv = vec2<f32>(
        (input.uv.x + frame_f32) / DESTROY_FRAMES,
        input.uv.y,
    );

    let destroy_color = textureSample(destroy_atlas, destroy_sampler, atlas_uv);

    // Use destroy alpha as crack overlay, blend dark cracks on block
    let alpha = destroy_color.a * 0.85;
    if alpha < 0.01 { discard; }
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}
