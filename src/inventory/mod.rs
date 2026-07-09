#[allow(dead_code)]
#[derive(Clone)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
    pub hotbar_start: usize,
    pub selected: usize,
}

#[derive(Clone)]
pub struct ItemStack {
    pub item_id: u16,
    pub count: u8,
}

impl Inventory {
    pub fn new() -> Self {
        Inventory {
            slots: vec![None; 41],
            hotbar_start: 0,
            selected: 0,
        }
    }
}
