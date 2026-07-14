use crate::world::block::{Block, BlockFace, BlockId, FACES};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use crate::world::block_registry::{registry, CollisionShape};
use crate::assets::BlockMeshAssets;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

type FaceTexMap = HashMap<(BlockId, BlockFace), u32>;
type CrossedTexMap = HashMap<BlockId, u32>;
static FACE_TEX: OnceLock<FaceTexMap> = OnceLock::new();
static CROSSED_TEX: OnceLock<CrossedTexMap> = OnceLock::new();
static MODEL_ASSETS: OnceLock<Arc<BlockMeshAssets>> = OnceLock::new();

pub fn set_texture_lookups(face: FaceTexMap, crossed: CrossedTexMap) {
    let _ = FACE_TEX.set(face);
    let _ = CROSSED_TEX.set(crossed);
}

pub fn set_model_assets(assets: Arc<BlockMeshAssets>) {
    let _ = MODEL_ASSETS.set(assets);
}

fn model_assets() -> Option<&'static BlockMeshAssets> {
    MODEL_ASSETS.get().map(Arc::as_ref)
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub tex_index: u32,
    pub light_data: u32,
}

#[derive(Clone, Debug)]
pub struct ChunkMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub transparent_vertices: Vec<MeshVertex>,
    pub transparent_indices: Vec<u32>,
}

fn get_face_normal(face: BlockFace) -> [f32; 3] {
    match face {
        BlockFace::Top => [0.0, 1.0, 0.0],
        BlockFace::Bottom => [0.0, -1.0, 0.0],
        BlockFace::Left => [-1.0, 0.0, 0.0],
        BlockFace::Right => [1.0, 0.0, 0.0],
        BlockFace::Front => [0.0, 0.0, 1.0],
        BlockFace::Back => [0.0, 0.0, -1.0],
    }
}

fn get_texture_index(block: Block, face: BlockFace) -> u32 {
    let tile_index = get_face_tile(block.id, face);
    tile_index | material_flags(block.id)
}

pub fn get_face_tile(block_id: BlockId, face: BlockFace) -> u32 {
    let tex = FACE_TEX
        .get()
        .and_then(|m| m.get(&(block_id, face)))
        .copied()
        .unwrap_or(u32::MAX);
    if tex == u32::MAX {
        log::warn!("Missing face texture for block {:?} face {:?}", block_id, face);
    }
    tex
}

// The shader must not infer a material from the atlas order: that order changes
// whenever the block texture list changes.
fn material_flags(block_id: BlockId) -> u32 {
    const WATER: u32 = 1 << 31;
    const LEAVES: u32 = 1 << 30;
    const TRANSLUCENT: u32 = 1 << 29;
    const CUTOUT: u32 = 1 << 28;

    match block_id {
        BlockId::Water => WATER,
        BlockId::OakLeaves
        | BlockId::OakLeaves2
        | BlockId::SpruceLeaves
        | BlockId::BirchLeaves
        | BlockId::JungleLeaves
        | BlockId::AcaciaLeaves
        | BlockId::DarkOakLeaves
        | BlockId::CherryLeaves
        | BlockId::MangroveLeaves
        | BlockId::AzaleaLeaves
        | BlockId::FloweringAzaleaLeaves => LEAVES,
        id if id.is_crossed() => CUTOUT,
        id if uses_translucent_blend(Block::new(id)) => TRANSLUCENT,
        _ => 0,
    }
}

/// Vanilla's leaves and other cutout blocks write depth; only fluids and glass
/// belong in the blended pass. Treating every transparent block as blended made
/// overlapping leaf layers look pale and see-through.
fn uses_translucent_blend(block: Block) -> bool {
    matches!(
        block.id,
        BlockId::Water
            | BlockId::Glass
            | BlockId::Ice
            | BlockId::TintedGlass
            | BlockId::WhiteStainedGlass
            | BlockId::OrangeStainedGlass
            | BlockId::MagentaStainedGlass
            | BlockId::LightBlueStainedGlass
            | BlockId::YellowStainedGlass
            | BlockId::LimeStainedGlass
            | BlockId::PinkStainedGlass
            | BlockId::GrayStainedGlass
            | BlockId::LightGrayStainedGlass
            | BlockId::CyanStainedGlass
            | BlockId::PurpleStainedGlass
            | BlockId::BlueStainedGlass
            | BlockId::BrownStainedGlass
            | BlockId::GreenStainedGlass
            | BlockId::RedStainedGlass
            | BlockId::BlackStainedGlass
    )
}

fn can_see_face(block: Block, neighbor: Block) -> bool {
    if neighbor.is_air() {
        return true;
    }
    if block.id.is_transparent() && neighbor.id.is_transparent() {
        return block.id != neighbor.id;
    }
    if block.id.is_transparent() {
        return true;
    }
    neighbor.id.is_transparent()
}

fn rotate_point(mut point: [f32; 3], origin: [f32; 3], axis: &str, angle: i32) -> [f32; 3] {
    let radians = (angle as f32).to_radians();
    let (sin, cos) = radians.sin_cos();
    for i in 0..3 { point[i] -= origin[i]; }
    match axis {
        "x" => {
            let (y, z) = (point[1], point[2]);
            point[1] = y * cos - z * sin;
            point[2] = y * sin + z * cos;
        }
        "y" => {
            let (x, z) = (point[0], point[2]);
            point[0] = x * cos + z * sin;
            point[2] = -x * sin + z * cos;
        }
        "z" => {
            let (x, y) = (point[0], point[1]);
            point[0] = x * cos - y * sin;
            point[1] = x * sin + y * cos;
        }
        _ => {}
    }
    for i in 0..3 { point[i] += origin[i]; }
    point
}

fn rotate_vector(mut vector: [f32; 3], axis: &str, angle: i32) -> [f32; 3] {
    let radians = (angle as f32).to_radians();
    let (sin, cos) = radians.sin_cos();
    match axis {
        "x" => {
            let (y, z) = (vector[1], vector[2]);
            vector[1] = y * cos - z * sin;
            vector[2] = y * sin + z * cos;
        }
        "y" => {
            let (x, z) = (vector[0], vector[2]);
            vector[0] = x * cos + z * sin;
            vector[2] = -x * sin + z * cos;
        }
        "z" => {
            let (x, y) = (vector[0], vector[1]);
            vector[0] = x * cos - y * sin;
            vector[1] = x * sin + y * cos;
        }
        _ => {}
    }
    vector
}

fn model_face(direction: &str, from: [f32; 3], to: [f32; 3]) -> Option<([[f32; 3]; 4], [f32; 3])> {
    Some(match direction {
        "down" => ([[from[0], from[1], from[2]], [to[0], from[1], from[2]], [to[0], from[1], to[2]], [from[0], from[1], to[2]]], [0.0, -1.0, 0.0]),
        "up" => ([[from[0], to[1], from[2]], [from[0], to[1], to[2]], [to[0], to[1], to[2]], [to[0], to[1], from[2]]], [0.0, 1.0, 0.0]),
        "north" => ([[from[0], from[1], from[2]], [from[0], to[1], from[2]], [to[0], to[1], from[2]], [to[0], from[1], from[2]]], [0.0, 0.0, -1.0]),
        "south" => ([[from[0], from[1], to[2]], [to[0], from[1], to[2]], [to[0], to[1], to[2]], [from[0], to[1], to[2]]], [0.0, 0.0, 1.0]),
        "west" => ([[from[0], from[1], from[2]], [from[0], from[1], to[2]], [from[0], to[1], to[2]], [from[0], to[1], from[2]]], [-1.0, 0.0, 0.0]),
        "east" => ([[to[0], from[1], from[2]], [to[0], to[1], from[2]], [to[0], to[1], to[2]], [to[0], from[1], to[2]]], [1.0, 0.0, 0.0]),
        _ => return None,
    })
}

