struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
    night_factor: vec4<f32>,
    shadow_vp_matrix: mat4x4<f32>,
    inv_vp_matrix: mat4x4<f32>,
    fog_params: vec4<f32>,
    time: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    let pos = array(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var out: VertexOutput;
    let p = pos[idx % 3u];
    out.clip_pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let night = clamp((1.0 - uniforms.night_factor.x) / 0.76, 0.0, 1.0);
    // Reconstruct world-space direction from screen UV
    let ndc = vec2<f32>(input.uv.x * 2.0 - 1.0, 1.0 - input.uv.y * 2.0);
    let clip_near = uniforms.inv_vp_matrix * vec4<f32>(ndc, 0.0, 1.0);
    let clip_far = uniforms.inv_vp_matrix * vec4<f32>(ndc, 1.0, 1.0);
    let near_world = clip_near.xyz / clip_near.w;
    let far_world = clip_far.xyz / clip_far.w;
    let view_dir = normalize(far_world - near_world);

    // 26.2 renders its upper sky disc with one environment-provided color.
    // A previous screen-space horizon gradient and panoramic orange band were
    // camera-locked approximations and produced a visible moving halo.
    let sky_color = mix(
        vec3<f32>(0.03, 0.10, 0.70),
        vec3<f32>(0.01, 0.01, 0.08),
        night,
    );

    // Underwater: fade sky to deep blue based on view direction
    let underwater = uniforms.fog_params.w;
    let underwater_sky = vec3<f32>(0.016, 0.086, 0.20);
    // Sky fades to underwater color more near the horizon (through water)
    let underwater_blend = smoothstep(-0.5, 0.5, view_dir.y);
    let blended_sky = mix(sky_color, underwater_sky, underwater * (0.5 + 0.5 * underwater_blend));

    return vec4<f32>(blended_sky, 1.0);
}
