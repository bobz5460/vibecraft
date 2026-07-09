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
    emit_quad_light(verts, indices, face, x, y, z, w, h, tex_index, u_axis, v_axis, 0);
}

fn emit_quad_light(
    verts: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    face: BlockFace,
    x: f32, y: f32, z: f32,
    w: f32, h: f32,
    tex_index: u32,
    u_axis: usize,
    v_axis: usize,
    light_data: u32,
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
            light_data,
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

/// Get packed light data at a world block position, reading from chunk or neighbor.
fn get_light_data<'a>(chunk: &'a Chunk, get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>, bx: i32, by: i32, bz: i32) -> u32 {
    if bx >= 0 && bx < CHUNK_SIZE as i32 && by >= 0 && by < CHUNK_HEIGHT as i32 && bz >= 0 && bz < CHUNK_SIZE as i32 {
        let (sky, block) = chunk.get_light_at(bx, by, bz);
        Chunk::pack_light(sky, block)
    } else {
        let cx = chunk.cx + bx.div_euclid(CHUNK_SIZE as i32);
        let cz = chunk.cz + bz.div_euclid(CHUNK_SIZE as i32);
        let lx = bx.rem_euclid(CHUNK_SIZE as i32);
        let lz = bz.rem_euclid(CHUNK_SIZE as i32);
        if by >= 0 && by < CHUNK_HEIGHT as i32 {
            match get_neighbor(cx, cz) {
                Some(nc) => {
                    let (sky, block) = nc.get_light_at(lx, by, lz);
                    Chunk::pack_light(sky, block)
                }
                None => Chunk::pack_light(15, 0),
            }
        } else {
            Chunk::pack_light(15, 0)
        }
    }
}

