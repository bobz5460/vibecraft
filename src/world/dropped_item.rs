use crate::inventory::item::ItemRegistry;
use crate::inventory::ItemStack;
use crate::world::block::BlockId;
use crate::world::chunk_manager::ChunkManager;
use crate::world::mesh::{build_item_cube_mesh, ChunkMesh};

const GRAVITY: f32 = -25.0;
const DESPAWN_TIME: f32 = 300.0;
const BOUNCE: f32 = 0.4;

pub struct DroppedItem {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    /// The authoritative dropped payload. Rendering derives an ItemAtlas sprite
    /// from this stack; no visual texture state is stored in the entity.
    pub stack: ItemStack,
    /// Retained for native-save compatibility and legacy item-ID recovery.
    /// Item rendering must use `stack.id`, not this field.
    pub block_id: BlockId,
    pub lifetime: f32,
    pub pickup_delay: f32,
    ground_y: Option<f32>,
    ground_revision: Option<u64>,
    last_bx: i32,
    last_bz: i32,
}

impl DroppedItem {
    pub fn new(x: f32, y: f32, z: f32, block_id: BlockId) -> Self {
        let mut item = DroppedItem {
            x,
            y,
            z,
            vx: 0.0,
            vy: 5.0,
            vz: 0.0,
            stack: ItemStack::new(block_id as u16, 1),
            block_id,
            lifetime: DESPAWN_TIME,
            pickup_delay: 0.5,
            ground_y: None,
            ground_revision: None,
            last_bx: 0,
            last_bz: 0,
        };
        let bx = x.floor() as i32;
        let bz = z.floor() as i32;
        item.last_bx = bx;
        item.last_bz = bz;
        item
    }

    pub fn from_stack(x: f32, y: f32, z: f32, stack: ItemStack, items: &ItemRegistry) -> Option<Self> {
        if stack.is_empty() {
            return None;
        }
        let block_id = items.block_from_item(stack.id).unwrap_or(BlockId::Stone);
        let mut item = Self::new(x, y, z, block_id);
        item.stack = stack;
        Some(item)
    }

    pub fn try_merge(&mut self, other: &mut Self, items: &ItemRegistry) -> bool {
        if !self.stack.can_merge_with(&other.stack) {
            return false;
        }
        let space = self.stack.max_stack(items) as u16 - self.stack.count;
        let moved = space.min(other.stack.count);
        self.stack.count += moved;
        other.stack.count -= moved;
        moved > 0
    }

    pub fn update(&mut self, dt: f32, cm: &ChunkManager) {
        self.vy += GRAVITY * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.z += self.vz * dt;
        self.vx *= 0.95;
        self.vz *= 0.95;
        self.lifetime -= dt;
        self.pickup_delay = (self.pickup_delay - dt).max(0.0);

        let bx = self.x.floor() as i32;
        let bz = self.z.floor() as i32;
        if bx != self.last_bx || bz != self.last_bz {
            self.ground_y = None;
            self.ground_revision = None;
            self.last_bx = bx;
            self.last_bz = bz;
        }
        let revision = cm.chunk_revision(
            bx.div_euclid(crate::world::chunk::CHUNK_SIZE as i32),
            bz.div_euclid(crate::world::chunk::CHUNK_SIZE as i32),
        );
        if self.ground_y.is_some() && self.ground_revision != revision {
            self.ground_y = None;
        }
        if self.ground_y.is_none() {
            self.ground_y = {
                let min_y = cm.coordinate_profile().min_y();
                let mut gy = (self.y.floor() as i32).max(min_y);
                loop {
                    let block = cm.get_block(bx, gy, bz);
                    if !block.is_air() && block.id != BlockId::Water && block.id != BlockId::Lava {
                        break Some((gy + 1) as f32);
                    }
                    if gy == min_y {
                        break None;
                    }
                    gy -= 1;
                }
            };
            self.ground_revision = revision;
        }
        if let Some(ground) = self.ground_y {
            if self.y < ground {
                self.y = ground;
                self.vy = -self.vy * BOUNCE;
                self.vx *= 0.8;
                self.vz *= 0.8;
                if self.vy.abs() < 0.5 {
                    self.vy = 0.0;
                }
            }
        }
    }

    pub fn is_alive(&self, cm: &ChunkManager) -> bool {
        let profile = cm.coordinate_profile();
        self.lifetime > 0.0
            && self.y > profile.min_y() as f32
            && self.y < profile.max_y_exclusive() as f32 + 16.0
    }
}

