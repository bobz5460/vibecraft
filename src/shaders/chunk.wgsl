struct Uniforms {
    vp_matrix: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_direction: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tex_index: u32,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_index: u32,
    @location(3) world_pos: vec3<f32>,
    @location(4) distance: f32,
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
    let ambient = 0.4;
    let brightness = ambient + ndotl * 0.6;
    color = vec4<f32>(color.r * brightness, color.g * brightness, color.b * brightness, color.a);

    let fog_factor = 1.0 - exp(-0.002 * input.distance);
    color = vec4<f32>(mix(color.rgb, vec3<f32>(0.5, 0.65, 0.85), fog_factor), color.a);

    return color;
}
