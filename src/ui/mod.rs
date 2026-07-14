use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiScreen {
    Playing,
    Inventory,
    Pause,
    Options,
    Controls,
    Accessibility,
    Connect,
    Title,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    None,
    Resume,
    Quit,
    ToggleGraphics,
    DecreaseRenderDistance,
    IncreaseRenderDistance,
    ToggleGuiScale,
    ToggleHighContrast,
    ToggleReducedMotion,
    OpenConnect,
    ConnectServer,
}

#[derive(Clone, Debug)]
pub struct UiSlot {
    pub name: String,
    pub sprite: String,
    pub count: u16,
    pub empty: bool,
    pub selected: bool,
    pub hint: u32,
}

#[derive(Clone, Debug)]
pub enum UiCommand {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    Text {
        x: f32,
        y: f32,
        size: f32,
        text: String,
        color: [f32; 4],
    },
    Sprite {
        name: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    Item {
        x: f32,
        y: f32,
        size: f32,
        name: String,
        sprite: String,
        count: u16,
        hint: u32,
    },
}

#[derive(Clone, Debug, Default)]
pub struct UiFrame {
    pub commands: Vec<UiCommand>,
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub screen: UiScreen,
    pub selected: usize,
    pub gui_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub chat_opacity: f32,
    pub render_distance: i32,
    pub graphics_vibrant: bool,
    pub server_address: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            screen: UiScreen::Playing,
            selected: 0,
            gui_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            chat_opacity: 0.72,
            render_distance: 6,
            graphics_vibrant: false,
            server_address: "127.0.0.1:25565".to_string(),
        }
    }
}

impl UiState {
    pub fn new(render_distance: i32, graphics_vibrant: bool) -> Self {
        Self {
            render_distance,
            graphics_vibrant,
            ..Self::default()
        }
    }

    pub fn open_inventory(&mut self) {
        self.screen = UiScreen::Inventory;
        self.selected = 0;
    }

    pub fn close_to_gameplay(&mut self) {
        self.screen = UiScreen::Playing;
        self.selected = 0;
    }

    pub fn open_pause(&mut self) {
        if self.screen == UiScreen::Playing {
            self.screen = UiScreen::Pause;
            self.selected = 0;
        }
    }

    pub fn is_menu_open(&self) -> bool {
        !matches!(self.screen, UiScreen::Playing)
    }

    pub fn captures_gameplay_input(&self) -> bool {
        !matches!(self.screen, UiScreen::Playing)
    }

    pub fn handle_escape(&mut self) -> UiAction {
        match self.screen {
            UiScreen::Playing => self.open_pause(),
            UiScreen::Inventory | UiScreen::Pause => self.close_to_gameplay(),
            UiScreen::Options | UiScreen::Controls | UiScreen::Accessibility => {
                self.screen = UiScreen::Pause;
                self.selected = 0;
            }
            UiScreen::Connect => {
                self.screen = UiScreen::Pause;
                self.selected = 0;
            }
            UiScreen::Title => return UiAction::Quit,
        }
        UiAction::None
    }

    pub fn move_focus(&mut self, direction: i32) {
        let count = self.button_count().max(1);
        self.selected = (self.selected as i32 + direction).rem_euclid(count as i32) as usize;
    }

    pub fn activate_focused(&mut self) -> UiAction {
        self.activate(self.selected)
    }

    pub fn click(&mut self, width: f32, height: f32, x: f32, y: f32) -> UiAction {
        self.button_rects(width, height)
            .iter()
            .enumerate()
            .find(|(_, rect)| contains(**rect, x, y))
            .map(|(index, _)| {
                self.selected = index;
                self.activate(index)
            })
            .unwrap_or(UiAction::None)
    }

    fn button_count(&self) -> usize {
        match self.screen {
            UiScreen::Pause => 6,
            UiScreen::Options => 6,
            UiScreen::Controls => 1,
            UiScreen::Accessibility => 4,
            UiScreen::Connect => 2,
            UiScreen::Title => 4,
            _ => 0,
        }
    }

