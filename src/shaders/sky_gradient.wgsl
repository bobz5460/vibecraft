struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
    night_factor: vec4<f32>,
    shadow_vp_matrix: mat4x4<f32>,
    inv_vp_matrix: mat4x4<f32>,
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
    let night = uniforms.night_factor.x;
    let day = 1.0 - night;

    // Reconstruct world-space direction from screen UV
    let ndc = vec2<f32>(input.uv.x * 2.0 - 1.0, 1.0 - input.uv.y * 2.0);
    let clip_near = uniforms.inv_vp_matrix * vec4<f32>(ndc, 0.0, 1.0);
    let clip_far = uniforms.inv_vp_matrix * vec4<f32>(ndc, 1.0, 1.0);
    let near_world = clip_near.xyz / clip_near.w;
    let far_world = clip_far.xyz / clip_far.w;
    let view_dir = normalize(far_world - near_world);

    // Sky gradient: darker at zenith, brighter near horizon
    let horizon_factor = 1.0 - abs(view_dir.y);
    let zenith_color = mix(
        vec3<f32>(0.03, 0.10, 0.70),
        vec3<f32>(0.01, 0.01, 0.08),
        night,
    );
    let horizon_color = mix(
        vec3<f32>(0.65, 0.80, 0.98),
        vec3<f32>(0.06, 0.06, 0.20),
        night,
    );
    var sky_color = mix(zenith_color, horizon_color, horizon_factor);

    // Sun glow
    let sun_dir = normalize(uniforms.light_direction.xyz);
    let sun_angle = max(dot(view_dir, sun_dir), 0.0);
    let sun_glow = pow(sun_angle, 64.0) * day * 0.8;
    let sun_halo = pow(sun_angle, 4.0) * day * 0.25;

    let sun_col = mix(
        vec3<f32>(1.0, 0.75, 0.45),
        vec3<f32>(1.0, 1.0, 1.0),
        smoothstep(0.0, 0.5, day),
    );
    sky_color += sun_glow * sun_col + sun_halo * sun_col * 0.25;

    // Dusk horizon glow
    let dusk_glow = pow(horizon_factor, 2.0) * max(0.0, sun_dir.y + 0.2) * 0.3;
    let dusk_color = vec3<f32>(1.0, 0.5, 0.2);
    sky_color += dusk_glow * dusk_color * (1.0 - night * 0.5);

    return vec4<f32>(sky_color, 1.0);
}
