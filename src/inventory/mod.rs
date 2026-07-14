pub mod item;
pub mod progression;

use item::{ItemId, ItemRegistry, AIR};

pub const HOTBAR_SLOTS: usize = 9;
pub const MAIN_SLOTS: usize = 27;
pub const ARMOR_SLOTS: usize = 4;
pub const OFFHAND_SLOT: usize = 1;
pub const TOTAL_SLOTS: usize = HOTBAR_SLOTS + MAIN_SLOTS + ARMOR_SLOTS + OFFHAND_SLOT;

pub const HOTBAR_START: usize = 0;
pub const MAIN_START: usize = HOTBAR_SLOTS;
pub const ARMOR_START: usize = HOTBAR_SLOTS + MAIN_SLOTS;
pub const OFFHAND_INDEX: usize = ARMOR_START + ARMOR_SLOTS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub id: ItemId,
    pub count: u16,
    /// Damage is per-stack because damageable items never stack.
    pub damage: u16,
}

impl ItemStack {
    pub fn new(id: ItemId, count: u16) -> Self {
        ItemStack {
            id,
            count,
            damage: 0,
        }
    }

    pub fn with_damage(id: ItemId, count: u16, damage: u16) -> Self {
        Self { id, count, damage }
    }

    pub fn is_empty(&self) -> bool {
        self.id == AIR || self.count == 0
    }

    pub fn max_stack(&self, registry: &ItemRegistry) -> u8 {
        if self.is_empty() {
            return 0;
        }
        registry.def(self.id).max_stack
    }

    pub fn can_merge_with(&self, other: &Self) -> bool {
        !self.is_empty() && self.id == other.id && self.damage == other.damage
    }

    pub fn normalize(&mut self, registry: &ItemRegistry) {
        if self.id == AIR || self.count == 0 || !registry.is_valid(self.id) {
            *self = EMPTY_STACK;
            return;
        }
        let definition = registry.def(self.id);
        self.count = self.count.min(definition.max_stack as u16);
        if definition.max_damage == 0 {
            self.damage = 0;
        } else if self.damage >= definition.max_damage {
            *self = EMPTY_STACK;
        }
    }

    /// Applies one point of durability damage and returns whether the item broke.
    pub fn damage_once(&mut self, registry: &ItemRegistry) -> bool {
        let max_damage = registry.def(self.id).max_damage;
        if max_damage == 0 || self.is_empty() {
            return false;
        }
        self.damage = self.damage.saturating_add(1);
        if self.damage >= max_damage {
            *self = EMPTY_STACK;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
pub struct Inventory {
    pub slots: Vec<ItemStack>,
    pub held_slot: usize,
}

pub const EMPTY_STACK: ItemStack = ItemStack { id: AIR, count: 0, damage: 0 };

/// General fixed-size stack storage used by crafting grids and block entities.
#[derive(Clone, Debug)]
pub struct SlotContainer {
    pub slots: Vec<ItemStack>,
}

impl SlotContainer {
    pub fn new(slot_count: usize) -> Self {
        Self { slots: vec![EMPTY_STACK; slot_count] }
    }

    pub fn insert(&mut self, mut stack: ItemStack, registry: &ItemRegistry) -> ItemStack {
        stack.normalize(registry);
        if stack.is_empty() {
            return stack;
        }
        for slot in &mut self.slots {
            if slot.can_merge_with(&stack) {
                let space = slot.max_stack(registry) as u16 - slot.count;
                let moved = space.min(stack.count);
                slot.count += moved;
                stack.count -= moved;
                if stack.count == 0 {
                    return EMPTY_STACK;
                }
            }
        }
        for slot in &mut self.slots {
            if slot.is_empty() {
                let moved = stack.count.min(stack.max_stack(registry) as u16);
                *slot = ItemStack::with_damage(stack.id, moved, stack.damage);
                stack.count -= moved;
                if stack.count == 0 {
                    return EMPTY_STACK;
                }
            }
        }
        stack
    }

    pub fn take(&mut self, slot: usize, amount: u16) -> ItemStack {
        let Some(stack) = self.slots.get_mut(slot) else { return EMPTY_STACK; };
        if stack.is_empty() || amount == 0 {
            return EMPTY_STACK;
        }
        let count = amount.min(stack.count);
        let taken = ItemStack::with_damage(stack.id, count, stack.damage);
        stack.count -= count;
        if stack.count == 0 {
            *stack = EMPTY_STACK;
        }
        taken
    }
}

impl Inventory {
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(TOTAL_SLOTS);
        for _ in 0..TOTAL_SLOTS {
            slots.push(ItemStack::new(AIR, 0));
        }
        Inventory {
            slots,
            held_slot: 0,
        }
    }

    pub fn hotbar_slot(&self, index: usize) -> &ItemStack {
        if index >= HOTBAR_SLOTS {
            return &EMPTY_STACK;
        }
        &self.slots[HOTBAR_START + index]
    }

    pub fn hotbar_slot_mut(&mut self, index: usize) -> &mut ItemStack {
        if index >= HOTBAR_SLOTS {
            return &mut self.slots[HOTBAR_START];
        }
        &mut self.slots[HOTBAR_START + index]
    }

    pub fn selected_stack(&self) -> &ItemStack {
        self.hotbar_slot(self.held_slot)
    }

    pub fn selected_id(&self) -> ItemId {
        self.selected_stack().id
    }

    pub fn add_item(&mut self, id: ItemId, count: u16, registry: &ItemRegistry) -> u16 {
        self.add_stack(ItemStack::new(id, count), registry).count
    }

    pub fn add_stack(&mut self, stack: ItemStack, registry: &ItemRegistry) -> ItemStack {
        let mut storage = SlotContainer { slots: std::mem::take(&mut self.slots) };
        let mut remainder = stack;
        // Equipment slots are intentionally excluded from automatic pickup.
        let mut player_slots = SlotContainer { slots: storage.slots.drain(..HOTBAR_SLOTS + MAIN_SLOTS).collect() };
        remainder = player_slots.insert(remainder, registry);
        storage.slots.splice(0..0, player_slots.slots);
        self.slots = storage.slots;
        remainder
    }

    pub fn remove_from_hotbar(&mut self, slot: usize, count: u16) -> ItemStack {
        if slot >= HOTBAR_SLOTS {
            return ItemStack::new(AIR, 0);
        }
        let idx = HOTBAR_START + slot;
        let stack = &self.slots[idx];
        if stack.is_empty() {
            return ItemStack::new(AIR, 0);
        }
        let remove = count.min(stack.count);
        let result = ItemStack::with_damage(stack.id, remove, stack.damage);
        self.slots[idx].count -= remove;
        if self.slots[idx].count == 0 {
            self.slots[idx] = EMPTY_STACK;
        }
        result
    }

    pub fn drop_selected(&mut self) -> ItemStack {
        self.remove_from_hotbar(self.held_slot, 1)
    }

    pub fn has_item(&self, id: ItemId) -> bool {
        self.slots.iter().any(|s| s.id == id && s.count > 0)
    }

    pub fn count_item(&self, id: ItemId) -> u32 {
        self.slots
            .iter()
            .filter(|s| s.id == id)
            .map(|s| s.count as u32)
            .sum()
    }

    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = EMPTY_STACK;
        }
    }