#[derive(Clone)]
pub struct XpOrb {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub value: u32,
    pub lifetime: f32,
    ground_y: Option<f32>,
    ground_revision: Option<u64>,
    last_bx: i32,
    last_bz: i32,
}

impl XpOrb {
    pub fn new(x: f32, y: f32, z: f32, value: u32) -> Self {
        let angle = rand::random::<f32>() * std::f32::consts::TAU;
        let speed = 1.0 + rand::random::<f32>() * 2.0;
        let bx = x.floor() as i32;
        let bz = z.floor() as i32;
        XpOrb {
            x, y, z,
            vx: angle.cos() * speed,
            vy: 3.0 + rand::random::<f32>() * 3.0,
            vz: angle.sin() * speed,
            value,
            lifetime: 60.0,
            ground_y: None,
            ground_revision: None,
            last_bx: bx,
            last_bz: bz,
        }
    }

    pub fn update(&mut self, dt: f32, cm: &ChunkManager, px: f32, py: f32, pz: f32) {
        self.vy += GRAVITY * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.z += self.vz * dt;
        self.vx *= 0.95;
        self.vz *= 0.95;
        self.lifetime -= dt;

        let dx = px - self.x;
        let dy = py - self.y;
        let dz = pz - self.z;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        if dist < 8.0 && dist > 0.1 {
            let attract = 4.0 / (dist + 0.5);
            self.vx += dx / dist * attract * dt;
            self.vy += dy / dist * attract * dt;
            self.vz += dz / dist * attract * dt;
        }

        let bx = self.x.floor() as i32;
        let bz = self.z.floor() as i32;
        if bx != self.last_bx || bz != self.last_bz {
            self.ground_y = None;
            self.ground_revision = None;
            self.last_bx = bx;
            self.last_bz = bz;
        }
        let revision = cm.chunk_revision(
            bx.div_euclid(crate::world::chunk::CHUNK_SIZE as i32),
            bz.div_euclid(crate::world::chunk::CHUNK_SIZE as i32),
        );
        if self.ground_y.is_some() && self.ground_revision != revision {
            self.ground_y = None;
        }
        if self.ground_y.is_none() {
            self.ground_y = {
                let min_y = cm.coordinate_profile().min_y();
                let mut gy = (self.y.floor() as i32).max(min_y);
                loop {
                    let block = cm.get_block(bx, gy, bz);
                    if !block.is_air() && block.id != BlockId::Water && block.id != BlockId::Lava {
                        break Some((gy + 1) as f32);
                    }
                    if gy == min_y {
                        break None;
                    }
                    gy -= 1;
                }
            };
            self.ground_revision = revision;
        }
        if let Some(ground) = self.ground_y {
            if self.y < ground {
                self.y = ground;
                self.vy = -self.vy * 0.4;
                self.vx *= 0.8;
                self.vz *= 0.8;
            }
        }
    }

    pub fn is_alive(&self, cm: &ChunkManager) -> bool {
        let profile = cm.coordinate_profile();
        self.lifetime > 0.0
            && self.y > profile.min_y() as f32
            && self.y < profile.max_y_exclusive() as f32 + 16.0
    }
}

pub fn xp_orbs_to_mesh(orbs: &[XpOrb]) -> ChunkMesh {
    let item_data: Vec<(f32, f32, f32, BlockId)> = orbs.iter().map(|o| (o.x, o.y, o.z, BlockId::EmeraldBlock)).collect();
    build_item_cube_mesh(&item_data)
}

/// Returns the base drop count for a given block when broken without Fortune.
/// Most blocks drop 1. Blocks like coal ore drop 2-4, etc.
pub fn block_drop_quantity(block_id: BlockId) -> u32 {
    match block_id {
        BlockId::CoalOre
        | BlockId::DeepslateCoalOre
        | BlockId::LapisOre
        | BlockId::DeepslateLapisOre => 3,
        BlockId::NetherGoldOre => 3,
        BlockId::RedstoneOre | BlockId::DeepslateRedstoneOre => 4,
        _ => 1,
    }
}

