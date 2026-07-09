struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
    night_factor: vec4<f32>,
    shadow_vp_matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct SkyOutput {
    @builtin(position) clip_pos: vec4<f32>,
}

// Stars: rendered as points at world-space far distance
@vertex
fn vs_star(@location(0) world_pos: vec3<f32>) -> SkyOutput {
    var out: SkyOutput;
    // Stars stay at a fixed Y height, rotate with camera
    out.clip_pos = uniforms.vp_matrix * vec4<f32>(world_pos, 1.0);
    // Push to far plane so depth test always passes behind geometry
    out.clip_pos.z = out.clip_pos.w * 0.9999;
    return out;
}

@fragment
fn fs_star() -> @location(0) vec4<f32> {
    let night = uniforms.night_factor.x;
    // Stars only visible at night, with slight twinkle
    if night < 0.3 { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, night);
}

// Moon: rendered as a full quad with a circle pattern
struct MoonOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_moon(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>) -> MoonOutput {
    var out: MoonOutput;
    out.clip_pos = uniforms.vp_matrix * vec4<f32>(pos, 1.0);
    out.clip_pos.z = out.clip_pos.w * 0.9999;
    out.uv = uv;
    return out;
}

@fragment
fn fs_moon(input: MoonOutput) -> @location(0) vec4<f32> {
    let night = uniforms.night_factor.x;
    if night < 0.2 { discard; }
    // Circle: distance from center
    let d = length(input.uv - vec2<f32>(0.5, 0.5));
    if d > 0.5 { discard; }
    // Slight glow at edge
    let glow = 1.0 - smoothstep(0.45, 0.5, d);
    let moon_color = mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(0.9, 0.85, 0.75), d * 1.5);
    return vec4<f32>(moon_color, night * (0.6 + 0.4 * glow));
}