fn emit_resolved_models<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    block: Block,
    local: (i32, i32, i32),
    opaque: &mut Vec<MeshVertex>,
    opaque_indices: &mut Vec<u32>,
    transparent: &mut Vec<MeshVertex>,
    transparent_indices: &mut Vec<u32>,
) -> bool {
    let Some(assets) = model_assets() else { return false; };
    if !assets.has_model(block.id) { return false; }
    let (x, y, z) = local;
    let models = assets.resolve(block.id, block.state, (chunk.cx * CHUNK_SIZE as i32 + x, y, chunk.cz * CHUNK_SIZE as i32 + z));
    if models.is_empty() { return false; }
    let blended = uses_translucent_blend(block);
    let (verts, indices) = if blended { (transparent, transparent_indices) } else { (opaque, opaque_indices) };
    let light = get_light_data(chunk, get_neighbor, x, y, z);
    let light_data = (block.id.light_level() as u32) << 12 | (3 << 8) | light;
    for (model, transform) in models {
        for element in &model.elements {
            let from = [element.from[0] / 16.0, element.from[1] / 16.0, element.from[2] / 16.0];
            let to = [element.to[0] / 16.0, element.to[1] / 16.0, element.to[2] / 16.0];
            for face in &element.faces {
                // A cullface only describes an element that reaches the block
                // boundary. Never use it to hide partial geometry behind another
                // partial model, which would create holes in fences and stairs.
                if let Some(cullface) = face.cullface.as_deref() {
                    let (dx, dy, dz) = match cullface {
                        "down" => (0, -1, 0),
                        "up" => (0, 1, 0),
                        "north" => (0, 0, -1),
                        "south" => (0, 0, 1),
                        "west" => (-1, 0, 0),
                        "east" => (1, 0, 0),
                        _ => (0, 0, 0),
                    };
                    let neighbor = if dx != 0 || dy != 0 || dz != 0 {
                        if y + dy < 0 || y + dy >= CHUNK_HEIGHT as i32 {
                            Block::air()
                        } else if x + dx >= 0 && x + dx < CHUNK_SIZE as i32 && z + dz >= 0 && z + dz < CHUNK_SIZE as i32 {
                            chunk.get_block((x + dx) as usize, (y + dy) as usize, (z + dz) as usize)
                        } else {
                            let cx = chunk.cx + (x + dx).div_euclid(CHUNK_SIZE as i32);
                            let cz = chunk.cz + (z + dz).div_euclid(CHUNK_SIZE as i32);
                            get_neighbor(cx, cz).map_or(Block::air(), |chunk| chunk.get_block((x + dx).rem_euclid(CHUNK_SIZE as i32) as usize, (y + dy) as usize, (z + dz).rem_euclid(CHUNK_SIZE as i32) as usize))
                        }
                    } else {
                        Block::air()
                    };
                    if registry().definition(neighbor.id).collision == CollisionShape::FullCube && !neighbor.id.is_transparent() {
                        continue;
                    }
                }
                let Some((mut corners, mut normal)) = model_face(&face.direction, from, to) else { continue; };
                if let Some(rotation) = &element.rotation {
                    let origin = [rotation.origin[0] / 16.0, rotation.origin[1] / 16.0, rotation.origin[2] / 16.0];
                    for corner in &mut corners { *corner = rotate_point(*corner, origin, &rotation.axis, rotation.angle as i32); }
                    normal = rotate_vector(normal, &rotation.axis, rotation.angle as i32);
                }
                for corner in &mut corners { *corner = rotate_point(*corner, [0.5, 0.5, 0.5], "x", transform.x); *corner = rotate_point(*corner, [0.5, 0.5, 0.5], "y", transform.y); }
                normal = rotate_vector(rotate_vector(normal, "x", transform.x), "y", transform.y);
                let Some(tile) = assets.texture_tile(&face.texture) else { log::warn!("missing generic-model atlas tile {}", face.texture); continue; };
                let base = verts.len() as u32;
                let [u0, v0, u1, v1] = face.uv.unwrap_or([0.0, 0.0, 16.0, 16.0]);
                let mut uvs = [[u0 / 16.0, v0 / 16.0], [u1 / 16.0, v0 / 16.0], [u1 / 16.0, v1 / 16.0], [u0 / 16.0, v1 / 16.0]];
                uvs.rotate_right((face.rotation.rem_euclid(360) / 90) as usize);
                for (corner, uv) in corners.into_iter().zip(uvs) {
                    verts.push(MeshVertex { pos: [corner[0] + chunk.cx as f32 * CHUNK_SIZE as f32 + x as f32, corner[1] + y as f32, corner[2] + chunk.cz as f32 * CHUNK_SIZE as f32 + z as f32], uv, normal, tex_index: tile | material_flags(block.id), light_data });
                }
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
    true
}

fn fluid_height(block: Block) -> f32 {
    if block.data == 0 || block.data >= 8 { 1.0 } else { (8 - block.data) as f32 / 8.0 }
}

fn emit_fluid<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    get_block: &impl Fn(i32, i32, i32) -> Block,
    block: Block,
    local: (i32, i32, i32),
    vertices: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
) -> bool {
    if !matches!(block.id, BlockId::Water | BlockId::Lava) { return false; }
    let (x, y, z) = local;
    let height = fluid_height(block);
    let world_x = chunk.cx as f32 * CHUNK_SIZE as f32 + x as f32;
    let world_z = chunk.cz as f32 * CHUNK_SIZE as f32 + z as f32;
    let tex = get_texture_index(block, BlockFace::Top);
    let light = get_light_data(chunk, get_neighbor, x, y, z);
    let light_data = (block.id.light_level() as u32) << 12 | (3 << 8) | light;
    let above = get_block(x, y + 1, z);
    if above.id != block.id {
        emit_quad_light(vertices, indices, BlockFace::Top, world_x, y as f32 + height, world_z, 1.0, 1.0, tex, 0, 2);
        let start = vertices.len() - 4;
        for vertex in &mut vertices[start..] { vertex.light_data = light_data; }
    }
    for (dx, dz, face) in [(0, -1, BlockFace::Back), (1, 0, BlockFace::Right), (0, 1, BlockFace::Front), (-1, 0, BlockFace::Left)] {
        let neighbor = get_block(x + dx, y, z + dz);
        let neighbor_height = if neighbor.id == block.id { fluid_height(neighbor) } else { 0.0 };
        if neighbor.id != block.id || neighbor_height < height {
            let (fx, fz, width) = match face {
                BlockFace::Back => (world_x, world_z, 1.0),
                BlockFace::Right => (world_x + 1.0, world_z, 1.0),
                BlockFace::Front => (world_x, world_z + 1.0, 1.0),
                BlockFace::Left => (world_x, world_z, 1.0),
                _ => unreachable!(),
            };
            let (u_axis, v_axis) = match face {
                BlockFace::Left | BlockFace::Right => (2, 1),
                _ => (0, 1),
            };
            emit_quad_light(vertices, indices, face, fx, y as f32 + neighbor_height, fz, width, height - neighbor_height, tex, u_axis, v_axis);
            let start = vertices.len() - 4;
            for vertex in &mut vertices[start..] { vertex.light_data = light_data; }
        }
    }
    true
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn model_face_has_outward_winding() {
        let (corners, normal) = model_face("up", [0.0; 3], [1.0; 3]).unwrap();
        let ab = [corners[1][0] - corners[0][0], corners[1][1] - corners[0][1], corners[1][2] - corners[0][2]];
        let ac = [corners[2][0] - corners[0][0], corners[2][1] - corners[0][1], corners[2][2] - corners[0][2]];
        let cross = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
        assert!(cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2] > 0.0);
    }

    #[test]
    fn model_rotation_preserves_unit_axes() {
        assert_eq!(rotate_vector([0.0, 0.0, -1.0], "y", 90).map(|value| value.round()), [-1.0, 0.0, 0.0]);
    }
}

fn emit_quad_light(
    verts: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    face: BlockFace,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    h: f32,
    tex_index: u32,
    u_axis: usize,
    v_axis: usize,
) {
    let normal = get_face_normal(face);
    let base = verts.len() as u32;

    let mut c = [[0.0f32; 3]; 4];
    c[0] = [x, y, z];
    let mut c1 = [x, y, z];
    c1[u_axis] += w;
    c[1] = c1;
    let mut c2 = [x, y, z];
    c2[u_axis] += w;
    c2[v_axis] += h;
    c[2] = c2;
    let mut c3 = [x, y, z];
    c3[v_axis] += h;
    c[3] = c3;

    let uvs = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];

    let reversed = match face {
        BlockFace::Top | BlockFace::Right | BlockFace::Back => true,
        BlockFace::Bottom | BlockFace::Left | BlockFace::Front => false,
    };

    for i in 0..4 {
        verts.push(MeshVertex {
            pos: c[i],
            uv: uvs[i],
            normal,
            tex_index,
            light_data: 0,
        });
    }

    if reversed {
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    } else {
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Check if a block at chunk-local coords blocks AO (opaque or full-cube transparent).
fn is_solid_block<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    bx: i32,
    by: i32,
    bz: i32,
) -> bool {
    let block = if bx >= 0
        && bx < CHUNK_SIZE as i32
        && by >= 0
        && by < CHUNK_HEIGHT as i32
        && bz >= 0
        && bz < CHUNK_SIZE as i32
    {
        chunk.get_block(bx as usize, by as usize, bz as usize)
    } else {
        let cx = chunk.cx + bx.div_euclid(CHUNK_SIZE as i32);
        let cz = chunk.cz + bz.div_euclid(CHUNK_SIZE as i32);
        let lx = bx.rem_euclid(CHUNK_SIZE as i32) as usize;
        let lz = bz.rem_euclid(CHUNK_SIZE as i32) as usize;
        if by >= 0 && by < CHUNK_HEIGHT as i32 {
            match get_neighbor(cx, cz) {
                Some(nc) => nc.get_block(lx, by as usize, lz),
                None => Block::air(),
            }
        } else {
            Block::air()
        }
    };
    if block.is_air() {
        return false;
    }
    if block.id.is_solid() {
        return true;
    }
    // Full-cube transparent blocks (glass, leaves, ice) still darken corners for AO.
    // Exclude partial blocks (crossed, slab, stair) and fluids.
    if block.id.is_crossed() || block.id.is_slab() || block.id.is_stair() {
        return false;
    }
    match block.id {
        BlockId::Water | BlockId::Lava => false,
        _ => block.id.is_transparent(),
    }
}

/// Apply per-vertex AO to the last 4 vertices emitted by emit_quad_light.
/// Uses the vanilla Minecraft AO formula: if both side edges are solid → AO=0,
/// otherwise AO = 3 - (side1 + side2 + corner). Each vertex also samples
/// the light at its own corner position (per-vertex light).
fn apply_vertex_lighting<'a>(
    verts: &mut Vec<MeshVertex>,
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    face: BlockFace,
    u_axis: usize,
    v_axis: usize,
    emissive: u8,
) {
    let n = verts.len();
    if n < 4 {
        return;
    }
    let base = n - 4;

    for i in 0..4 {
        verts[base + i].light_data = vertex_light_data(
            chunk,
            get_neighbor,
            face,
            u_axis,
            v_axis,
            emissive,
            verts[base + i].pos,
            i,
        );
    }
}

