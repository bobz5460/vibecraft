use std::collections::HashMap;
use std::sync::OnceLock;
use crate::world::block::{Block, BlockFace, BlockId, FACES};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};

type FaceTexMap = HashMap<(BlockId, BlockFace), u32>;
type CrossedTexMap = HashMap<BlockId, u32>;
static FACE_TEX: OnceLock<FaceTexMap> = OnceLock::new();
static CROSSED_TEX: OnceLock<CrossedTexMap> = OnceLock::new();

pub fn set_texture_lookups(face: FaceTexMap, crossed: CrossedTexMap) {
    let _ = FACE_TEX.set(face);
    let _ = CROSSED_TEX.set(crossed);
}

#[derive(Clone, Debug)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub tex_index: u32,
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
    FACE_TEX.get()
        .and_then(|m| m.get(&(block.id, face)))
        .copied()
        .unwrap_or(0)
}

fn is_transparent(block: Block) -> bool {
    block.id.is_transparent()
}

fn can_see_face(block: Block, neighbor: Block) -> bool {
    neighbor.is_air() || neighbor.id.is_transparent() || block.id.is_transparent()
}

fn emit_quad(
    verts: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    face: BlockFace,
    x: f32, y: f32, z: f32,
    w: f32, h: f32,
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
        });
    }

    if reversed {
        indices.extend_from_slice(&[
            base, base + 2, base + 1,
            base, base + 3, base + 2,
        ]);
    } else {
        indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }
}

