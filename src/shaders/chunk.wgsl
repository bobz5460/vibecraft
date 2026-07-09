struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
    night_factor: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

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
    return output;
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

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tile_uv = get_tile_uv(input.uv, input.tex_index);
    var color = textureSample(atlas, atlas_sampler, tile_uv);

    let light_dir = normalize(uniforms.light_direction.xyz);
    let ndotl = max(dot(input.normal, light_dir), 0.0);

    let night = uniforms.night_factor.x;
    let ambient = mix(0.35, 0.06, night);
    let directional = ndotl * (1.0 - night * 0.5);

    // AO multiplier: 0.75 at full occlusion, 1.0 at no occlusion
    let ao_factor = 0.75 + 0.25 * input.ao;

    let brightness = (ambient + directional) * input.light * ao_factor;
    color = vec4<f32>(color.r * brightness, color.g * brightness, color.b * brightness, color.a);

    // Sky color tint on upward-facing surfaces (daytime)
    let is_top = input.normal.y > 0.5;
    let day_factor = 1.0 - night;
    let sky_tint = vec3<f32>(0.85, 0.9, 1.0);
    let warm_tint = vec3<f32>(1.0, 0.7, 0.4);
    let sunset_tint = mix(sky_tint, warm_tint, smoothstep(0.0, 0.5, night) * (1.0 - smoothstep(0.5, 1.0, night)));
    if is_top && day_factor > 0.1 {
        let tint_strength = 0.12 * day_factor;
        color = vec4<f32>(mix(color.rgb, color.rgb * sunset_tint, tint_strength), color.a);
    }

    let night_sky = mix(vec3<f32>(0.5, 0.65, 0.85), vec3<f32>(0.05, 0.05, 0.15), night);
    let fog_factor = 1.0 - exp(-0.002 * input.distance);
    color = vec4<f32>(mix(color.rgb, night_sky, fog_factor), color.a);

    return color;
}
