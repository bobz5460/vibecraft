struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
    night_factor: vec4<f32>,
    shadow_vp_matrix: mat4x4<f32>,
    inv_vp_matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct SkyOutput {
    @builtin(position) clip_pos: vec4<f32>,
}

@vertex
fn vs_star(@location(0) world_pos: vec3<f32>) -> SkyOutput {
    var out: SkyOutput;
    out.clip_pos = uniforms.vp_matrix * vec4<f32>(world_pos + uniforms.camera_pos.xyz, 1.0);
    out.clip_pos.z = out.clip_pos.w * 0.9999;
    return out;
}

@fragment
fn fs_star() -> @location(0) vec4<f32> {
    let night = uniforms.night_factor.x;
    let fade = smoothstep(0.15, 0.5, night);
    return vec4<f32>(1.0, 1.0, 1.0, fade);
}

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
    let d = length(input.uv - vec2<f32>(0.5, 0.5));
    if d > 0.5 { discard; }
    let glow = 1.0 - smoothstep(0.45, 0.5, d);
    let moon_color = mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(0.9, 0.85, 0.75), d * 1.5);
    return vec4<f32>(moon_color, night * (0.6 + 0.4 * glow));
}

struct CloudVertexIn {
    @location(0) pos: vec3<f32>,
}

struct CloudVertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
}

@vertex
fn vs_cloud_main(in: CloudVertexIn) -> CloudVertexOut {
    var out: CloudVertexOut;
    out.clip_pos = uniforms.vp_matrix * vec4<f32>(in.pos, 1.0);
    out.clip_pos.z = out.clip_pos.w * 0.9999;
    out.world_pos = in.pos;
    return out;
}

@fragment
fn fs_cloud(input: CloudVertexOut) -> @location(0) vec4<f32> {
    let night = uniforms.night_factor.x;
    let alpha = 0.8 * (1.0 - night * 0.5);
    let cloud_color = mix(vec3<f32>(0.95, 0.95, 0.95), vec3<f32>(0.35, 0.35, 0.5), night);
    return vec4<f32>(cloud_color, alpha);
}
