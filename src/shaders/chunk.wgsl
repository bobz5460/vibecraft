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
    @location(5) light: vec3<f32>,
    @location(6) ao: f32,
    @location(7) emissive: f32,
    @location(8) shadow_pos: vec3<f32>,
}

struct ShadowOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tex_index: u32,
}

fn light_brightness(level: f32) -> f32 {
    return level / max(4.0 - 3.0 * level, 0.001);
}

fn not_gamma(color: vec3<f32>) -> vec3<f32> {
    let maximum = max(max(color.x, color.y), color.z);
    if maximum <= 0.0 { return color; }
    let inverted = 1.0 - maximum;
    let scaled = 1.0 - inverted * inverted * inverted * inverted;
    return color * (scaled / maximum);
}

// Minecraft 26.2 lightmap.fsh evaluated directly from the packed light levels.
// night_factor stores sky factor and sky-light RGB.
fn unpack_light(light_data: u32) -> vec3<f32> {
    let block_light = f32(light_data & 0xFu);
    let sky_light = f32((light_data >> 4u) & 0xFu);
    let block_level = block_light / 15.0;
    let sky_level = sky_light / 15.0;
    let block_tint = vec3<f32>(1.0, 0.843, 0.549);
    let parabolic = (2.0 * block_level - 1.0) * (2.0 * block_level - 1.0);
    let block_color = mix(block_tint, vec3<f32>(1.0), 0.9 * parabolic);
    var color = vec3<f32>(10.0 / 255.0);
    color += uniforms.night_factor.yzw * light_brightness(sky_level) * uniforms.night_factor.x;
    color += block_color * light_brightness(block_level) * 1.4;
    color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    return mix(color, not_gamma(color), clamp(uniforms.fog_params.z, 0.0, 1.0));
}

fn unpack_ao(light_data: u32) -> f32 {
    let ao_raw = f32((light_data >> 8u) & 0xFu) / 3.0;
    return mix(0.2, 1.0, ao_raw);
}

