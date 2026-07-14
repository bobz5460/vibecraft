//! Pure survival-progression rules shared by UI/input and fixed simulation.

use super::item::{ItemId, ItemRegistry, ToolType};
use super::{ItemStack, SlotContainer, EMPTY_STACK};
use crate::world::block::BlockId;
use crate::world::block_registry::registry;
use crate::world::dropped_item::{block_drop_quantity, map_drop};

pub const PLAYER_CRAFTING_SLOTS: usize = 4;
pub const TABLE_CRAFTING_SLOTS: usize = 9;
pub const FURNACE_INPUT: usize = 0;
pub const FURNACE_FUEL: usize = 1;
pub const FURNACE_OUTPUT: usize = 2;
pub const FURNACE_COOK_TICKS: u16 = 200;

#[derive(Clone, Debug)]
pub struct CraftingGrid {
    pub slots: SlotContainer,
    pub width: usize,
}

impl CraftingGrid {
    pub fn player() -> Self {
        Self { slots: SlotContainer::new(PLAYER_CRAFTING_SLOTS), width: 2 }
    }

    pub fn table() -> Self {
        Self { slots: SlotContainer::new(TABLE_CRAFTING_SLOTS), width: 3 }
    }

    pub fn result(&self, items: &ItemRegistry) -> ItemStack {
        let planks = items.item_id_from_block(BlockId::OakPlanks);
        let crafting_table = items.item_id_from_block(BlockId::CraftingTable);
        let stick = items.id_by_name("Stick");
        let all_planks = [Some(planks), Some(planks), Some(planks), Some(planks)];
        if self.width == 2 && matches_pattern(&self.slots.slots, self.width, &[&all_planks[..2], &all_planks[2..]]) {
            return ItemStack::new(crafting_table, 1);
        }
        if let Some(stick) = stick {
            let empty = None;
            for column in 0..self.width {
                let mut pattern = vec![vec![empty; self.width]; 2];
                pattern[0][column] = Some(planks);
                pattern[1][column] = Some(planks);
                let rows: Vec<&[Option<ItemId>]> = pattern.iter().map(Vec::as_slice).collect();
                if matches_pattern(&self.slots.slots, self.width, &rows) {
                    return ItemStack::new(stick, 4);
                }
            }
        }
        for (material, prefix) in [
            (planks, "Wooden"),
            (items.item_id_from_block(BlockId::Cobblestone), "Stone"),
        ] {
            let Some(stick) = stick else { break };
            // Sword and shovel share the same handle placement but differ in the
            // head shape; check the sword before the shovel and use explicit rows.
            let recipes = [
                (vec![vec![Some(material), Some(material), Some(material)], vec![None, Some(stick), None], vec![None, Some(stick), None]], "Pickaxe"),
                (vec![vec![Some(material), Some(material), None], vec![Some(material), Some(stick), None], vec![None, Some(stick), None]], "Axe"),
                (vec![vec![Some(material), None, None], vec![None, Some(stick), None], vec![None, Some(stick), None]], "Shovel"),
                (vec![vec![Some(material), None, None], vec![Some(material), None, None], vec![None, Some(stick), None]], "Sword"),
                (vec![vec![Some(material), Some(material), None], vec![None, Some(stick), None], vec![None, Some(stick), None]], "Hoe"),
            ];
            for (pattern, suffix) in recipes {
                let rows: Vec<&[Option<ItemId>]> = pattern.iter().map(Vec::as_slice).collect();
                if self.width == 3 && matches_pattern(&self.slots.slots, self.width, &rows) {
                    if let Some(id) = items.id_by_name(&format!("{prefix} {suffix}")) {
                        return ItemStack::new(id, 1);
                    }
                }
            }
        }
        EMPTY_STACK
    }

    /// Consumes exactly the ingredients for the current output. The caller can
    /// insert the output atomically and must leave the grid untouched on failure.
    pub fn take_result(&mut self, items: &ItemRegistry) -> ItemStack {
        let result = self.result(items);
        if result.is_empty() {
            return result;
        }
        for slot in &mut self.slots.slots {
            if !slot.is_empty() {
                slot.count -= 1;
                if slot.count == 0 {
                    *slot = EMPTY_STACK;
                }
            }
        }
        result
    }
}

fn matches_pattern(slots: &[ItemStack], width: usize, pattern: &[&[Option<ItemId>]]) -> bool {
    if pattern.is_empty() || pattern.iter().any(|row| row.is_empty() || row.len() > width) {
        return false;
    }
    let height = slots.len() / width;
    if height == 0 || pattern.len() > height {
        return false;
    }
    for row_offset in 0..=(height - pattern.len()) {
        for column_offset in 0..=(width - pattern[0].len()) {
            let mut matches = true;
            for row in 0..height {
                for column in 0..width {
                    let expected = if row >= row_offset
                        && row < row_offset + pattern.len()
                        && column >= column_offset
                        && column < column_offset + pattern[0].len()
                    {
                        pattern[row - row_offset][column - column_offset]
                    } else {
                        None
                    };
                    let actual = &slots[row * width + column];
                    matches &= match expected {
                        Some(id) => actual.id == id && !actual.is_empty(),
                        None => actual.is_empty(),
                    };
                }
            }
            if matches {
                return true;
            }
        }
    }
    false
}

