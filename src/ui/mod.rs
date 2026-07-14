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
    Loading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    None,
    Resume,
    Quit,
    QuitToTitle,
    StartGame,
    ToggleGraphics,
    DecreaseRenderDistance,
    IncreaseRenderDistance,
    ToggleGuiScale,
    ToggleHighContrast,
    ToggleReducedMotion,
    ToggleViewBobbing,
    ToggleAutoJump,
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
    CenteredText {
        center_x: f32,
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
    SpriteProgress {
        name: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        progress: f32,
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
    NineSlice {
        sprite: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        border: f32,
        color: [f32; 4],
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
    keyboard_focus: Option<usize>,
    pub prev_screen: Option<UiScreen>,
    pub gui_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub view_bobbing: bool,
    pub auto_jump: bool,
    pub chat_opacity: f32,
    pub render_distance: i32,
    pub graphics_vibrant: bool,
    pub server_address: String,
    pub connect_username: String,
    pub connect_field: usize,
    pub loading_progress: f32,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            screen: UiScreen::Playing,
            selected: 0,
            keyboard_focus: None,
            prev_screen: None,
            gui_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            view_bobbing: true,
            auto_jump: true,
            chat_opacity: 0.72,
            render_distance: 6,
            graphics_vibrant: false,
            server_address: "127.0.0.1:25565".to_string(),
            connect_username: "Player".to_string(),
            connect_field: 0,
            loading_progress: 0.0,
        }
    }
}

impl UiState {
    pub fn new(render_distance: i32, graphics_vibrant: bool, screen_height: f32) -> Self {
        let auto_scale = if screen_height >= 1080.0 { 3 } else if screen_height >= 720.0 { 2 } else { 1 };
        Self {
            render_distance,
            graphics_vibrant,
            gui_scale: auto_scale as f32,
            ..Self::default()
        }
    }

    pub fn open_inventory(&mut self) {
        self.screen = UiScreen::Inventory;
        self.selected = 0;
        self.keyboard_focus = None;
    }

    pub fn close_to_gameplay(&mut self) {
        self.screen = UiScreen::Playing;
        self.selected = 0;
        self.keyboard_focus = None;
    }

    pub fn open_pause(&mut self) {
        if self.screen == UiScreen::Playing {
            self.screen = UiScreen::Pause;
            self.selected = 0;
            self.keyboard_focus = None;
        }
    }

    pub fn open_title(&mut self) {
        self.screen = UiScreen::Title;
        self.selected = 0;
        self.keyboard_focus = None;
    }