fn vertex_light_data<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    face: BlockFace,
    u_axis: usize,
    v_axis: usize,
    emissive: u8,
    pos: [f32; 3],
    corner: usize,
) -> u32 {
    let vx = pos[0].floor() as i32;
    let vy = pos[1].floor() as i32;
    let vz = pos[2].floor() as i32;
    let (sx, sy, sz) = match face {
        BlockFace::Bottom => (vx, vy - 1, vz),
        BlockFace::Left => (vx - 1, vy, vz),
        BlockFace::Back => (vx, vy, vz - 1),
        _ => (vx, vy, vz),
    };
    let ao_dirs: [[i32; 2]; 4] = [[-1, -1], [1, -1], [1, 1], [-1, 1]];
    let d = ao_dirs[corner];

    // Both light and AO sample the air plane immediately outside the face.
    let mut base = [sx, sy, sz];
    if d[0] > 0 {
        base[u_axis] -= 1;
    }
    if d[1] > 0 {
        base[v_axis] -= 1;
    }

    let mut off_a = [0i32; 3];
    let mut off_b = [0i32; 3];
    off_a[u_axis] = d[0];
    off_b[v_axis] = d[1];

    let solid = |offset: [i32; 3]| {
        is_solid_block(
            chunk,
            get_neighbor,
            base[0] + offset[0] - chunk.cx * CHUNK_SIZE as i32,
            base[1] + offset[1],
            base[2] + offset[2] - chunk.cz * CHUNK_SIZE as i32,
        ) as u32
    };
    let s1 = solid(off_a);
    let s2 = solid(off_b);
    let s3 = solid([
        off_a[0] + off_b[0],
        off_a[1] + off_b[1],
        off_a[2] + off_b[2],
    ]);
    let ao = if s1 == 1 && s2 == 1 {
        0
    } else {
        3 - (s1 + s2 + s3)
    };
    let sample_light = |offset: [i32; 3]| {
        get_light_data(
            chunk,
            get_neighbor,
            base[0] + offset[0] - chunk.cx * CHUNK_SIZE as i32,
            base[1] + offset[1],
            base[2] + offset[2] - chunk.cz * CHUNK_SIZE as i32,
        )
    };
    // Solid side cells store zero light. Preserve the brightest value in each
    // channel so one solid cell cannot turn an otherwise exposed corner black.
    let light_samples = [
        sample_light([0, 0, 0]),
        sample_light(off_a),
        sample_light(off_b),
        sample_light([
            off_a[0] + off_b[0],
            off_a[1] + off_b[1],
            off_a[2] + off_b[2],
        ]),
    ];
    let sky = light_samples
        .iter()
        .map(|light| (light >> 4) & 0xF)
        .max()
        .unwrap_or(0);
    let block = light_samples
        .iter()
        .map(|light| light & 0xF)
        .max()
        .unwrap_or(0);
    (emissive as u32) << 12 | (ao << 8) | (sky << 4) | block
}

/// Get packed light data at a world block position, reading from chunk or neighbor.
fn get_light_data<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    bx: i32,
    by: i32,
    bz: i32,
) -> u32 {
    if bx >= 0
        && bx < CHUNK_SIZE as i32
        && by >= 0
        && by < CHUNK_HEIGHT as i32
        && bz >= 0
        && bz < CHUNK_SIZE as i32
    {
        let (sky, block) = chunk.get_light_at(bx, by, bz);
        Chunk::pack_light(sky, block)
    } else {
        if by < 0 {
            return Chunk::pack_light(0, 0);
        }
        if by >= CHUNK_HEIGHT as i32 {
            return Chunk::pack_light(15, 0);
        }
        let cx = chunk.cx + bx.div_euclid(CHUNK_SIZE as i32);
        let cz = chunk.cz + bz.div_euclid(CHUNK_SIZE as i32);
        let lx = bx.rem_euclid(CHUNK_SIZE as i32);
        let lz = bz.rem_euclid(CHUNK_SIZE as i32);
        match get_neighbor(cx, cz) {
            Some(nc) => {
                let (sky, block) = nc.get_light_at(lx, by, lz);
                Chunk::pack_light(sky, block)
            }
            // Missing horizontal neighbors are rendered as open air, so their
            // light fallback must match instead of producing black frontier faces.
            None => Chunk::pack_light(15, 0),
        }
    }
}