#[derive(Clone, Debug)]
pub struct FurnaceState {
    pub slots: SlotContainer,
    pub burn_time: u16,
    pub burn_total: u16,
    pub cook_time: u16,
}

impl FurnaceState {
    pub fn new() -> Self {
        Self {
            slots: SlotContainer::new(3),
            burn_time: 0,
            burn_total: 0,
            cook_time: 0,
        }
    }

    pub fn lit(&self) -> bool {
        self.burn_time > 0
    }

    /// Advances one fixed 20 TPS tick. Returns an output stack when a recipe
    /// completes, so the owner can trigger advancement/experience feedback.
    pub fn tick(&mut self, items: &ItemRegistry) -> Option<ItemStack> {
        if self.burn_time > 0 {
            self.burn_time -= 1;
        }
        let output = furnace_output(self.slots.slots[FURNACE_INPUT].id, items)?;
        if !can_accept_output(&self.slots.slots[FURNACE_OUTPUT], &output, items) {
            self.cook_time = 0;
            return None;
        }
        if self.burn_time == 0 {
            let fuel_ticks = fuel_ticks(self.slots.slots[FURNACE_FUEL].id, items);
            if fuel_ticks == 0 {
                self.cook_time = 0;
                return None;
            }
            consume_one(&mut self.slots.slots[FURNACE_FUEL]);
            self.burn_time = fuel_ticks;
            self.burn_total = fuel_ticks;
        }
        self.cook_time += 1;
        if self.cook_time < FURNACE_COOK_TICKS {
            return None;
        }
        self.cook_time = 0;
        consume_one(&mut self.slots.slots[FURNACE_INPUT]);
        let output_slot = &mut self.slots.slots[FURNACE_OUTPUT];
        if output_slot.is_empty() {
            *output_slot = output.clone();
        } else {
            output_slot.count += output.count;
        }
        Some(output)
    }
}

fn consume_one(stack: &mut ItemStack) {
    if stack.is_empty() {
        return;
    }
    stack.count -= 1;
    if stack.count == 0 {
        *stack = EMPTY_STACK;
    }
}

fn can_accept_output(existing: &ItemStack, output: &ItemStack, items: &ItemRegistry) -> bool {
    existing.is_empty()
        || (existing.can_merge_with(output)
            && existing.count.saturating_add(output.count) <= existing.max_stack(items) as u16)
}

pub fn fuel_ticks(item: ItemId, items: &ItemRegistry) -> u16 {
    if item == items.item_id_from_block(BlockId::OakLog) || item == items.item_id_from_block(BlockId::OakPlanks) {
        300
    } else if items.name(item) == "Coal" {
        1_600
    } else {
        0
    }
}

pub fn furnace_output(input: ItemId, items: &ItemRegistry) -> Option<ItemStack> {
    let output_name = match items.block_from_item(input) {
        Some(BlockId::IronOre | BlockId::DeepslateIronOre) => "Iron Ingot",
        Some(BlockId::GoldOre | BlockId::DeepslateGoldOre) => "Gold Ingot",
        Some(BlockId::CopperOre | BlockId::DeepslateCopperOre) => "Raw Copper",
        Some(BlockId::Sand) => items.name(items.item_id_from_block(BlockId::Glass)),
        _ => return None,
    };
    let output = if output_name == items.name(items.item_id_from_block(BlockId::Glass)) {
        items.item_id_from_block(BlockId::Glass)
    } else {
        items.id_by_name(output_name)?
    };
    Some(ItemStack::new(output, 1))
}

#[derive(Clone, Debug, PartialEq)]
pub struct MiningOutcome {
    pub break_seconds: f32,
    pub harvestable: bool,
    pub drop: ItemStack,
    pub experience: u32,
    pub damages_tool: bool,
}