pub fn build_chunk_mesh<'a>(
    chunk: &'a Chunk,
    get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
) -> ChunkMesh {
    let mut vertices = Vec::with_capacity(16384);
    let mut indices = Vec::with_capacity(32768);
    let mut transparent_vertices = Vec::with_capacity(4096);
    let mut transparent_indices = Vec::with_capacity(8192);

    let get_block_fn = |x: i32, y: i32, z: i32| -> Block {
        if x >= 0 && x < CHUNK_SIZE as i32 && y >= 0 && y < CHUNK_HEIGHT as i32 && z >= 0 && z < CHUNK_SIZE as i32 {
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

    for &face in &FACES {
        let (_u_axis, _v_axis, _w_axis, w_start, w_end, w_step, u_size, v_size) = match face {
            BlockFace::Top | BlockFace::Bottom => {
                (0, 2, 1, 0, CHUNK_HEIGHT as i32, 1, CHUNK_SIZE as i32, CHUNK_SIZE as i32)
            }
            BlockFace::Left | BlockFace::Right => {
                (2, 1, 0, 0, CHUNK_SIZE as i32, 1, CHUNK_SIZE as i32, CHUNK_HEIGHT as i32)
            }
            BlockFace::Front | BlockFace::Back => {
                (0, 1, 2, 0, CHUNK_SIZE as i32, 1, CHUNK_SIZE as i32, CHUNK_HEIGHT as i32)
            }
        };

        for w in (w_start..w_end).step_by(w_step as usize) {
            let mut mask: Vec<bool> = vec![false; (u_size * v_size) as usize];

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
                    if block.is_air() || block.id.is_crossed() {
                        continue;
                    }

                    let neighbor = get_block_fn(nx, ny, nz);
                    if can_see_face(block, neighbor) {
                        mask[(v * u_size + u) as usize] = true;
                    }
                }
            }

            if !mask.iter().any(|&m| m) {
                continue;
            }

            let mut processed: Vec<bool> = vec![false; (u_size * v_size) as usize];

            let same_block = |ux: i32, vy: i32, start_id: BlockId| -> bool {
                let (cbx, cby, cbz) = match face {
                    BlockFace::Top | BlockFace::Bottom => (ux, w, vy),
                    BlockFace::Left | BlockFace::Right => (w, vy, ux),
                    BlockFace::Front | BlockFace::Back => (ux, vy, w),
                };
                get_block_fn(cbx, cby, cbz).id == start_id
            };

            for y in 0..v_size {
                for x in 0..u_size {
                    let idx = (y * u_size + x) as usize;
                    if !mask[idx] || processed[idx] {
                        continue;
                    }

                    let (sbx, sby, sbz) = match face {
                        BlockFace::Top | BlockFace::Bottom => (x as i32, w, y as i32),
                        BlockFace::Left | BlockFace::Right => (w, y as i32, x as i32),
                        BlockFace::Front | BlockFace::Back => (x as i32, y as i32, w),
                    };
                    let start_id = get_block_fn(sbx, sby, sbz).id;

                    let mut rect_w = 1;
                    while x + rect_w < u_size
                        && mask[(y * u_size + x + rect_w) as usize]
                        && !processed[(y * u_size + x + rect_w) as usize]
                        && same_block(x as i32 + rect_w, y as i32, start_id)
                    {
                        rect_w += 1;
                    }

                    let mut rect_h = 1;
                    'outer: while y + rect_h < v_size {
                        for cx in x..x + rect_w {
                            let ci = ((y + rect_h) * u_size + cx) as usize;
                            if !mask[ci] || processed[ci] || !same_block(cx as i32, y as i32 + rect_h, start_id) {
                                break 'outer;
                            }
                        }
                        rect_h += 1;
                    }

                    for dy in 0..rect_h {
                        for dx in 0..rect_w {
                            let pi = ((y + dy) * u_size + (x + dx)) as usize;
                            processed[pi] = true;
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

                    let is_transp = is_transparent(block);

                    let verts = if is_transp { &mut transparent_vertices } else { &mut vertices };
                    let inds = if is_transp { &mut transparent_indices } else { &mut indices };

                    let (fx, fy, fz) = match face {
                        BlockFace::Top    => (bx as f32, by as f32 + 1.0, bz as f32),
                        BlockFace::Bottom => (bx as f32, by as f32, bz as f32),
                        BlockFace::Left   => (bx as f32, by as f32, bz as f32),
                        BlockFace::Right  => (bx as f32 + 1.0, by as f32, bz as f32),
                        BlockFace::Front  => (bx as f32, by as f32, bz as f32 + 1.0),
                        BlockFace::Back   => (bx as f32, by as f32, bz as f32),
                    };
                    let wox = chunk.cx as f32 * CHUNK_SIZE as f32;
                    let woz = chunk.cz as f32 * CHUNK_SIZE as f32;
                    let (u_axis, v_axis) = match face {
                        BlockFace::Top | BlockFace::Bottom => (0usize, 2usize),
                        BlockFace::Left | BlockFace::Right => (2usize, 1usize),
                        BlockFace::Front | BlockFace::Back => (0usize, 1usize),
                    };
                    emit_quad(
                        verts, inds, face,
                        wox + fx, fy, woz + fz,
                        rect_w as f32, rect_h as f32,
                        face_tex,
                        u_axis, v_axis,
                    );
                }
            }
        }
    }

    let wox = chunk.cx as f32 * CHUNK_SIZE as f32;
    let woz = chunk.cz as f32 * CHUNK_SIZE as f32;
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                let block = chunk.get_block(x, y, z);
                if block.id.is_crossed() {
                    let tex = get_crossed_texture(block);
                    emit_crossed_quad(
                        &mut transparent_vertices, &mut transparent_indices,
                        wox + x as f32, y as f32 + 0.5, woz + z as f32,
                        tex,
                    );
                }
            }
        }
    }

    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                let block = chunk.get_block(x, y, z);
                if !block.id.is_slab() { continue; }
                let is_top = block.data == 1;
                let tex = get_texture_index(block, BlockFace::Top);
                let wx = wox + x as f32;
                let wz = woz + z as f32;
                let (y0, y1) = if is_top { (y as f32 + 0.5, y as f32 + 1.0) } else { (y as f32, y as f32 + 0.5) };

                if y + 1 >= CHUNK_HEIGHT || chunk.get_block(x, y + 1, z).is_air() || chunk.get_block(x, y + 1, z).id.is_transparent() {
                    emit_quad(&mut vertices, &mut indices, BlockFace::Top, wx, y1, wz, 1.0, 1.0, tex, 0, 2);
                }
                if y == 0 || chunk.get_block(x, y - 1, z).is_air() || chunk.get_block(x, y - 1, z).id.is_transparent() {
                    emit_quad(&mut vertices, &mut indices, BlockFace::Bottom, wx, y0, wz, 1.0, 1.0, tex, 0, 2);
                }
                let side_checks: &[(i32,i32,BlockFace,i32,i32)] = &[(1,0,BlockFace::Right,2,1), (-1,0,BlockFace::Left,2,1), (0,1,BlockFace::Front,0,1), (0,-1,BlockFace::Back,0,1)];
                for &(dx, dz, sface, u_ax, v_ax) in side_checks {
                    let nx = x as i32 + dx;
                    let nz = z as i32 + dz;
                    let neighbor_air = if nx < 0 || nx >= CHUNK_SIZE as i32 || nz < 0 || nz >= CHUNK_SIZE as i32 {
                        true
                    } else {
                        chunk.get_block(nx as usize, y, nz as usize).is_air()
                            || chunk.get_block(nx as usize, y, nz as usize).id.is_transparent()
                    };
                    if neighbor_air {
                        let side_tex = get_texture_index(block, sface);
                        let (sx, sy, sz) = match sface {
                            BlockFace::Right => (wx + 1.0, y0, wz),
                            BlockFace::Left => (wx, y0, wz),
                            BlockFace::Front => (wx, y0, wz + 1.0),
                            BlockFace::Back => (wx, y0, wz),
                            _ => (wx, y0, wz),
                        };
                        emit_quad(&mut vertices, &mut indices, sface, sx, sy, sz, 1.0, 0.5, side_tex, u_ax as usize, v_ax as usize);
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
    CROSSED_TEX.get()
        .and_then(|m| m.get(&block.id))
        .copied()
        .unwrap_or(0)
}

fn emit_crossed_quad(
    verts: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    x: f32, y: f32, z: f32,
    tex_index: u32,
) {
    let h = 0.5;
    let normal = [0.0, 1.0, 0.0];
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    for plane_verts in [
        [[x - h, y - h, z - h], [x + h, y - h, z + h], [x + h, y + h, z + h], [x - h, y + h, z - h]],
        [[x + h, y - h, z - h], [x - h, y - h, z + h], [x - h, y + h, z + h], [x + h, y + h, z - h]],
    ] {
        let base = verts.len() as u32;
        for i in 0..4 {
            verts.push(MeshVertex {
                pos: plane_verts[i],
                uv: uvs[i],
                normal,
                tex_index,
            });
        }
        indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
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
                BlockFace::Right => (x - h, y - h, z - h, 2, 1),
                BlockFace::Front => (x - h, y - h, z + h, 0, 1),
                BlockFace::Back => (x - h, y - h, z - h, 0, 1),
            };
            emit_quad(
                &mut vertices, &mut indices, face,
                ox, oy, oz,
                2.0 * h, 2.0 * h,
                tex,
                u_axis, v_axis,
            );
        }
    }

    ChunkMesh {
        vertices,
        indices,
        transparent_vertices: Vec::new(),
        transparent_indices: Vec::new(),
    }
}
