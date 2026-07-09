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

fn unpack_light(light_data: u32) -> vec2<f32> {
    let block_light = f32(light_data & 0xFu);
    let sky_light = f32((light_data >> 4u) & 0xFu);
    let ao = f32((light_data >> 8u) & 0xFu) / 15.0;
    let night = uniforms.night_factor.x;
    let night_sky = sky_light * (1.0 - night * 0.85);
    let combined = max(block_light, night_sky) / 15.0;
    return vec2<f32>(combined, ao);
}

fn sample_shadow(world_pos: vec3<f32>) -> f32 {
    let light_clip = uniforms.shadow_vp_matrix * vec4<f32>(world_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;
    // Convert from [-1,1] to [0,1] for texture coords
    let shadow_uv = light_ndc.xy * 0.5 + vec2<f32>(0.5, 0.5);
    if shadow_uv.x < 0.0 || shadow_uv.x > 1.0 || shadow_uv.y < 0.0 || shadow_uv.y > 1.0 || light_ndc.z > 1.0 {
        return 1.0; // outside shadow map = fully lit
    }
    let sampled_depth = textureSampleLevel(shadow_map, atlas_sampler, shadow_uv, 0.0);
    // Bias to reduce shadow acne
    let bias = 0.005;
    if light_ndc.z - bias > sampled_depth {
        return 0.3; // in shadow
    }
    return 1.0;
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
    let light_info = unpack_light(input.light_data);
    output.light = light_info.x;
    output.ao = light_info.y;
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
    // Water textures in the atlas: 8, 9 (water_still, water_flow)
    return tile_index == 8u || tile_index == 9u || tile_index == 17u || tile_index == 28u;
}

fn is_leaf_tile(tile_index: u32) -> bool {
    // Leaf textures: oak=14, spruce=118, birch=119, jungle=120, acacia=121, dark_oak=122
    return tile_index == 14u || (tile_index >= 118u && tile_index <= 122u);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tile_uv = get_tile_uv(input.uv, input.tex_index);
    var color = textureSample(atlas, atlas_sampler, tile_uv);

    let light_dir = normalize(uniforms.light_direction.xyz);
    let ndotl = max(dot(input.normal, light_dir), 0.0);

    let night = uniforms.night_factor.x;
    let day_factor = 1.0 - night;

    // Shadow
    let shadow = sample_shadow(input.shadow_pos);

    // Sunlight: warm at sunrise/sunset, neutral white at noon
    let sun_color = mix(
        vec3<f32>(1.0, 0.85, 0.55), // warm sunrise/sunset
        vec3<f32>(1.0, 0.95, 0.85), // neutral noon
        smoothstep(0.0, 0.4, day_factor)
    );

    // Ambient: sky contributes blue, ground contributes brown bounce
    let sky_ambient = vec3<f32>(0.5, 0.6, 0.9);
    let ground_ambient = vec3<f32>(0.35, 0.3, 0.25);
    let is_top = input.normal.y > 0.5;
    let is_side = abs(input.normal.y) < 0.5;
    let ambient_color = mix(ground_ambient, sky_ambient, input.normal.y * 0.5 + 0.5);
    let ambient_strength = mix(0.30, 0.08, night);

    // Directional light with shadow
    let directional = ndotl * day_factor * max(0.3, shadow);
    let ambient_contrib = ambient_strength * ambient_color;
    let sun_contrib = directional * sun_color;

    // AO
    let ao_factor = 0.75 + 0.25 * input.ao;

    let brightness = (ambient_contrib + sun_contrib) * input.light * ao_factor;
    color = vec4<f32>(color.r * brightness, color.g * brightness, color.b * brightness, color.a);

    // Leaves: subtle backlight transmission
    if is_leaf_tile(input.tex_index) {
        let back_ndotl = max(dot(input.normal, -light_dir), 0.0);
        let transmission = back_ndotl * 0.15 * day_factor * (1.0 - shadow * 0.5);
        color = vec4<f32>(color.r + transmission, color.g + transmission * 0.8, color.b + transmission * 0.4, color.a);
    }

    // Water: Fresnel + depth color + animated shimmer
    if is_water_tile(input.tex_index) {
        // Fresnel: more reflective at glancing angles
        let view_dir = normalize(uniforms.camera_pos.xyz - input.world_pos);
        let fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(view_dir, input.normal), 0.0), 3.0);
        // Sky reflection color
        let sky_color = vec3<f32>(0.5, 0.7, 1.0) * day_factor + vec3<f32>(0.05, 0.05, 0.15) * night;
        // Depth-based color: shallow = clear, deep = dark blue
        let depth_factor = 0.4;
        let water_deep = vec3<f32>(0.0, 0.1, 0.3);
        let water_shallow = vec3<f32>(0.1, 0.4, 0.5);
        let water_color = mix(water_shallow, water_deep, depth_factor);
        // Combine: reflection + water color + texture
        let reflect_color = mix(color.rgb, sky_color, fresnel * 0.6);
        color = vec4<f32>(mix(reflect_color, water_color, 0.3), 0.7);
    }

    // Sky color tint on upward-facing surfaces (daytime)
    if is_top && day_factor > 0.1 {
        let sky_tint = vec3<f32>(0.85, 0.9, 1.0);
        let warm_tint = vec3<f32>(1.0, 0.7, 0.4);
        let sunset_tint = mix(sky_tint, warm_tint, smoothstep(0.0, 0.5, night) * (1.0 - smoothstep(0.5, 1.0, night)));
        let tint_strength = 0.12 * day_factor;
        color = vec4<f32>(mix(color.rgb, color.rgb * sunset_tint, tint_strength), color.a);
    }

    // Tone mapping: subtle contrast boost
    let contrast = 1.1;
    color = vec4<f32>(pow(color.rgb, vec3<f32>(1.0 / contrast)), color.a);

    // Atmosphere: fog with sky color, denser at night
    let night_sky = mix(vec3<f32>(0.4, 0.55, 0.8), vec3<f32>(0.02, 0.02, 0.1), night);
    let day_sky = mix(vec3<f32>(0.5, 0.65, 0.85), vec3<f32>(0.3, 0.5, 0.7), smoothstep(0.3, 1.0, day_factor));
    let sky_fog = mix(day_sky, night_sky, night);
    let fog_density = 0.0015 + night * 0.002;
    let fog_factor = 1.0 - exp(-fog_density * input.distance);
    color = vec4<f32>(mix(color.rgb, sky_fog, fog_factor), color.a);

    return color;
}