/// Get light at the neighbor position adjacent to a face.
fn face_light_signature<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    face: BlockFace,
    bx: i32,
    by: i32,
    bz: i32,
) -> [u32; 4] {
    let (x, y, z, u_axis, v_axis) = match face {
        BlockFace::Top => (bx as f32, by as f32 + 1.0, bz as f32, 0, 2),
        BlockFace::Bottom => (bx as f32, by as f32, bz as f32, 0, 2),
        BlockFace::Left => (bx as f32, by as f32, bz as f32, 2, 1),
        BlockFace::Right => (bx as f32 + 1.0, by as f32, bz as f32, 2, 1),
        BlockFace::Front => (bx as f32, by as f32, bz as f32 + 1.0, 0, 1),
        BlockFace::Back => (bx as f32, by as f32, bz as f32, 0, 1),
    };
    let mut corners = [[x, y, z]; 4];
    corners[1][u_axis] += 1.0;
    corners[2][u_axis] += 1.0;
    corners[2][v_axis] += 1.0;
    corners[3][v_axis] += 1.0;
    let emissive = chunk.get_block(bx as usize, by as usize, bz as usize).id.light_level();
    [
        vertex_light_data(chunk, get_neighbor, face, u_axis, v_axis, emissive, [
            corners[0][0] + chunk.cx as f32 * CHUNK_SIZE as f32,
            corners[0][1],
            corners[0][2] + chunk.cz as f32 * CHUNK_SIZE as f32,
        ], 0),
        vertex_light_data(chunk, get_neighbor, face, u_axis, v_axis, emissive, [
            corners[1][0] + chunk.cx as f32 * CHUNK_SIZE as f32,
            corners[1][1],
            corners[1][2] + chunk.cz as f32 * CHUNK_SIZE as f32,
        ], 1),
        vertex_light_data(chunk, get_neighbor, face, u_axis, v_axis, emissive, [
            corners[2][0] + chunk.cx as f32 * CHUNK_SIZE as f32,
            corners[2][1],
            corners[2][2] + chunk.cz as f32 * CHUNK_SIZE as f32,
        ], 2),
        vertex_light_data(chunk, get_neighbor, face, u_axis, v_axis, emissive, [
            corners[3][0] + chunk.cx as f32 * CHUNK_SIZE as f32,
            corners[3][1],
            corners[3][2] + chunk.cz as f32 * CHUNK_SIZE as f32,
        ], 3),
    ]
}

