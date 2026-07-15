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
    /// When Some([top, front, right]), the item is a block and should be drawn
    /// as an isometric 3D cube using the terrain atlas tiles.
    pub block_tiles: Option<[u32; 3]>,
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
    IsometricBlock {
        x: f32,
        y: f32,
        size: f32,
        count: u16,
        top_tile: u32,
        front_tile: u32,
        right_tile: u32,
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
    pub blur_intensity: f32,
    pub frame_count: u64,
    pub connecting: bool,
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
            blur_intensity: 3.0,
            frame_count: 0,
            connecting: false,
        }
    }
}

const GUI_SCALE_VALUES: &[f32] = &[1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0];

impl UiState {
    fn next_gui_scale(&mut self) {
        if let Some(pos) = GUI_SCALE_VALUES.iter().position(|&s| (s - self.gui_scale).abs() < f32::EPSILON) {
            self.gui_scale = GUI_SCALE_VALUES[(pos + 1) % GUI_SCALE_VALUES.len()];
        } else {
            self.gui_scale = GUI_SCALE_VALUES[0];
        }
    }
    fn prev_gui_scale(&mut self) {
        if let Some(pos) = GUI_SCALE_VALUES.iter().position(|&s| (s - self.gui_scale).abs() < f32::EPSILON) {
            self.gui_scale = GUI_SCALE_VALUES[(pos + GUI_SCALE_VALUES.len() - 1) % GUI_SCALE_VALUES.len()];
        } else {
            self.gui_scale = GUI_SCALE_VALUES[GUI_SCALE_VALUES.len() - 1];
        }
    }
    pub fn new(render_distance: i32, graphics_vibrant: bool, screen_height: f32) -> Self {
        // Pick closest GUI scale from available values based on screen height.
        // Minecraft formula: scale = floor(min(width, height) / 427), but we snap to our discrete set.
        let ideal = (screen_height / 360.0).round().max(1.0);
        let auto_scale = GUI_SCALE_VALUES
            .iter()
            .copied()
            .min_by(|a, b| {
                (a - ideal).abs().partial_cmp(&(b - ideal).abs()).unwrap()
            })
            .unwrap_or(2.0);
        Self {
            render_distance,
            graphics_vibrant,
            gui_scale: auto_scale,
            ..Self::default()
        }
    }

