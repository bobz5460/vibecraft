use winit::keyboard::KeyCode;
use crate::engine::text::{ASCII_GLYPH_COUNT, measure_text_width};

fn char_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(i, _)| i)
}

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
    WorldSelect,
    CreateWorld,
    Loading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    None,
    Resume,
    Quit,
    QuitToTitle,
    OpenWorldSelect,
    LoadSelectedWorld,
    CreateWorld,
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
    DeleteWorld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateGameMode {
    Survival,
    Hardcore,
    Creative,
}

impl CreateGameMode {
    fn next(self) -> Self {
        match self {
            Self::Survival => Self::Hardcore,
            Self::Hardcore => Self::Creative,
            Self::Creative => Self::Survival,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Survival => "Survival",
            Self::Hardcore => "Hardcore",
            Self::Creative => "Creative",
        }
    }

    pub const fn level_gamemode(self) -> &'static str {
        match self {
            Self::Creative => "creative",
            Self::Survival | Self::Hardcore => "survival",
        }
    }

    pub const fn hardcore(self) -> bool {
        matches!(self, Self::Hardcore)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiWorld {
    pub name: String,
    pub gamemode: String,
    pub hardcore: bool,
    pub last_played: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateWorldOptions {
    pub name: String,
    pub seed: Option<u64>,
    pub gamemode: CreateGameMode,
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
        hint: u32,
    },
    IsometricBlock {
        x: f32,
        y: f32,
        size: f32,
        top_tile: u32,
        front_tile: u32,
        right_tile: u32,
    },
    /// Renders a sprite centered horizontally at `center_x`, with its height
    /// scaled to `pixel_height` screen pixels while preserving the source
    /// image's aspect ratio. Used for the title-screen logo.
    TitleLogo {
        sprite: String,
        center_x: f32,
        y: f32,
        pixel_height: f32,
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
    Cursor {
        x: f32,
        y: f32,
        h: f32,
    },
    TextSelection {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    /// Tiles a sprite across the given area using texture repeat addressing.
    TiledBackground {
        sprite: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
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
    pub worlds: Vec<UiWorld>,
    pub selected_world: Option<usize>,
    pub confirm_delete_world: bool,
    pub world_scroll_offset: usize,
    pub world_name: String,
    pub world_seed: String,
    pub world_gamemode: CreateGameMode,
    pub create_field: usize,
    create_name_cursor: usize,
    create_seed_cursor: usize,
    create_name_selection: Option<usize>,
    create_seed_selection: Option<usize>,
    pub glyph_advances: [f32; ASCII_GLYPH_COUNT],
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
            worlds: Vec::new(),
            selected_world: None,
            confirm_delete_world: false,
            world_scroll_offset: 0,
            world_name: "New World".to_string(),
            world_seed: String::new(),
            world_gamemode: CreateGameMode::Survival,
            create_field: 0,
            create_name_cursor: 9,
            create_seed_cursor: 0,
            create_name_selection: None,
            create_seed_selection: None,
            glyph_advances: {
                let mut a = [crate::engine::text::GLYPH_SIZE; ASCII_GLYPH_COUNT];
                a[0] = 4.0;
                a
            },
        }
    }
}

const GUI_SCALE_VALUES: &[f32] = &[1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0];
const RENDER_DISTANCE_PRESETS: &[(i32, &str)] = &[(2, "Short"), (6, "Medium"), (12, "Far"), (32, "Ultra")];

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
        self.worlds.clear();
        self.selected_world = None;
        self.confirm_delete_world = false;
    }

    pub fn open_world_select(&mut self, worlds: Vec<UiWorld>) {
        self.worlds = worlds;
        self.selected_world = (!self.worlds.is_empty()).then_some(0);
        self.confirm_delete_world = false;
        self.world_scroll_offset = 0;
        self.screen = UiScreen::WorldSelect;
        self.selected = 0;
        self.keyboard_focus = None;
    }

    pub fn open_create_world(&mut self) {
        self.screen = UiScreen::CreateWorld;
        self.selected = 0;
        self.keyboard_focus = None;
        self.world_name = "New World".to_string();
        self.world_seed.clear();
        self.world_gamemode = CreateGameMode::Survival;
        self.create_field = 0;
        self.create_name_cursor = 9;
        self.create_seed_cursor = 0;
        self.create_name_selection = None;
        self.create_seed_selection = None;
    }

    pub fn create_options(&self) -> CreateWorldOptions {
        CreateWorldOptions {
            name: self.world_name.trim().to_string(),
            seed: parse_world_seed(&self.world_seed),
            gamemode: self.world_gamemode,
        }
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
            UiScreen::WorldSelect => self.open_title(),
            UiScreen::CreateWorld => {
                self.screen = UiScreen::WorldSelect;
                self.selected = 0;
                self.keyboard_focus = None;
            }
            UiScreen::Title => {}
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
        if self.screen == UiScreen::WorldSelect {
            if let Some((visual_index, _)) = world_row_rects(width, height, self.gui_scale, self.worlds.len(), self.world_scroll_offset)
                .iter()
                .enumerate()
                .find(|(_, rect)| contains(**rect, x, y))
            {
                let world_index = self.world_scroll_offset + visual_index;
                self.selected_world = Some(world_index);
                self.confirm_delete_world = false;
                self.keyboard_focus = None;
                return UiAction::None;
            }
        }
        if self.screen == UiScreen::CreateWorld {
            let (name_rect, seed_rect) = create_field_rects(width, height, self.gui_scale);
            if contains(name_rect, x, y) {
                self.create_field = 0;
            } else if contains(seed_rect, x, y) {
                self.create_field = 1;
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
            UiScreen::Controls => 1,
            UiScreen::Accessibility => 5,
            UiScreen::Connect => 2,
            UiScreen::Title => 4,
            UiScreen::WorldSelect => 4,
            UiScreen::CreateWorld => 3,
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
                    let current = RENDER_DISTANCE_PRESETS.iter().position(|(d, _)| *d == self.render_distance).unwrap_or(1);
                    let next_index = if left {
                        (current + RENDER_DISTANCE_PRESETS.len() - 1) % RENDER_DISTANCE_PRESETS.len()
                    } else {
                        (current + 1) % RENDER_DISTANCE_PRESETS.len()
                    };
                    self.render_distance = RENDER_DISTANCE_PRESETS[next_index].0;
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
                0 => {
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
                    UiAction::OpenWorldSelect
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
            UiScreen::WorldSelect => match index {
                0 if self.selected_world.is_some() => UiAction::LoadSelectedWorld,
                1 => {
                    self.open_create_world();
                    UiAction::None
                }
                2 => {
                    self.open_title();
                    UiAction::None
                }
                3 => {
                    if self.confirm_delete_world {
                        UiAction::DeleteWorld
                    } else {
                        self.confirm_delete_world = true;
                        UiAction::None
                    }
                }
                _ => UiAction::None,
            },
            UiScreen::CreateWorld => match index {
                0 => {
                    self.world_gamemode = self.world_gamemode.next();
                    UiAction::None
                }
                1 if !self.world_name.trim().is_empty() => UiAction::CreateWorld,
                2 => {
                    self.screen = UiScreen::WorldSelect;
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
                } else if self.screen == UiScreen::WorldSelect {
                    self.move_world_selection(-1);
                    UiAction::None
                } else {
                    UiAction::None
                }
            }
            KeyCode::ArrowRight => {
                if self.screen == UiScreen::Options {
                    self.activate_with_direction(self.selected, false)
                } else if self.screen == UiScreen::WorldSelect {
                    self.move_world_selection(1);
                    UiAction::None
                } else {
                    UiAction::None
                }
            }
            KeyCode::Tab => {
                if self.screen == UiScreen::Connect {
                    self.switch_connect_field();
                } else if self.screen == UiScreen::CreateWorld {
                    self.create_field = (self.create_field + 1) % 2;
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

    pub fn append_create_text(&mut self, value: &str) {
        if self.screen == UiScreen::CreateWorld {
            if self.create_field == 0 {
                self.world_name.push_str(value);
            } else {
                self.world_seed.push_str(value);
            }
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

    pub fn insert_create_text(&mut self, value: &str) {
        if self.screen != UiScreen::CreateWorld { return; }
        let cursor = if self.create_field == 0 { &mut self.create_name_cursor } else { &mut self.create_seed_cursor };
        let selection = if self.create_field == 0 { &mut self.create_name_selection } else { &mut self.create_seed_selection };
        let field = if self.create_field == 0 { &mut self.world_name } else { &mut self.world_seed };
        if selection.is_some() {
            let sel = selection.take().unwrap();
            let start = sel.min(*cursor);
            let end = sel.max(*cursor);
            let byte_start = char_byte_index(field, start);
            let byte_end = char_byte_index(field, end);
            field.replace_range(byte_start..byte_end, "");
            *cursor = start;
        }
        if field.chars().count() >= 64 || value.chars().any(char::is_control) {
            return;
        }
        let byte_idx = char_byte_index(field, *cursor);
        field.insert_str(byte_idx, value);
        *cursor += value.chars().count();
    }

    pub fn backspace_create_text(&mut self) {
        if self.screen != UiScreen::CreateWorld { return; }
        let cursor = if self.create_field == 0 { &mut self.create_name_cursor } else { &mut self.create_seed_cursor };
        let selection = if self.create_field == 0 { &mut self.create_name_selection } else { &mut self.create_seed_selection };
        let field = if self.create_field == 0 { &mut self.world_name } else { &mut self.world_seed };
        if let Some(sel) = selection.take() {
            let start = sel.min(*cursor);
            let end = sel.max(*cursor);
            let byte_start = char_byte_index(field, start);
            let byte_end = char_byte_index(field, end);
            field.replace_range(byte_start..byte_end, "");
            *cursor = start;
            return;
        }
        if *cursor == 0 { return; }
        let byte_start = char_byte_index(field, *cursor - 1);
        let byte_end = char_byte_index(field, *cursor);
        field.replace_range(byte_start..byte_end, "");
        *cursor -= 1;
    }

    pub fn delete_create_text(&mut self) {
        if self.screen != UiScreen::CreateWorld { return; }
        let cursor = if self.create_field == 0 { &mut self.create_name_cursor } else { &mut self.create_seed_cursor };
        let selection = if self.create_field == 0 { &mut self.create_name_selection } else { &mut self.create_seed_selection };
        let field = if self.create_field == 0 { &mut self.world_name } else { &mut self.world_seed };
        if let Some(sel) = selection.take() {
            let start = sel.min(*cursor);
            let end = sel.max(*cursor);
            let byte_start = char_byte_index(field, start);
            let byte_end = char_byte_index(field, end);
            field.replace_range(byte_start..byte_end, "");
            *cursor = start;
            return;
        }
        if *cursor >= field.chars().count() { return; }
        let byte_start = char_byte_index(field, *cursor);
        let byte_end = char_byte_index(field, *cursor + 1);
        field.replace_range(byte_start..byte_end, "");
    }

    pub fn move_create_cursor(&mut self, delta: i32, extend_selection: bool) {
        if self.screen != UiScreen::CreateWorld { return; }
        let cursor = if self.create_field == 0 { &mut self.create_name_cursor } else { &mut self.create_seed_cursor };
        let selection = if self.create_field == 0 { &mut self.create_name_selection } else { &mut self.create_seed_selection };
        let field = if self.create_field == 0 { &self.world_name } else { &self.world_seed };
        let length = field.chars().count() as i32;
        if !extend_selection {
            *selection = None;
        } else if selection.is_none() {
            *selection = Some(*cursor);
        }
        *cursor = (*cursor as i32 + delta).clamp(0, length) as usize;
    }

    pub fn move_create_to_start(&mut self, extend_selection: bool) {
        if self.screen != UiScreen::CreateWorld { return; }
        let cursor = if self.create_field == 0 { &mut self.create_name_cursor } else { &mut self.create_seed_cursor };
        let selection = if self.create_field == 0 { &mut self.create_name_selection } else { &mut self.create_seed_selection };
        if !extend_selection {
            *selection = None;
        } else if selection.is_none() {
            *selection = Some(*cursor);
        }
        *cursor = 0;
    }

    pub fn move_create_to_end(&mut self, extend_selection: bool) {
        if self.screen != UiScreen::CreateWorld { return; }
        let cursor = if self.create_field == 0 { &mut self.create_name_cursor } else { &mut self.create_seed_cursor };
        let selection = if self.create_field == 0 { &mut self.create_name_selection } else { &mut self.create_seed_selection };
        let field = if self.create_field == 0 { &self.world_name } else { &self.world_seed };
        if !extend_selection {
            *selection = None;
        } else if selection.is_none() {
            *selection = Some(*cursor);
        }
        *cursor = field.chars().count();
    }

    pub fn create_cursor_info(&self) -> Option<(usize, Option<(usize, usize)>)> {
        if self.screen != UiScreen::CreateWorld { return None; }
        let (cursor, selection) = if self.create_field == 0 {
            (self.create_name_cursor, self.create_name_selection)
        } else {
            (self.create_seed_cursor, self.create_seed_selection)
        };
        Some((cursor, selection.map(|s| (s.min(cursor), s.max(cursor)))))
    }

    fn move_world_selection(&mut self, direction: i32) {
        if self.worlds.is_empty() { return; }
        let current = self.selected_world.unwrap_or(0) as i32;
        self.selected_world = Some((current + direction).rem_euclid(self.worlds.len() as i32) as usize);
    }

    pub fn clamp_world_scroll(&mut self, visible_count: usize) {
        if visible_count == 0 || self.worlds.is_empty() { return; }
        if let Some(sel) = self.selected_world {
            if sel < self.world_scroll_offset {
                self.world_scroll_offset = sel;
            } else if self.world_scroll_offset + visible_count <= sel {
                self.world_scroll_offset = sel + 1 - visible_count;
            }
        }
        self.world_scroll_offset = self.world_scroll_offset.min(self.worlds.len().saturating_sub(visible_count));
    }

    pub fn scroll_world_list(&mut self, direction: isize, visible_count: usize) {
        if self.worlds.is_empty() || visible_count == 0 { return; }
        let max_offset = self.worlds.len().saturating_sub(visible_count);
        self.world_scroll_offset = self.world_scroll_offset
            .saturating_add_signed(direction)
            .min(max_offset);
    }

    fn button_rects(&self, width: f32, height: f32) -> Vec<(f32, f32, f32, f32)> {
        let button_w = self.button_width(width);
        let button_h = 20.0 * self.gui_scale;
        let gap = 4.0 * self.gui_scale;
        let left = (width - button_w) * 0.5;
        let count = self.button_count();
        let top = match self.screen {
            // Title screen buttons follow the classic Minecraft layout:
            // virtual Y = height/4 + 48, then 24px spacing between tops.
            UiScreen::Title => height / 4.0 + 48.0 * self.gui_scale,
            UiScreen::WorldSelect => {
                let total_h = count as f32 * button_h + (count - 1) as f32 * gap;
                height - total_h - 12.0 * self.gui_scale
            }
            UiScreen::CreateWorld => {
                let total_h = count as f32 * button_h + (count - 1) as f32 * gap;
                let seed_bottom = height * 0.29 + 74.0 * self.gui_scale;
                let from_bottom = height - 84.0 * self.gui_scale;
                let max_top = (height - total_h).max(0.0);
                from_bottom.max(seed_bottom).min(max_top)
            }
            _ => {
                let total_h = count as f32 * button_h + (count - 1) as f32 * gap;
                (height - total_h) * 0.5 + 20.0 * self.gui_scale
            }
        };
        (0..count)
            .map(|index| {
                let y = top + index as f32 * 24.0 * self.gui_scale;
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
        saturation: f32,
        absorption: f32,
        armor_points: f32,
        experience: f32,
        selected_item_name: &str,
        chat_lines: &[String],
        feedback: Option<&str>,
        cursor: Option<(f32, f32)>,
        show_crosshair: bool,
        hurt_timer: f32,
        health_blink_timer: f32,
        hunger_shake_timer: f32,
        has_regen: bool,
        has_poison: bool,
        has_wither: bool,
        has_hunger_effect: bool,
        tick: u64,
        chat_cursor_info: Option<(usize, Option<(usize, usize)>)>,
        show_chat_cursor: bool,
    ) -> UiFrame {
        let mut frame = UiFrame::default();
        match self.screen {
            UiScreen::Playing => {
                draw_hud(&mut frame, width, height, self.gui_scale, hotbar, health, hunger, saturation, absorption, armor_points, experience, selected_item_name, show_crosshair, hurt_timer, health_blink_timer, hunger_shake_timer, has_regen, has_poison, has_wither, has_hunger_effect, tick);
                draw_chat(&mut frame, width, height, chat_lines, self.chat_opacity, chat_cursor_info, &self.glyph_advances, show_chat_cursor);
                if let Some(feedback) = feedback.filter(|text| !text.is_empty()) {
                    draw_toast(&mut frame, width, feedback);
                }
            }
            UiScreen::Inventory => {
                draw_hud(&mut frame, width, height, self.gui_scale, hotbar, health, hunger, saturation, absorption, armor_points, experience, selected_item_name, false, hurt_timer, health_blink_timer, hunger_shake_timer, has_regen, has_poison, has_wither, has_hunger_effect, tick);
                draw_inventory(&mut frame, width, height, self.gui_scale, inventory.unwrap_or(&[]), crafting.unwrap_or(&[]), craft_result, carried, cursor, self.high_contrast);
            }
            UiScreen::Pause => draw_menu(&mut frame, width, height, self, cursor, "Game Menu", &["Back to Game", "Options...", "Controls...", "Save and Quit to Title"]),
            UiScreen::Options => draw_options(&mut frame, width, height, self, cursor),
            UiScreen::Controls => draw_controls(&mut frame, width, height, self, cursor),
            UiScreen::Accessibility => draw_accessibility(&mut frame, width, height, self, cursor),
            UiScreen::Connect => draw_connect(&mut frame, width, height, self, cursor, feedback),
            UiScreen::Title => draw_title_screen(&mut frame, width, height, self, cursor),
            UiScreen::WorldSelect => draw_world_select(&mut frame, width, height, self, cursor),
            UiScreen::CreateWorld => draw_create_world(&mut frame, width, height, self, cursor),
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
            hint: item.hint,
        });
    }
    // Count text anchored to bottom-right of the slot area
    if item.count > 1 {
        let cw = 8.0 * scale;
        let text = item.count.to_string();
        let text_w = cw * text.len() as f32 * 0.75;
        let tx = x + 16.0 * scale - text_w - 1.0;
        let ty = y + 16.0 * scale - cw - 1.0;
        // Text commands use the shared Minecraft-style shadow path.
        frame.commands.push(UiCommand::Text {
            x: tx, y: ty, size: cw, text,
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
}

fn draw_hud(frame: &mut UiFrame, width: f32, height: f32, scale: f32, hotbar: &[UiSlot], health: f32, hunger: f32, saturation: f32, absorption: f32, armor: f32, experience: f32, _selected_item_name: &str, show_crosshair: bool, hurt_timer: f32, health_blink_timer: f32, hunger_shake_timer: f32, has_regen: bool, has_poison: bool, has_wither: bool, has_hunger_effect: bool, tick: u64) {
    let bar_w = 182.0 * scale;
    let bar_h = 22.0 * scale;
    let left = (width - bar_w) * 0.5;
    let top = height - bar_h - 4.0 * scale;

    // Experience bar
    let exp_bar_w = 182.0 * scale;
    let exp_bar_h = 5.0 * scale;
    let exp_left = (width - exp_bar_w) * 0.5;
    let exp_top = top - 7.0 * scale;
    rect(frame, exp_left, exp_top, exp_bar_w, exp_bar_h, [0.0, 0.0, 0.0, 0.6]);
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
                x: slot_left - 4.0 * scale,
                y: top - 1.0 * scale,
                w: 24.0 * scale,
                h: 24.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
        if let Some(item) = hotbar.get(index).filter(|item| !item.empty) {
            draw_slot_item(frame, slot_left, top + 3.0 * scale, scale, item);
        }
    }

    // Status bars row
    let status_y = top - 13.0 * scale;
    let half_width = 91.0 * scale;

    // ── Health bar ──────────────────────────────────────────────────────
    let health_left = width * 0.5 - half_width;
    let hurt_active = hurt_timer > 0.0;
    let (shake_x, hurt_color) = if hurt_active {
        let shake = (hurt_timer * 60.0 * 3.14).sin() * 2.0 * scale * (1.0 - hurt_timer / 0.5);
        let flash = 1.0 - hurt_timer / 0.5;
        (shake, [1.0, 1.0 - flash * 0.6, 1.0 - flash * 0.6, 1.0])
    } else {
        (0.0, [1.0, 1.0, 1.0, 1.0])
    };

    // Blinking alternates every 3 ticks when blink timer is active
    let blinking = health_blink_timer > 0.0 && (tick.wrapping_mul(3)) % 6 < 3;

    // Total hearts to render (normal + absorption)
    let total_hearts = 10 + (absorption / 2.0).ceil() as usize;
    let abs_start = 10usize;
    let mut absorb_remaining = absorption;

    for index in 0..total_hearts {
        let mut hy = status_y;
        let hx = health_left + index as f32 * 8.0 * scale + shake_x;

        // Low-health wobble: random Y offset (seeded by tick + index)
        if health <= 4.0 && index < 10 {
            let seed = tick.wrapping_add(index as u64 * 7);
            let wobble = (seed % 3) as f32 - 1.0;
            hy += wobble * scale * 0.5;
        }

        // Regeneration bounce: one heart bounces up 2px
        if has_regen && index < 10 && index == (tick as usize % 10) {
            hy -= 2.0 * scale;
        }

        if index < abs_start {
            // ── Normal hearts (indices 0-9) ──
            let container_name = if blinking { "hud/heart/container_blinking" } else { "hud/heart/container" };
            frame.commands.push(UiCommand::Sprite {
                name: container_name.to_string(),
                x: hx,
                y: hy,
                w: 9.0 * scale,
                h: 9.0 * scale,
                color: hurt_color,
            });

            let is_full = health >= index as f32 * 2.0 + 2.0;
            let is_half = !is_full && health > index as f32 * 2.0;
            if is_full || is_half {
                let (heart_name, blinking_name) = if has_wither {
                    if is_full { ("hud/heart/withered_full", "hud/heart/withered_full_blinking") }
                    else { ("hud/heart/withered_half", "hud/heart/withered_half_blinking") }
                } else if has_poison {
                    if is_full { ("hud/heart/poisoned_full", "hud/heart/poisoned_full_blinking") }
                    else { ("hud/heart/poisoned_half", "hud/heart/poisoned_half_blinking") }
                } else {
                    if is_full { ("hud/heart/full", "hud/heart/full_blinking") }
                    else { ("hud/heart/half", "hud/heart/half_blinking") }
                };
                let name = if blinking { blinking_name } else { heart_name };
                frame.commands.push(UiCommand::Sprite {
                    name: name.to_string(),
                    x: hx,
                    y: hy,
                    w: 9.0 * scale,
                    h: 9.0 * scale,
                    color: hurt_color,
                });
            }
        } else {
            // ── Absorption hearts (yellow) ──
            let container_name = if blinking { "hud/heart/container_blinking" } else { "hud/heart/container" };
            frame.commands.push(UiCommand::Sprite {
                name: container_name.to_string(),
                x: hx,
                y: hy,
                w: 9.0 * scale,
                h: 9.0 * scale,
                color: [1.0, 1.0, 1.0, 1.0],
            });

            if absorb_remaining >= 2.0 {
                let abs_name = if blinking { "hud/heart/absorbing_full_blinking" } else { "hud/heart/absorbing_full" };
                frame.commands.push(UiCommand::Sprite {
                    name: abs_name.to_string(),
                    x: hx,
                    y: hy,
                    w: 9.0 * scale,
                    h: 9.0 * scale,
                    color: [1.0, 1.0, 1.0, 1.0],
                });
                absorb_remaining -= 2.0;
            } else if absorb_remaining > 0.0 {
                let abs_name = if blinking { "hud/heart/absorbing_half_blinking" } else { "hud/heart/absorbing_half" };
                frame.commands.push(UiCommand::Sprite {
                    name: abs_name.to_string(),
                    x: hx,
                    y: hy,
                    w: 9.0 * scale,
                    h: 9.0 * scale,
                    color: [1.0, 1.0, 1.0, 1.0],
                });
                absorb_remaining = 0.0;
            }
        }
    }

    // ── Armor bar ─────────────────────────────────────────────────────
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

    // ── Food bar ───────────────────────────────────────────────────────
    let food_left = width * 0.5 + half_width - 81.0 * scale;
    let use_hunger_textures = has_hunger_effect;
    for index in 0..10 {
        let fx = food_left + index as f32 * 8.0 * scale;
        let mut fy = status_y;

        // Low hunger / saturation wobble
        let hunger_low = hunger <= 6.0;
        let no_saturation = saturation <= 0.0;
        if has_hunger_effect || (hunger_low && no_saturation) || hunger_shake_timer > 0.0 {
            let seed = tick.wrapping_add(index as u64 * 13);
            let wobble = (seed % 3) as f32 - 1.0;
            fy += wobble * scale * 0.5;
            if hunger_shake_timer > 0.0 {
                let extra = ((tick as f32 * 60.0).sin() * 0.5) as f32;
                fy += extra;
            }
        }

        let empty_sprite = if use_hunger_textures { "hud/food_empty_hunger" } else { "hud/food_empty" };
        let (fill_sprite, show_fill) = if hunger >= index as f32 * 2.0 + 2.0 {
            (if use_hunger_textures { "hud/food_full_hunger" } else { "hud/food_full" }, true)
        } else if hunger > index as f32 * 2.0 {
            (if use_hunger_textures { "hud/food_half_hunger" } else { "hud/food_half" }, true)
        } else {
            ("", false)
        };

        // Saturation overlay: show a subtle background food when saturation covers this drumstick
        if saturation > index as f32 * 2.0 {
            let sat_sprite = if saturation >= index as f32 * 2.0 + 2.0 { "hud/food_full" } else { "hud/food_half" };
            frame.commands.push(UiCommand::Sprite {
                name: sat_sprite.to_string(),
                x: fx,
                y: fy - scale * 0.5,
                w: 9.0 * scale,
                h: 9.0 * scale,
                color: [1.0, 1.0, 1.0, 0.25],
            });
        }

        // Empty background
        frame.commands.push(UiCommand::Sprite {
            name: empty_sprite.to_string(),
            x: fx,
            y: fy,
            w: 9.0 * scale,
            h: 9.0 * scale,
            color: [1.0, 1.0, 1.0, 1.0],
        });

        // Filled portion
        if show_fill {
            frame.commands.push(UiCommand::Sprite {
                name: fill_sprite.to_string(),
                x: fx,
                y: fy,
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
            (8.0 + (index - 9) as f32 % 9.0 * 18.0, 83.0 + (index - 9) as f32 / 9.0 * 18.0)
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

fn draw_chat(
    frame: &mut UiFrame,
    width: f32,
    height: f32,
    lines: &[String],
    opacity: f32,
    cursor_info: Option<(usize, Option<(usize, usize)>)>,
    glyph_advances: &[f32; ASCII_GLYPH_COUNT],
    show_cursor: bool,
) {
    if lines.is_empty() { return; }
    let line_h = 18.0;
    let chat_width = (width * 0.45).clamp(240.0, 520.0);
    let max_chars = ((chat_width - 20.0) / 7.0).floor().max(1.0) as usize;
    let wrapped: Vec<String> = lines.iter().flat_map(|line| wrap_chat_line(line, max_chars)).collect();
    let visible_start = wrapped.len().saturating_sub(10);
    let visible = &wrapped[visible_start..];
    let top = height - 34.0 - visible.len() as f32 * line_h;
    rect(frame, 8.0, top - 4.0, chat_width, visible.len() as f32 * line_h + 8.0, [0.0, 0.0, 0.0, opacity]);
    for (index, line) in visible.iter().enumerate() {
        text(frame, 14.0, top + index as f32 * line_h, 14.0, line, [1.0, 1.0, 1.0, 1.0]);
    }
    // Draw cursor and selection on the input line (last original line)
    if let (Some((cursor_char, selection)), Some(input_text)) = (cursor_info, lines.last()) {
        let input_line_index = visible.len() - 1;
        let input_y = top + input_line_index as f32 * line_h;
        let text_x = 14.0;
        let text_size = 14.0;
        if show_cursor {
            if let Some((sel_start, sel_end)) = selection {
                let sel_byte_start = char_byte_index(input_text, sel_start);
                let sel_byte_end = char_byte_index(input_text, sel_end);
                let sel_start_x = text_x + measure_text_width(&input_text[..sel_byte_start], text_size, glyph_advances);
                let sel_w = measure_text_width(&input_text[sel_byte_start..sel_byte_end], text_size, glyph_advances);
                frame.commands.push(UiCommand::TextSelection {
                    x: sel_start_x,
                    y: input_y,
                    w: sel_w,
                    h: text_size,
                });
            }
            let cursor_byte = char_byte_index(input_text, cursor_char);
            let cursor_x = text_x + measure_text_width(&input_text[..cursor_byte], text_size, glyph_advances);
            frame.commands.push(UiCommand::Cursor {
                x: cursor_x,
                y: input_y,
                h: text_size,
            });
        }
    }
}

fn wrap_chat_line(line: &str, max_chars: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    let mut wrapped = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        let required = word.chars().count() + usize::from(!current.is_empty());
        if !current.is_empty() && current.chars().count() + required > max_chars {
            wrapped.push(std::mem::take(&mut current));
        }
        if word.chars().count() > max_chars {
            if !current.is_empty() {
                current.push(' ');
            }
            for character in word.chars() {
                if current.chars().count() == max_chars {
                    wrapped.push(std::mem::take(&mut current));
                }
                current.push(character);
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        wrapped.push(current);
    }
    wrapped
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
        let text_end = input_x + pad_x + measure_text_width(&state.server_address, text_size, &state.glyph_advances);
        rect(frame, text_end, address_y + pad_y, 2.0 * scale, text_size, [1.0, 1.0, 1.0, 0.8]);
    }

    let field_gap = 18.0 * scale;
    let username_y = address_y + input_h + field_gap;
    text(frame, input_x, username_y - label_size - pad_y, label_size, "Username", [0.82, 0.82, 0.88, 1.0]);
    nine_slice(frame, "widget/text_field", input_x, username_y, input_w, input_h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    text(frame, input_x + pad_x, username_y + pad_y, text_size, &state.connect_username,
         if state.connect_field == 1 { [1.0, 1.0, 1.0, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
    if state.connect_field == 1 && state.frame_count % 60 < 30 {
        let text_end = input_x + pad_x + measure_text_width(&state.connect_username, text_size, &state.glyph_advances);
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
        {
            let preset_name = RENDER_DISTANCE_PRESETS.iter().find(|(d, _)| *d == state.render_distance).map(|(_, n)| *n).unwrap_or("Custom");
            format!("Render Distance: {} ({} chunks)", preset_name, state.render_distance)
        },
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
        ("W / A / S / D", "Move / Strafe"),
        ("Space", "Jump"),
        ("Shift", "Sneak"),
        ("Ctrl", "Sprint"),
        ("E", "Inventory"),
        ("Q", "Drop Held Item"),
        ("1-9", "Select Hotbar Slot"),
        ("Scroll", "Cycle Hotbar Slot"),
        ("Left Click", "Attack / Break"),
        ("Right Click", "Place / Use"),
        ("T", "Chat"),
        ("/", "Command"),
        ("F", "Toggle Flight"),
        ("F1", "Toggle Debug HUD"),
        ("F3", "Toggle Profiler"),
        ("Esc", "Pause / Menu"),
    ];
    let text_size = 11.0 * state.gui_scale;
    let col_w = (width * 0.50).clamp(280.0, 500.0);
    let left = (width - col_w) * 0.5;
    let col1_x = left + col_w * 0.08;
    let col2_x = left + col_w * 0.42;
    let start_y = first_y + 6.0 * state.gui_scale;
    for (index, (key, action)) in lines.iter().enumerate() {
        let y = start_y + index as f32 * 18.0 * state.gui_scale;
        text(frame, col1_x, y, text_size, *key, [0.55, 0.43, 0.18, 1.0]);
        text(frame, col2_x, y, text_size, *action, [1.0, 1.0, 1.0, 1.0]);
    }
    // "Done" button only
    if let Some(&(x, y, w, h)) = rects.last() {
        let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
        minecraft_button(frame, x, y, w, h, "Done", state.keyboard_focus == Some(0), hovered);
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
    frame.commands.push(UiCommand::TiledBackground {
        sprite: "gui/menu_background".to_string(),
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
    });
    panel(frame, width, height, state.high_contrast);

    // Title logo sprite
    // Matches Minecraft Java Edition LogoRenderer: LOGO_HEIGHT=44, DEFAULT_HEIGHT_OFFSET=30 (virtual pixels)
    let logo_height = 44.0 * state.gui_scale;
    let logo_y = 30.0 * state.gui_scale;
    frame.commands.push(UiCommand::TitleLogo {
        sprite: "title/vibecraft".to_string(),
        center_x: width * 0.5,
        y: logo_y,
        pixel_height: logo_height,
    });

    // Buttons: Singleplayer, Multiplayer, Options..., Quit
    let labels = ["Singleplayer", "Multiplayer", "Options...", "Quit"];
    let rects = state.button_rects(width, height);
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = rects.get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }

    // Copyright footer
    let copyright = "Copyright Bobby AI";
    let text_size = 10.0 * state.gui_scale;
    centered_text(frame, width * 0.5, height - 18.0 * state.gui_scale, text_size, copyright, [0.5, 0.5, 0.5, 1.0]);
}

pub fn world_list_visible_count(height: f32, scale: f32) -> usize {
    let button_h = 20.0 * scale;
    let gap = 4.0 * scale;
    let buttons_total = 4.0 * button_h + 3.0 * gap;
    let buttons_top = height - buttons_total - 12.0 * scale;
    let list_top = height * 0.18;
    let avail = buttons_top - 8.0 * scale - list_top;
    if avail <= 0.0 { return 0; }
    let row_total = 36.0 * scale + 6.0 * scale;
    (avail / row_total) as usize
}

fn world_row_rects(width: f32, height: f32, scale: f32, count: usize, scroll_offset: usize) -> Vec<(f32, f32, f32, f32)> {
    let row_w = (width - 48.0 * scale).min(460.0 * scale);
    let row_h = 36.0 * scale;
    let left = (width - row_w) * 0.5;
    let visible = world_list_visible_count(height, scale);
    let list_top = height * 0.18;
    let end = (scroll_offset + visible).min(count);
    (scroll_offset..end)
        .map(|i| (left, list_top + (i - scroll_offset) as f32 * (row_h + 6.0 * scale), row_w, row_h))
        .collect()
}

fn create_field_rects(width: f32, height: f32, scale: f32) -> ((f32, f32, f32, f32), (f32, f32, f32, f32)) {
    let field_w = (width - 48.0 * scale).min(400.0 * scale);
    let field_h = 20.0 * scale;
    let left = (width - field_w) * 0.5;
    let name_y = height * 0.29;
    ((left, name_y, field_w, field_h), (left, name_y + 54.0 * scale, field_w, field_h))
}

fn draw_world_select(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    // Dirt background (Minecraft-style settings/menu background)
    frame.commands.push(UiCommand::TiledBackground {
        sprite: "gui/menu_background".to_string(),
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
    });
    panel(frame, width, height, state.high_contrast);
    let scale = state.gui_scale;
    centered_text(frame, width * 0.5, height * 0.10, 12.0 * scale, "Select World", [1.0, 1.0, 1.0, 1.0]);
    if state.worlds.is_empty() {
        centered_text(frame, width * 0.5, height * 0.34, 10.0 * scale, "No worlds found", [0.72, 0.72, 0.72, 1.0]);
    } else {
        let rows = world_row_rects(width, height, scale, state.worlds.len(), state.world_scroll_offset);
        for (visual_index, ((x, y, w, h), world)) in rows.into_iter().zip(state.worlds.iter().skip(state.world_scroll_offset)).enumerate() {
            let world_index = state.world_scroll_offset + visual_index;
            let selected = state.selected_world == Some(world_index);
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            nine_slice(frame, if selected || hovered { "widget/button_highlighted" } else { "widget/button" }, x, y, w, h, 3.0, [1.0, 1.0, 1.0, 1.0]);
            text(frame, x + 7.0 * scale, y + 5.0 * scale, 10.0 * scale, &world.name, [1.0, 1.0, 1.0, 1.0]);
            let mode = if world.hardcore { "Hardcore" } else { &world.gamemode };
            text(frame, x + 7.0 * scale, y + 19.0 * scale, 8.0 * scale, format!("{}  |  Last played {}", mode, world_time_label(world.last_played)), [0.72, 0.72, 0.72, 1.0]);
        }
    }
    let delete_label = if state.confirm_delete_world { "Confirm Delete" } else { "Delete" };
    let labels = ["Play Selected World", "Create New World", "Cancel", delete_label];
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = state.button_rects(width, height).get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }
}

fn draw_create_world(frame: &mut UiFrame, width: f32, height: f32, state: &UiState, cursor: Option<(f32, f32)>) {
    frame.commands.push(UiCommand::TiledBackground {
        sprite: "gui/menu_background".to_string(),
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
    });
    panel(frame, width, height, state.high_contrast);
    let scale = state.gui_scale;
    centered_text(frame, width * 0.5, height * 0.12, 12.0 * scale, "Create New World", [1.0, 1.0, 1.0, 1.0]);
    let (name_rect, seed_rect) = create_field_rects(width, height, scale);
    let show_create_cursor = state.frame_count % 60 < 30;
    let name_cursor = if state.create_field == 0 { state.create_cursor_info() } else { None };
    let seed_cursor = if state.create_field == 1 { state.create_cursor_info() } else { None };
    draw_create_field(frame, "World Name", &state.world_name, name_rect, state.create_field == 0, scale, &state.glyph_advances, name_cursor, show_create_cursor);
    draw_create_field(frame, "Seed for the world generator", &state.world_seed, seed_rect, state.create_field == 1, scale, &state.glyph_advances, seed_cursor, show_create_cursor);
    let labels = [
        format!("Game Mode: {}", state.world_gamemode.label()),
        "Create New World".to_string(),
        "Cancel".to_string(),
    ];
    for (index, label) in labels.iter().enumerate() {
        if let Some(&(x, y, w, h)) = state.button_rects(width, height).get(index) {
            let hovered = cursor.map_or(false, |(cx, cy)| contains((x, y, w, h), cx, cy));
            minecraft_button(frame, x, y, w, h, label, state.keyboard_focus == Some(index), hovered);
        }
    }
}

fn draw_create_field(frame: &mut UiFrame, label: &str, value: &str, (x, y, w, h): (f32, f32, f32, f32), active: bool, scale: f32, glyph_advances: &[f32; ASCII_GLYPH_COUNT], cursor_info: Option<(usize, Option<(usize, usize)>)>, show_cursor: bool) {
    let label_size = 12.0 * scale;
    let pad_y = 4.0 * scale;
    text(frame, x, y - label_size - pad_y, label_size, label, [0.82, 0.82, 0.88, 1.0]);
    nine_slice(frame, "widget/text_field", x, y, w, h, 3.0, [1.0, 1.0, 1.0, 1.0]);
    let text_size = 12.0 * scale;
    let pad_x = 6.0 * scale;
    text(frame, x + pad_x, y + pad_y, text_size, value, if active { [1.0; 4] } else { [0.6, 0.6, 0.6, 1.0] });
    if active && show_cursor {
        if let Some((cursor_char, selection)) = cursor_info {
            if let Some((sel_start, sel_end)) = selection {
                let sel_byte_start = char_byte_index(value, sel_start);
                let sel_byte_end = char_byte_index(value, sel_end);
                let sel_start_x = x + pad_x + measure_text_width(&value[..sel_byte_start], text_size, glyph_advances);
                let sel_w = measure_text_width(&value[sel_byte_start..sel_byte_end], text_size, glyph_advances);
                frame.commands.push(UiCommand::TextSelection {
                    x: sel_start_x,
                    y: y + pad_y,
                    w: sel_w,
                    h: text_size,
                });
            }
            let cursor_byte = char_byte_index(value, cursor_char);
            let cursor_x = x + pad_x + measure_text_width(&value[..cursor_byte], text_size, glyph_advances);
            frame.commands.push(UiCommand::Cursor {
                x: cursor_x,
                y: y + pad_y,
                h: text_size,
            });
        }
    }
}

fn world_time_label(value: u64) -> String {
    if value == 0 { "unknown".to_string() } else { format!("{}", value / 1_000) }
}

/// Matches Java Edition's seed handling for this native generator: signed long
/// text is used directly; other text becomes Java String.hashCode() sign-extended.
pub fn parse_world_seed(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() { return None; }
    if let Ok(seed) = value.parse::<i64>() { return Some(seed as u64); }
    let hash = value.encode_utf16().fold(0i32, |hash, unit| hash.wrapping_mul(31).wrapping_add(unit as i32));
    Some((hash as i64) as u64)
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
            20.0,
            0.0,
            0.0,
            0.0,
            "",
            &[],
            None,
            None,
            false,
            0.0,
            0.0,
            0.0,
            false,
            false,
            false,
            false,
            0,
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
            800.0, 600.0, &[], None, None, None, None, 20.0, 20.0, 20.0, 0.0, 0.0, 0.0, "", &[], None, None, false, 0.0, 0.0, 0.0, false, false, false, false, 0, None, false,
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
            800.0, 600.0, &[], None, None, None, None, 20.0, 20.0, 20.0, 0.0, 0.0, 0.0, "", &[], None, None, false, 0.0, 0.0, 0.0, false, false, false, false, 0, None, false,
        );
        assert!(frame.commands.iter().any(|command| matches!(command, UiCommand::Sprite { name, .. } if name == "hud/experience_bar_background")));
        assert!(frame.commands.iter().any(|command| matches!(command, UiCommand::SpriteProgress { name, progress, .. } if name == "hud/experience_bar_progress" && *progress == 0.5)));
    }

    #[test]
    fn connect_screen_edits_address_and_submits() {
        let mut ui = UiState::default();
        ui.screen = UiScreen::Title;
        assert_eq!(ui.activate_focused(), UiAction::OpenWorldSelect);
        assert_eq!(ui.screen, UiScreen::Title);
        ui.open_world_select(Vec::new());
        assert_eq!(ui.handle_escape(), UiAction::None);
        assert_eq!(ui.screen, UiScreen::Title);
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
    fn create_world_uses_java_style_seed_input() {
        assert_eq!(parse_world_seed(""), None);
        assert_eq!(parse_world_seed("-1"), Some(u64::MAX));
        assert_eq!(parse_world_seed("Aa"), parse_world_seed("BB"));
        assert_eq!(parse_world_seed("seed"), Some(3_526_257));
    }

    #[test]
    fn world_select_and_create_flow_use_textured_controls() {
        let mut ui = UiState::default();
        ui.open_world_select(vec![UiWorld {
            name: "New World".to_string(),
            gamemode: "survival".to_string(),
            hardcore: false,
            last_played: 0,
        }]);
        assert_eq!(ui.activate_focused(), UiAction::LoadSelectedWorld);
        ui.open_create_world();
        ui.append_create_text(" Test");
        assert_eq!(ui.world_name, "New World Test");
        let frame = ui.frame(800.0, 600.0, &[], None, None, None, None, 20.0, 20.0, 20.0, 0.0, 0.0, 0.0, "", &[], None, None, false, 0.0, 0.0, 0.0, false, false, false, false, 0, None, false);
        assert!(frame.commands.iter().any(|command| matches!(command, UiCommand::NineSlice { sprite, .. } if sprite == "widget/text_field")));
        assert!(frame.commands.iter().any(|command| matches!(command, UiCommand::NineSlice { sprite, .. } if sprite == "widget/button")));
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
        draw_hud(&mut frame, 400.0, 240.0, 1.0, &[item], 20.0, 20.0, 20.0, 0.0, 0.0, 0.0, "", true, 0.0, 0.0, 0.0, false, false, false, false, 0);
        let icon = frame.commands.iter().find_map(|command| match command {
            UiCommand::Item { x, y, size, .. } => Some((*x, *y, *size)),
            _ => None,
        }).unwrap();
        assert_eq!(icon.0, (400.0 - 182.0) * 0.5 + 4.0);
        assert_eq!(icon.1, 240.0 - 22.0 - 4.0 + 4.0);
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