pub fn map_drop(block_id: BlockId) -> BlockId {
    match block_id {
        BlockId::Stone | BlockId::Granite | BlockId::Diorite | BlockId::Andesite => {
            BlockId::Cobblestone
        }
        BlockId::GrassBlock | BlockId::Podzol => BlockId::Dirt,
        BlockId::Deepslate => BlockId::CobbledDeepslate,
        BlockId::Glass | BlockId::TintedGlass => block_id,
        BlockId::Ice | BlockId::BlueIce | BlockId::PackedIce | BlockId::FrostedIce => block_id,
        BlockId::OakLeaves
        | BlockId::SpruceLeaves
        | BlockId::BirchLeaves
        | BlockId::JungleLeaves
        | BlockId::AcaciaLeaves
        | BlockId::DarkOakLeaves
        | BlockId::CherryLeaves
        | BlockId::MangroveLeaves
        | BlockId::AzaleaLeaves
        | BlockId::FloweringAzaleaLeaves => block_id,
        BlockId::OakSapling
        | BlockId::SpruceSapling
        | BlockId::BirchSapling
        | BlockId::JungleSapling
        | BlockId::AcaciaSapling
        | BlockId::DarkOakSapling
        | BlockId::CherrySapling => BlockId::OakSapling,
        BlockId::SweetBerryBush => BlockId::SweetBerryBush,
        _ => block_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::item::ItemRegistry;
    use crate::world::block::Block;
    use crate::world::chunk::Chunk;
    use crate::world::coordinates::WorldCoordinateProfile;
    use std::sync::Arc;

    #[test]
    fn stack_merge_preserves_every_item() {
        let registry = ItemRegistry::new();
        let stack = ItemStack::new(registry.item_id_from_block(BlockId::Stone), 40);
        let mut first = DroppedItem::from_stack(0.0, 64.0, 0.0, stack.clone(), &registry).unwrap();
        let mut second = DroppedItem::from_stack(0.0, 64.0, 0.0, ItemStack::new(stack.id, 40), &registry).unwrap();
        assert!(first.try_merge(&mut second, &registry));
        assert_eq!(first.stack.count, 64);
        assert_eq!(second.stack.count, 16);
    }

    #[test]
    fn block_drops_keep_their_legacy_block_identity() {
        let registry = ItemRegistry::new();
        let stack = ItemStack::new(registry.item_id_from_block(BlockId::Stone), 1);
        let dropped = DroppedItem::from_stack(0.0, 64.0, 0.0, stack, &registry).unwrap();
        assert_eq!(dropped.block_id, BlockId::Stone);
    }

    #[test]
    fn drops_and_xp_orbs_land_on_the_inclusive_world_bottom() {
        for profile in [
            WorldCoordinateProfile::LegacyLocal,
            WorldCoordinateProfile::JavaOverworld,
        ] {
            let mut manager = ChunkManager::new(7, 1, profile, crate::world::generation::WorldGenerationProfile::legacy());
            manager.chunks.insert((0, 0), Arc::new(Chunk::new(0, 0)));
            manager.set_block(0, profile.min_y(), 0, Block::new(BlockId::Bedrock));

            let mut item = DroppedItem::new(0.5, profile.min_y() as f32 + 0.2, 0.5, BlockId::Stone);
            item.vy = -20.0;
            item.update(0.1, &manager);
            assert_eq!(item.y, profile.min_y() as f32 + 1.0);

            let mut orb = XpOrb::new(0.5, profile.min_y() as f32 + 0.2, 0.5, 1);
            orb.vy = -20.0;
            orb.update(0.1, &manager, 100.0, 100.0, 100.0);
            assert_eq!(orb.y, profile.min_y() as f32 + 1.0);
        }
    }

    #[test]
    fn floor_caches_revalidate_after_a_block_edit_in_the_same_column() {
        let profile = WorldCoordinateProfile::JavaOverworld;
        let mut manager = ChunkManager::new(7, 1, profile, crate::world::generation::WorldGenerationProfile::legacy());
        manager.chunks.insert((0, 0), Arc::new(Chunk::new(0, 0)));
        manager.set_block(0, -62, 0, Block::new(BlockId::Bedrock));
        manager.set_block(0, -60, 0, Block::new(BlockId::Stone));

        let mut item = DroppedItem::new(0.5, -59.2, 0.5, BlockId::Stone);
        item.vy = -1.0;
        item.update(0.01, &manager);
        assert_eq!(item.y, -59.0);

        let mut orb = XpOrb::new(0.5, -59.2, 0.5, 1);
        orb.vy = -1.0;
        orb.update(0.01, &manager, 100.0, 100.0, 100.0);
        assert_eq!(orb.y, -59.0);

        manager.set_block(0, -60, 0, Block::air());
        item.vy = -20.0;
        item.update(0.1, &manager);
        assert_eq!(item.y, -61.0);

        orb.vy = -20.0;
        orb.update(0.1, &manager, 100.0, 100.0, 100.0);
        assert_eq!(orb.y, -61.0);
    }
}
