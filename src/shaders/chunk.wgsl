struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
    night_factor: vec4<f32>,
    shadow_vp_matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var shadow_map: texture_depth_2d;
@group(0) @binding(4) var shadow_sampler: sampler_comparison;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tex_index: u32,
    @location(4) light_data: u32,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_index: u32,
    @location(3) world_pos: vec3<f32>,
    @location(4) distance: f32,
    @location(5) light: f32,
    @location(6) ao: f32,
    @location(7) shadow_pos: vec3<f32>,
}

fn unpack_light(light_data: u32) -> f32 {
    let block_light = f32(light_data & 0xFu);
    let sky_light = f32((light_data >> 4u) & 0xFu);
    let night = uniforms.night_factor.x;
    let night_sky = sky_light * (1.0 - night * 0.85);
    return max(block_light, night_sky) / 15.0;
}

fn unpack_ao(light_data: u32) -> f32 {
    let ao_raw = f32((light_data >> 8u) & 0xFu) / 15.0;
    // Subtle AO: 0.88 at full occlusion, 1.0 at none
    return 0.88 + 0.12 * ao_raw;
}

fn sample_shadow(world_pos: vec3<f32>) -> f32 {
    let light_clip = uniforms.shadow_vp_matrix * vec4<f32>(world_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;
    let shadow_uv = light_ndc.xy * 0.5 + vec2<f32>(0.5, 0.5);
    if shadow_uv.x < 0.0 || shadow_uv.x > 1.0 || shadow_uv.y < 0.0 || shadow_uv.y > 1.0 || light_ndc.z > 1.0 {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_map, shadow_sampler, shadow_uv, light_ndc.z - 0.005);
}

/// Vanilla Minecraft face brightness multipliers.
fn face_multiplier(normal: vec3<f32>) -> f32 {
    let n = normalize(normal);
    if n.y > 0.5 { return 1.0; }       // Top
    if n.y < -0.5 { return 0.5; }      // Bottom
    if abs(n.x) > 0.5 { return 0.8; }  // East / West
    return 0.6;                          // North / South
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_pos = input.position;
    output.clip_pos = uniforms.vp_matrix * vec4<f32>(world_pos, 1.0);
    output.uv = input.uv;
    output.normal = input.normal;
    output.tex_index = input.tex_index;
    output.world_pos = world_pos;
    output.distance = length(world_pos - uniforms.camera_pos.xyz);
    output.light = unpack_light(input.light_data);
    output.ao = unpack_ao(input.light_data);
    output.shadow_pos = world_pos;
    return output;
}

@vertex
fn vs_shadow(input: VertexInput) -> @builtin(position) vec4<f32> {
    return uniforms.shadow_vp_matrix * vec4<f32>(input.position, 1.0);
}

const TILES_PER_ROW: f32 = 32.0;
const TILE_SIZE: f32 = 1.0 / TILES_PER_ROW;

fn get_tile_uv(uv: vec2<f32>, tile_index: u32) -> vec2<f32> {
    let tile_x = f32(tile_index % 32u);
    let tile_y = f32(tile_index / 32u);
    let base = vec2<f32>(tile_x, tile_y) * TILE_SIZE;
    let wrapped = fract(uv);
    return base + vec2<f32>(wrapped.x, 1.0 - wrapped.y) * TILE_SIZE;
}

fn is_water_tile(tile_index: u32) -> bool {
    return tile_index == 8u || tile_index == 9u || tile_index == 17u || tile_index == 28u;
}

fn is_leaf_tile(tile_index: u32) -> bool {
    return tile_index == 14u || (tile_index >= 118u && tile_index <= 122u);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tile_uv = get_tile_uv(input.uv, input.tex_index);
    var color = textureSample(atlas, atlas_sampler, tile_uv);

    // Vanilla face brightness (applied to base color before lighting)
    let face_mult = face_multiplier(input.normal);
    color = vec4<f32>(color.rgb * face_mult, color.a);

    // Normalized light direction
    let light_dir = normalize(uniforms.light_direction.xyz);
    let ndotl = max(dot(input.normal, light_dir), 0.0);
    let night = uniforms.night_factor.x;
    let day = 1.0 - night;

    // === Lighting ===
    // Minecraft uses skylight as the dominant light source during the day.
    // Skylight (input.light) is the combined sky+block light, 0-1.
    // It's multiplied by the face brightness for the base ambient.

    // Face-dependent skylight: skylight fully illuminates all faces
    let sky_base = input.light; // 0-1 from chunk light data

    // Directional sunlight (neutral white at noon, warm at edges)
    let sun_color = mix(
        vec3<f32>(1.0, 0.78, 0.45),  // warm sunrise/sunset
        vec3<f32>(1.0, 0.92, 0.80),  // neutral noon
        smoothstep(0.0, 0.5, day)
    );
    let shadow = sample_shadow(input.shadow_pos);
    let sun_diffuse = ndotl * day * shadow;

    // Combine: skylight base + directional sunlight
    // Skylight provides strong ambient; sun adds directional emphasis
    let skylight_contrib = sky_base * (0.6 + 0.4 * day);
    let sunlight_contrib = sun_diffuse * 0.7;
    let brightness = (skylight_contrib + sunlight_contrib) * input.ao;

    color = vec4<f32>(color.rgb * brightness * sun_color, color.a);

    // === Special block handling ===

    // Leaves: brighten interiors, partial transmission
    if is_leaf_tile(input.tex_index) {
        // Brighter base from skylight
        let leaf_transmission = 0.12 * max(0.0, dot(input.normal, -light_dir)) * day * shadow;
        color = vec4<f32>(color.rgb + vec3<f32>(leaf_transmission * 0.9, leaf_transmission * 0.7, leaf_transmission * 0.3), color.a);
        // Leaves use alpha test
        if color.a < 0.5 { discard; }
    }

    // Water: sky reflection + blue tint
    if is_water_tile(input.tex_index) {
        let view_dir = normalize(uniforms.camera_pos.xyz - input.world_pos);
        let fresnel = 0.05 + 0.95 * pow(1.0 - max(dot(view_dir, input.normal), 0.0), 4.0);
        let sky_reflect = mix(vec3<f32>(0.3, 0.5, 0.9), vec3<f32>(0.02, 0.02, 0.1), night);
        let water_shallow = vec3<f32>(0.2, 0.6, 0.7);
        let water_deep = vec3<f32>(0.0, 0.15, 0.35);
        let water_color = mix(water_shallow, water_deep, 0.5);
        let colored = mix(color.rgb, sky_reflect, fresnel * 0.5);
        color = vec4<f32>(mix(colored, water_color, 0.25), 0.6);
    }

    // === Gamma correction: linear → sRGB ===
    // Minecraft renders in gamma space. Convert our linear result to sRGB.
    let gamma_corrected = pow(color.rgb, vec3<f32>(1.0 / 2.2));

    // === Fog ===
    let day_sky = vec3<f32>(0.6, 0.75, 0.95);
    let night_sky = vec3<f32>(0.03, 0.03, 0.12);
    let sky_color = mix(day_sky, night_sky, night);
    let fog_density = 0.0008 + night * 0.0015;
    let fog_factor = 1.0 - exp(-fog_density * input.distance * input.distance);
    color = vec4<f32>(mix(gamma_corrected, sky_color, fog_factor), color.a);

    return color;
}
