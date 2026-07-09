use crate::world::block::BlockId;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::mesh::ChunkMesh;

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
    pub block_id: BlockId,
    pub lifetime: f32,
}

impl DroppedItem {
    pub fn new(x: f32, y: f32, z: f32, block_id: BlockId) -> Self {
        DroppedItem {
            x, y, z,
            vx: 0.0, vy: 5.0, vz: 0.0,
            block_id,
            lifetime: DESPAWN_TIME,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.vy += GRAVITY * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.z += self.vz * dt;
        self.vx *= 0.95;
        self.vz *= 0.95;
        self.lifetime -= dt;

        if self.y < 1.0 {
            self.y = 1.0;
            self.vy = -self.vy * BOUNCE;
            self.vx *= 0.8;
            self.vz *= 0.8;
            if self.vy.abs() < 0.5 {
                self.vy = 0.0;
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.lifetime > 0.0 && self.y > -64.0 && self.y < CHUNK_HEIGHT as f32 + 16.0
    }
}

pub fn map_drop(block_id: BlockId) -> BlockId {
    match block_id {
        BlockId::Stone | BlockId::Granite | BlockId::Diorite | BlockId::Andesite => BlockId::Cobblestone,
        BlockId::GrassBlock | BlockId::Podzol => BlockId::Dirt,
        BlockId::Deepslate => BlockId::CobbledDeepslate,
        BlockId::Glass | BlockId::TintedGlass => block_id,
        BlockId::Ice | BlockId::BlueIce | BlockId::PackedIce | BlockId::FrostedIce => block_id,
        BlockId::OakLeaves | BlockId::SpruceLeaves | BlockId::BirchLeaves | BlockId::JungleLeaves
        | BlockId::AcaciaLeaves | BlockId::DarkOakLeaves | BlockId::CherryLeaves
        | BlockId::MangroveLeaves | BlockId::AzaleaLeaves | BlockId::FloweringAzaleaLeaves => block_id,
        BlockId::OakSapling | BlockId::SpruceSapling | BlockId::BirchSapling | BlockId::JungleSapling
        | BlockId::AcaciaSapling | BlockId::DarkOakSapling | BlockId::CherrySapling => BlockId::OakSapling,
        _ => block_id,
    }
}

pub fn dropped_items_to_mesh(items: &[DroppedItem]) -> ChunkMesh {
    let item_data: Vec<(f32, f32, f32, BlockId)> = items.iter()
        .map(|i| (i.x, i.y, i.z, i.block_id))
        .collect();
    crate::world::mesh::build_item_cube_mesh(&item_data)
}
