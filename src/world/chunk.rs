use crate::inventory::progression::FurnaceState;
use crate::inventory::SlotContainer;
use crate::world::block::{Block, BlockId};
use std::collections::HashMap;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 384;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
pub const CHEST_SLOTS: usize = 27;

/// Stateful data owned by a block position rather than the compact block cell.
#[derive(Clone, Debug)]
pub enum BlockEntity {
    Chest { slots: SlotContainer },
    Furnace { state: FurnaceState },
}

impl BlockEntity {
    pub fn for_block(id: BlockId) -> Option<Self> {
        match id {
            BlockId::Chest => Some(Self::Chest {
                slots: SlotContainer::new(CHEST_SLOTS),
            }),
            BlockId::Furnace => Some(Self::Furnace {
                state: FurnaceState::new(),
            }),
            _ => None,
        }
    }

    pub fn matches_block(&self, id: BlockId) -> bool {
        matches!(
            (self, id),
            (Self::Chest { .. }, BlockId::Chest) | (Self::Furnace { .. }, BlockId::Furnace)
        )
    }
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub blocks: Box<[Block; CHUNK_VOLUME]>,
    pub sky_light: Box<[u8; CHUNK_VOLUME]>,
    pub block_light: Box<[u8; CHUNK_VOLUME]>,
    pub is_dirty: bool,
    pub light_dirty: bool,
    pub has_mesh: bool,
    pub has_water: bool,
    pub has_lava: bool,
    pub cx: i32,
    pub cz: i32,
    pub water_count: u32,
    pub lava_count: u32,
    pub fluid_positions: Vec<(u8, u16, u8)>,
    /// Sparse state keyed by the corresponding `Chunk::index` value.
    pub block_entities: HashMap<usize, BlockEntity>,
}

impl Chunk {
    pub fn new(cx: i32, cz: i32) -> Self {
        Chunk {
            blocks: Box::new([Block::air(); CHUNK_VOLUME]),
            sky_light: Box::new([15u8; CHUNK_VOLUME]),
            block_light: Box::new([0u8; CHUNK_VOLUME]),
            is_dirty: true,
            light_dirty: true,
            has_mesh: false,
            has_water: false,
            has_lava: false,
            cx,
            cz,
            water_count: 0,
            lava_count: 0,
            fluid_positions: Vec::new(),
            block_entities: HashMap::new(),
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
            self.sync_block_entity(idx);
            let pos = (x as u8, y as u16, z as u8);
            let new_is_water = block.id == crate::world::block::BlockId::Water;
            let new_is_lava = block.id == crate::world::block::BlockId::Lava;
            let old_is_water = old_id == crate::world::block::BlockId::Water;
            let old_is_lava = old_id == crate::world::block::BlockId::Lava;

            if old_is_water || old_is_lava {
                self.fluid_positions.retain(|&p| p != pos);
            }
            if new_is_water {
                self.water_count += 1;
                self.has_water = true;
                self.fluid_positions.push(pos);
            } else if old_is_water {
                self.water_count = self.water_count.saturating_sub(1);
                self.has_water = self.water_count > 0;
            }

            if new_is_lava {
                self.lava_count += 1;
                self.has_lava = true;
                self.fluid_positions.push(pos);
            } else if old_is_lava {
                self.lava_count = self.lava_count.saturating_sub(1);
                self.has_lava = self.lava_count > 0;
            }
        }
    }

    pub fn get_block_entity(&self, x: usize, y: usize, z: usize) -> Option<&BlockEntity> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        self.block_entities.get(&Self::index(x, y, z))
    }

    /// Replaces state only when it belongs to the block currently at this position.
    pub fn set_block_entity(&mut self, x: usize, y: usize, z: usize, entity: BlockEntity) -> bool {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return false;
        }
        let index = Self::index(x, y, z);
        if !entity.matches_block(self.blocks[index].id) {
            return false;
        }
        self.block_entities.insert(index, entity);
        true
    }

    /// Restores the invariant after bulk block loading: each supported stateful
    /// block has default state, and no state outlives its owning block.
    pub fn reconcile_block_entities(&mut self) {
        self.block_entities.retain(|&index, entity| {
            index < CHUNK_VOLUME && entity.matches_block(self.blocks[index].id)
        });
        for (index, block) in self.blocks.iter().copied().enumerate() {
            if !self.block_entities.contains_key(&index) {
                if let Some(entity) = BlockEntity::for_block(block.id) {
                    self.block_entities.insert(index, entity);
                }
            }
        }
    }

    fn sync_block_entity(&mut self, index: usize) {
        let id = self.blocks[index].id;
        if self
            .block_entities
            .get(&index)
            .is_some_and(|entity| entity.matches_block(id))
        {
            return;
        }
        match BlockEntity::for_block(id) {
            Some(entity) => {
                self.block_entities.insert(index, entity);
            }
            None => {
                self.block_entities.remove(&index);
            }
        }
    }

    pub fn recount_fluids(&mut self) {
        let (mut w, mut l) = (0u32, 0u32);
        self.fluid_positions.clear();
        for lx in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                for lz in 0..CHUNK_SIZE {
                    let b = self.blocks[Self::index(lx, y, lz)];
                    if b.id == BlockId::Water {
                        w += 1;
                        self.fluid_positions.push((lx as u8, y as u16, lz as u8));
                    } else if b.id == BlockId::Lava {
                        l += 1;
                        self.fluid_positions.push((lx as u8, y as u16, lz as u8));
                    }
                }
            }
        }
        self.water_count = w;
        self.lava_count = l;
        self.has_water = w > 0;
        self.has_lava = l > 0;
    }

    /// Get combined sky + block light at a position (clamped to chunk bounds).
    pub fn get_light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if x >= 0
            && x < CHUNK_SIZE as i32
            && y >= 0
            && y < CHUNK_HEIGHT as i32
            && z >= 0
            && z < CHUNK_SIZE as i32
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemStack;

    #[test]
    fn block_entity_lifecycle_tracks_owning_block() {
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(1, 2, 3, Block::new(BlockId::Chest));
        let BlockEntity::Chest { slots } = chunk.get_block_entity(1, 2, 3).unwrap() else {
            panic!("chest block should create chest state");
        };
        assert_eq!(slots.slots.len(), CHEST_SLOTS);

        let mut chest = BlockEntity::for_block(BlockId::Chest).unwrap();
        let BlockEntity::Chest { slots } = &mut chest else { unreachable!() };
        slots.slots[0] = ItemStack::new(1, 4);
        assert!(chunk.set_block_entity(1, 2, 3, chest));

        chunk.set_block(1, 2, 3, Block::new(BlockId::Furnace));
        assert!(matches!(
            chunk.get_block_entity(1, 2, 3),
            Some(BlockEntity::Furnace { .. })
        ));
        chunk.set_block(1, 2, 3, Block::air());
        assert!(chunk.get_block_entity(1, 2, 3).is_none());
    }
}