pub fn build_chunk_mesh<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
) -> ChunkMesh {
    // Avoid retaining megabytes of mostly-unused capacity for every loaded chunk.
    let mut vertices = Vec::with_capacity(1024);
    let mut indices = Vec::with_capacity(1536);
    let mut transparent_vertices = Vec::with_capacity(256);
    let mut transparent_indices = Vec::with_capacity(384);

    let get_block_fn = |x: i32, y: i32, z: i32| -> Block {
        if x >= 0
            && x < CHUNK_SIZE as i32
            && y >= 0
            && y < CHUNK_HEIGHT as i32
            && z >= 0
            && z < CHUNK_SIZE as i32
        {
            chunk.get_block(x as usize, y as usize, z as usize)
        } else {
            let cx = chunk.cx + x.div_euclid(CHUNK_SIZE as i32);
            let cz = chunk.cz + z.div_euclid(CHUNK_SIZE as i32);
            let lx = x.rem_euclid(CHUNK_SIZE as i32) as usize;
            let lz = z.rem_euclid(CHUNK_SIZE as i32) as usize;
            if y >= 0 && y < CHUNK_HEIGHT as i32 {
                match get_neighbor(cx, cz) {
                    Some(nc) => nc.get_block(lx, y as usize, lz),
                    None => Block::air(),
                }
            } else {
                Block::air()
            }
        }
    };

    let wox = chunk.cx as f32 * CHUNK_SIZE as f32;
    let woz = chunk.cz as f32 * CHUNK_SIZE as f32;

    let max_mask_size = (CHUNK_SIZE as i32 * CHUNK_HEIGHT as i32) as usize;
    let mut mask: Vec<u64> = Vec::with_capacity((max_mask_size + 63) / 64);
    let mut processed: Vec<u64> = Vec::with_capacity((max_mask_size + 63) / 64);

    for &face in &FACES {
        let (_u_axis, _v_axis, _w_axis, w_start, w_end, w_step, u_size, v_size) = match face {
            BlockFace::Top | BlockFace::Bottom => (
                0,
                2,
                1,
                0,
                CHUNK_HEIGHT as i32,
                1,
                CHUNK_SIZE as i32,
                CHUNK_SIZE as i32,
            ),
            BlockFace::Left | BlockFace::Right => (
                2,
                1,
                0,
                0,
                CHUNK_SIZE as i32,
                1,
                CHUNK_SIZE as i32,
                CHUNK_HEIGHT as i32,
            ),
            BlockFace::Front | BlockFace::Back => (
                0,
                1,
                2,
                0,
                CHUNK_SIZE as i32,
                1,
                CHUNK_SIZE as i32,
                CHUNK_HEIGHT as i32,
            ),
        };

        for w in (w_start..w_end).step_by(w_step as usize) {
            let size = (u_size * v_size) as usize;
            let u64_len = (size + 63) / 64;
            mask.clear();
            mask.resize(u64_len, 0);

            for u in 0..u_size {
                for v in 0..v_size {
                    let (bx, by, bz, nx, ny, nz) = match face {
                        BlockFace::Top => (u, w, v, u, w + 1, v),
                        BlockFace::Bottom => (u, w, v, u, w - 1, v),
                        BlockFace::Left => (w, v, u, w - 1, v, u),
                        BlockFace::Right => (w, v, u, w + 1, v, u),
                        BlockFace::Front => (u, v, w, u, v, w + 1),
                        BlockFace::Back => (u, v, w, u, v, w - 1),
                    };

                    let block = get_block_fn(bx, by, bz);
                    if block.is_air()
                        || block.id.is_crossed()
                        || matches!(block.id, BlockId::Water | BlockId::Lava)
                        || block.id.is_stair()
                        || block.id.is_slab()
                        || model_assets().is_some_and(|assets| assets.has_model(block.id))
                    {
                        continue;
                    }

                    let neighbor = get_block_fn(nx, ny, nz);
                    if can_see_face(block, neighbor) {
                        let mi = (v * u_size + u) as usize;
                        mask[mi >> 6] |= 1u64 << (mi & 63);
                    }
                }
            }

            if !mask.iter().any(|&m| m != 0) {
                continue;
            }

            processed.clear();
            processed.resize(u64_len, 0);

            let same_block = |ux: i32,
                              vy: i32,
                              start_id: BlockId,
                              start_state: u16,
                              start_data: u8| -> bool {
                let (cbx, cby, cbz) = match face {
                    BlockFace::Top | BlockFace::Bottom => (ux, w, vy),
                    BlockFace::Left | BlockFace::Right => (w, vy, ux),
                    BlockFace::Front | BlockFace::Back => (ux, vy, w),
                };
                let b = get_block_fn(cbx, cby, cbz);
                b.id == start_id && b.state == start_state && b.data == start_data
            };

            for y in 0..v_size {
                for x in 0..u_size {
                    let idx = (y * u_size + x) as usize;
                    if (mask[idx >> 6] >> (idx & 63)) & 1 == 0
                        || (processed[idx >> 6] >> (idx & 63)) & 1 != 0
                    {
                        continue;
                    }

                    let (sbx, sby, sbz) = match face {
                        BlockFace::Top | BlockFace::Bottom => (x as i32, w, y as i32),
                        BlockFace::Left | BlockFace::Right => (w, y as i32, x as i32),
                        BlockFace::Front | BlockFace::Back => (x as i32, y as i32, w),
                    };
                    let start_block = get_block_fn(sbx, sby, sbz);
                    let start_id = start_block.id;
                    let start_state = start_block.state;
                    let start_data = start_block.data;
                    let start_light =
                        face_light_signature(chunk, get_neighbor, face, sbx, sby, sbz);

                    let mut rect_w = 1;
                    while x + rect_w < u_size {
                        let ri = (y * u_size + x + rect_w) as usize;
                        if (mask[ri >> 6] >> (ri & 63)) & 1 == 0
                            || (processed[ri >> 6] >> (ri & 63)) & 1 != 0
                            || !same_block(
                                x as i32 + rect_w,
                                y as i32,
                                start_id,
                                start_state,
                                start_data,
                            )
                        {
                            break;
                        }
                        let (cbx, cby, cbz) = match face {
                            BlockFace::Top | BlockFace::Bottom => (x as i32 + rect_w, w, y as i32),
                            BlockFace::Left | BlockFace::Right => (w, y as i32, x as i32 + rect_w),
                            BlockFace::Front | BlockFace::Back => (x as i32 + rect_w, y as i32, w),
                        };
                        if face_light_signature(chunk, get_neighbor, face, cbx, cby, cbz)
                            != start_light
                        {
                            break;
                        }
                        rect_w += 1;
                    }

                    let mut rect_h = 1;
                    'outer: while y + rect_h < v_size {
                        for cx in x..x + rect_w {
                            let ci = ((y + rect_h) * u_size + cx) as usize;
                            if (mask[ci >> 6] >> (ci & 63)) & 1 == 0
                                || (processed[ci >> 6] >> (ci & 63)) & 1 != 0
                                || !same_block(
                                    cx as i32,
                                    y as i32 + rect_h,
                                    start_id,
                                    start_state,
                                    start_data,
                                )
                            {
                                break 'outer;
                            }
                            let (cbx, cby, cbz) = match face {
                                BlockFace::Top | BlockFace::Bottom => (cx as i32, w, y as i32 + rect_h),
                                BlockFace::Left | BlockFace::Right => (w, y as i32 + rect_h, cx as i32),
                                BlockFace::Front | BlockFace::Back => (cx as i32, y as i32 + rect_h, w),
                            };
                            if face_light_signature(chunk, get_neighbor, face, cbx, cby, cbz)
                                != start_light
                            {
                                break 'outer;
                            }
                        }
                        rect_h += 1;
                    }

                    for dy in 0..rect_h {
                        for dx in 0..rect_w {
                            let pi = ((y + dy) * u_size + (x + dx)) as usize;
                            processed[pi >> 6] |= 1u64 << (pi & 63);
                        }
                    }

                    let (bx, by, bz) = match face {
                        BlockFace::Top => (x as i32, w, y as i32),
                        BlockFace::Bottom => (x as i32, w, y as i32),
                        BlockFace::Left => (w, y as i32, x as i32),
                        BlockFace::Right => (w, y as i32, x as i32),
                        BlockFace::Front => (x as i32, y as i32, w),
                        BlockFace::Back => (x as i32, y as i32, w),
                    };
                    let block = get_block_fn(bx, by, bz);
                    let face_tex = get_texture_index(block, face);

                    let is_transp = uses_translucent_blend(block);

                    let verts = if is_transp {
                        &mut transparent_vertices
                    } else {
                        &mut vertices
                    };
                    let inds = if is_transp {
                        &mut transparent_indices
                    } else {
                        &mut indices
                    };

                    let (fx, fy, fz) = match face {
                        BlockFace::Top => (bx as f32, by as f32 + 1.0, bz as f32),
                        BlockFace::Bottom => (bx as f32, by as f32, bz as f32),
                        BlockFace::Left => (bx as f32, by as f32, bz as f32),
                        BlockFace::Right => (bx as f32 + 1.0, by as f32, bz as f32),
                        BlockFace::Front => (bx as f32, by as f32, bz as f32 + 1.0),
                        BlockFace::Back => (bx as f32, by as f32, bz as f32),
                    };
                    let (u_axis, v_axis) = match face {
                        BlockFace::Top | BlockFace::Bottom => (0usize, 2usize),
                        BlockFace::Left | BlockFace::Right => (2usize, 1usize),
                        BlockFace::Front | BlockFace::Back => (0usize, 1usize),
                    };
                    emit_quad_light(
                        verts,
                        inds,
                        face,
                        wox + fx,
                        fy,
                        woz + fz,
                        rect_w as f32,
                        rect_h as f32,
                        face_tex,
                        u_axis,
                        v_axis,
                    );
                    apply_vertex_lighting(
                        verts,
                        chunk,
                        get_neighbor,
                        face,
                        u_axis,
                        v_axis,
                        block.id.light_level(),
                    );
                }
            }
        }
    }

    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                let block = chunk.get_block(x, y, z);
                if emit_fluid(
                    chunk,
                    get_neighbor,
                    &get_block_fn,
                    block,
                    (x as i32, y as i32, z as i32),
                    &mut transparent_vertices,
                    &mut transparent_indices,
                ) {
                    continue;
                } else if emit_resolved_models(
                    chunk,
                    get_neighbor,
                    block,
                    (x as i32, y as i32, z as i32),
                    &mut vertices,
                    &mut indices,
                    &mut transparent_vertices,
                    &mut transparent_indices,
                ) {
                    continue;
                } else if block.id.is_crossed() {
                    let tex = get_crossed_texture(block);
                    let light = get_light_data(chunk, get_neighbor, x as i32, y as i32, z as i32);
                    let emissive = block.id.light_level();
                    emit_crossed_quad_light(
                        &mut vertices,
                        &mut indices,
                        wox + x as f32 + 0.5,
                        y as f32 + 0.5,
                        woz + z as f32 + 0.5,
                        tex,
                        light,
                        emissive,
                        chunk,
                        get_neighbor,
                        x as i32,
                        y as i32,
                        z as i32,
                    );
                } else if block.id.is_slab() {
                    let is_top = block.data == 1;
                    let tex = get_texture_index(block, BlockFace::Top);
                    let wx = wox + x as f32;
                    let wz = woz + z as f32;
                    let (y0, y1) = if is_top {
                        (y as f32 + 0.5, y as f32 + 1.0)
                    } else {
                        (y as f32, y as f32 + 0.5)
                    };

                    let slab_emissive = block.id.light_level();
                    let above = get_block_fn(x as i32, y as i32 + 1, z as i32);
                    if above.is_air() || above.id.is_transparent() {
                        emit_quad_light(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Top,
                            wx,
                            y1,
                            wz,
                            1.0,
                            1.0,
                            tex,
                            0,
                            2,
                        );
                        apply_vertex_lighting(
                            &mut vertices,
                            chunk,
                            get_neighbor,
                            BlockFace::Top,
                            0,
                            2,
                            slab_emissive,
                        );
                    }
                    let below = get_block_fn(x as i32, y as i32 - 1, z as i32);
                    if below.is_air() || below.id.is_transparent() {
                        emit_quad_light(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Bottom,
                            wx,
                            y0,
                            wz,
                            1.0,
                            1.0,
                            tex,
                            0,
                            2,
                        );
                        apply_vertex_lighting(
                            &mut vertices,
                            chunk,
                            get_neighbor,
                            BlockFace::Bottom,
                            0,
                            2,
                            slab_emissive,
                        );
                    }
                    let side_checks: &[(i32, i32, BlockFace, i32, i32)] = &[
                        (1, 0, BlockFace::Right, 2, 1),
                        (-1, 0, BlockFace::Left, 2, 1),
                        (0, 1, BlockFace::Front, 0, 1),
                        (0, -1, BlockFace::Back, 0, 1),
                    ];
                    for &(dx, dz, sface, u_ax, v_ax) in side_checks {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        let neighbor_block = get_block_fn(nx, y as i32, nz);
                        let neighbor_air =
                            neighbor_block.is_air() || neighbor_block.id.is_transparent();
                        if neighbor_air {
                            let side_tex = get_texture_index(block, sface);
                            let (sx, sy, sz) = match sface {
                                BlockFace::Right => (wx + 1.0, y0, wz),
                                BlockFace::Left => (wx, y0, wz),
                                BlockFace::Front => (wx, y0, wz + 1.0),
                                BlockFace::Back => (wx, y0, wz),
                                _ => (wx, y0, wz),
                            };
                            emit_quad_light(
                                &mut vertices,
                                &mut indices,
                                sface,
                                sx,
                                sy,
                                sz,
                                1.0,
                                0.5,
                                side_tex,
                                u_ax as usize,
                                v_ax as usize,
                            );
                            apply_vertex_lighting(
                                &mut vertices,
                                chunk,
                                get_neighbor,
                                sface,
                                u_ax as usize,
                                v_ax as usize,
                                slab_emissive,
                            );
                        }
                    }
                } else if block.id.is_stair() {
                let facing = block.data & 0x03;
                let is_top = (block.data & 0x04) != 0;
                let tex = get_texture_index(block, BlockFace::Top);
                let wx = wox + x as f32;
                let wz = woz + z as f32;
                let by = y as f32;

                // Determine the "back" and "front" axes based on facing
                // facing: 0=South(+Z), 1=West(-X), 2=North(-Z), 3=East(+X)
                // back_fx, back_fz = direction of the full-height back portion
                // step_fx, step_fz = direction of the step front portion
                let (back_fx, back_fz, step_fx, step_fz) = match facing {
                    0 => (0, -1, 0, 1), // South: back=North(-Z), step=South(+Z)
                    1 => (-1, 0, 1, 0), // West: back=East(+X), step=West(-X)
                    2 => (0, 1, 0, -1), // North: back=South(+Z), step=North(-Z)
                    _ => (1, 0, -1, 0), // East: back=West(-X), step=East(+X)
                };

                // Stair structure:
                // Bottom half [y, y+0.5]: filled (full horizontal area for bottom stair,
                //                          only back half for top stair)
                // Top half [y+0.5, y+1]: back half only for bottom stair,
                //                       filled for top stair

                // Helper to check if a neighboring block is air/transparent
                let is_open = |nx: i32, ny: i32, nz: i32| -> bool {
                    let b = get_block_fn(nx, ny, nz);
                    b.is_air() || b.id.is_transparent()
                };

                // Helper to emit a face (u_axis, v_axis based on face)
                let emit = |verts: &mut Vec<MeshVertex>,
                            inds: &mut Vec<u32>,
                            face: BlockFace,
                            fx: f32,
                            fy: f32,
                            fz: f32,
                            w: f32,
                            h: f32,
                            tex: u32| {
                    let (u_ax, v_ax) = match face {
                        BlockFace::Top | BlockFace::Bottom => (0usize, 2usize),
                        BlockFace::Left | BlockFace::Right => (2usize, 1usize),
                        BlockFace::Front | BlockFace::Back => (0usize, 1usize),
                    };
                    emit_quad_light(verts, inds, face, fx, fy, fz, w, h, tex, u_ax, v_ax);
                    apply_vertex_lighting(
                        verts,
                        chunk,
                        get_neighbor,
                        face,
                        u_ax,
                        v_ax,
                        block.id.light_level(),
                    );
                };

                if !is_top {
                    // === BOTTOM STAIR (normal orientation) ===
                    // Bottom half [y, y+0.5]: full area filled
                    // Top half [y+0.5, y+1]: only back half filled

                    // Bottom face: full (check block below)
                    if y == 0 || is_open(x as i32, y as i32 - 1, z as i32) {
                        let bot_tex = get_texture_index(block, BlockFace::Bottom);
                        emit(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Bottom,
                            wx,
                            by,
                            wz,
                            1.0,
                            1.0,
                            bot_tex,
                        );
                    }

                    // Top face of back (full height) portion: at y+1, back half
                    if y + 1 >= CHUNK_HEIGHT || is_open(x as i32, y as i32 + 1, z as i32) {
                        let (top_x, top_z, top_w, top_h) = match facing {
                            0 => (wx, wz, 1.0, 0.5),       // South: z..z+0.5
                            2 => (wx, wz + 0.5, 1.0, 0.5), // North: z+0.5..z+1
                            1 => (wx, wz, 0.5, 1.0),       // West: x..x+0.5
                            _ => (wx + 0.5, wz, 0.5, 1.0), // East: x+0.5..x+1
                        };
                        emit(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Top,
                            top_x,
                            by + 1.0,
                            top_z,
                            top_w,
                            top_h,
                            tex,
                        );
                    }

                    // Step top: at y+0.5, front half
                    let (step_x, step_z, step_w, step_h) = match facing {
                        0 => (wx, wz + 0.5, 1.0, 0.5), // South: z+0.5..z+1
                        2 => (wx, wz, 1.0, 0.5),       // North: z..z+0.5
                        1 => (wx + 0.5, wz, 0.5, 1.0), // West: x+0.5..x+1
                        _ => (wx, wz, 0.5, 1.0),       // East: x..x+0.5
                    };
                    // Only emit step top if the block above at the front half is open
                    let above = get_block_fn(x as i32 + step_fx, y as i32 + 1, z as i32 + step_fz);
                    if above.is_air() || above.id.is_transparent() || above.id.is_stair() {
                        let step_tex = get_texture_index(block, BlockFace::Top);
                        emit(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Top,
                            step_x,
                            by + 0.5,
                            step_z,
                            step_w,
                            step_h,
                            step_tex,
                        );
                    }

                    // Back face: full height, at the back edge (opposite the facing direction)
                    let (back_face, back_ox, back_oz) = match facing {
                        0 => (BlockFace::Back, wx, wz),        // South: back at z (north)
                        2 => (BlockFace::Front, wx, wz + 1.0), // North: back at z+1 (south)
                        1 => (BlockFace::Right, wx + 1.0, wz), // West: back at x+1 (east)
                        _ => (BlockFace::Left, wx, wz),        // East: back at x (west)
                    };
                    let back_side = x as i32 + back_fx;
                    let back_sz = z as i32 + back_fz;
                    if is_open(back_side, y as i32, back_sz) {
                        let back_tex = match back_face {
                            BlockFace::Back | BlockFace::Front => {
                                get_texture_index(block, BlockFace::Front)
                            }
                            BlockFace::Left | BlockFace::Right => {
                                get_texture_index(block, BlockFace::Left)
                            }
                            _ => tex,
                        };
                        emit(
                            &mut vertices,
                            &mut indices,
                            back_face,
                            back_ox,
                            by,
                            back_oz,
                            1.0,
                            1.0,
                            back_tex,
                        );
                    }

                    // Front (step) face: half height, at the front edge
                    let (front_face, front_ox, front_oz) = match facing {
                        0 => (BlockFace::Front, wx, wz + 1.0), // South: front at z+1
                        2 => (BlockFace::Back, wx, wz),        // North: front at z
                        1 => (BlockFace::Left, wx, wz),        // West: front at x
                        _ => (BlockFace::Right, wx + 1.0, wz), // East: front at x+1
                    };
                    let front_side = x as i32 + step_fx;
                    let front_sz = z as i32 + step_fz;
                    if is_open(front_side, y as i32, front_sz) {
                        let front_tex = match front_face {
                            BlockFace::Front | BlockFace::Back => {
                                get_texture_index(block, BlockFace::Front)
                            }
                            BlockFace::Left | BlockFace::Right => {
                                get_texture_index(block, BlockFace::Left)
                            }
                            _ => tex,
                        };
                        emit(
                            &mut vertices,
                            &mut indices,
                            front_face,
                            front_ox,
                            by,
                            front_oz,
                            1.0,
                            0.5,
                            front_tex,
                        );
                    }

                    // Side faces lie on X for north/south stairs and on Z for
                    // east/west stairs. Their neighbor checks must use the same axis.
                    let side_pairs: &[(i32, i32, BlockFace)] = match facing {
                        0 | 2 => &[(-1, 0, BlockFace::Left), (1, 0, BlockFace::Right)],
                        _ => &[(0, -1, BlockFace::Back), (0, 1, BlockFace::Front)],
                    };

                    for &(sdx, sdz, side_face) in side_pairs {
                        let sx = x as i32 + sdx;
                        let sz = z as i32 + sdz;
                        if is_open(sx, y as i32, sz) {
                            let side_tex = get_texture_index(block, side_face);
                            // Bottom half of side: full depth, half height
                            let (sx_pos, sy_pos, sz_pos) = match side_face {
                                BlockFace::Left => (wx, by, wz),
                                BlockFace::Right => (wx + 1.0, by, wz),
                                BlockFace::Back => (wx, by, wz),
                                BlockFace::Front => (wx, by, wz + 1.0),
                                _ => unreachable!(),
                            };
                            emit(
                                &mut vertices,
                                &mut indices,
                                side_face,
                                sx_pos,
                                sy_pos,
                                sz_pos,
                                1.0,
                                0.5,
                                side_tex,
                            );
                            // Top half of side: only back half depth
                            let (back_depth_w, back_depth_h, back_ox_s, back_oz_s) = match facing {
                                0 => (0.5, 0.5, 0.0, 0.0), // South: back half in z [z..z+0.5]
                                2 => (0.5, 0.5, 0.0, 0.5), // North: back half in z [z+0.5..z+1]
                                1 => (0.5, 0.5, 0.0, 0.0), // West: back half in x [x..x+0.5]
                                _ => (0.5, 0.5, 0.5, 0.0), // East: back half in x [x+0.5..x+1]
                            };
                            let (sx2, sy2, sz2) = match side_face {
                                BlockFace::Left => (wx + back_ox_s, by + 0.5, wz + back_oz_s),
                                BlockFace::Right => {
                                    (wx + 1.0 + back_ox_s, by + 0.5, wz + back_oz_s)
                                }
                                BlockFace::Back => (wx + back_ox_s, by + 0.5, wz),
                                BlockFace::Front => (wx + back_ox_s, by + 0.5, wz + 1.0),
                                _ => unreachable!(),
                            };
                            emit(
                                &mut vertices,
                                &mut indices,
                                side_face,
                                sx2,
                                sy2,
                                sz2,
                                back_depth_w,
                                back_depth_h,
                                side_tex,
                            );
                        }
                    }
                } else {
                    // === TOP STAIR (upside down) ===
                    // Bottom half [y, y+0.5]: only back half filled
                    // Top half [y+0.5, y+1]: full area filled

                    // Top face: full (check block above)
                    if y + 1 >= CHUNK_HEIGHT || is_open(x as i32, y as i32 + 1, z as i32) {
                        let top_tex = get_texture_index(block, BlockFace::Top);
                        emit(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Top,
                            wx,
                            by + 1.0,
                            wz,
                            1.0,
                            1.0,
                            top_tex,
                        );
                    }

                    // Bottom face: only back half (at y)
                    let (bot_x, bot_z, bot_w, bot_h) = match facing {
                        0 => (wx, wz, 1.0, 0.5),
                        2 => (wx, wz + 0.5, 1.0, 0.5),
                        1 => (wx, wz, 0.5, 1.0),
                        _ => (wx + 0.5, wz, 0.5, 1.0),
                    };
                    if y == 0 || is_open(x as i32, y as i32 - 1, z as i32) {
                        let bot_tex = get_texture_index(block, BlockFace::Bottom);
                        emit(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Bottom,
                            bot_x,
                            by,
                            bot_z,
                            bot_w,
                            bot_h,
                            bot_tex,
                        );
                    }

                    // Step ceiling (bottom of the upper full slab visible from below): at y+0.5, front half
                    let (ceil_x, ceil_z, ceil_w, ceil_h) = match facing {
                        0 => (wx, wz + 0.5, 1.0, 0.5),
                        2 => (wx, wz, 1.0, 0.5),
                        1 => (wx + 0.5, wz, 0.5, 1.0),
                        _ => (wx, wz, 0.5, 1.0),
                    };
                    if y == 0 || is_open(x as i32, y as i32 - 1, z as i32) {
                        let ceil_tex = get_texture_index(block, BlockFace::Bottom);
                        emit(
                            &mut vertices,
                            &mut indices,
                            BlockFace::Bottom,
                            ceil_x,
                            by + 0.5,
                            ceil_z,
                            ceil_w,
                            ceil_h,
                            ceil_tex,
                        );
                    }

                    // Back face: full height
                    let (back_face, back_ox, back_oz) = match facing {
                        0 => (BlockFace::Back, wx, wz),
                        2 => (BlockFace::Front, wx, wz + 1.0),
                        1 => (BlockFace::Right, wx + 1.0, wz),
                        _ => (BlockFace::Left, wx, wz),
                    };
                    let back_side = x as i32 + back_fx;
                    let back_sz = z as i32 + back_fz;
                    if is_open(back_side, y as i32, back_sz) {
                        let back_tex = match back_face {
                            BlockFace::Back | BlockFace::Front => {
                                get_texture_index(block, BlockFace::Front)
                            }
                            BlockFace::Left | BlockFace::Right => {
                                get_texture_index(block, BlockFace::Left)
                            }
                            _ => tex,
                        };
                        emit(
                            &mut vertices,
                            &mut indices,
                            back_face,
                            back_ox,
                            by,
                            back_oz,
                            1.0,
                            1.0,
                            back_tex,
                        );
                    }

                    // Front (step) face: top half only at the front edge
                    let (front_face, front_ox, front_oz) = match facing {
                        0 => (BlockFace::Front, wx, wz + 1.0),
                        2 => (BlockFace::Back, wx, wz),
                        1 => (BlockFace::Left, wx, wz),
                        _ => (BlockFace::Right, wx + 1.0, wz),
                    };
                    let front_side = x as i32 + step_fx;
                    let front_sz = z as i32 + step_fz;
                    if is_open(front_side, y as i32, front_sz) {
                        let front_tex = match front_face {
                            BlockFace::Front | BlockFace::Back => {
                                get_texture_index(block, BlockFace::Front)
                            }
                            BlockFace::Left | BlockFace::Right => {
                                get_texture_index(block, BlockFace::Left)
                            }
                            _ => tex,
                        };
                        emit(
                            &mut vertices,
                            &mut indices,
                            front_face,
                            front_ox,
                            by + 0.5,
                            front_oz,
                            1.0,
                            0.5,
                            front_tex,
                        );
                    }

                    // Side faces for top stair
                    // Bottom half of side: only back half depth
                    // Top half of side: full depth
                    let side_pairs: &[(i32, i32, BlockFace)] = match facing {
                        0 | 2 => &[(-1, 0, BlockFace::Left), (1, 0, BlockFace::Right)],
                        _ => &[(0, -1, BlockFace::Back), (0, 1, BlockFace::Front)],
                    };
                    for &(sdx, sdz, side_face) in side_pairs {
                        let sx = x as i32 + sdx;
                        let sz = z as i32 + sdz;
                        if is_open(sx, y as i32, sz) {
                            let side_tex = get_texture_index(block, side_face);
                            // Bottom half: only back depth
                            let (back_ox_s, back_oz_s) = match facing {
                                0 => (0.0, 0.0),
                                2 => (0.0, 0.5),
                                1 => (0.0, 0.0),
                                _ => (0.5, 0.0),
                            };
                            // For left/right faces at bottom half: extends from back edge to center
                            // Top half: full depth

                            // Actually for emit_quad with Left face: origin at (x, y, z), extends +z by w, +y by h
                            // Bottom half, back portion: at the back edge
                            let (bsx, bsy, bsz) = match side_face {
                                BlockFace::Left => (wx + back_ox_s, by, wz + back_oz_s),
                                BlockFace::Right => (wx + 1.0 + back_ox_s, by, wz + back_oz_s),
                                BlockFace::Back => (wx + back_ox_s, by, wz),
                                BlockFace::Front => (wx + back_ox_s, by, wz + 1.0),
                                _ => unreachable!(),
                            };
                            emit(
                                &mut vertices,
                                &mut indices,
                                side_face,
                                bsx,
                                bsy,
                                bsz,
                                1.0,
                                0.5,
                                side_tex,
                            );

                            // Top half: full depth
                            let (tsx, tsy, tsz) = match side_face {
                                BlockFace::Left => (wx, by + 0.5, wz),
                                BlockFace::Right => (wx + 1.0, by + 0.5, wz),
                                BlockFace::Back => (wx, by + 0.5, wz),
                                BlockFace::Front => (wx, by + 0.5, wz + 1.0),
                                _ => unreachable!(),
                            };
                            emit(
                                &mut vertices,
                                &mut indices,
                                side_face,
                                tsx,
                                tsy,
                                tsz,
                                1.0,
                                0.5,
                                side_tex,
                            );
                        }
                    }
                }
                }
            }
        }
    }

    ChunkMesh {
        vertices,
        indices,
        transparent_vertices,
        transparent_indices,
    }
}