    fn start_loading(&mut self) {
        self.screen = UiScreen::Loading;
        self.selected = 0;
        self.keyboard_focus = None;
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
                self.keyboard_focus = None;
            }
            UiScreen::Connect => {
                self.screen = self.prev_screen.unwrap_or(UiScreen::Pause);
                self.prev_screen = None;
                self.selected = 0;
                self.keyboard_focus = None;
            }
            UiScreen::Title => return UiAction::Quit,
            UiScreen::Loading => {}
        }
        UiAction::None
    }

    pub fn move_focus(&mut self, direction: i32) {
        let count = self.button_count().max(1);
        self.selected = (self.selected as i32 + direction).rem_euclid(count as i32) as usize;
        self.keyboard_focus = Some(self.selected);
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
                self.keyboard_focus = None;
                self.activate(index)
            })
            .unwrap_or(UiAction::None)
    }

    fn button_count(&self) -> usize {
        match self.screen {
            UiScreen::Pause => 4,
            UiScreen::Options => 7,
            UiScreen::Controls => 2,
            UiScreen::Accessibility => 5,
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
                    self.screen = UiScreen::Options;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                2 => {
                    self.screen = UiScreen::Controls;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                3 => UiAction::QuitToTitle,
                _ => UiAction::None,
            },
            UiScreen::Options => match index {
                0 => {
                    self.graphics_vibrant = !self.graphics_vibrant;
                    UiAction::ToggleGraphics
                }
                1 => {
                    if self.render_distance > 2 { self.render_distance -= 1; }
                    UiAction::DecreaseRenderDistance
                }
                2 => {
                    self.gui_scale = if self.gui_scale >= 3.0 { 1.0 } else { self.gui_scale + 1.0 };
                    UiAction::ToggleGuiScale
                }
                3 => {
                    self.view_bobbing = !self.view_bobbing;
                    UiAction::None
                }
                4 => {
                    self.auto_jump = !self.auto_jump;
                    UiAction::None
                }
                5 => {
                    self.screen = UiScreen::Accessibility;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                6 => {
                    self.screen = UiScreen::Pause;
                    self.selected = 0;
                    self.keyboard_focus = None;
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
                    self.gui_scale = if self.gui_scale >= 3.0 { 1.0 } else { self.gui_scale + 1.0 };
                    UiAction::ToggleGuiScale
                }
                4 => {
                    self.screen = UiScreen::Options;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                _ => UiAction::None,
            },
            UiScreen::Controls => match index {
                0 => UiAction::None,
                1 => {
                    self.screen = UiScreen::Pause;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                _ => UiAction::None,
            },
            UiScreen::Title => match index {
                0 => {
                    self.start_loading();
                    UiAction::StartGame
                }
                1 => {
                    self.prev_screen = Some(self.screen);
                    self.screen = UiScreen::Connect;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                2 => {
                    self.screen = UiScreen::Options;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                3 => UiAction::Quit,
                _ => UiAction::None,
            },
            UiScreen::Connect => match index {
                0 => UiAction::ConnectServer,
                1 => {
                    self.screen = self.prev_screen.unwrap_or(UiScreen::Pause);
                    self.prev_screen = None;
                    self.selected = 0;
                    self.keyboard_focus = None;
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
            KeyCode::Tab => {
                if self.screen == UiScreen::Connect {
                    self.switch_connect_field();
                }
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

    pub fn append_connect_username(&mut self, value: &str) {
        if self.screen == UiScreen::Connect && self.connect_username.len() + value.len() <= 16 {
            self.connect_username.push_str(value);
        }
    }

    pub fn backspace_connect_username(&mut self) {
        if self.screen == UiScreen::Connect {
            self.connect_username.pop();
        }
    }

    pub fn switch_connect_field(&mut self) {
        if self.screen == UiScreen::Connect {
            self.connect_field = (self.connect_field + 1) % 2;
        }
    }

    fn button_rects(&self, width: f32, height: f32) -> Vec<(f32, f32, f32, f32)> {
        let button_w = (width * 0.34).clamp(240.0, 420.0);
        let button_h = 20.0 * self.gui_scale;
        let gap = 4.0 * self.gui_scale;
        let left = (width - button_w) * 0.5;
        let count = self.button_count();
        let total_h = count as f32 * button_h + (count - 1) as f32 * gap;
        let top = (height - total_h) * 0.5 + 20.0 * self.gui_scale;
        (0..count)
            .map(|index| {
                let y = top + index as f32 * (button_h + gap);
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
        armor_points: f32,
        experience: f32,
        selected_item_name: &str,
        chat_lines: &[String],
        feedback: Option<&str>,
        cursor: Option<(f32, f32)>,
        show_crosshair: bool,
    ) -> UiFrame {
        let mut frame = UiFrame::default();
        match self.screen {
            UiScreen::Playing => {
                draw_hud(&mut frame, width, height, self.gui_scale, hotbar, health, hunger, armor_points, experience, selected_item_name, show_crosshair);
                draw_chat(&mut frame, width, height, chat_lines, self.chat_opacity);
                if let Some(feedback) = feedback.filter(|text| !text.is_empty()) {
                    draw_toast(&mut frame, width, feedback);
                }
            }
            UiScreen::Inventory => {
                draw_hud(&mut frame, width, height, self.gui_scale, hotbar, health, hunger, armor_points, experience, selected_item_name, false);
                draw_inventory(&mut frame, width, height, self.gui_scale, inventory.unwrap_or(&[]), crafting.unwrap_or(&[]), craft_result, carried, cursor, self.high_contrast);
            }
            UiScreen::Pause => draw_menu(&mut frame, width, height, self, cursor, "Game Menu", &["Back to Game", "Options...", "Controls...", "Save and Quit to Title"]),
            UiScreen::Options => draw_options(&mut frame, width, height, self, cursor),
            UiScreen::Controls => draw_controls(&mut frame, width, height, self, cursor),
            UiScreen::Accessibility => draw_accessibility(&mut frame, width, height, self, cursor),
            UiScreen::Connect => draw_connect(&mut frame, width, height, self, cursor, feedback),
            UiScreen::Title => draw_title_screen(&mut frame, width, height, self, cursor),
            UiScreen::Loading => draw_loading(&mut frame, width, height, self.gui_scale, self.loading_progress),
        }
        frame
    }
}

pub fn inventory_slot_at(width: f32, height: f32, scale: f32, x: f32, y: f32) -> Option<usize> {
    let slot = 18.0 * scale;
    let content_w = 176.0;
    let content_h = 166.0;
    let left = (width - content_w * scale) * 0.5;
    let top = (height - content_h * scale) * 0.5;
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
    let content_w = 176.0;
    let content_h = 166.0;
    let left = (width - content_w * scale) * 0.5;
    let top = (height - content_h * scale) * 0.5;
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
    let content_w = 176.0;
    let content_h = 166.0;
    let left = (width - content_w * scale) * 0.5;
    let top = (height - content_h * scale) * 0.5;
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

fn centered_text(frame: &mut UiFrame, center_x: f32, y: f32, size: f32, value: impl Into<String>, color: [f32; 4]) {
    frame.commands.push(UiCommand::CenteredText { center_x, y, size, text: value.into(), color });
}

fn panel(frame: &mut UiFrame, width: f32, height: f32, high_contrast: bool) {
    rect(frame, 0.0, 0.0, width, height, [0.0, 0.0, 0.0, if high_contrast { 0.72 } else { 0.52 }]);
}

fn nine_slice(frame: &mut UiFrame, sprite: &str, x: f32, y: f32, w: f32, h: f32, border: f32, color: [f32; 4]) {
    frame.commands.push(UiCommand::NineSlice {
        sprite: sprite.to_string(),
        x, y, w, h, border, color,
    });
}

fn minecraft_button(frame: &mut UiFrame, x: f32, y: f32, w: f32, h: f32, label: &str, focused: bool, hover: bool) {
    let sprite = if focused || hover { "widget/button_highlighted" } else { "widget/button" };
    nine_slice(frame, sprite, x, y, w, h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    let text_size = h * 0.4;
    let text_y = y + (h - text_size) * 0.5;
    centered_text(frame, x + w * 0.5, text_y, text_size, label, [1.0, 1.0, 1.0, 1.0]);
}

fn draw_loading(frame: &mut UiFrame, width: f32, height: f32, scale: f32, progress: f32) {
    rect(frame, 0.0, 0.0, width, height, [0.0, 0.0, 0.0, 1.0]);
    centered_text(frame, width * 0.5, height * 0.5 - 8.0 * scale, 8.0 * scale, "Loading terrain...", [1.0, 1.0, 1.0, 1.0]);
    let bar_w = 182.0 * scale;
    let bar_h = 5.0 * scale;
    let bar_x = (width - bar_w) * 0.5;
    let bar_y = height * 0.5 + 8.0 * scale;
    frame.commands.push(UiCommand::Sprite {
        name: "hud/experience_bar_background".to_string(),
        x: bar_x,
        y: bar_y,
        w: bar_w,
        h: bar_h,
        color: [1.0, 1.0, 1.0, 1.0],
    });
    frame.commands.push(UiCommand::SpriteProgress {
        name: "hud/experience_bar_progress".to_string(),
        x: bar_x,
        y: bar_y,
        w: bar_w,
        h: bar_h,
        progress,
        color: [1.0, 1.0, 1.0, 1.0],
    });
}

fn draw_hud(frame: &mut UiFrame, width: f32, height: f32, scale: f32, hotbar: &[UiSlot], health: f32, hunger: f32, armor: f32, experience: f32, _selected_item_name: &str, show_crosshair: bool) {
    let bar_w = 182.0 * scale;
    let bar_h = 22.0 * scale;
    let left = (width - bar_w) * 0.5;
    let top = height - bar_h - 8.0 * scale;

    // Experience bar
    let exp_bar_w = 182.0 * scale;
    let exp_bar_h = 5.0 * scale;
    let exp_left = (width - exp_bar_w) * 0.5;
    let exp_top = top - 7.0 * scale;
    // Experience bar background
    rect(frame, exp_left, exp_top, exp_bar_w, exp_bar_h, [0.0, 0.0, 0.0, 0.6]);
    // Filled portion
    let exp_filled = (experience / 100.0).clamp(0.0, 1.0) * exp_bar_w;
    if exp_filled > 0.0 {
        rect(frame, exp_left, exp_top, exp_filled, exp_bar_h, [0.38, 0.92, 0.08, 1.0]);
    }

    // Hotbar background
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

    // Status bars row: health left, armor middle, food right
    let status_y = top - 11.0 * scale;
    let half_width = 91.0 * scale;

    // Health bar (left side)
    let health_left = width * 0.5 - half_width;
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
            x: health_left + index as f32 * 9.0 * scale,
            y: status_y,
            w: 9.0 * scale,
            h: 9.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }

    // Armor bar (between health and food) - rendered as colored rects
    // since armor sprites may not be available in all asset packs
    if armor > 0.0 {
        let armor_icons = (armor / 2.0).ceil() as usize;
        let armor_width = armor_icons as f32 * 9.0 * scale;
        let armor_left = (width - armor_width) * 0.5;
        for index in 0..armor_icons {
            let filled = armor >= index as f32 * 2.0 + 2.0;
            let color = if filled { [0.35, 0.50, 0.70, 1.0] } else { [0.15, 0.20, 0.30, 0.8] };
            rect(frame, armor_left + index as f32 * 9.0 * scale, status_y, 9.0 * scale, 9.0 * scale, color);
            if !filled {
                rect(frame, armor_left + index as f32 * 9.0 * scale + 1.0, status_y + 1.0, 7.0 * scale, 7.0 * scale, [0.0, 0.0, 0.0, 0.5]);
            }
        }
    }

    // Food bar (right side)
    let food_left = width * 0.5 + half_width - 9.0 * 9.0 * scale;
    for index in 0..10 {
        let hunger_sprite = if hunger >= index as f32 * 2.0 + 2.0 {
            "hud/food_full"
        } else if hunger > index as f32 * 2.0 {
            "hud/food_half"
        } else {
            "hud/food_empty"
        };
        frame.commands.push(UiCommand::Sprite {
            name: hunger_sprite.to_string(),
            x: food_left + index as f32 * 9.0 * scale,
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
    // Content area within the 256×256 texture is 176×166 (top-left aligned)
    let content_w = 176.0;
    let content_h = 166.0;
    let left = (width - content_w * scale) * 0.5;
    let top = (height - content_h * scale) * 0.5;
    // Render the full 256×256 texture; transparent padding provides margin
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
                let text_size = 8.0 * scale;
                let pad = 4.0 * scale;
                let tooltip_w = (label.chars().count() as f32 * text_size * 0.6 + pad * 2.0).min(width - 24.0);
                let tooltip_h = text_size + pad * 1.5;
                let tooltip_x = (x + 8.0 * scale).clamp(8.0, width - tooltip_w - 8.0);
                let tooltip_y = (y - tooltip_h - 4.0 * scale).max(8.0);
                nine_slice(frame, "popup/background", tooltip_x, tooltip_y, tooltip_w, tooltip_h, 6.0, [1.0, 1.0, 1.0, 1.0]);
                text(frame, tooltip_x + pad, tooltip_y + pad * 0.75, text_size, label, [1.0, 1.0, 1.0, 1.0]);
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

fn draw_menu(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>, title: &str, labels: &[&str]) {
    panel(frame, width, height, state.high_contrast);
    let rects = state.button_rects(width, height);
    let first_button_y = rects.first().map(|r| r.1).unwrap_or(0.0);
    let title_size = 8.0 * state.gui_scale;
    centered_text(frame, width * 0.5, first_button_y - 20.0 * state.gui_scale, title_size, title, [1.0, 1.0, 1.0, 1.0]);
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }
}

fn draw_connect(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>, feedback: Option<&str>) {
    panel(frame, width, height, state.high_contrast);
    let title = "Join Server";
    let title_size = 8.0 * state.gui_scale;
    centered_text(frame, width * 0.5, height * 0.14, title_size, title, [1.0, 1.0, 1.0, 1.0]);
    let input_w = (width * 0.34).clamp(240.0, 420.0);
    let input_h = 20.0;
    let input_x = (width - input_w) * 0.5;

    let address_y = height * 0.26;
    text(frame, input_x, address_y - 22.0, 12.0, "Server Address", [0.82, 0.82, 0.88, 1.0]);
    nine_slice(frame, "widget/text_field", input_x, address_y, input_w, input_h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    let addr_color = if state.connect_field == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.6, 0.6, 0.6, 1.0] };
    text(frame, input_x + 6.0, address_y + 4.0, 12.0, &state.server_address, addr_color);

    let username_y = address_y + input_h + 18.0;
    text(frame, input_x, username_y - 22.0, 12.0, "Username", [0.82, 0.82, 0.88, 1.0]);
    nine_slice(frame, "widget/text_field", input_x, username_y, input_w, input_h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    let name_color = if state.connect_field == 1 { [1.0, 1.0, 1.0, 1.0] } else { [0.6, 0.6, 0.6, 1.0] };
    text(frame, input_x + 6.0, username_y + 4.0, 12.0, &state.connect_username, name_color);

    if let Some(feedback) = feedback.filter(|text| !text.is_empty()) {
        text(frame, input_x, username_y + input_h + 6.0, 11.0, feedback, [1.0, 0.55, 0.45, 1.0]);
    }
    let labels = ["Connect", "Back"];
    let rects = state.button_rects(width, height);
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }
}

fn draw_options(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    panel(frame, width, height, state.high_contrast);
    let rects = state.button_rects(width, height);
    let first_y = rects.first().map(|r| r.1).unwrap_or(0.0);
    let title_size = 8.0 * state.gui_scale;
    let t = "Options";
    centered_text(frame, width * 0.5, first_y - 20.0 * state.gui_scale, title_size, t, [1.0, 1.0, 1.0, 1.0]);
    let labels = [
        format!("Graphics: {}", if state.graphics_vibrant { "Fabulous!" } else { "Fancy" }),
        format!("Render Distance: {}", state.render_distance),
        format!("GUI Scale: {}", state.gui_scale as i32),
        format!("View Bobbing: {}", if state.view_bobbing { "ON" } else { "OFF" }),
        format!("Auto-Jump: {}", if state.auto_jump { "ON" } else { "OFF" }),
        "Accessibility Settings...".to_string(),
        "Done".to_string(),
    ];
    for (index, label) in labels.iter().enumerate() {
        let (x, y, w, h) = rects[index];
        let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
        minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
    }
}

fn draw_controls(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    panel(frame, width, height, state.high_contrast);
    let rects = state.button_rects(width, height);
    let first_y = rects.first().map(|r| r.1).unwrap_or(0.0);
    let title_size = 8.0 * state.gui_scale;
    let t = "Controls";
    centered_text(frame, width * 0.5, first_y - 20.0 * state.gui_scale, title_size, t, [1.0, 1.0, 1.0, 1.0]);
    let lines = [
        ("WASD", "Move"),
        ("Space", "Jump"),
        ("Shift", "Sneak"),
        ("Ctrl", "Sprint"),
        ("E", "Inventory"),
        ("Q", "Drop Item"),
        ("T", "Chat"),
        ("F", "Toggle Flight"),
        ("Esc", "Pause / Menu"),
    ];
    let text_size = 12.0 * state.gui_scale;
    let col1_x = width * 0.5 - 200.0 * state.gui_scale;
    let col2_x = width * 0.5 - 60.0 * state.gui_scale;
    let start_y = height * 0.25;
    for (index, (key, action)) in lines.iter().enumerate() {
        let y = start_y + index as f32 * 22.0 * state.gui_scale;
        text(frame, col1_x, y, text_size, *key, [0.55, 0.43, 0.18, 1.0]);
        text(frame, col2_x, y, text_size, *action, [1.0, 1.0, 1.0, 1.0]);
    }
    for (index, label) in ["Reset", "Done"].iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }
}

fn draw_accessibility(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    panel(frame, width, height, state.high_contrast);
    let rects = state.button_rects(width, height);
    let first_y = rects.first().map(|r| r.1).unwrap_or(0.0);
    let title_size = 8.0 * state.gui_scale;
    let t = "Accessibility";
    centered_text(frame, width * 0.5, first_y - 20.0 * state.gui_scale, title_size, t, [1.0, 1.0, 1.0, 1.0]);
    let labels = [
        format!("High Contrast: {}", if state.high_contrast { "ON" } else { "OFF" }),
        format!("Reduced Motion: {}", if state.reduced_motion { "ON" } else { "OFF" }),
        format!("Chat Opacity: {:.0}%", state.chat_opacity * 100.0),
        format!("GUI Scale: {}", state.gui_scale as i32),
        "Done".to_string(),
    ];
    for (index, label) in labels.iter().enumerate() {
        let (x, y, w, h) = rects[index];
        let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
        minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
    }
}

fn draw_title_screen(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    panel(frame, width, height, state.high_contrast);

    // Title text rendered with the Minecraft font (gold color, large)
    let title = "Vibecraft";
    let title_size = 36.0 * state.gui_scale;
    let title_y = (height * 0.10).max(24.0);
    centered_text(frame, width * 0.5, title_y, title_size, title, [1.0, 0.84, 0.0, 1.0]);

    // Subtitle in smaller text
    let subtitle = "A Minecraft-inspired game";
    let sub_size = 14.0 * state.gui_scale;
    centered_text(frame, width * 0.5, title_y + title_size * 1.1, sub_size, subtitle, [0.6, 0.6, 0.6, 1.0]);

    // Buttons: Singleplayer, Multiplayer, Options..., Quit
    let button_w = (width * 0.34).clamp(240.0, 420.0);
    let button_h = 20.0 * state.gui_scale;
    let gap = 3.0 * state.gui_scale;
    let count = 4;
    let total_h = count as f32 * button_h + (count - 1) as f32 * gap;
    let left = (width - button_w) * 0.5;
    let top = (height - total_h) * 0.5 + 20.0 * state.gui_scale;

    let labels = ["Singleplayer", "Multiplayer", "Options...", "Quit"];
    for (index, label) in labels.iter().enumerate() {
        let y = top + index as f32 * (button_h + gap);
        let hovered = cursor.map_or(false, |(cx, cy)| contains((left, y, button_w, button_h), cx, cy));
        minecraft_button(frame, left, y, button_w, button_h, label, state.keyboard_focus == Some(index), hovered);
    }

    // Copyright footer
    let copyright = "Copyright Bobby AI";
    let text_size = 10.0 * state.gui_scale;
    centered_text(frame, width * 0.5, height - 18.0 * state.gui_scale, text_size, copyright, [0.5, 0.5, 0.5, 1.0]);
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
        // Click on the "Options..." button (index 1)
        let rects = ui.button_rects(800.0, 600.0);
        let (bx, by, bw, bh) = rects[1];
        let action = ui.click(800.0, 600.0, bx + bw * 0.5, by + bh * 0.5);
        assert_eq!(action, UiAction::None);
        assert_eq!(ui.screen, UiScreen::Options);
    }

    #[test]
    fn pause_frame_emits_textured_buttons_and_centered_labels() {
        let mut ui = UiState::default();
        ui.open_pause();
        let frame = ui.frame(
            800.0,
            600.0,
            &[],
            None,
            None,
            None,
            None,
            20.0,
            20.0,
            0.0,
            0.0,
            "",
            &[],
            None,
            None,
            false,
        );

        let button_sprites: Vec<&str> = frame.commands.iter().filter_map(|command| match command {
            UiCommand::NineSlice { sprite, .. } if sprite.starts_with("widget/button") => Some(sprite.as_str()),
            _ => None,
        }).collect();
        assert_eq!(button_sprites.len(), 4);
        assert_eq!(button_sprites.iter().filter(|sprite| **sprite == "widget/button_highlighted").count(), 0);

        ui.move_focus(1);
        let focused_frame = ui.frame(
            800.0, 600.0, &[], None, None, None, None, 20.0, 20.0, 0.0, 0.0, "", &[], None, None, false,
        );
        assert_eq!(
            focused_frame.commands.iter().filter(|command| matches!(command, UiCommand::NineSlice { sprite, .. } if sprite == "widget/button_highlighted")).count(),
            1,
        );

        let labels: Vec<&str> = frame.commands.iter().filter_map(|command| match command {
            UiCommand::CenteredText { center_x, text, .. } if *center_x == 400.0 => Some(text.as_str()),
            _ => None,
        }).collect();
        assert!(labels.contains(&"Back to Game"));
        assert!(labels.contains(&"Options..."));
        assert!(labels.contains(&"Controls..."));
        assert!(labels.contains(&"Save and Quit to Title"));
    }

    #[test]
    fn loading_frame_uses_the_vanilla_experience_bar_sprites() {
        let mut ui = UiState::default();
        ui.screen = UiScreen::Loading;
        ui.loading_progress = 0.5;
        let frame = ui.frame(
            800.0, 600.0, &[], None, None, None, None, 20.0, 20.0, 0.0, 0.0, "", &[], None, None, false,
        );
        assert!(frame.commands.iter().any(|command| matches!(command, UiCommand::Sprite { name, .. } if name == "hud/experience_bar_background")));
        assert!(frame.commands.iter().any(|command| matches!(command, UiCommand::SpriteProgress { name, progress, .. } if name == "hud/experience_bar_progress" && *progress == 0.5)));
    }

    #[test]
    fn connect_screen_edits_address_and_submits() {
        let mut ui = UiState::default();
        ui.screen = UiScreen::Title;
        assert_eq!(ui.activate_focused(), UiAction::StartGame);
        assert_eq!(ui.screen, UiScreen::Loading);
        assert_eq!(ui.handle_escape(), UiAction::None);
        assert_eq!(ui.screen, UiScreen::Loading);
        ui.screen = UiScreen::Title;
        ui.selected = 0;
        ui.move_focus(3);
        assert_eq!(ui.activate_focused(), UiAction::Quit);
        ui.screen = UiScreen::Connect;
        ui.selected = 0;
        ui.server_address.clear();
        ui.append_server_address("localhost:25565");
        assert_eq!(ui.activate_focused(), UiAction::ConnectServer);
    }

    #[test]
    fn inventory_layout_maps_hotbar_and_armor_slots() {
        let width = 960.0;
        let height = 720.0;
        let scale = 1.0;
        let slot = 18.0;
        let left = (width - 176.0) * 0.5;
        let top = (height - 166.0) * 0.5;
        assert_eq!(inventory_slot_at(width, height, scale, left + 7.0 + slot * 0.5, top + 141.0 + slot * 0.5), Some(0));
        assert_eq!(inventory_slot_at(width, height, scale, left + 7.0 + slot * 0.5, top + 8.0 + slot * 0.5), Some(36));
        assert_eq!(inventory_slot_at(width, height, scale, left + 77.0 + slot * 0.5, top + 61.0 + slot * 0.5), Some(40));
        assert_eq!(player_crafting_slot_at(width, height, scale, left + 89.0 + slot * 0.5, top + 19.0 + slot * 0.5), Some(0));
        assert!(player_crafting_result_at(width, height, scale, left + 145.0 + slot * 0.5, top + 28.0 + slot * 0.5));
    }
}