    fn activate(&mut self, index: usize) -> UiAction {
        match self.screen {
            UiScreen::Pause => match index {
                0 => {
                    self.close_to_gameplay();
                    UiAction::Resume
                }
                1 => {
                    self.screen = UiScreen::Connect;
                    self.selected = 0;
                    UiAction::OpenConnect
                }
                2 => {
                    self.screen = UiScreen::Options;
                    self.selected = 0;
                    UiAction::None
                }
                3 => {
                    self.screen = UiScreen::Controls;
                    self.selected = 0;
                    UiAction::None
                }
                4 => {
                    self.screen = UiScreen::Accessibility;
                    self.selected = 0;
                    UiAction::None
                }
                5 => UiAction::Quit,
                _ => UiAction::None,
            },
            UiScreen::Options => match index {
                0 => {
                    self.graphics_vibrant = !self.graphics_vibrant;
                    UiAction::ToggleGraphics
                }
                1 => {
                    self.render_distance = (self.render_distance - 1).max(2);
                    UiAction::DecreaseRenderDistance
                }
                2 => {
                    self.render_distance = (self.render_distance + 1).min(32);
                    UiAction::IncreaseRenderDistance
                }
                3 => {
                    self.gui_scale = if self.gui_scale >= 3.0 { 1.0 } else { self.gui_scale + 1.0 };
                    UiAction::ToggleGuiScale
                }
                4 => {
                    self.screen = UiScreen::Pause;
                    self.selected = 0;
                    UiAction::None
                }
                5 => {
                    self.screen = UiScreen::Accessibility;
                    self.selected = 0;
                    UiAction::None
                }
                _ => UiAction::None,
            },
            UiScreen::Accessibility => match index {
                0 => {
                    self.high_contrast = !self.high_contrast;
                    UiAction::ToggleHighContrast
                }
                1 => {
                    self.reduced_motion = !self.reduced_motion;
                    UiAction::ToggleReducedMotion
                }
                2 => {
                    self.chat_opacity = if self.chat_opacity > 0.8 { 0.55 } else { self.chat_opacity + 0.15 };
                    UiAction::None
                }
                3 => {
                    self.screen = UiScreen::Pause;
                    self.selected = 0;
                    UiAction::None
                }
                _ => UiAction::None,
            },
            UiScreen::Controls => {
                self.screen = UiScreen::Pause;
                self.selected = 0;
                UiAction::None
            }
            UiScreen::Title => match index {
                0 => {
                    self.close_to_gameplay();
                    UiAction::Resume
                }
                1 => {
                    self.screen = UiScreen::Connect;
                    self.selected = 0;
                    UiAction::OpenConnect
                }
                2 => {
                    self.screen = UiScreen::Options;
                    self.selected = 0;
                    UiAction::None
                }
                3 => UiAction::Quit,
                _ => UiAction::None,
            },
            UiScreen::Connect => match index {
                0 => UiAction::ConnectServer,
                1 => {
                    self.screen = UiScreen::Pause;
                    self.selected = 0;
                    UiAction::None
                }
                _ => UiAction::None,
            },
            _ => UiAction::None,
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> UiAction {
        match key {
            KeyCode::ArrowUp => {
                self.move_focus(-1);
                UiAction::None
            }
            KeyCode::ArrowDown => {
                self.move_focus(1);
                UiAction::None
            }
            KeyCode::Enter | KeyCode::NumpadEnter => self.activate_focused(),
            KeyCode::Escape => self.handle_escape(),
            _ => UiAction::None,
        }
    }

    pub fn append_server_address(&mut self, value: &str) {
        if self.screen == UiScreen::Connect {
            self.server_address.push_str(value);
        }
    }

    pub fn backspace_server_address(&mut self) {
        if self.screen == UiScreen::Connect {
            self.server_address.pop();
        }
    }

    fn button_rects(&self, width: f32, height: f32) -> Vec<(f32, f32, f32, f32)> {
        let button_w = (width * 0.34).clamp(240.0, 420.0);
        let button_h = 34.0 * self.gui_scale;
        let left = (width - button_w) * 0.5;
        let top = height * 0.34;
        let count = self.button_count();
        (0..count)
            .map(|index| {
                let y = if self.screen == UiScreen::Controls {
                    top + 170.0
                } else if self.screen == UiScreen::Connect {
                    top + 70.0 + index as f32 * (button_h + 8.0)
                } else {
                    top + index as f32 * (button_h + 8.0)
                };
                (left, y, button_w, button_h)
            })
            .collect()
    }

    pub fn frame(
        &self,
        width: f32,
        height: f32,
        hotbar: &[UiSlot],
        inventory: Option<&[UiSlot]>,
        crafting: Option<&[UiSlot]>,
        craft_result: Option<&UiSlot>,
        carried: Option<&UiSlot>,
        health: f32,
        hunger: f32,
        chat_lines: &[String],
        feedback: Option<&str>,
        cursor: Option<(f32, f32)>,
        show_crosshair: bool,
    ) -> UiFrame {
        let mut frame = UiFrame::default();
        match self.screen {
            UiScreen::Playing => {
                draw_hud(&mut frame, width, height, self.gui_scale, hotbar, health, hunger, show_crosshair);
                draw_chat(&mut frame, width, height, chat_lines, self.chat_opacity);
                if let Some(feedback) = feedback.filter(|text| !text.is_empty()) {
                    draw_toast(&mut frame, width, feedback);
                }
            }
            UiScreen::Inventory => {
                draw_hud(&mut frame, width, height, self.gui_scale, hotbar, health, hunger, false);
                draw_inventory(&mut frame, width, height, self.gui_scale, inventory.unwrap_or(&[]), crafting.unwrap_or(&[]), craft_result, carried, cursor, self.high_contrast);
            }
            UiScreen::Pause => draw_menu(&mut frame, width, height, self, "Paused", &["Resume", "Join Server", "Options", "Controls", "Accessibility", "Quit to desktop"]),
            UiScreen::Options => draw_options(&mut frame, width, height, self),
            UiScreen::Controls => draw_controls(&mut frame, width, height, self),
            UiScreen::Accessibility => draw_accessibility(&mut frame, width, height, self),
            UiScreen::Connect => draw_connect(&mut frame, width, height, self, feedback),
            UiScreen::Title => draw_menu(&mut frame, width, height, self, "Vibecraft", &["Continue", "Join Server", "Options", "Quit"]),
        }
        frame
    }
}

pub fn inventory_slot_at(width: f32, height: f32, scale: f32, x: f32, y: f32) -> Option<usize> {
    let slot = 18.0 * scale;
    let left = (width - 256.0 * scale) * 0.5;
    let top = (height - 256.0 * scale) * 0.5;
    for index in 0..36 {
        let (grid_x, grid_y) = if index < 9 {
            (7.0 + index as f32 * 18.0, 141.0)
        } else {
            (7.0 + (index - 9) as f32 % 9.0 * 18.0, 83.0 + (index - 9) as f32 / 9.0 * 18.0)
        };
        let sx = left + grid_x * scale;
        let sy = top + grid_y * scale;
        if contains((sx, sy, slot, slot), x, y) {
            return Some(index);
        }
    }
    for index in 36..40 {
        let sx = left + 7.0 * scale;
        let sy = top + (8.0 + (index - 36) as f32 * 18.0) * scale;
        if contains((sx, sy, slot, slot), x, y) {
            return Some(index);
        }
    }
    let offhand_x = left + 77.0 * scale;
    let offhand_y = top + 61.0 * scale;
    if contains((offhand_x, offhand_y, slot, slot), x, y) {
        return Some(40);
    }
    None
}

pub fn player_crafting_slot_at(width: f32, height: f32, scale: f32, x: f32, y: f32) -> Option<usize> {
    let slot = 18.0 * scale;
    let left = (width - 256.0 * scale) * 0.5;
    let top = (height - 256.0 * scale) * 0.5;
    for index in 0..4 {
        let sx = left + (89.0 + index as f32 % 2.0 * 18.0) * scale;
        let sy = top + (19.0 + index as f32 / 2.0 * 18.0) * scale;
        if contains((sx, sy, slot, slot), x, y) {
            return Some(index);
        }
    }
    None
}

pub fn player_crafting_result_at(width: f32, height: f32, scale: f32, x: f32, y: f32) -> bool {
    let slot = 18.0 * scale;
    let left = (width - 256.0 * scale) * 0.5;
    let top = (height - 256.0 * scale) * 0.5;
    contains((left + 145.0 * scale, top + 28.0 * scale, slot, slot), x, y)
}

fn contains((x, y, w, h): (f32, f32, f32, f32), px: f32, py: f32) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

fn rect(frame: &mut UiFrame, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    frame.commands.push(UiCommand::Rect { x, y, w, h, color });
}

fn text(frame: &mut UiFrame, x: f32, y: f32, size: f32, value: impl Into<String>, color: [f32; 4]) {
    frame.commands.push(UiCommand::Text { x, y, size, text: value.into(), color });
}

fn panel(frame: &mut UiFrame, width: f32, height: f32, high_contrast: bool) {
    rect(frame, 0.0, 0.0, width, height, [0.0, 0.0, 0.0, if high_contrast { 0.72 } else { 0.52 }]);
}

fn draw_hud(frame: &mut UiFrame, width: f32, height: f32, scale: f32, hotbar: &[UiSlot], health: f32, hunger: f32, show_crosshair: bool) {
    let bar_w = 182.0 * scale;
    let bar_h = 22.0 * scale;
    let left = (width - bar_w) * 0.5;
    let top = height - bar_h - 8.0 * scale;
    frame.commands.push(UiCommand::Sprite {
        name: "hud/hotbar".to_string(),
        x: left,
        y: top,
        w: bar_w,
        h: bar_h,
        color: [1.0, 1.0, 1.0, 1.0],
    });
    for index in 0..9 {
        let x = left + index as f32 * 20.0 * scale - 1.0 * scale;
        if hotbar.get(index).map(|item| item.selected).unwrap_or(false) {
            frame.commands.push(UiCommand::Sprite {
                name: "hud/hotbar_selection".to_string(),
                x,
                y: top - 0.5 * scale,
                w: 24.0 * scale,
                h: 23.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
        if let Some(item) = hotbar.get(index).filter(|item| !item.empty) {
            frame.commands.push(UiCommand::Item {
                x: left + (2.0 + index as f32 * 20.0) * scale,
                y: top + 3.0 * scale,
                size: 16.0 * scale,
                name: item.name.clone(),
                sprite: item.sprite.clone(),
                count: item.count,
                hint: item.hint,
            });
        }
    }
    let status_y = top - 11.0 * scale;
    let status_left = width * 0.5 - 91.0 * scale;
    for index in 0..10 {
        let health_sprite = if health >= index as f32 * 2.0 + 2.0 {
            "hud/heart/full"
        } else if health > index as f32 * 2.0 {
            "hud/heart/half"
        } else {
            "hud/heart/container"
        };
        frame.commands.push(UiCommand::Sprite {
            name: health_sprite.to_string(),
            x: status_left + index as f32 * 9.0 * scale,
            y: status_y,
            w: 9.0 * scale,
            h: 9.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        let hunger_sprite = if hunger >= index as f32 * 2.0 + 2.0 {
            "hud/food_full"
        } else if hunger > index as f32 * 2.0 {
            "hud/food_half"
        } else {
            "hud/food_empty"
        };
        frame.commands.push(UiCommand::Sprite {
            name: hunger_sprite.to_string(),
            x: status_left + 100.0 * scale + index as f32 * 9.0 * scale,
            y: status_y,
            w: 9.0 * scale,
            h: 9.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
    if show_crosshair {
        frame.commands.push(UiCommand::Sprite {
            name: "hud/crosshair".to_string(),
            x: width * 0.5 - 7.5 * scale,
            y: height * 0.5 - 7.5 * scale,
            w: 15.0 * scale,
            h: 15.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
}

fn draw_inventory(frame: &mut UiFrame, width: f32, height: f32, scale: f32, slots: &[UiSlot], crafting: &[UiSlot], craft_result: Option<&UiSlot>, carried: Option<&UiSlot>, cursor: Option<(f32, f32)>, high_contrast: bool) {
    panel(frame, width, height, high_contrast);
    let left = (width - 256.0 * scale) * 0.5;
    let top = (height - 256.0 * scale) * 0.5;
    frame.commands.push(UiCommand::Sprite {
        name: "container/inventory".to_string(),
        x: left,
        y: top,
        w: 256.0 * scale,
        h: 256.0 * scale,
        color: [1.0, 1.0, 1.0, 1.0],
    });
    for (index, item) in crafting.iter().take(4).enumerate() {
        let x = left + (90.0 + index as f32 % 2.0 * 18.0) * scale;
        let y = top + (20.0 + index as f32 / 2.0 * 18.0) * scale;
        draw_inventory_item(frame, x, y, scale, item);
    }
    if let Some(item) = craft_result.filter(|item| !item.empty) {
        draw_inventory_item(frame, left + 146.0 * scale, top + 29.0 * scale, scale, item);
    }
    for (index, item) in slots.iter().take(36).enumerate() {
        let (item_x, item_y) = if index < 9 {
            (8.0 + index as f32 * 18.0, 142.0)
        } else {
            (8.0 + (index - 9) as f32 % 9.0 * 18.0, 84.0 + (index - 9) as f32 / 9.0 * 18.0)
        };
        draw_inventory_item(frame, left + item_x * scale, top + item_y * scale, scale, item);
    }
    for index in 36..40 {
        if let Some(item) = slots.get(index) {
            draw_inventory_item(frame, left + 8.0 * scale, top + (9.0 + (index - 36) as f32 * 18.0) * scale, scale, item);
        }
    }
    if let Some(item) = slots.get(40) {
        draw_inventory_item(frame, left + 78.0 * scale, top + 62.0 * scale, scale, item);
    }
    if let Some(item) = carried.filter(|item| !item.empty) {
        if let Some((x, y)) = cursor {
            frame.commands.push(UiCommand::Item { x: x - 8.0 * scale, y: y - 8.0 * scale, size: 16.0 * scale, name: item.name.clone(), sprite: item.sprite.clone(), count: item.count, hint: item.hint });
        }
    }
    if let Some((x, y)) = cursor {
        if let Some(index) = inventory_slot_at(width, height, scale, x, y) {
            if let Some(item) = slots.get(index).filter(|item| !item.empty) {
                let label = if item.count > 1 {
                    format!("{} x{}", item.name, item.count)
                } else {
                    item.name.clone()
                };
                let tooltip_w = (label.chars().count() as f32 * 8.0 + 18.0).min(width - 16.0);
                let tooltip_x = x.clamp(8.0, width - tooltip_w - 8.0);
                let tooltip_y = (y - 34.0 * scale).max(8.0);
                rect(frame, tooltip_x, tooltip_y, tooltip_w, 24.0, [0.02, 0.02, 0.03, 0.94]);
                text(frame, tooltip_x + 9.0, tooltip_y + 5.0, 12.0, label, [1.0, 0.92, 0.55, 1.0]);
            }
        }
    }
}

fn draw_inventory_item(frame: &mut UiFrame, x: f32, y: f32, scale: f32, item: &UiSlot) {
    if !item.empty {
        frame.commands.push(UiCommand::Item { x, y, size: 16.0 * scale, name: item.name.clone(), sprite: item.sprite.clone(), count: item.count, hint: item.hint });
    }
}

fn draw_chat(frame: &mut UiFrame, _width: f32, height: f32, lines: &[String], opacity: f32) {
    if lines.is_empty() { return; }
    let line_h = 18.0;
    let top = height - 92.0 - lines.len().min(8) as f32 * line_h;
    rect(frame, 8.0, top - 4.0, 520.0, lines.len().min(8) as f32 * line_h + 8.0, [0.0, 0.0, 0.0, opacity]);
    for (index, line) in lines.iter().rev().take(8).enumerate() {
        text(frame, 14.0, height - 28.0 - index as f32 * line_h, 14.0, line, [1.0, 1.0, 1.0, 1.0]);
    }
}

fn draw_toast(frame: &mut UiFrame, width: f32, message: &str) {
    let toast_w = (message.chars().count() as f32 * 8.0 + 32.0).min(width - 32.0);
    let x = (width - toast_w) * 0.5;
    rect(frame, x, 26.0, toast_w, 32.0, [0.02, 0.02, 0.03, 0.88]);
    text(frame, x + 16.0, 35.0, 14.0, message, [1.0, 0.92, 0.55, 1.0]);
}

fn draw_menu(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, title: &str, labels: &[&str]) {
    panel(frame, width, height, state.high_contrast);
    text(frame, (width - title.len() as f32 * 18.0) * 0.5, height * 0.18, 30.0, title, [1.0, 0.86, 0.35, 1.0]);
    let rects = state.button_rects(width, height);
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            rect(frame, x, y, w, h, if index == state.selected { [0.55, 0.43, 0.18, 1.0] } else { [0.12, 0.12, 0.16, 0.98] });
            text(frame, x + 16.0, y + 9.0, 16.0, *label, [1.0, 1.0, 1.0, 1.0]);
        }
    }
}

fn draw_connect(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, feedback: Option<&str>) {
    panel(frame, width, height, state.high_contrast);
    text(frame, width * 0.5 - 110.0, height * 0.18, 28.0, "Join Server", [1.0, 0.86, 0.35, 1.0]);
    let input_w = (width * 0.34).clamp(240.0, 420.0);
    let input_x = (width - input_w) * 0.5;
    let input_y = height * 0.30;
    text(frame, input_x, input_y - 24.0, 14.0, "Server address (host:port)", [0.82, 0.82, 0.88, 1.0]);
    rect(frame, input_x, input_y, input_w, 38.0, [0.04, 0.04, 0.06, 1.0]);
    text(frame, input_x + 12.0, input_y + 10.0, 16.0, &state.server_address, [1.0, 1.0, 1.0, 1.0]);
    if let Some(feedback) = feedback.filter(|text| !text.is_empty()) {
        text(frame, input_x, input_y + 48.0, 13.0, feedback, [1.0, 0.55, 0.45, 1.0]);
    }
    let labels = ["Connect", "Back"];
    let rects = state.button_rects(width, height);
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            rect(frame, x, y, w, h, if index == state.selected { [0.55, 0.43, 0.18, 1.0] } else { [0.12, 0.12, 0.16, 0.98] });
            text(frame, x + 16.0, y + 9.0, 16.0, *label, [1.0, 1.0, 1.0, 1.0]);
        }
    }
}

fn draw_options(frame: &mut UiFrame, width: f32, height: f32, state: &UiState) {
    panel(frame, width, height, state.high_contrast);
    text(frame, width * 0.5 - 70.0, height * 0.18, 28.0, "Options", [1.0, 0.86, 0.35, 1.0]);
    let labels = [
        format!("Graphics: {}", if state.graphics_vibrant { "Vibrant" } else { "Regular" }),
        format!("Render distance: {}", state.render_distance),
        "Increase render distance".to_string(),
        format!("GUI scale: {}", state.gui_scale as i32),
        "Back".to_string(),
        "Accessibility".to_string(),
    ];
    let rects = state.button_rects(width, height);
    for (index, label) in labels.iter().enumerate() {
        let (x, y, w, h) = rects[index];
        rect(frame, x, y, w, h, if index == state.selected { [0.55, 0.43, 0.18, 1.0] } else { [0.12, 0.12, 0.16, 0.98] });
        text(frame, x + 16.0, y + 9.0, 16.0, label, [1.0, 1.0, 1.0, 1.0]);
    }
}

fn draw_controls(frame: &mut UiFrame, width: f32, height: f32, state: &UiState) {
    panel(frame, width, height, state.high_contrast);
    text(frame, width * 0.5 - 85.0, height * 0.16, 28.0, "Controls", [1.0, 0.86, 0.35, 1.0]);
    let lines = ["WASD   Move", "Space   Jump", "Shift   Sneak", "E       Inventory", "T       Chat", "Esc     Pause"];
    for (index, line) in lines.iter().enumerate() {
        text(frame, width * 0.5 - 150.0, height * 0.30 + index as f32 * 25.0, 16.0, *line, [1.0, 1.0, 1.0, 1.0]);
    }
    let (x, y, w, h) = state.button_rects(width, height)[0];
    rect(frame, x, y + 170.0, w, h, if state.selected == 0 { [0.55, 0.43, 0.18, 1.0] } else { [0.12, 0.12, 0.16, 0.98] });
    text(frame, x + 16.0, y + 179.0, 16.0, "Back", [1.0, 1.0, 1.0, 1.0]);
}

fn draw_accessibility(frame: &mut UiFrame, width: f32, height: f32, state: &UiState) {
    panel(frame, width, height, state.high_contrast);
    text(frame, width * 0.5 - 135.0, height * 0.18, 28.0, "Accessibility", [1.0, 0.86, 0.35, 1.0]);
    let labels = [
        format!("High contrast: {}", if state.high_contrast { "On" } else { "Off" }),
        format!("Reduced motion: {}", if state.reduced_motion { "On" } else { "Off" }),
        format!("Chat opacity: {:.0}%", state.chat_opacity * 100.0),
        "Back".to_string(),
    ];
    let rects = state.button_rects(width, height);
    for (index, label) in labels.iter().enumerate() {
        let (x, y, w, h) = rects[index];
        rect(frame, x, y, w, h, if index == state.selected { [0.55, 0.43, 0.18, 1.0] } else { [0.12, 0.12, 0.16, 0.98] });
        text(frame, x + 16.0, y + 9.0, 16.0, label, [1.0, 1.0, 1.0, 1.0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_opens_and_closes_pause() {
        let mut ui = UiState::default();
        ui.handle_escape();
        assert_eq!(ui.screen, UiScreen::Pause);
        ui.handle_escape();
        assert_eq!(ui.screen, UiScreen::Playing);
    }

    #[test]
    fn menu_click_activates_options() {
        let mut ui = UiState::default();
        ui.open_pause();
        let action = ui.click(800.0, 600.0, 400.0, 0.34 * 600.0 + 2.0 * 42.0);
        assert_eq!(action, UiAction::None);
        assert_eq!(ui.screen, UiScreen::Options);
    }

    #[test]
    fn connect_screen_edits_address_and_submits() {
        let mut ui = UiState::default();
        ui.open_pause();
        assert_eq!(ui.activate_focused(), UiAction::Resume);
        ui.open_pause();
        ui.move_focus(1);
        assert_eq!(ui.activate_focused(), UiAction::OpenConnect);
        ui.server_address.clear();
        ui.append_server_address("localhost:25565");
        assert_eq!(ui.server_address, "localhost:25565");
        assert_eq!(ui.activate_focused(), UiAction::ConnectServer);
    }

    #[test]
    fn inventory_layout_maps_hotbar_and_armor_slots() {
        let width = 960.0;
        let height = 720.0;
        let scale = 1.0;
        let slot = 18.0;
        let left = (width - 256.0) * 0.5;
        let top = (height - 256.0) * 0.5;
        assert_eq!(inventory_slot_at(width, height, scale, left + 7.0 + slot * 0.5, top + 141.0 + slot * 0.5), Some(0));
        assert_eq!(inventory_slot_at(width, height, scale, left + 7.0 + slot * 0.5, top + 8.0 + slot * 0.5), Some(36));
        assert_eq!(inventory_slot_at(width, height, scale, left + 77.0 + slot * 0.5, top + 61.0 + slot * 0.5), Some(40));
        assert_eq!(player_crafting_slot_at(width, height, scale, left + 89.0 + slot * 0.5, top + 19.0 + slot * 0.5), Some(0));
        assert!(player_crafting_result_at(width, height, scale, left + 145.0 + slot * 0.5, top + 28.0 + slot * 0.5));
    }
}
