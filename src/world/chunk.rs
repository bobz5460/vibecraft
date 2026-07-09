use crate::world::block::{Block, BlockId};

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 384;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub blocks: Box<[Block; CHUNK_VOLUME]>,
    pub sky_light: Box<[u8; CHUNK_VOLUME]>,
    pub block_light: Box<[u8; CHUNK_VOLUME]>,
    pub is_dirty: bool,
    pub has_mesh: bool,
    pub has_water: bool,
    pub has_lava: bool,
    pub cx: i32,
    pub cz: i32,
    water_count: u32,
    lava_count: u32,
}

impl Chunk {
    pub fn new(cx: i32, cz: i32) -> Self {
        Chunk {
            blocks: Box::new([Block::air(); CHUNK_VOLUME]),
            sky_light: Box::new([15u8; CHUNK_VOLUME]),
            block_light: Box::new([0u8; CHUNK_VOLUME]),
            is_dirty: true,
            has_mesh: false,
            has_water: false,
            has_lava: false,
            cx,
            cz,
            water_count: 0,
            lava_count: 0,
        }
    }

    pub fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> Block {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return Block::air();
        }
        self.blocks[Self::index(x, y, z)]
    }

    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: Block) {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return;
        }
        let idx = Self::index(x, y, z);
        if self.blocks[idx] != block {
            let old_id = self.blocks[idx].id;
            self.blocks[idx] = block;
            self.is_dirty = true;
            if block.id == crate::world::block::BlockId::Water {
                self.water_count += 1;
                self.has_water = true;
            } else if old_id == crate::world::block::BlockId::Water {
                self.water_count = self.water_count.saturating_sub(1);
                self.has_water = self.water_count > 0;
            }
            if block.id == crate::world::block::BlockId::Lava {
                self.lava_count += 1;
                self.has_lava = true;
            } else if old_id == crate::world::block::BlockId::Lava {
                self.lava_count = self.lava_count.saturating_sub(1);
                self.has_lava = self.lava_count > 0;
            }
        }
    }

    pub fn recount_fluids(&mut self) {
        let (mut w, mut l) = (0u32, 0u32);
        for b in self.blocks.iter() {
            if b.id == BlockId::Water { w += 1; }
            else if b.id == BlockId::Lava { l += 1; }
        }
        self.water_count = w;
        self.lava_count = l;
        self.has_water = w > 0;
        self.has_lava = l > 0;
    }

    /// Compute skylight for each column from top down.
    /// Opaque blocks block all light, transparent blocks attenuate by 1, air passes through.
    pub fn compute_skylight(&mut self) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut light: u8 = 15;
                for y in (0..CHUNK_HEIGHT).rev() {
                    let idx = Self::index(x, y, z);
                    self.sky_light[idx] = light;
                    let block = &self.blocks[idx];
                    if block.is_air() {
                        // Light passes through
                    } else if block.id == BlockId::Water || block.id == BlockId::Lava {
                        light = light.saturating_sub(2);
                    } else if block.id.is_transparent() {
                        light = light.saturating_sub(1);
                    } else {
                        light = 0;
                    }
                }
            }
        }
    }

    /// Compute block light via BFS flood-fill from light-emitting blocks.
    pub fn compute_block_light(&mut self) {
        self.block_light.fill(0);
        let mut queue = Vec::with_capacity(1024);

        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                for z in 0..CHUNK_SIZE {
                    let idx = Self::index(x, y, z);
                    let light = self.blocks[idx].id.light_level();
                    if light > 0 {
                        self.block_light[idx] = light;
                        queue.push((x as i32, y as i32, z as i32));
                    }
                }
            }
        }

        let mut head = 0;
        while head < queue.len() {
            let (x, y, z) = queue[head];
            head += 1;
            let current = self.block_light[Self::index(x as usize, y as usize, z as usize)];
            let next = current.saturating_sub(1);
            if next == 0 { continue; }

            for &(dx, dy, dz) in &[(1,0,0), (-1,0,0), (0,1,0), (0,-1,0), (0,0,1), (0,0,-1)] {
                let nx = x + dx;
                let ny = y + dy;
                let nz = z + dz;
                if nx < 0 || nx >= CHUNK_SIZE as i32 || ny < 0 || ny >= CHUNK_HEIGHT as i32 || nz < 0 || nz >= CHUNK_SIZE as i32 {
                    continue;
                }
                let nidx = Self::index(nx as usize, ny as usize, nz as usize);
                if self.block_light[nidx] >= next { continue; }
                let block = &self.blocks[nidx];
                if block.is_air() || block.id.is_transparent() {
                    self.block_light[nidx] = next;
                    queue.push((nx, ny, nz));
                }
            }
        }
    }

    /// Get combined sky + block light at a position (clamped to chunk bounds).
    pub fn get_light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if x >= 0 && x < CHUNK_SIZE as i32 && y >= 0 && y < CHUNK_HEIGHT as i32 && z >= 0 && z < CHUNK_SIZE as i32 {
            let idx = Self::index(x as usize, y as usize, z as usize);
            (self.sky_light[idx], self.block_light[idx])
        } else {
            (0, 0)
        }
    }

    /// Pack sky and block light into a u32 (sky in bits 4-7, block in bits 0-3).
    pub fn pack_light(sky: u8, block: u8) -> u32 {
        ((sky as u32).min(15) << 4) | (block as u32).min(15)
    }
}
