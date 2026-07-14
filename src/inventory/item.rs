use crate::world::block::BlockId;

pub type ItemId = u16;

pub const AIR: ItemId = 0;
pub const GOLDEN_APPLE: ItemId = 109;
pub const ENCHANTED_APPLE: ItemId = 124;
/// Block items above the legacy contiguous range use a distinct ID domain.
/// Keeping block and item identities separate prevents a later block from
/// being interpreted as a tool or food with the same numeric value.
pub const BLOCK_ITEM_BASE: ItemId = 1_000;
const BLOCK_ID_COUNT: usize = 414;

pub struct ItemDef {
    pub name: &'static str,
    pub max_stack: u8,
    pub max_damage: u16,
    pub food_value: f32,
    pub saturation_ratio: f32,
    pub tool_tier: ToolTier,
    pub tool_type: ToolType,
    pub mining_speed: f32,
    pub attack_damage: f32,
    pub attack_speed: f32,
    pub armor_points: f32,
    pub armor_toughness: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolTier {
    None,
    Wood,
    Stone,
    Iron,
    Gold,
    Diamond,
    Netherite,
    Leather,
    Chain,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolType {
    None,
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
    Sword,
}

impl ItemDef {
    pub const fn tool(tier: ToolTier, ttype: ToolType, speed: f32, damage: f32, aspd: f32) -> Self {
        ItemDef {
            name: "",
            max_stack: 1,
            max_damage: match tier {
                ToolTier::Wood => 60,
                ToolTier::Stone => 132,
                ToolTier::Iron => 251,
                ToolTier::Gold => 33,
                ToolTier::Diamond => 1562,
                ToolTier::Netherite => 2032,
                ToolTier::Leather => 55,
                ToolTier::Chain => 95,
                ToolTier::None => 0,
            },
            food_value: 0.0,
            saturation_ratio: 0.0,
            tool_tier: tier,
            tool_type: ttype,
            mining_speed: speed,
            attack_damage: damage,
            attack_speed: aspd,
            armor_points: 0.0,
            armor_toughness: 0.0,
        }
    }

    pub const fn food(value: f32, sat_ratio: f32) -> Self {
        ItemDef {
            name: "",
            max_stack: 64,
            max_damage: 0,
            food_value: value,
            saturation_ratio: sat_ratio,
            tool_tier: ToolTier::None,
            tool_type: ToolType::None,
            mining_speed: 1.0,
            attack_damage: 1.0,
            attack_speed: 4.0,
            armor_points: 0.0,
            armor_toughness: 0.0,
        }
    }

    pub const fn block() -> Self {
        ItemDef {
            name: "",
            max_stack: 64,
            max_damage: 0,
            food_value: 0.0,
            saturation_ratio: 0.0,
            tool_tier: ToolTier::None,
            tool_type: ToolType::None,
            mining_speed: 1.0,
            attack_damage: 1.0,
            attack_speed: 4.0,
            armor_points: 0.0,
            armor_toughness: 0.0,
        }
    }

    pub const fn armor(tier: ToolTier, pts: f32, toughness: f32) -> Self {
        ItemDef {
            name: "",
            max_stack: 1,
            max_damage: match tier {
                ToolTier::Leather => 55,
                ToolTier::Chain => 95,
                ToolTier::Iron => 165,
                ToolTier::Diamond => 365,
                ToolTier::Netherite => 490,
                _ => 0,
            },
            food_value: 0.0,
            saturation_ratio: 0.0,
            tool_tier: tier,
            tool_type: ToolType::None,
            mining_speed: 1.0,
            attack_damage: 1.0,
            attack_speed: 4.0,
            armor_points: pts,
            armor_toughness: toughness,
        }
    }
}

impl ToolTier {
    pub const fn multiplier(self) -> f32 {
        match self {
            ToolTier::None => 1.0,
            ToolTier::Wood => 2.0,
            ToolTier::Stone => 4.0,
            ToolTier::Iron => 6.0,
            ToolTier::Gold => 12.0,
            ToolTier::Diamond => 8.0,
            ToolTier::Netherite => 9.0,
            ToolTier::Leather => 1.0,
            ToolTier::Chain => 1.0,
        }
    }

    pub const fn level(self) -> u8 {
        match self {
            ToolTier::None => 0,
            ToolTier::Wood => 1,
            ToolTier::Stone => 2,
            ToolTier::Iron => 3,
            ToolTier::Diamond => 4,
            ToolTier::Netherite => 5,
            ToolTier::Gold => 1,
            ToolTier::Leather => 0,
            ToolTier::Chain => 0,
        }
    }
}

pub struct ItemRegistry {
    pub items: Vec<ItemDef>,
    block_to_item: Vec<ItemId>,
    item_to_block: Vec<Option<BlockId>>,
}

fn tool_item(
    name: &'static str,
    tier: ToolTier,
    ttype: ToolType,
    speed: f32,
    damage: f32,
    aspd: f32,
) -> ItemDef {
    let mut def = ItemDef::tool(tier, ttype, speed, damage, aspd);
    def.name = name;
    def
}

fn food_item(name: &'static str, value: f32, sat_ratio: f32) -> ItemDef {
    let mut def = ItemDef::food(value, sat_ratio);
    def.name = name;
    def
}

fn block_item(name: &'static str) -> ItemDef {
    let mut def = ItemDef::block();
    def.name = name;
    def
}

fn armor_item(name: &'static str, tier: ToolTier, pts: f32, toughness: f32) -> ItemDef {
    let mut def = ItemDef::armor(tier, pts, toughness);
    def.name = name;
    def
}

impl ItemRegistry {
    pub fn new() -> Self {
        let mut items = Vec::new();
        items.push(block_item("Air")); // 0

        items.push(block_item("Stone")); // 1
        items.push(block_item("Grass Block")); // 2
        items.push(block_item("Dirt")); // 3
        items.push(block_item("Cobblestone")); // 4
        items.push(block_item("Oak Planks")); // 5
        items.push(block_item("Bedrock")); // 6
        items.push(block_item("Water")); // 7
        items.push(block_item("Sand")); // 8
        items.push(block_item("Gravel")); // 9
        items.push(block_item("Gold Ore")); // 10
        items.push(block_item("Iron Ore")); // 11
        items.push(block_item("Coal Ore")); // 12
        items.push(block_item("Oak Log")); // 13
        items.push(block_item("Oak Leaves")); // 14
        items.push(block_item("Glass")); // 15
        items.push(block_item("Crafting Table")); // 16
        items.push(block_item("Furnace")); // 17
        items.push(block_item("Chest")); // 18
        items.push(block_item("Torch")); // 19
        items.push(block_item("Snow")); // 20
        items.push(block_item("Ice")); // 21
        items.push(block_item("Glowstone")); // 22
        items.push(block_item("Netherrack")); // 23
        items.push(block_item("Soul Sand")); // 24
        items.push(block_item("Deepslate")); // 25
        items.push(block_item("Snow Block")); // 26
        items.push(block_item("Coarse Dirt")); // 27
        items.push(block_item("Podzol")); // 28
        items.push(block_item("Deepslate Iron")); // 29
        items.push(block_item("Deepslate Coal")); // 30
        items.push(block_item("Deepslate Gold")); // 31
        items.push(block_item("Deepslate Redstone")); // 32
        items.push(block_item("Deepslate Diamond")); // 33
        items.push(block_item("Deepslate Emerald")); // 34
        items.push(block_item("Deepslate Lapis")); // 35
        items.push(block_item("Deepslate Copper")); // 36
        items.push(block_item("Diamond Block")); // 37
        items.push(block_item("Iron Block")); // 38
        items.push(block_item("Gold Block")); // 39
        items.push(block_item("Emerald Block")); // 40
        items.push(block_item("Lapis Block")); // 41
        items.push(block_item("Redstone Block")); // 42
        items.push(block_item("Bricks")); // 43
        items.push(block_item("Bookshelf")); // 44
        items.push(block_item("Mossy Cobblestone")); // 45
        items.push(block_item("Obsidian")); // 46
        items.push(block_item("Spawner")); // 47
        items.push(block_item("Sandstone")); // 48
        items.push(block_item("Stone Bricks")); // 49
        items.push(block_item("Granite")); // 50
        items.push(block_item("Diorite")); // 51
        items.push(block_item("Andesite")); // 52
        items.push(block_item("Calcite")); // 53
        items.push(block_item("Tuff")); // 54
        items.push(block_item("Dripstone")); // 55
        items.push(block_item("Cobbled Deepslate")); // 56
        items.push(block_item("Polished Deepslate")); // 57
        items.push(block_item("Deepslate Bricks")); // 58
        items.push(block_item("Deepslate Tiles")); // 59
        items.push(block_item("Blackstone")); // 60
        items.push(block_item("Polished Blackstone")); // 61
        items.push(block_item("Polished Bricks")); // 62
        items.push(block_item("Crimson Nylium")); // 63
        items.push(block_item("Warped Nylium")); // 64
        items.push(block_item("Red Sand")); // 65
        items.push(block_item("Sponge")); // 66
        items.push(block_item("Wet Sponge")); // 67
        items.push(block_item("Lapis Ore")); // 68
        items.push(block_item("Redstone Ore")); // 69
        items.push(block_item("Emerald Ore")); // 70
        items.push(block_item("Diamond Ore")); // 71
        items.push(block_item("Lava")); // 72
        items.push(block_item("Wall Torch")); // 73
        items.push(block_item("Fire")); // 74

        items.push(tool_item(
            "Wooden Pickaxe",
            ToolTier::Wood,
            ToolType::Pickaxe,
            2.0,
            2.0,
            1.2,
        ));
        items.push(tool_item(
            "Wooden Axe",
            ToolTier::Wood,
            ToolType::Axe,
            2.0,
            7.0,
            0.8,
        ));
        items.push(tool_item(
            "Wooden Shovel",
            ToolTier::Wood,
            ToolType::Shovel,
            2.0,
            2.5,
            1.0,
        ));
        items.push(tool_item(
            "Wooden Hoe",
            ToolTier::Wood,
            ToolType::Hoe,
            2.0,
            1.0,
            1.0,
        ));
        items.push(tool_item(
            "Wooden Sword",
            ToolTier::Wood,
            ToolType::Sword,
            2.0,
            4.0,
            1.6,
        ));
        items.push(tool_item(
            "Stone Pickaxe",
            ToolTier::Stone,
            ToolType::Pickaxe,
            4.0,
            3.0,
            1.2,
        ));
        items.push(tool_item(
            "Stone Axe",
            ToolTier::Stone,
            ToolType::Axe,
            4.0,
            9.0,
            0.8,
        ));
        items.push(tool_item(
            "Stone Shovel",
            ToolTier::Stone,
            ToolType::Shovel,
            4.0,
            3.5,
            1.0,
        ));
        items.push(tool_item(
            "Stone Hoe",
            ToolTier::Stone,
            ToolType::Hoe,
            4.0,
            1.0,
            2.0,
        ));
        items.push(tool_item(
            "Stone Sword",
            ToolTier::Stone,
            ToolType::Sword,
            4.0,
            5.0,
            1.6,
        ));
        items.push(tool_item(
            "Iron Pickaxe",
            ToolTier::Iron,
            ToolType::Pickaxe,
            6.0,
            4.0,
            1.2,
        ));
        items.push(tool_item(
            "Iron Axe",
            ToolTier::Iron,
            ToolType::Axe,
            6.0,
            9.0,
            0.9,
        ));
        items.push(tool_item(
            "Iron Shovel",
            ToolTier::Iron,
            ToolType::Shovel,
            6.0,
            4.5,
            1.0,
        ));
        items.push(tool_item(
            "Iron Hoe",
            ToolTier::Iron,
            ToolType::Hoe,
            6.0,
            1.0,
            3.0,
        ));
        items.push(tool_item(
            "Iron Sword",
            ToolTier::Iron,
            ToolType::Sword,
            6.0,
            6.0,
            1.6,
        ));
        items.push(tool_item(
            "Golden Pickaxe",
            ToolTier::Gold,
            ToolType::Pickaxe,
            12.0,
            2.0,
            1.2,
        ));
        items.push(tool_item(
            "Golden Axe",
            ToolTier::Gold,
            ToolType::Axe,
            12.0,
            7.0,
            1.0,
        ));
        items.push(tool_item(
            "Golden Shovel",
            ToolTier::Gold,
            ToolType::Shovel,
            12.0,
            2.5,
            1.0,
        ));
        items.push(tool_item(
            "Golden Hoe",
            ToolTier::Gold,
            ToolType::Hoe,
            12.0,
            1.0,
            1.0,
        ));
        items.push(tool_item(
            "Golden Sword",
            ToolTier::Gold,
            ToolType::Sword,
            12.0,
            4.0,
            1.6,
        ));
        items.push(tool_item(
            "Diamond Pickaxe",
            ToolTier::Diamond,
            ToolType::Pickaxe,
            8.0,
            5.0,
            1.2,
        ));
        items.push(tool_item(
            "Diamond Axe",
            ToolTier::Diamond,
            ToolType::Axe,
            8.0,
            9.0,
            1.0,
        ));
        items.push(tool_item(
            "Diamond Shovel",
            ToolTier::Diamond,
            ToolType::Shovel,
            8.0,
            5.5,
            1.0,
        ));
        items.push(tool_item(
            "Diamond Hoe",
            ToolTier::Diamond,
            ToolType::Hoe,
            8.0,
            1.0,
            4.0,
        ));
        items.push(tool_item(
            "Diamond Sword",
            ToolTier::Diamond,
            ToolType::Sword,
            8.0,
            7.0,
            1.6,
        ));
        items.push(tool_item(
            "Netherite Pickaxe",
            ToolTier::Netherite,
            ToolType::Pickaxe,
            9.0,
            6.0,
            1.2,
        ));
        items.push(tool_item(
            "Netherite Axe",
            ToolTier::Netherite,
            ToolType::Axe,
            9.0,
            10.0,
            1.0,
        ));
        items.push(tool_item(
            "Netherite Shovel",
            ToolTier::Netherite,
            ToolType::Shovel,
            9.0,
            6.5,
            1.0,
        ));
        items.push(tool_item(
            "Netherite Hoe",
            ToolTier::Netherite,
            ToolType::Hoe,
            9.0,
            1.0,
            4.0,
        ));
        items.push(tool_item(
            "Netherite Sword",
            ToolTier::Netherite,
            ToolType::Sword,
            9.0,
            8.0,
            1.6,
        ));

        items.push(food_item("Apple", 4.0, 0.3));
        items.push(food_item("Bread", 5.0, 0.6));
        items.push(food_item("Cooked Porkchop", 8.0, 0.8));
        items.push(food_item("Cooked Beef", 8.0, 0.8));
        items.push(food_item("Golden Apple", 4.0, 1.2));
        items.push(food_item("Carrot", 3.0, 0.6));
        items.push(food_item("Potato", 1.0, 0.3));
        items.push(food_item("Baked Potato", 5.0, 0.6));
        items.push(food_item("Cooked Chicken", 6.0, 0.6));
        items.push(food_item("Cooked Cod", 5.0, 0.6));
        items.push(food_item("Cooked Salmon", 6.0, 0.8));
        items.push(food_item("Cookie", 2.0, 0.1));
        items.push(food_item("Melon Slice", 2.0, 0.3));
        items.push(food_item("Pumpkin Pie", 8.0, 0.3));
        items.push(food_item("Steak", 8.0, 0.8));
        items.push(food_item("Mushroom Stew", 6.0, 0.6));
        items.push(food_item("Beetroot Soup", 6.0, 0.6));
        items.push(food_item("Sweet Berries", 2.0, 0.3));
        items.push(food_item("Glow Berries", 2.0, 0.3));
        items.push(food_item("Enchanted Apple", 4.0, 1.2));
        items.push(food_item("Dried Kelp", 1.0, 0.3));
        items.push(food_item("Chorus Fruit", 4.0, 0.3));

        items.push(block_item("Stick"));
        items.push(block_item("String"));
        items.push(block_item("Feather"));
        items.push(block_item("Gunpowder"));
        items.push(block_item("Flint"));
        items.push(block_item("Leather"));
        items.push(block_item("Iron Ingot"));
        items.push(block_item("Gold Ingot"));
        items.push(block_item("Diamond"));
        items.push(block_item("Emerald"));
        items.push(block_item("Netherite Ingot"));
        items.push(block_item("Coal"));
        items.push(block_item("Raw Iron"));
        items.push(block_item("Raw Gold"));
        items.push(block_item("Raw Copper"));
        items.push(block_item("Redstone Dust"));
        items.push(block_item("Lapis Lazuli"));
        items.push(block_item("Bone Meal"));
        items.push(block_item("Sugar"));
        items.push(block_item("Paper"));
        items.push(block_item("Book"));
        items.push(block_item("Bowl"));
        items.push(block_item("Wheat"));

        items.push(armor_item("Leather Helmet", ToolTier::Leather, 1.0, 0.0));
        items.push(armor_item(
            "Leather Chestplate",
            ToolTier::Leather,
            3.0,
            0.0,
        ));
        items.push(armor_item("Leather Leggings", ToolTier::Leather, 2.0, 0.0));
        items.push(armor_item("Leather Boots", ToolTier::Leather, 1.0, 0.0));
        items.push(armor_item("Chain Helmet", ToolTier::Chain, 2.0, 0.0));
        items.push(armor_item("Chain Chestplate", ToolTier::Chain, 5.0, 0.0));
        items.push(armor_item("Chain Leggings", ToolTier::Chain, 4.0, 0.0));
        items.push(armor_item("Chain Boots", ToolTier::Chain, 1.0, 0.0));
        items.push(armor_item("Iron Helmet", ToolTier::Iron, 2.0, 0.0));
        items.push(armor_item("Iron Chestplate", ToolTier::Iron, 6.0, 0.0));
        items.push(armor_item("Iron Leggings", ToolTier::Iron, 5.0, 0.0));
        items.push(armor_item("Iron Boots", ToolTier::Iron, 2.0, 0.0));
        items.push(armor_item("Diamond Helmet", ToolTier::Diamond, 3.0, 2.0));
        items.push(armor_item(
            "Diamond Chestplate",
            ToolTier::Diamond,
            8.0,
            2.0,
        ));
        items.push(armor_item("Diamond Leggings", ToolTier::Diamond, 6.0, 2.0));
        items.push(armor_item("Diamond Boots", ToolTier::Diamond, 3.0, 2.0));
        items.push(armor_item(
            "Netherite Helmet",
            ToolTier::Netherite,
            3.0,
            3.0,
        ));
        items.push(armor_item(
            "Netherite Chestplate",
            ToolTier::Netherite,
            8.0,
            3.0,
        ));
        items.push(armor_item(
            "Netherite Leggings",
            ToolTier::Netherite,
            6.0,
            3.0,
        ));
        items.push(armor_item("Netherite Boots", ToolTier::Netherite, 3.0, 3.0));

        items.push(block_item("Shield"));
        items.push(block_item("Flint and Steel"));
        items.push(block_item("Shears"));
        items.push(block_item("Compass"));
        items.push(block_item("Clock"));

        let mut block_to_item = vec![AIR; BLOCK_ID_COUNT];
        let required_len = BLOCK_ITEM_BASE as usize + BLOCK_ID_COUNT;
        while items.len() < required_len {
            items.push(block_item("Block"));
        }
        let mut item_to_block = vec![None; items.len()];
        for raw_id in 0..BLOCK_ID_COUNT as u16 {
            let block = BlockId::from_repr(raw_id).expect("block ID range must remain contiguous");
            let item_id = if raw_id <= 74 { raw_id } else { BLOCK_ITEM_BASE + raw_id };
            block_to_item[raw_id as usize] = item_id;
            item_to_block[item_id as usize] = Some(block);
        }

        ItemRegistry { items, block_to_item, item_to_block }
    }

    pub fn name(&self, id: ItemId) -> &'static str {
        self.items.get(id as usize).map(|d| d.name).unwrap_or("?")
    }

    pub fn id_by_name(&self, name: &str) -> Option<ItemId> {
        self.items.iter().position(|definition| definition.name == name).map(|index| index as ItemId)
    }

    pub fn def(&self, id: ItemId) -> &ItemDef {
        static DEFAULT: ItemDef = ItemDef {
            name: "?",
            max_stack: 0,
            max_damage: 0,
            food_value: 0.0,
            saturation_ratio: 0.0,
            tool_tier: ToolTier::None,
            tool_type: ToolType::None,
            mining_speed: 0.0,
            attack_damage: 0.0,
            attack_speed: 0.0,
            armor_points: 0.0,
            armor_toughness: 0.0,
        };
        match self.items.get(id as usize) {
            Some(d) => d,
            None => {
                log::warn!("Out-of-bounds item ID: {}", id);
                &DEFAULT
            }
        }
    }

    pub fn is_food(&self, id: ItemId) -> bool {
        self.def(id).food_value > 0.0
    }

    pub fn is_tool(&self, id: ItemId) -> bool {
        self.def(id).tool_type != ToolType::None
    }

    pub fn is_armor(&self, id: ItemId) -> bool {
        self.def(id).armor_points > 0.0
    }

    /// Returns the canonical armor slot: helmet, chestplate, leggings, boots.
    pub fn armor_slot(&self, id: ItemId) -> Option<usize> {
        if !self.is_armor(id) {
            return None;
        }
        let name = self.name(id);
        if name.ends_with("Helmet") {
            Some(0)
        } else if name.ends_with("Chestplate") {
            Some(1)
        } else if name.ends_with("Leggings") {
            Some(2)
        } else if name.ends_with("Boots") {
            Some(3)
        } else {
            None
        }
    }

    pub fn is_valid(&self, id: ItemId) -> bool {
        id != AIR && self.items.get(id as usize).is_some_and(|definition| definition.max_stack > 0)
    }

    pub fn item_id_from_block(&self, block: BlockId) -> ItemId {
        self.block_to_item[block as usize]
    }

    pub fn block_from_item(&self, item: ItemId) -> Option<BlockId> {
        self.item_to_block
            .get(item as usize)
            .copied()
            .flatten()
            .or_else(|| {
                // Pre-registry saves used the raw block discriminant as the
                // item ID. Only accept the old high-ID range, which cannot
                // overlap the real legacy tool and food entries.
                if item >= 256 && item < BLOCK_ITEM_BASE {
                    BlockId::from_repr(item)
                } else {
                    None
                }
            })
    }

    pub fn is_golden_apple(&self, id: ItemId) -> bool {
        id == GOLDEN_APPLE || id == ENCHANTED_APPLE
    }
}