    pub fn armor_stats(&self, registry: &ItemRegistry) -> (f32, f32) {
        self.slots[ARMOR_START..OFFHAND_INDEX].iter().fold((0.0, 0.0), |(points, toughness), stack| {
            let definition = registry.def(stack.id);
            (points + definition.armor_points, toughness + definition.armor_toughness)
        })
    }

    /// Equips armor from the selected hotbar slot, returning the displaced item
    /// to that slot. Slot-specific armor models are deferred, but equipment
    /// still has one authoritative, persisted home and affects protection.
    pub fn equip_selected_armor(&mut self, registry: &ItemRegistry) -> bool {
        let selected = HOTBAR_START + self.held_slot.min(HOTBAR_SLOTS - 1);
        let Some(armor_slot) = registry.armor_slot(self.slots[selected].id) else {
            return false;
        };
        let target = ARMOR_START + armor_slot;
        self.slots.swap(selected, target);
        true
    }

    pub fn can_place_in_slot(&self, slot: usize, stack: &ItemStack, registry: &ItemRegistry) -> bool {
        if stack.is_empty() {
            return true;
        }
        if (ARMOR_START..OFFHAND_INDEX).contains(&slot) {
            return registry.armor_slot(stack.id) == Some(slot - ARMOR_START);
        }
        slot != OFFHAND_INDEX || !registry.is_armor(stack.id)
    }

    pub fn move_selected_to_offhand(&mut self) {
        let selected = HOTBAR_START + self.held_slot.min(HOTBAR_SLOTS - 1);
        self.slots.swap(selected, OFFHAND_INDEX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_item_fills_existing_then_empty_slots() {
        let registry = ItemRegistry::new();
        let mut inventory = Inventory::new();
        inventory.slots[0] = ItemStack::new(1, 63);
        assert_eq!(inventory.add_item(1, 2, &registry), 0);
        assert_eq!(inventory.slots[0], ItemStack::new(1, 64));
        assert_eq!(inventory.slots[1], ItemStack::new(1, 1));
    }

    #[test]
    fn dropping_selected_stack_removes_one_item() {
        let mut inventory = Inventory::new();
        inventory.slots[0] = ItemStack::new(1, 1);
        assert_eq!(inventory.drop_selected(), ItemStack::new(1, 1));
        assert!(inventory.selected_stack().is_empty());
    }

    #[test]
    fn equipping_armor_moves_it_out_of_the_hotbar() {
        let registry = ItemRegistry::new();
        let armor = registry.id_by_name("Iron Helmet").unwrap();
        let mut inventory = Inventory::new();
        inventory.slots[0] = ItemStack::new(armor, 1);
        assert!(inventory.equip_selected_armor(&registry));
        assert!(inventory.slots[0].is_empty());
        assert_eq!(inventory.slots[ARMOR_START].id, armor);
    }

    #[test]
    fn armor_equips_into_matching_slot_and_rejects_wrong_slot() {
        let registry = ItemRegistry::new();
        let helmet = registry.id_by_name("Iron Helmet").unwrap();
        let boots = registry.id_by_name("Iron Boots").unwrap();
        let inventory = Inventory::new();
        assert!(inventory.can_place_in_slot(ARMOR_START, &ItemStack::new(helmet, 1), &registry));
        assert!(!inventory.can_place_in_slot(ARMOR_START + 3, &ItemStack::new(helmet, 1), &registry));
        assert!(inventory.can_place_in_slot(ARMOR_START + 3, &ItemStack::new(boots, 1), &registry));
    }
}