fn unpack_emissive(light_data: u32) -> f32 {
    return f32((light_data >> 12u) & 0xFu) / 15.0;
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

// Vanilla face brightness: top=1.0, north/south=0.8, east/west=0.6, bottom=0.5
fn face_multiplier(normal: vec3<f32>) -> f32 {
    if normal.y > 0.5 { return 1.0; }
    if normal.y < -0.5 { return 0.5; }
    if abs(normal.z) > 0.5 { return 0.8; }
    return 0.6;
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
    output.emissive = unpack_emissive(input.light_data);
    output.shadow_pos = world_pos;
    return output;
}

@vertex
fn vs_shadow(input: VertexInput) -> ShadowOutput {
    var output: ShadowOutput;
    output.clip_pos = uniforms.shadow_vp_matrix * vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    output.tex_index = input.tex_index;
    return output;
}

// NOTE: These constants must match src/assets/mod.rs:
//   ATLAS_TILES_PER_ROW = 32  →  TILES_PER_ROW = 32.0
//   ATLAS_TILE_SIZE     = 16  →  TILE_SIZE = 1.0 / 32.0
//   ATLAS_SIZE          = 512
const TILES_PER_ROW: f32 = 32.0;
const TILE_SIZE: f32 = 1.0 / TILES_PER_ROW;
const WATER_FLAG: u32 = 1u << 31u;
const LEAVES_FLAG: u32 = 1u << 30u;
const TRANSLUCENT_FLAG: u32 = 1u << 29u;
const CUTOUT_FLAG: u32 = 1u << 28u;
const ANIMATION_KIND_MASK: u32 = 15u << 24u;
const TILE_INDEX_MASK: u32 = (1u << 24u) - 1u;

fn get_tile_uv(uv: vec2<f32>, tile_index: u32) -> vec2<f32> {
    // 32u below must match ATLAS_TILES_PER_ROW (32) from src/assets/mod.rs
    let animation_kind = (tile_index & ANIMATION_KIND_MASK) >> 24u;
    let tick = u32(floor(uniforms.time.x * 20.0));
    var frame = 0u;
    if animation_kind == 1u {
        frame = (tick / 2u) % 32u;
    } else if animation_kind == 2u {
        let sequence = (tick / 2u) % 38u;
        frame = select(38u - sequence, sequence, sequence < 20u);
    } else if animation_kind == 3u || animation_kind == 4u {
        frame = (tick / 2u) % 20u;
    }
    let raw_tile_index = (tile_index & TILE_INDEX_MASK) + frame;
    let tile_x = f32(raw_tile_index % 32u);
    let tile_y = f32(raw_tile_index / 32u);
    let base = vec2<f32>(tile_x, tile_y) * TILE_SIZE;
    let wrapped = fract(uv);
    return base + vec2<f32>(wrapped.x, 1.0 - wrapped.y) * TILE_SIZE;
}

fn is_water_tile(tex_index: u32) -> bool {
    return (tex_index & WATER_FLAG) != 0u;
}

fn is_leaf_tile(tex_index: u32) -> bool {
    return (tex_index & LEAVES_FLAG) != 0u;
}

fn is_cutout_tile(tex_index: u32) -> bool {
    return (tex_index & (LEAVES_FLAG | CUTOUT_FLAG)) != 0u;
}

@fragment
fn fs_shadow(input: ShadowOutput) {
    if is_cutout_tile(input.tex_index) {
        let alpha = textureSample(atlas, atlas_sampler, get_tile_uv(input.uv, input.tex_index)).a;
        if alpha < 0.1 { discard; }
    }
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tile_uv = get_tile_uv(input.uv, input.tex_index);
    var color = textureSample(atlas, atlas_sampler, tile_uv);

    // Cutout blocks write depth only for opaque texels. Without this, foliage
    // and crossed vegetation become opaque rectangles in both render passes.
    if is_cutout_tile(input.tex_index) && color.a < 0.1 {
        discard;
    }

    let night = clamp((1.0 - uniforms.night_factor.x) / 0.76, 0.0, 1.0);
    // Face brightness multiplier (vanilla Minecraft style)
    let face_mult = face_multiplier(input.normal);
    color = vec4<f32>(color.rgb * face_mult, color.a);

    // === Lighting ===
    // Base illumination from sky/block light (Minecraft's lightmap curve)
    // Vanilla terrain uses the lightmap and ambient occlusion rather than
    // camera-relative real-time sun shadows. This keeps terrain lighting fixed
    // as the player moves and avoids directional shadow-map seams.
    var brightness = input.light * input.ao;
    // Emissive blocks (glowstone, torch, lava) add their own light
    brightness = max(brightness, vec3<f32>(input.emissive));

    color = vec4<f32>(color.rgb * brightness, color.a);

    // Leaves: subtle light transmission
    if is_leaf_tile(input.tex_index) {
        color = vec4<f32>(color.rgb, color.a);
    }

    // Water: subtle reflection + blue tint
    if is_water_tile(input.tex_index) {
        let view_dir = normalize(uniforms.camera_pos.xyz - input.world_pos);
        let fresnel = 0.05 + 0.3 * pow(1.0 - max(dot(view_dir, input.normal), 0.0), 4.0);
        let sky_reflect = mix(vec3<f32>(0.4, 0.6, 0.9), vec3<f32>(0.02, 0.02, 0.1), night);
        let water_blue = vec3<f32>(0.1, 0.3, 0.4);
        let reflected = mix(color.rgb, sky_reflect, fresnel);
        color = vec4<f32>(mix(reflected, water_blue, 0.15), mix(0.60, 0.94, uniforms.fog_params.w));
    }

    // Approximate 26.2's dark-blue water environment at half of its default
    // 96-block range until the water-vision ramp is represented.
    let underwater = uniforms.fog_params.w;
    let underwater_fog_color = vec3<f32>(0.016, 0.086, 0.20);
    let underwater_fog_start = -8.0;
    let underwater_fog_end = 48.0;

    // Vanilla-style linear fog with squared curve
    let day_fog = vec3<f32>(0.65, 0.80, 0.98);
    let night_fog = vec3<f32>(0.02, 0.02, 0.1);
    let fog_color = mix(day_fog, night_fog, night);
    let fog_start = mix(uniforms.fog_params.x, underwater_fog_start, underwater);
    let fog_end = mix(uniforms.fog_params.y, underwater_fog_end, underwater);
    let final_fog_color = mix(fog_color, underwater_fog_color, underwater);
    let fog_factor = clamp((input.distance - fog_start) / (fog_end - fog_start), 0.0, 1.0);
    color = vec4<f32>(mix(color.rgb, final_fog_color, fog_factor), color.a);

    return color;
}