/// Get light at the neighbor position adjacent to a face.
fn get_face_center_light<'a>(chunk: &'a Chunk, get_neighbor: &impl Fn(i32, i32) -> Option<&'a Chunk>,
    face: BlockFace, bx: i32, by: i32, bz: i32, rect_w: i32, rect_h: i32) -> u32 {
    let half_w = rect_w as f32 / 2.0;
    let half_h = rect_h as f32 / 2.0;
    let (cx, cy, cz) = match face {
        BlockFace::Top => (bx as f32 + half_w, (by + 1) as f32, bz as f32 + half_h),
        BlockFace::Bottom => (bx as f32 + half_w, (by - 1) as f32, bz as f32 + half_h),
        BlockFace::Left => ((bx - 1) as f32, by as f32 + half_h, bz as f32 + half_w),
        BlockFace::Right => ((bx + 1) as f32, by as f32 + half_h, bz as f32 + half_w),
        BlockFace::Front => (bx as f32 + half_w, by as f32 + half_h, (bz + 1) as f32),
        BlockFace::Back => (bx as f32 + half_w, by as f32 + half_h, (bz - 1) as f32),
    };
    let lx = cx.floor() as i32;
    let ly = cy.floor() as i32;
    let lz = cz.floor() as i32;
    get_light_data(chunk, get_neighbor, lx, ly, lz)
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
                    if block.is_air() || block.id.is_crossed() || block.id.is_stair() {
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
                    let light = get_face_center_light(chunk, get_neighbor, face, bx, by, bz, rect_w, rect_h);
                    emit_quad_light(
                        verts, inds, face,
                        wox + fx, fy, woz + fz,
                        rect_w as f32, rect_h as f32,
                        face_tex,
                        u_axis, v_axis,
                        light,
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
                    let light = get_light_data(chunk, get_neighbor, x as i32, y as i32, z as i32);
                    emit_crossed_quad_light(
                        &mut transparent_vertices, &mut transparent_indices,
                        wox + x as f32, y as f32 + 0.5, woz + z as f32,
                        tex, light,
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

                let slab_top_light = get_light_data(chunk, get_neighbor, x as i32, y as i32 + 1, z as i32);
                let slab_bot_light = get_light_data(chunk, get_neighbor, x as i32, y as i32 - 1, z as i32);

                if y + 1 >= CHUNK_HEIGHT || chunk.get_block(x, y + 1, z).is_air() || chunk.get_block(x, y + 1, z).id.is_transparent() {
                    emit_quad_light(&mut vertices, &mut indices, BlockFace::Top, wx, y1, wz, 1.0, 1.0, tex, 0, 2, slab_top_light);
                }
                if y == 0 || chunk.get_block(x, y - 1, z).is_air() || chunk.get_block(x, y - 1, z).id.is_transparent() {
                    emit_quad_light(&mut vertices, &mut indices, BlockFace::Bottom, wx, y0, wz, 1.0, 1.0, tex, 0, 2, slab_bot_light);
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
                        let slab_side_light = get_light_data(chunk, get_neighbor, x as i32 + dx, y as i32, z as i32 + dz);
                        emit_quad_light(&mut vertices, &mut indices, sface, sx, sy, sz, 1.0, 0.5, side_tex, u_ax as usize, v_ax as usize, slab_side_light);
                    }
                }
            }
        }
    }

    // Stair post-processing pass
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                let block = chunk.get_block(x, y, z);
                if !block.id.is_stair() { continue; }

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
                    0 => (0, -1, 0, 1),  // South: back=North(-Z), step=South(+Z)
                    1 => (-1, 0, 1, 0),  // West: back=East(+X), step=West(-X)
                    2 => (0, 1, 0, -1),  // North: back=South(+Z), step=North(-Z)
                    _ => (1, 0, -1, 0),  // East: back=West(-X), step=East(+X)
                };

                // Stair structure:
                // Bottom half [y, y+0.5]: filled (full horizontal area for bottom stair,
                //                          only back half for top stair)
                // Top half [y+0.5, y+1]: back half only for bottom stair,
                //                       filled for top stair

                // Helper to check if a neighboring block is air/transparent
                let is_open = |nx: i32, ny: i32, nz: i32| -> bool {
                    if nx < 0 || nx >= CHUNK_SIZE as i32 || nz < 0 || nz >= CHUNK_SIZE as i32 { return true; }
                    if ny < 0 || ny >= CHUNK_HEIGHT as i32 { return true; }
                    let b = chunk.get_block(nx as usize, ny as usize, nz as usize);
                    b.is_air() || b.id.is_transparent()
                };

                // Helper to emit a face (u_axis, v_axis based on face)
                let emit = |verts: &mut Vec<MeshVertex>, inds: &mut Vec<u32>,
                           face: BlockFace, fx: f32, fy: f32, fz: f32, w: f32, h: f32, tex: u32| {
                    let (lx, ly, lz) = match face {
                        BlockFace::Top => (x as i32, y as i32 + 1, z as i32),
                        BlockFace::Bottom => (x as i32, y as i32 - 1, z as i32),
                        BlockFace::Left => (x as i32 - 1, y as i32, z as i32),
                        BlockFace::Right => (x as i32 + 1, y as i32, z as i32),
                        BlockFace::Front => (x as i32, y as i32, z as i32 + 1),
                        BlockFace::Back => (x as i32, y as i32, z as i32 - 1),
                    };
                    let light = get_light_data(chunk, get_neighbor, lx, ly, lz);
                    let (u_ax, v_ax) = match face {
                        BlockFace::Top | BlockFace::Bottom => (0usize, 2usize),
                        BlockFace::Left | BlockFace::Right => (2usize, 1usize),
                        BlockFace::Front | BlockFace::Back => (0usize, 1usize),
                    };
                    emit_quad_light(verts, inds, face, fx, fy, fz, w, h, tex, u_ax, v_ax, light);
                };

                if !is_top {
                    // === BOTTOM STAIR (normal orientation) ===
                    // Bottom half [y, y+0.5]: full area filled
                    // Top half [y+0.5, y+1]: only back half filled

                    // Bottom face: full (check block below)
                    if y == 0 || is_open(x as i32, y as i32 - 1, z as i32) {
                        let bot_tex = get_texture_index(block, BlockFace::Bottom);
                        emit(&mut vertices, &mut indices, BlockFace::Bottom, wx, by, wz, 1.0, 1.0, bot_tex);
                    }

                    // Top face of back (full height) portion: at y+1, back half
                    if y + 1 >= CHUNK_HEIGHT || is_open(x as i32, y as i32 + 1, z as i32) {
                        let (top_x, top_z, top_w, top_h) = match facing {
                            0 => (wx, wz, 1.0, 0.5),      // South: z..z+0.5
                            2 => (wx, wz + 0.5, 1.0, 0.5), // North: z+0.5..z+1
                            1 => (wx, wz, 0.5, 1.0),      // West: x..x+0.5
                            _ => (wx + 0.5, wz, 0.5, 1.0), // East: x+0.5..x+1
                        };
                        emit(&mut vertices, &mut indices, BlockFace::Top, top_x, by + 1.0, top_z, top_w, top_h, tex);
                    }

                    // Step top: at y+0.5, front half
                    let (step_x, step_z, step_w, step_h) = match facing {
                        0 => (wx, wz + 0.5, 1.0, 0.5),      // South: z+0.5..z+1
                        2 => (wx, wz, 1.0, 0.5),            // North: z..z+0.5
                        1 => (wx + 0.5, wz, 0.5, 1.0),      // West: x+0.5..x+1
                        _ => (wx, wz, 0.5, 1.0),            // East: x..x+0.5
                    };
                    // Only emit step top if the block above at the front half is open
                    let step_cx = (x as i32 + step_fx).max(0).min(CHUNK_SIZE as i32 - 1);
                    let step_cz = (z as i32 + step_fz).max(0).min(CHUNK_SIZE as i32 - 1);
                    let above = chunk.get_block(step_cx as usize, y + 1, step_cz as usize);
                    if y + 1 >= CHUNK_HEIGHT || above.is_air() || above.id.is_transparent() || above.id.is_stair() {
                        let step_tex = get_texture_index(block, BlockFace::Top);
                        emit(&mut vertices, &mut indices, BlockFace::Top, step_x, by + 0.5, step_z, step_w, step_h, step_tex);
                    }

                    // Back face: full height, at the back edge (opposite the facing direction)
                    let (back_face, back_ox, back_oz) = match facing {
                        0 => (BlockFace::Back,  wx,        wz),  // South: back at z (north)
                        2 => (BlockFace::Front, wx,        wz + 1.0),  // North: back at z+1 (south)
                        1 => (BlockFace::Right, wx + 1.0,  wz),  // West: back at x+1 (east)
                        _ => (BlockFace::Left,  wx,        wz),  // East: back at x (west)
                    };
                    let back_side = x as i32 + back_fx;
                    let back_sz = z as i32 + back_fz;
                    if is_open(back_side, y as i32, back_sz) {
                        let back_tex = match back_face {
                            BlockFace::Back | BlockFace::Front => get_texture_index(block, BlockFace::Front),
                            BlockFace::Left | BlockFace::Right => get_texture_index(block, BlockFace::Left),
                            _ => tex,
                        };
                        emit(&mut vertices, &mut indices, back_face, back_ox, by, back_oz, 1.0, 1.0, back_tex);
                    }

                    // Front (step) face: half height, at the front edge
                    let (front_face, front_ox, front_oz) = match facing {
                        0 => (BlockFace::Front, wx,        wz + 1.0),  // South: front at z+1
                        2 => (BlockFace::Back,  wx,        wz),        // North: front at z
                        1 => (BlockFace::Left,  wx,        wz),        // West: front at x
                        _ => (BlockFace::Right, wx + 1.0,  wz),        // East: front at x+1
                    };
                    let front_side = x as i32 + step_fx;
                    let front_sz = z as i32 + step_fz;
                    if is_open(front_side, y as i32, front_sz) {
                        let front_tex = match front_face {
                            BlockFace::Front | BlockFace::Back => get_texture_index(block, BlockFace::Front),
                            BlockFace::Left | BlockFace::Right => get_texture_index(block, BlockFace::Left),
                            _ => tex,
                        };
                        emit(&mut vertices, &mut indices, front_face, front_ox, by, front_oz, 1.0, 0.5, front_tex);
                    }

                    // Left/Right sides: each has 2 quads (bottom half full depth, top half back depth)
                    let side_pairs: &[(i32, i32, BlockFace, BlockFace)] = &[
                        (0, -1, BlockFace::Left, BlockFace::Right),  // Left face (-X)
                        (0, 1, BlockFace::Right, BlockFace::Left),   // Right face (+X) -- swapped for axis alignment
                    ];

                    for &(sdx, sdz, side_face, opp_face) in side_pairs {
                        let sx = x as i32 + sdx;
                        let sz = z as i32 + sdz;
                        if is_open(sx, y as i32, sz) {
                            let side_tex = get_texture_index(block, opp_face);
                            // Bottom half of side: full depth, half height
                            let (sx_pos, sy_pos, sz_pos) = match side_face {
                                BlockFace::Left => (wx, by, wz),
                                BlockFace::Right => (wx + 1.0, by, wz),
                                _ => unreachable!(),
                            };
                            emit(&mut vertices, &mut indices, side_face, sx_pos, sy_pos, sz_pos, 1.0, 0.5, side_tex);
                            // Top half of side: only back half depth
                            let (back_depth_w, back_depth_h, back_ox_s, back_oz_s) = match facing {
                                0 => (0.5, 0.5, 0.0, 0.0),    // South: back half in z [z..z+0.5]
                                2 => (0.5, 0.5, 0.0, 0.5),    // North: back half in z [z+0.5..z+1]
                                1 => (0.5, 0.5, 0.0, 0.0),    // West: back half in x [x..x+0.5]
                                _ => (0.5, 0.5, 0.5, 0.0),    // East: back half in x [x+0.5..x+1]
                            };
                            let (sx2, sy2, sz2) = match side_face {
                                BlockFace::Left => (wx + back_ox_s, by + 0.5, wz + back_oz_s),
                                BlockFace::Right => (wx + 1.0 + back_ox_s, by + 0.5, wz + back_oz_s),
                                _ => unreachable!(),
                            };
                            emit(&mut vertices, &mut indices, side_face, sx2, sy2, sz2, back_depth_w, back_depth_h, side_tex);
                        }
                    }
                } else {
                    // === TOP STAIR (upside down) ===
                    // Bottom half [y, y+0.5]: only back half filled
                    // Top half [y+0.5, y+1]: full area filled

                    // Top face: full (check block above)
                    if y + 1 >= CHUNK_HEIGHT || is_open(x as i32, y as i32 + 1, z as i32) {
                        let top_tex = get_texture_index(block, BlockFace::Top);
                        emit(&mut vertices, &mut indices, BlockFace::Top, wx, by + 1.0, wz, 1.0, 1.0, top_tex);
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
                        emit(&mut vertices, &mut indices, BlockFace::Bottom, bot_x, by, bot_z, bot_w, bot_h, bot_tex);
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
                        emit(&mut vertices, &mut indices, BlockFace::Bottom, ceil_x, by + 0.5, ceil_z, ceil_w, ceil_h, ceil_tex);
                    }

                    // Back face: full height
                    let (back_face, back_ox, back_oz) = match facing {
                        0 => (BlockFace::Back,  wx,        wz),
                        2 => (BlockFace::Front, wx,        wz + 1.0),
                        1 => (BlockFace::Right, wx + 1.0,  wz),
                        _ => (BlockFace::Left,  wx,        wz),
                    };
                    let back_side = x as i32 + back_fx;
                    let back_sz = z as i32 + back_fz;
                    if is_open(back_side, y as i32, back_sz) {
                        let back_tex = match back_face {
                            BlockFace::Back | BlockFace::Front => get_texture_index(block, BlockFace::Front),
                            BlockFace::Left | BlockFace::Right => get_texture_index(block, BlockFace::Left),
                            _ => tex,
                        };
                        emit(&mut vertices, &mut indices, back_face, back_ox, by, back_oz, 1.0, 1.0, back_tex);
                    }

                    // Front (step) face: top half only at the front edge
                    let (front_face, front_ox, front_oz) = match facing {
                        0 => (BlockFace::Front, wx,        wz + 1.0),
                        2 => (BlockFace::Back,  wx,        wz),
                        1 => (BlockFace::Left,  wx,        wz),
                        _ => (BlockFace::Right, wx + 1.0,  wz),
                    };
                    let front_side = x as i32 + step_fx;
                    let front_sz = z as i32 + step_fz;
                    if is_open(front_side, y as i32, front_sz) {
                        let front_tex = match front_face {
                            BlockFace::Front | BlockFace::Back => get_texture_index(block, BlockFace::Front),
                            BlockFace::Left | BlockFace::Right => get_texture_index(block, BlockFace::Left),
                            _ => tex,
                        };
                        emit(&mut vertices, &mut indices, front_face, front_ox, by + 0.5, front_oz, 1.0, 0.5, front_tex);
                    }

                    // Side faces for top stair
                    // Bottom half of side: only back half depth
                    // Top half of side: full depth
                    let side_pairs: &[(i32, i32, BlockFace, BlockFace)] = &[
                        (0, -1, BlockFace::Left, BlockFace::Right),
                        (0, 1, BlockFace::Right, BlockFace::Left),
                    ];
                    for &(sdx, sdz, side_face, opp_face) in side_pairs {
                        let sx = x as i32 + sdx;
                        let sz = z as i32 + sdz;
                        if is_open(sx, y as i32, sz) {
                            let side_tex = get_texture_index(block, opp_face);
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
                                _ => unreachable!(),
                            };
                            emit(&mut vertices, &mut indices, side_face, bsx, bsy, bsz, 1.0, 0.5, side_tex);

                            // Top half: full depth
                            let (tsx, tsy, tsz) = match side_face {
                                BlockFace::Left => (wx, by + 0.5, wz),
                                BlockFace::Right => (wx + 1.0, by + 0.5, wz),
                                _ => unreachable!(),
                            };
                            emit(&mut vertices, &mut indices, side_face, tsx, tsy, tsz, 1.0, 0.5, side_tex);
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
    CROSSED_TEX.get()
        .and_then(|m| m.get(&block.id))
        .copied()
        .unwrap_or(0)
}

fn emit_crossed_quad_light(
    verts: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    x: f32, y: f32, z: f32,
    tex_index: u32,
    light_data: u32,
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
                light_data,
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