fn get_crossed_texture(block: Block) -> u32 {
    let tex = CROSSED_TEX
        .get()
        .and_then(|m| m.get(&block.id))
        .copied()
        .unwrap_or(u32::MAX);
    if tex == u32::MAX {
        log::warn!("Missing crossed texture for block {:?}", block.id);
    }
    tex | material_flags(block.id)
}

fn emit_crossed_quad_light<'a>(
    verts: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    x: f32,
    y: f32,
    z: f32,
    tex_index: u32,
    _light_data: u32,
    emissive: u8,
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    bx: i32,
    by: i32,
    bz: i32,
) {
    let h = 0.5;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let light = get_light_data(chunk, get_neighbor, bx, by, bz);
    let sky = (light >> 4) & 0xF;
    let block = light & 0xF;
    let packed = ((emissive as u32) << 12) | (3 << 8) | (sky << 4) | block;
    let planes = [
        (
            [
                [x - h, y - h, z - h],
                [x + h, y - h, z + h],
                [x + h, y + h, z + h],
                [x - h, y + h, z - h],
            ],
            [-0.707, 0.0, 0.707],
        ),
        (
            [
                [x - h, y - h, z + h],
                [x + h, y - h, z - h],
                [x + h, y + h, z - h],
                [x - h, y + h, z + h],
            ],
            [0.707, 0.0, 0.707],
        ),
    ];

    for (positions, normal) in planes {
        let base = verts.len() as u32;
        for i in 0..4 {
            verts.push(MeshVertex {
                pos: positions[i],
                uv: uvs[i],
                normal,
                tex_index,
                light_data: packed,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        // Cutout plants use double-sided intersecting planes in vanilla.
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

pub fn build_item_cube_mesh(items: &[(f32, f32, f32, BlockId)]) -> ChunkMesh {
    let mut vertices = Vec::with_capacity(items.len() * 24);
    let mut indices = Vec::with_capacity(items.len() * 36);

    for &(x, y, z, block_id) in items {
        let h = 0.2;
        let block = Block::new(block_id);

        for &face in &FACES {
            let tex = get_texture_index(block, face);
            let (ox, oy, oz, u_axis, v_axis) = match face {
                BlockFace::Top => (x - h, y + h, z - h, 0, 2),
                BlockFace::Bottom => (x - h, y - h, z - h, 0, 2),
                BlockFace::Left => (x - h, y - h, z - h, 2, 1),
                BlockFace::Right => (x + h, y - h, z - h, 2, 1),
                BlockFace::Front => (x - h, y - h, z + h, 0, 1),
                BlockFace::Back => (x - h, y - h, z - h, 0, 1),
            };
            let vert_start = vertices.len();
            emit_quad_light(
                &mut vertices,
                &mut indices,
                face,
                ox,
                oy,
                oz,
                2.0 * h,
                2.0 * h,
                tex,
                u_axis,
                v_axis,
            );
            let light_data = (15u32 << 8) | (15u32 << 4) | 15;
            for v in &mut vertices[vert_start..] {
                v.light_data = light_data;
            }
        }
    }

    ChunkMesh {
        vertices,
        indices,
        transparent_vertices: Vec::new(),
        transparent_indices: Vec::new(),
    }
}

/// Render state for a remote player. Positions are already interpolated by the
/// client; keeping this type in the mesh module keeps animation independent of
/// the network transport and renderer.
#[derive(Clone, Copy, Debug)]
pub struct PlayerMeshInstance {
    pub position: [f32; 3],
    pub yaw: f32,
    pub walk_phase: f32,
    pub walk_amount: f32,
}

fn player_transform(
    point: [f32; 3],
    pivot: [f32; 3],
    limb_pitch: f32,
    yaw: f32,
    origin: [f32; 3],
) -> [f32; 3] {
    let dy = point[1] - pivot[1];
    let dz = point[2] - pivot[2];
    let pitch_cos = limb_pitch.cos();
    let pitch_sin = limb_pitch.sin();
    let pitched = [
        point[0],
        pivot[1] + dy * pitch_cos - dz * pitch_sin,
        pivot[2] + dy * pitch_sin + dz * pitch_cos,
    ];
    let local_x = pitched[0];
    let local_z = pitched[2];
    [
        origin[0] + local_x * yaw.cos() + local_z * yaw.sin(),
        origin[1] + pitched[1],
        origin[2] - local_x * yaw.sin() + local_z * yaw.cos(),
    ]
}

fn emit_player_box(
    vertices: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    origin: [f32; 3],
    yaw: f32,
    min: [f32; 3],
    max: [f32; 3],
    pivot: [f32; 3],
    limb_pitch: f32,
    block_id: BlockId,
) {
    let tex = get_texture_index(Block::new(block_id), BlockFace::Front);
    let light_data = (15u32 << 8) | (15u32 << 4) | 15;
    let faces = [
        (BlockFace::Top, [
            [min[0], max[1], min[2]], [max[0], max[1], min[2]],
            [max[0], max[1], max[2]], [min[0], max[1], max[2]],
        ], true),
        (BlockFace::Bottom, [
            [min[0], min[1], min[2]], [max[0], min[1], min[2]],
            [max[0], min[1], max[2]], [min[0], min[1], max[2]],
        ], false),
        (BlockFace::Left, [
            [min[0], min[1], min[2]], [min[0], min[1], max[2]],
            [min[0], max[1], max[2]], [min[0], max[1], min[2]],
        ], false),
        (BlockFace::Right, [
            [max[0], min[1], min[2]], [max[0], min[1], max[2]],
            [max[0], max[1], max[2]], [max[0], max[1], min[2]],
        ], true),
        (BlockFace::Front, [
            [min[0], min[1], max[2]], [max[0], min[1], max[2]],
            [max[0], max[1], max[2]], [min[0], max[1], max[2]],
        ], false),
        (BlockFace::Back, [
            [min[0], min[1], min[2]], [max[0], min[1], min[2]],
            [max[0], max[1], min[2]], [min[0], max[1], min[2]],
        ], true),
    ];

    for (face, corners, reversed) in faces {
        let normal = get_face_normal(face);
        let base = vertices.len() as u32;
        let transformed = corners.map(|corner| player_transform(corner, pivot, limb_pitch, yaw, origin));
        let transformed_normal = player_transform(
            [normal[0], normal[1], normal[2]],
            [0.0, 0.0, 0.0],
            limb_pitch,
            yaw,
            [0.0, 0.0, 0.0],
        );
        let normal = [transformed_normal[0], transformed_normal[1], transformed_normal[2]];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for (pos, uv) in transformed.into_iter().zip(uvs) {
            vertices.push(MeshVertex { pos, uv, normal, tex_index: tex, light_data });
        }
        if reversed {
            indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        } else {
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

/// Build the simple multiplayer avatar used until player skin assets exist.
/// Each avatar has independently animated limbs, so movement remains readable
/// even when a server only sends transforms and velocity.
pub fn build_player_mesh(players: &[PlayerMeshInstance]) -> ChunkMesh {
    let mut vertices = Vec::with_capacity(players.len() * 6 * 6 * 4);
    let mut indices = Vec::with_capacity(players.len() * 6 * 6 * 6);

    for player in players {
        let swing = player.walk_phase.sin() * 0.65 * player.walk_amount;
        let arm_swing = -swing;
        let head = BlockId::BrownWool;
        let shirt = BlockId::BlueWool;
        let sleeves = BlockId::LightBlueWool;
        let pants = BlockId::BlackWool;

        emit_player_box(&mut vertices, &mut indices, player.position, player.yaw,
            [-0.25, 1.50, -0.25], [0.25, 2.00, 0.25], [0.0, 1.50, 0.0], 0.0, head);
        emit_player_box(&mut vertices, &mut indices, player.position, player.yaw,
            [-0.25, 0.75, -0.14], [0.25, 1.50, 0.14], [0.0, 0.75, 0.0], 0.0, shirt);
        emit_player_box(&mut vertices, &mut indices, player.position, player.yaw,
            [-0.40, 0.75, -0.14], [-0.25, 1.50, 0.14], [-0.325, 1.50, 0.0], arm_swing, sleeves);
        emit_player_box(&mut vertices, &mut indices, player.position, player.yaw,
            [0.25, 0.75, -0.14], [0.40, 1.50, 0.14], [0.325, 1.50, 0.0], swing, sleeves);
        emit_player_box(&mut vertices, &mut indices, player.position, player.yaw,
            [-0.24, 0.0, -0.12], [-0.04, 0.75, 0.12], [-0.14, 0.75, 0.0], swing, pants);
        emit_player_box(&mut vertices, &mut indices, player.position, player.yaw,
            [0.04, 0.0, -0.12], [0.24, 0.75, 0.12], [0.14, 0.75, 0.0], arm_swing, pants);
    }

    ChunkMesh { vertices, indices, transparent_vertices: Vec::new(), transparent_indices: Vec::new() }
}