    pub fn tick(&mut self) {
        self.frame_count = self.frame_count.wrapping_add(1);
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
            UiScreen::Options | UiScreen::Controls => {
                self.screen = self.prev_screen.unwrap_or(UiScreen::Pause);
                self.prev_screen = None;
                self.selected = 0;
                self.keyboard_focus = None;
            }
            UiScreen::Accessibility => {
                self.screen = UiScreen::Options;
                self.selected = 0;
                self.keyboard_focus = None;
            }
            UiScreen::Connect => {
                self.screen = UiScreen::Title;
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
        if self.screen == UiScreen::Connect {
            let scale = self.gui_scale;
            let input_w = (width * 0.34).clamp(240.0 * scale, 420.0 * scale);
            let input_h = 20.0 * scale;
            let input_x = (width - input_w) * 0.5;
            let field_gap = 18.0 * scale;
            let address_y = height * 0.26;
            let username_y = address_y + input_h + field_gap;
            if contains((input_x, address_y, input_w, input_h), x, y) {
                self.connect_field = 0;
            } else if contains((input_x, username_y, input_w, input_h), x, y) {
                self.connect_field = 1;
            }
        }
        self.button_rects(width, height)
            .iter()
            .enumerate()
            .find(|(_, rect)| contains(**rect, x, y))
            .map(|(index, rect)| {
                let left = x < (rect.0 + rect.2 * 0.5);
                self.selected = index;
                self.keyboard_focus = None;
                self.activate_with_direction(index, left)
            })
            .unwrap_or(UiAction::None)
    }

    fn button_count(&self) -> usize {
        match self.screen {
            UiScreen::Pause => 4,
            UiScreen::Options => 9,
            UiScreen::Controls => 2,
            UiScreen::Accessibility => 5,
            UiScreen::Connect => 2,
            UiScreen::Title => 4,
            _ => 0,
        }
    }

    fn activate(&mut self, index: usize) -> UiAction {
        self.activate_with_direction(index, false)
    }

    fn activate_with_direction(&mut self, index: usize, left: bool) -> UiAction {
        match self.screen {
            UiScreen::Pause => match index {
                0 => {
                    self.close_to_gameplay();
                    UiAction::Resume
                }
                1 => {
                    self.prev_screen = Some(self.screen);
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
                    if self.render_distance > 2 && left {
                        self.render_distance -= 1;
                    } else if self.render_distance < 32 && !left {
                        self.render_distance += 1;
                    }
                    if left { UiAction::DecreaseRenderDistance } else { UiAction::IncreaseRenderDistance }
                }
                2 => {
                    if left { self.prev_gui_scale(); } else { self.next_gui_scale(); }
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
                    if left {
                        self.blur_intensity = (self.blur_intensity - 1.0).max(0.0);
                    } else {
                        self.blur_intensity = (self.blur_intensity + 1.0).min(10.0);
                    }
                    UiAction::None
                }
                6 => {
                    self.screen = UiScreen::Accessibility;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                7 => {
                    self.screen = self.prev_screen.unwrap_or(UiScreen::Pause);
                    self.prev_screen = None;
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
                    if left { self.prev_gui_scale(); } else { self.next_gui_scale(); }
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
                    self.screen = self.prev_screen.unwrap_or(UiScreen::Pause);
                    self.prev_screen = None;
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
                    self.screen = UiScreen::Connect;
                    self.selected = 0;
                    self.keyboard_focus = None;
                    UiAction::None
                }
                2 => {
                    self.prev_screen = Some(self.screen);
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
                    self.screen = UiScreen::Title;
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
            KeyCode::ArrowLeft => {
                if self.screen == UiScreen::Options {
                    self.activate_with_direction(self.selected, true)
                } else {
                    UiAction::None
                }
            }
            KeyCode::ArrowRight => {
                if self.screen == UiScreen::Options {
                    self.activate_with_direction(self.selected, false)
                } else {
                    UiAction::None
                }
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
        let button_w = self.button_width(width);
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

    /// The authored widget is 200×20. Keep that aspect ratio at every GUI
    /// scale so large UI uses a larger whole texture rather than a narrow,
    /// horizontally stretched nine-slice button.
    fn button_width(&self, viewport_width: f32) -> f32 {
        let scale = self.gui_scale;
        let desired = 200.0 * scale;
        let available = (viewport_width - 32.0 * scale).max(1.0);
        desired.min(available)
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
            UiScreen::Loading => draw_loading(&mut frame, width, height, self.gui_scale, self.loading_progress, self.connecting),
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

fn draw_loading(frame: &mut UiFrame, width: f32, height: f32, scale: f32, progress: f32, connecting: bool) {
    rect(frame, 0.0, 0.0, width, height, [0.0, 0.0, 0.0, 1.0]);
    let label = if connecting { "Connecting to server..." } else { "Loading terrain..." };
    centered_text(frame, width * 0.5, height * 0.5 - 8.0 * scale, 8.0 * scale, label, [1.0, 1.0, 1.0, 1.0]);
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

// Vanilla slots are 18px with a beveled inner area of about 16px.
// A 12px icon occupies ~75% of the inner area and leaves consistent padding.
const ITEM_ICON_SIZE: f32 = 14.0;

fn draw_slot_item(frame: &mut UiFrame, x: f32, y: f32, scale: f32, item: &UiSlot) {
    if item.empty {
        return;
    }
    let inset = (16.0 - ITEM_ICON_SIZE) * 0.5 * scale;
    if let Some([top, front, right]) = item.block_tiles {
        // Isometric 3D block rendering using terrain atlas tiles
        let s = ITEM_ICON_SIZE * scale;
        frame.commands.push(UiCommand::IsometricBlock {
            x: x + inset + s * 0.5,
            y: y + inset + s * 0.5,
            size: s,
            count: item.count,
            top_tile: top,
            front_tile: front,
            right_tile: right,
        });
    } else {
        // Flat 2D sprite
        frame.commands.push(UiCommand::Item {
            x: x + inset,
            y: y + inset,
            size: ITEM_ICON_SIZE * scale,
            name: item.name.clone(),
            sprite: item.sprite.clone(),
            count: item.count,
            hint: item.hint,
        });
    }
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
        let slot_left = left + (index as f32 * 20.0 + 3.0) * scale;
        if hotbar.get(index).map(|item| item.selected).unwrap_or(false) {
            frame.commands.push(UiCommand::Sprite {
                name: "hud/hotbar_selection".to_string(),
                x: slot_left - 2.0 * scale,
                y: top - 1.0 * scale,
                w: 24.0 * scale,
                h: 24.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
        if let Some(item) = hotbar.get(index).filter(|item| !item.empty) {
            draw_slot_item(frame, slot_left + 2.0 * scale, top + 3.0 * scale, scale, item);
        }
    }

    // Status bars row: health left, armor middle, food right
    let status_y = top - 13.0 * scale;
    let half_width = 91.0 * scale;

    // Health bar (left side) - always render container background then fill/half overlay
    let health_left = width * 0.5 - half_width;
    for index in 0..10 {
        let hx = health_left + index as f32 * 8.0 * scale;
        // Background container (always same size)
        frame.commands.push(UiCommand::Sprite {
            name: "hud/heart/container".to_string(),
            x: hx,
            y: status_y,
            w: 9.0 * scale,
            h: 9.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        // Fill/half overlay
        if health >= index as f32 * 2.0 + 2.0 {
            frame.commands.push(UiCommand::Sprite {
                name: "hud/heart/full".to_string(),
                x: hx,
                y: status_y,
                w: 9.0 * scale,
                h: 9.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        } else if health > index as f32 * 2.0 {
            frame.commands.push(UiCommand::Sprite {
                name: "hud/heart/half".to_string(),
                x: hx,
                y: status_y,
                w: 9.0 * scale,
                h: 9.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
    }

    // Armor bar (between health and food) - rendered as colored rects
    // since armor sprites may not be available in all asset packs
    if armor > 0.0 {
        let armor_icons = (armor / 2.0).ceil() as usize;
        let armor_width = armor_icons as f32 * 8.0 * scale;
        let armor_left = (width - armor_width) * 0.5;
        for index in 0..armor_icons {
            let filled = armor >= index as f32 * 2.0 + 2.0;
            let color = if filled { [0.35, 0.50, 0.70, 1.0] } else { [0.15, 0.20, 0.30, 0.8] };
            rect(frame, armor_left + index as f32 * 8.0 * scale, status_y, 9.0 * scale, 9.0 * scale, color);
            if !filled {
                rect(frame, armor_left + index as f32 * 8.0 * scale + 1.0, status_y + 1.0, 7.0 * scale, 7.0 * scale, [0.0, 0.0, 0.0, 0.5]);
            }
        }
    }

    // Food bar (right side)
    let food_left = width * 0.5 + half_width - 81.0 * scale;
    for index in 0..10 {
        let fx = food_left + index as f32 * 8.0 * scale;
        // Determine which food sprites to use
        let (fill_sprite, show_fill) = if hunger >= index as f32 * 2.0 + 2.0 {
            ("hud/food_full", true)
        } else if hunger > index as f32 * 2.0 {
            ("hud/food_half", true)
        } else {
            ("", false)
        };
        // Background empty sprite
        frame.commands.push(UiCommand::Sprite {
            name: "hud/food_empty".to_string(),
            x: fx,
            y: status_y,
            w: 9.0 * scale,
            h: 9.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        if show_fill {
            // Foreground fill
            frame.commands.push(UiCommand::Sprite {
                name: fill_sprite.to_string(),
                x: fx,
                y: status_y,
                w: 9.0 * scale,
                h: 9.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
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
            draw_slot_item(frame, x - 8.0 * scale, y - 8.0 * scale, scale, item);
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
                let max_tooltip_w = (width - 16.0).max(pad * 2.0 + text_size);
                // A bitmap glyph can occupy its full cell, so reserve one
                // cell per character instead of using an average-width guess.
                let max_chars = ((max_tooltip_w - pad * 2.0) / text_size).floor().max(1.0) as usize;
                let label = if label.chars().count() > max_chars {
                    if max_chars <= 3 {
                        label.chars().take(max_chars).collect()
                    } else {
                        let visible = max_chars - 3;
                        format!("{}...", label.chars().take(visible).collect::<String>())
                    }
                } else {
                    label
                };
                let tooltip_w = (label.chars().count() as f32 * text_size + pad * 2.0).min(max_tooltip_w);
                let tooltip_h = text_size + pad * 2.0;
                let tooltip_x = (x + 8.0 * scale).clamp(8.0, (width - tooltip_w - 8.0).max(8.0));
                let tooltip_y = (y - tooltip_h - 4.0 * scale).max(8.0);
                nine_slice(frame, "popup/background", tooltip_x, tooltip_y, tooltip_w, tooltip_h, 6.0, [1.0, 1.0, 1.0, 1.0]);
                text(frame, tooltip_x + pad, tooltip_y + pad, text_size, label, [1.0, 1.0, 1.0, 1.0]);
            }
        }
    }
}

fn draw_inventory_item(frame: &mut UiFrame, x: f32, y: f32, scale: f32, item: &UiSlot) {
    draw_slot_item(frame, x, y, scale, item);
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
    let title_size = 12.0 * state.gui_scale;
    centered_text(frame, width * 0.5, first_button_y - title_size * 1.8, title_size, title, [1.0, 1.0, 1.0, 1.0]);
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
    let title_size = 12.0 * state.gui_scale;
    centered_text(frame, width * 0.5, height * 0.14 + title_size * 0.5, title_size, title, [1.0, 1.0, 1.0, 1.0]);
    let scale = state.gui_scale;
    let input_w = (width * 0.34).clamp(240.0 * scale, 420.0 * scale);
    let input_h = 20.0 * scale;
    let input_x = (width - input_w) * 0.5;
    let label_size = 12.0 * scale;
    let text_size = 12.0 * scale;
    let pad_x = 6.0 * scale;
    let pad_y = 4.0 * scale;

    let address_y = height * 0.26;
    text(frame, input_x, address_y - label_size - pad_y, label_size, "Server Address", [0.82, 0.82, 0.88, 1.0]);
    nine_slice(frame, "widget/text_field", input_x, address_y, input_w, input_h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    text(frame, input_x + pad_x, address_y + pad_y, text_size, &state.server_address,
         if state.connect_field == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
    if state.connect_field == 0 && state.frame_count % 60 < 30 {
        let text_end = input_x + pad_x + state.server_address.chars().count() as f32 * text_size * 0.75;
        rect(frame, text_end, address_y + pad_y, 2.0 * scale, text_size, [1.0, 1.0, 1.0, 0.8]);
    }

    let field_gap = 18.0 * scale;
    let username_y = address_y + input_h + field_gap;
    text(frame, input_x, username_y - label_size - pad_y, label_size, "Username", [0.82, 0.82, 0.88, 1.0]);
    nine_slice(frame, "widget/text_field", input_x, username_y, input_w, input_h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    text(frame, input_x + pad_x, username_y + pad_y, text_size, &state.connect_username,
         if state.connect_field == 1 { [1.0, 1.0, 1.0, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
    if state.connect_field == 1 && state.frame_count % 60 < 30 {
        let text_end = input_x + pad_x + state.connect_username.chars().count() as f32 * text_size * 0.75;
        rect(frame, text_end, username_y + pad_y, 2.0 * scale, text_size, [1.0, 1.0, 1.0, 0.8]);
    }

    if let Some(feedback) = feedback.filter(|text| !text.is_empty()) {
        text(frame, input_x, username_y + input_h + 6.0 * scale, 11.0 * scale, feedback, [1.0, 0.55, 0.45, 1.0]);
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
    let title_size = 12.0 * state.gui_scale;
    let t = "Options";
    centered_text(frame, width * 0.5, first_y - title_size * 1.8, title_size, t, [1.0, 1.0, 1.0, 1.0]);
    let labels = [
        format!("Graphics: {}", if state.graphics_vibrant { "Fabulous!" } else { "Fancy" }),
        format!("Render Distance: {} chunks", state.render_distance),
        format!("GUI Scale: {}", if state.gui_scale.fract() == 0.0 { format!("{}", state.gui_scale as i32) } else { format!("{:.1}", state.gui_scale) }),
        format!("View Bobbing: {}", if state.view_bobbing { "ON" } else { "OFF" }),
        format!("Auto-Jump: {}", if state.auto_jump { "ON" } else { "OFF" }),
        format!("Pause Blur: {:.0}%", state.blur_intensity * 10.0),
        "Accessibility Settings...".to_string(),
        "Done".to_string(),
    ];
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }
}

fn draw_controls(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    panel(frame, width, height, state.high_contrast);
    let rects = state.button_rects(width, height);
    let first_y = rects.first().map(|r| r.1).unwrap_or(0.0);
    let title_size = 12.0 * state.gui_scale;
    let t = "Controls";
    centered_text(frame, width * 0.5, first_y - title_size * 1.8, title_size, t, [1.0, 1.0, 1.0, 1.0]);
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
    let text_size = 11.0 * state.gui_scale;
    let col_w = (width * 0.34).clamp(240.0, 420.0);
    let left = (width - col_w) * 0.5;
    let col1_x = left + col_w * 0.1;
    let col2_x = left + col_w * 0.45;
    let start_y = first_y + 10.0 * state.gui_scale;
    for (index, (key, action)) in lines.iter().enumerate() {
        let y = start_y + index as f32 * 20.0 * state.gui_scale;
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
    let title_size = 12.0 * state.gui_scale;
    let t = "Accessibility";
    centered_text(frame, width * 0.5, first_y - title_size * 1.8, title_size, t, [1.0, 1.0, 1.0, 1.0]);
    let labels = [
        format!("High Contrast: {}", if state.high_contrast { "ON" } else { "OFF" }),
        format!("Reduced Motion: {}", if state.reduced_motion { "ON" } else { "OFF" }),
        format!("Chat Opacity: {:.0}%", state.chat_opacity * 100.0),
        format!("GUI Scale: {}", if state.gui_scale.fract() == 0.0 { format!("{}", state.gui_scale as i32) } else { format!("{:.1}", state.gui_scale) }),
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

    // Minecraft-style title: large gold text with shadow
    let title = "Vibecraft";
    let title_size = (56.0 * state.gui_scale).min(width * 0.15).max(24.0);
    let title_y = (height * 0.08).max(20.0);

    // Main title (gold gradient effect)
    centered_text(frame, width * 0.5, title_y, title_size, title, [1.0, 0.84, 0.0, 1.0]);

    // Buttons: Singleplayer, Multiplayer, Options..., Quit
    let button_w = state.button_width(width);
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

    #[test]
    fn hotbar_icons_are_centered_within_slots() {
        let item = UiSlot {
            name: "Stone".to_string(),
            sprite: "block/stone".to_string(),
            count: 64,
            empty: false,
            selected: true,
            hint: 0,
            block_tiles: None,
        };
        let mut frame = UiFrame::default();
        draw_hud(&mut frame, 400.0, 240.0, 1.0, &[item], 20.0, 20.0, 0.0, 0.0, "", true);
        let icon = frame.commands.iter().find_map(|command| match command {
            UiCommand::Item { x, y, size, .. } => Some((*x, *y, *size)),
            _ => None,
        }).unwrap();
        assert_eq!(icon.0, (400.0 - 182.0) * 0.5 + 6.0);
        assert_eq!(icon.1, 240.0 - 22.0 - 8.0 + 4.0);
        assert_eq!(icon.2, ITEM_ICON_SIZE);
    }

    #[test]
    fn menu_buttons_scale_the_authored_200px_width() {
        let mut ui = UiState::default();
        ui.gui_scale = 3.0;
        assert_eq!(ui.button_width(1920.0), 600.0);
        assert_eq!(ui.button_width(500.0), 404.0);
    }
}
