//! Immutable 1.21.1 block metadata and compact property-state definitions.
//!
//! Chunk storage keeps a `BlockId` plus a registry-local state ordinal.  This
//! keeps the existing world format cheap while allowing each block family to
//! define independent property domains instead of sharing the legacy data byte.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::block::BlockId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionShape {
    Empty,
    FullCube,
    Crossed,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMaterial {
    Opaque,
    Cutout,
    Translucent,
    Fluid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundGroup {
    Stone,
    Grass,
    Wood,
    Glass,
    Metal,
    Sand,
    Wool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockTags(u16);

impl BlockTags {
    pub const SOLID: Self = Self(1 << 0);
    pub const TRANSPARENT: Self = Self(1 << 1);
    pub const CROSSED: Self = Self(1 << 2);
    pub const CLIMBABLE: Self = Self(1 << 3);
    pub const FLUID: Self = Self(1 << 4);

    pub const fn contains(self, tag: Self) -> bool {
        self.0 & tag.0 != 0
    }

    const fn insert(&mut self, tag: Self) {
        self.0 |= tag.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PropertyDefinition {
    pub name: &'static str,
    pub values: &'static [&'static str],
}

#[derive(Clone, Debug)]
pub struct BlockDefinition {
    pub id: BlockId,
    pub name: &'static str,
    pub tags: BlockTags,
    pub hardness: f32,
    pub collision: CollisionShape,
    pub light_opacity: u8,
    pub light_emission: u8,
    pub drop: BlockId,
    pub sound: SoundGroup,
    pub material: RenderMaterial,
    pub properties: &'static [PropertyDefinition],
    pub state_count: u16,
}

pub struct BlockRegistry {
    definitions: Vec<BlockDefinition>,
}

const LEVEL_VALUES: [&str; 16] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
];
const BOOL_VALUES: [&str; 2] = ["false", "true"];
// Preserve the pre-registry placement default: legacy stair data 0 faces south.
const HORIZONTAL_FACING: [&str; 4] = ["south", "west", "north", "east"];
const SLAB_TYPE: [&str; 3] = ["bottom", "top", "double"];
const STAIR_HALF: [&str; 2] = ["bottom", "top"];
const STAIR_SHAPE: [&str; 5] = ["straight", "inner_left", "inner_right", "outer_left", "outer_right"];
const CHEST_TYPE: [&str; 3] = ["single", "left", "right"];

const FLUID_PROPERTIES: [PropertyDefinition; 2] = [
    PropertyDefinition { name: "level", values: &LEVEL_VALUES },
    PropertyDefinition { name: "falling", values: &BOOL_VALUES },
];
const SLAB_PROPERTIES: [PropertyDefinition; 1] = [PropertyDefinition { name: "type", values: &SLAB_TYPE }];
const STAIR_PROPERTIES: [PropertyDefinition; 3] = [
    PropertyDefinition { name: "facing", values: &HORIZONTAL_FACING },
    PropertyDefinition { name: "half", values: &STAIR_HALF },
    PropertyDefinition { name: "shape", values: &STAIR_SHAPE },
];
const FURNACE_PROPERTIES: [PropertyDefinition; 2] = [
    PropertyDefinition { name: "facing", values: &HORIZONTAL_FACING },
    PropertyDefinition { name: "lit", values: &BOOL_VALUES },
];
const CHEST_PROPERTIES: [PropertyDefinition; 2] = [
    PropertyDefinition { name: "facing", values: &HORIZONTAL_FACING },
    PropertyDefinition { name: "type", values: &CHEST_TYPE },
];
const TORCH_PROPERTIES: [PropertyDefinition; 1] = [PropertyDefinition { name: "facing", values: &HORIZONTAL_FACING }];
const DOOR_HALF: [&str; 2] = ["lower", "upper"];
const DOOR_HINGE: [&str; 2] = ["left", "right"];
const CONNECTION: [&str; 3] = ["none", "side", "up"];
const DOOR_PROPERTIES: [PropertyDefinition; 5] = [
    PropertyDefinition { name: "facing", values: &HORIZONTAL_FACING },
    PropertyDefinition { name: "half", values: &DOOR_HALF },
    PropertyDefinition { name: "hinge", values: &DOOR_HINGE },
    PropertyDefinition { name: "open", values: &BOOL_VALUES },
    PropertyDefinition { name: "powered", values: &BOOL_VALUES },
];
const FENCE_PROPERTIES: [PropertyDefinition; 5] = [
    PropertyDefinition { name: "north", values: &BOOL_VALUES },
    PropertyDefinition { name: "east", values: &BOOL_VALUES },
    PropertyDefinition { name: "south", values: &BOOL_VALUES },
    PropertyDefinition { name: "west", values: &BOOL_VALUES },
    PropertyDefinition { name: "waterlogged", values: &BOOL_VALUES },
];
const REDSTONE_PROPERTIES: [PropertyDefinition; 5] = [
    PropertyDefinition { name: "north", values: &CONNECTION },
    PropertyDefinition { name: "east", values: &CONNECTION },
    PropertyDefinition { name: "south", values: &CONNECTION },
    PropertyDefinition { name: "west", values: &CONNECTION },
    PropertyDefinition { name: "power", values: &LEVEL_VALUES },
];

pub fn registry() -> &'static BlockRegistry {
    static REGISTRY: OnceLock<BlockRegistry> = OnceLock::new();
    REGISTRY.get_or_init(BlockRegistry::new)
}

impl BlockRegistry {
    fn new() -> Self {
        let mut definitions = Vec::with_capacity(414);
        for raw_id in 0..=413 {
            let id = BlockId::from_repr(raw_id).expect("BlockId discriminants must remain contiguous");
            let mut tags = BlockTags(0);
            if id.is_solid() {
                tags.insert(BlockTags::SOLID);
            }
            if id.is_transparent() {
                tags.insert(BlockTags::TRANSPARENT);
            }
            if id.is_crossed() {
                tags.insert(BlockTags::CROSSED);
            }
            if id.is_climbable() {
                tags.insert(BlockTags::CLIMBABLE);
            }
            if matches!(id, BlockId::Water | BlockId::Lava) {
                tags.insert(BlockTags::FLUID);
            }

            let properties = properties_for(id);
            let state_count = properties.iter().fold(1u16, |count, property| {
                count.checked_mul(property.values.len() as u16).expect("block state domain exceeds u16")
            });
            let material = if tags.contains(BlockTags::FLUID) {
                RenderMaterial::Fluid
            } else if tags.contains(BlockTags::CROSSED) {
                RenderMaterial::Cutout
            } else if tags.contains(BlockTags::TRANSPARENT) {
                RenderMaterial::Translucent
            } else {
                RenderMaterial::Opaque
            };
            let collision = if id == BlockId::Air || tags.contains(BlockTags::FLUID) {
                CollisionShape::Empty
            } else if tags.contains(BlockTags::CROSSED) {
                CollisionShape::Crossed
            } else if id.is_slab() || id.is_stair() || matches!(id, BlockId::OakDoor | BlockId::OakFence | BlockId::RedstoneDust) || tags.contains(BlockTags::CLIMBABLE) {
                CollisionShape::Custom
            } else {
                CollisionShape::FullCube
            };
            definitions.push(BlockDefinition {
                id,
                name: id.name(),
                tags,
                hardness: hardness_for(id),
                collision,
                light_opacity: if id.is_transparent() { 0 } else { 15 },
                light_emission: id.light_level(),
                drop: id,
                sound: sound_for(id),
                material,
                properties,
                state_count,
            });
        }
        Self { definitions }
    }

    pub fn definition(&self, id: BlockId) -> &BlockDefinition {
        &self.definitions[id as usize]
    }

    pub fn state_for_properties<'a>(
        &self,
        id: BlockId,
        properties: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<u16> {
        let definition = self.definition(id);
        let supplied: HashMap<_, _> = properties.into_iter().collect();
        if supplied.len() != definition.properties.len() {
            return None;
        }
        let mut state = 0u16;
        for property in definition.properties {
            let value = supplied.get(property.name)?;
            let value_index = property.values.iter().position(|candidate| candidate == value)? as u16;
            state = state * property.values.len() as u16 + value_index;
        }
        Some(state)
    }

    pub fn properties_for_state(&self, id: BlockId, state: u16) -> Option<Vec<(&'static str, &'static str)>> {
        let definition = self.definition(id);
        if state >= definition.state_count {
            return None;
        }
        let mut remaining = state;
        let mut out = Vec::with_capacity(definition.properties.len());
        for property in definition.properties.iter().rev() {
            let index = (remaining % property.values.len() as u16) as usize;
            remaining /= property.values.len() as u16;
            out.push((property.name, property.values[index]));
        }
        out.reverse();
        Some(out)
    }
}

fn properties_for(id: BlockId) -> &'static [PropertyDefinition] {
    match id {
        BlockId::Water | BlockId::Lava => &FLUID_PROPERTIES,
        BlockId::StoneSlab | BlockId::OakSlab => &SLAB_PROPERTIES,
        BlockId::StoneStairs | BlockId::OakStairs => &STAIR_PROPERTIES,
        BlockId::Furnace => &FURNACE_PROPERTIES,
        BlockId::Chest => &CHEST_PROPERTIES,
        BlockId::Torch | BlockId::WallTorch | BlockId::SoulTorch | BlockId::SoulWallTorch => &TORCH_PROPERTIES,
        BlockId::OakDoor => &DOOR_PROPERTIES,
        BlockId::OakFence => &FENCE_PROPERTIES,
        BlockId::RedstoneDust => &REDSTONE_PROPERTIES,
        _ => &[],
    }
}