pub fn mining_outcome(block: BlockId, tool: &ItemStack, items: &ItemRegistry) -> MiningOutcome {
    let definition = registry().definition(block);
    let item = items.def(tool.id);
    let required_tool = match block {
        BlockId::Stone | BlockId::Cobblestone | BlockId::Deepslate | BlockId::IronOre | BlockId::GoldOre
        | BlockId::CoalOre | BlockId::DiamondOre | BlockId::EmeraldOre | BlockId::LapisOre
        | BlockId::RedstoneOre | BlockId::Obsidian => ToolType::Pickaxe,
        BlockId::Dirt | BlockId::Sand | BlockId::Gravel | BlockId::Snow | BlockId::SnowBlock => ToolType::Shovel,
        BlockId::OakLog | BlockId::OakPlanks | BlockId::CraftingTable | BlockId::Chest => ToolType::Axe,
        _ => ToolType::None,
    };
    let tier_needed = match block {
        BlockId::DiamondOre | BlockId::EmeraldOre | BlockId::GoldOre | BlockId::RedstoneOre | BlockId::Obsidian => 3,
        BlockId::IronOre | BlockId::LapisOre => 2,
        BlockId::CoalOre | BlockId::Deepslate => 1,
        _ => 0,
    };
    let correct_tool = required_tool == ToolType::None || item.tool_type == required_tool;
    let harvestable = tier_needed == 0 || (correct_tool && item.tool_tier.level() >= tier_needed);
    let speed = if correct_tool { item.mining_speed.max(1.0) * item.tool_tier.multiplier() } else { 1.0 };
    let divisor = if harvestable { 30.0 } else { 100.0 };
    let break_seconds = if definition.hardness <= 0.0 {
        f32::INFINITY
    } else {
        // Mining progress advances once per simulation tick at 20 TPS.
        definition.hardness * divisor / (speed * 20.0)
    };
    let drop = if harvestable {
        let drop_block = map_drop(definition.drop);
        ItemStack::new(items.item_id_from_block(drop_block), block_drop_quantity(block).min(u16::MAX as u32) as u16)
    } else {
        EMPTY_STACK
    };
    let experience = if harvestable {
        match block {
            BlockId::CoalOre | BlockId::DeepslateCoalOre => 2,
            BlockId::DiamondOre | BlockId::DeepslateDiamondOre | BlockId::EmeraldOre | BlockId::DeepslateEmeraldOre => 5,
            BlockId::LapisOre | BlockId::DeepslateLapisOre | BlockId::RedstoneOre | BlockId::DeepslateRedstoneOre => 3,
            _ => 0,
        }
    } else { 0 };
    MiningOutcome { break_seconds, harvestable, drop, experience, damages_tool: correct_tool && item.max_damage > 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_grid_crafts_table_and_consumes_exactly_one_per_slot() {
        let items = ItemRegistry::new();
        let planks = items.item_id_from_block(BlockId::OakPlanks);
        let mut grid = CraftingGrid::player();
        for slot in &mut grid.slots.slots {
            *slot = ItemStack::new(planks, 2);
        }
        assert_eq!(grid.take_result(&items), ItemStack::new(items.item_id_from_block(BlockId::CraftingTable), 1));
        assert!(grid.slots.slots.iter().all(|stack| stack.count == 1));
    }

    #[test]
    fn furnace_consumes_fuel_and_emits_recipe_output() {
        let items = ItemRegistry::new();
        let mut furnace = FurnaceState::new();
        furnace.slots.slots[FURNACE_INPUT] = ItemStack::new(items.item_id_from_block(BlockId::IronOre), 1);
        furnace.slots.slots[FURNACE_FUEL] = ItemStack::new(items.item_id_from_block(BlockId::OakPlanks), 1);
        for _ in 0..FURNACE_COOK_TICKS {
            furnace.tick(&items);
        }
        assert_eq!(items.name(furnace.slots.slots[FURNACE_OUTPUT].id), "Iron Ingot");
    }

    #[test]
    fn high_block_item_ids_do_not_alias_tools() {
        let items = ItemRegistry::new();
        let block_item = items.item_id_from_block(BlockId::OakDoor);
        assert!(block_item >= super::super::item::BLOCK_ITEM_BASE);
        assert_eq!(items.block_from_item(block_item), Some(BlockId::OakDoor));
        assert_eq!(items.block_from_item(BlockId::OakDoor as u16), Some(BlockId::OakDoor));
    }

    #[test]
    fn table_recipes_require_a_shaped_pattern() {
        let items = ItemRegistry::new();
        let planks = items.item_id_from_block(BlockId::OakPlanks);
        let sticks = items.id_by_name("Stick").unwrap();
        let pickaxe = items.id_by_name("Wooden Pickaxe").unwrap();
        let mut grid = CraftingGrid::table();
        grid.slots.slots[0] = ItemStack::new(planks, 1);
        grid.slots.slots[1] = ItemStack::new(planks, 1);
        grid.slots.slots[2] = ItemStack::new(planks, 1);
        grid.slots.slots[4] = ItemStack::new(sticks, 1);
        grid.slots.slots[7] = ItemStack::new(sticks, 1);
        assert_eq!(grid.result(&items), ItemStack::new(pickaxe, 1));
        grid.slots.slots[3] = ItemStack::new(planks, 1);
        assert!(grid.result(&items).is_empty());
    }

    #[test]
    fn mining_uses_harvest_drop_mapping_and_quantity() {
        let items = ItemRegistry::new();
        let outcome = mining_outcome(BlockId::Stone, &EMPTY_STACK, &items);
        assert_eq!(outcome.drop.id, items.item_id_from_block(BlockId::Cobblestone));
        assert_eq!(outcome.drop.count, 1);
        let pickaxe = ItemStack::new(items.id_by_name("Wooden Pickaxe").unwrap(), 1);
        let ore = mining_outcome(BlockId::CoalOre, &pickaxe, &items);
        assert_eq!(ore.drop.count, 3);
    }
}
