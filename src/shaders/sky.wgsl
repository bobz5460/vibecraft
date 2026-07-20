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
@group(0) @binding(5) var celestial_texture: texture_2d<f32>;
@group(0) @binding(6) var celestial_sampler: sampler;

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
    let night = clamp((1.0 - uniforms.night_factor.x) / 0.76, 0.0, 1.0);
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
    let night = clamp((1.0 - uniforms.night_factor.x) / 0.76, 0.0, 1.0);
    let celestial = textureSample(celestial_texture, celestial_sampler, input.uv);
    let visibility = select(night, 1.0 - night, input.uv.x < 0.5);
    if visibility < 0.02 { discard; }
    return vec4<f32>(celestial.rgb, celestial.a * visibility);
}

struct CloudVertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) shade: f32,
}

struct CloudVertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) shade: f32,
}

@vertex
fn vs_cloud_main(in: CloudVertexIn) -> CloudVertexOut {
    var out: CloudVertexOut;
    let cloud_pos = in.pos - vec3<f32>(uniforms.time.x * 0.60, 0.0, 0.0);
    out.clip_pos = uniforms.vp_matrix * vec4<f32>(cloud_pos, 1.0);
    out.clip_pos.z = out.clip_pos.w * 0.9999;
    out.shade = in.shade;
    return out;
}

@fragment
fn fs_cloud(input: CloudVertexOut) -> @location(0) vec4<f32> {
    if uniforms.fog_params.w > 0.5 { discard; }
    let night = clamp((1.0 - uniforms.night_factor.x) / 0.76, 0.0, 1.0);
    let alpha = 0.82 * (1.0 - night * 0.45);
    let cloud_color = mix(vec3<f32>(0.95), vec3<f32>(0.35, 0.35, 0.50), night) * input.shade;
    return vec4<f32>(cloud_color, alpha);
}
