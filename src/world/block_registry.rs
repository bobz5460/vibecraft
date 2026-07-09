use crate::world::block::{Block, BlockId};

#[derive(Clone)]
pub struct BlockProperties {
    pub id: BlockId,
    pub name: &'static str,
    pub solid: bool,
    pub transparent: bool,
    pub emissive: bool,
    pub light_level: u8,
    pub hardiness: f32,
    pub tool: ToolRequirement,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ToolRequirement {
    None,
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
    Shears,
}

pub struct BlockRegistry {
    pub blocks: Vec<BlockProperties>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        let mut blocks = Vec::with_capacity(512);

        for i in 0u16..512u16 {
            let id: BlockId = unsafe { std::mem::transmute(i) };
            let props = get_properties(id);
            blocks.push(props);
        }

        BlockRegistry { blocks }
    }

    pub fn get(&self, id: BlockId) -> &BlockProperties {
        &self.blocks[id as u16 as usize]
    }

    pub fn get_block(&self, block: Block) -> &BlockProperties {
        &self.blocks[block.id as u16 as usize]
    }
}

fn get_properties(id: BlockId) -> BlockProperties {
    match id {
        BlockId::Air => BlockProperties {
            id, name: "Air", solid: false, transparent: true, emissive: false, light_level: 0, hardiness: 0.0, tool: ToolRequirement::None,
        },
        BlockId::Stone => BlockProperties {
            id, name: "Stone", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 1.5, tool: ToolRequirement::Pickaxe,
        },
        BlockId::GrassBlock => BlockProperties {
            id, name: "Grass Block", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.6, tool: ToolRequirement::Shovel,
        },
        BlockId::Dirt => BlockProperties {
            id, name: "Dirt", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.5, tool: ToolRequirement::Shovel,
        },
        BlockId::Cobblestone => BlockProperties {
            id, name: "Cobblestone", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 2.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::OakPlanks => BlockProperties {
            id, name: "Oak Planks", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 2.0, tool: ToolRequirement::Axe,
        },
        BlockId::Bedrock => BlockProperties {
            id, name: "Bedrock", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: -1.0, tool: ToolRequirement::None,
        },
        BlockId::Water => BlockProperties {
            id, name: "Water", solid: false, transparent: true, emissive: false, light_level: 1, hardiness: 100.0, tool: ToolRequirement::None,
        },
        BlockId::Sand => BlockProperties {
            id, name: "Sand", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.5, tool: ToolRequirement::Shovel,
        },
        BlockId::Gravel => BlockProperties {
            id, name: "Gravel", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.6, tool: ToolRequirement::Shovel,
        },
        BlockId::GoldOre => BlockProperties {
            id, name: "Gold Ore", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::IronOre => BlockProperties {
            id, name: "Iron Ore", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::CoalOre => BlockProperties {
            id, name: "Coal Ore", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::OakLog => BlockProperties {
            id, name: "Oak Log", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 2.0, tool: ToolRequirement::Axe,
        },
        BlockId::OakLeaves => BlockProperties {
            id, name: "Oak Leaves", solid: true, transparent: true, emissive: false, light_level: 0, hardiness: 0.2, tool: ToolRequirement::Hoe,
        },
        BlockId::Glass => BlockProperties {
            id, name: "Glass", solid: true, transparent: true, emissive: false, light_level: 0, hardiness: 0.3, tool: ToolRequirement::None,
        },
        BlockId::CraftingTable => BlockProperties {
            id, name: "Crafting Table", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 2.5, tool: ToolRequirement::Axe,
        },
        BlockId::Furnace => BlockProperties {
            id, name: "Furnace", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.5, tool: ToolRequirement::Pickaxe,
        },
        BlockId::Chest => BlockProperties {
            id, name: "Chest", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 2.5, tool: ToolRequirement::Axe,
        },
        BlockId::Torch => BlockProperties {
            id, name: "Torch", solid: false, transparent: true, emissive: true, light_level: 14, hardiness: 0.0, tool: ToolRequirement::None,
        },
        BlockId::Snow => BlockProperties {
            id, name: "Snow", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.1, tool: ToolRequirement::Shovel,
        },
        BlockId::Ice => BlockProperties {
            id, name: "Ice", solid: true, transparent: true, emissive: false, light_level: 0, hardiness: 0.5, tool: ToolRequirement::Pickaxe,
        },
        BlockId::Glowstone => BlockProperties {
            id, name: "Glowstone", solid: true, transparent: true, emissive: true, light_level: 15, hardiness: 0.3, tool: ToolRequirement::None,
        },
        BlockId::Netherrack => BlockProperties {
            id, name: "Netherrack", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.4, tool: ToolRequirement::Pickaxe,
        },
        BlockId::SoulSand => BlockProperties {
            id, name: "Soul Sand", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 0.5, tool: ToolRequirement::Shovel,
        },
        BlockId::Deepslate => BlockProperties {
            id, name: "Deepslate", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::Obsidian => BlockProperties {
            id, name: "Obsidian", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 50.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::NetheriteBlock => BlockProperties {
            id, name: "Block of Netherite", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 50.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::AncientDebris => BlockProperties {
            id, name: "Ancient Debris", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 30.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::Bricks => BlockProperties {
            id, name: "Bricks", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 2.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::Bookshelf => BlockProperties {
            id, name: "Bookshelf", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 1.5, tool: ToolRequirement::Axe,
        },
        BlockId::DiamondBlock => BlockProperties {
            id, name: "Diamond Block", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 5.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::IronBlock => BlockProperties {
            id, name: "Iron Block", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 5.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::GoldBlock => BlockProperties {
            id, name: "Gold Block", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::EmeraldBlock => BlockProperties {
            id, name: "Emerald Block", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 5.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::LapisBlock => BlockProperties {
            id, name: "Lapis Lazuli Block", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 3.0, tool: ToolRequirement::Pickaxe,
        },
        BlockId::RedstoneBlock => BlockProperties {
            id, name: "Block of Redstone", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 5.0, tool: ToolRequirement::Pickaxe,
        },
        _ => BlockProperties {
            id, name: "Unknown", solid: true, transparent: false, emissive: false, light_level: 0, hardiness: 1.0, tool: ToolRequirement::None,
        },
    }
}
