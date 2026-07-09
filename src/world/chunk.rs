use crate::world::block::Block;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 384;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub blocks: Box<[Block; CHUNK_VOLUME]>,
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
        use crate::world::block::BlockId;
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
}