fn hardness_for(id: BlockId) -> f32 {
    match id {
        BlockId::Air | BlockId::Water | BlockId::Lava | BlockId::Fire | BlockId::SoulFire => 0.0,
        BlockId::Bedrock => -1.0,
        BlockId::Obsidian => 50.0,
        BlockId::OakLeaves | BlockId::OakLeaves2 => 0.2,
        BlockId::Glass => 0.3,
        BlockId::Dirt | BlockId::GrassBlock | BlockId::Sand | BlockId::Gravel => 0.5,
        BlockId::OakLog | BlockId::OakPlanks | BlockId::OakPlanks2 => 2.0,
        _ => 1.5,
    }
}

fn sound_for(id: BlockId) -> SoundGroup {
    match id {
        BlockId::GrassBlock | BlockId::Dirt | BlockId::Gravel => SoundGroup::Grass,
        BlockId::OakLog | BlockId::OakPlanks | BlockId::OakPlanks2 | BlockId::Chest => SoundGroup::Wood,
        BlockId::Glass | BlockId::Ice | BlockId::PackedIce | BlockId::BlueIce => SoundGroup::Glass,
        BlockId::IronBlock | BlockId::GoldBlock | BlockId::CopperBlock => SoundGroup::Metal,
        BlockId::WhiteWool | BlockId::BlackWool | BlockId::RedWool | BlockId::BlueWool => SoundGroup::Wool,
        BlockId::Sand | BlockId::RedSand => SoundGroup::Sand,
        _ => SoundGroup::Stone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_properties_round_trip() {
        let registry = registry();
        let state = registry
            .state_for_properties(BlockId::StoneStairs, [("facing", "west"), ("half", "top"), ("shape", "outer_right")])
            .unwrap();
        assert_eq!(registry.properties_for_state(BlockId::StoneStairs, state).unwrap(), vec![
            ("facing", "west"),
            ("half", "top"),
            ("shape", "outer_right"),
        ]);
    }

    #[test]
    fn state_rejects_missing_or_invalid_property() {
        let registry = registry();
        assert!(registry.state_for_properties(BlockId::Water, [("level", "0")]).is_none());
        assert!(registry.state_for_properties(BlockId::Water, [("level", "16"), ("falling", "false")]).is_none());
    }
}
