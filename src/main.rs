use vibecraft::config::{AppConfig, GraphicsQuality, ResolvedKeyBindings};
use vibecraft::chat;
use vibecraft::engine;
use vibecraft::gamemode::{Difficulty, GameMode};
use vibecraft::inventory;
use vibecraft::player;
use vibecraft::profiler;
use vibecraft::world;

use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::net::ToSocketAddrs;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::CursorGrabMode;

use engine::camera::Camera;
use engine::input::InputState;
use engine::renderer::{BreakOverlay, ChunkRenderData, HighlightData, RenderContext, Renderer};
use engine::window::WindowState;
use inventory::item::{ItemId, ItemRegistry};
use inventory::progression::{fuel_ticks, mining_outcome, CraftingGrid};
use inventory::Inventory;
use vibecraft::network::client::{ClientError, ClientPhase, ClientTransport};
use vibecraft::network::{decode_chunk_data, BlockEditAction, ClientMessage, DisconnectCode, Face, ServerMessage, WireBlockState};
use player::Player;
use world::block::{Block, BlockId};
use world::chunk::CHUNK_SIZE;
use world::chunk::BlockEntity;
use world::chunk_manager::ChunkManager;
use world::coordinates::WorldCoordinateProfile;
use world::generation::WorldGenerationProfile;
use world::world_gen::generator::VanillaWorldGenerator;
use world::dropped_item::{xp_orbs_to_mesh, DroppedItem, XpOrb};
use world::entity::{EntityKind, EntityStore, Transform};
use world::mesh::{build_item_cube_mesh, build_player_mesh, PlayerMeshInstance};
use world::persistence::{discover_worlds, DroppedItemData, LevelData, PlayerData, StorageError, WorldStorage, XpOrbData};
use world::simulation::{ScheduledTick, ScheduledTickKind, TickScheduler};
use vibecraft::ui::{self, UiAction, UiSlot};

#[derive(Clone, Debug)]
enum WorldOpenRequest {
    Existing(std::path::PathBuf),
    Create(ui::CreateWorldOptions),
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Resolve either an asset checkout root (`.../minecraft-26.2-assets`) or the
/// `assets/minecraft` directory within one. The reader always receives the
/// Minecraft namespace root.
fn minecraft_asset_directory(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let namespace_root = root.join("assets/minecraft");
    if namespace_root.is_dir() {
        return Some(namespace_root);
    }

    if root.join("textures").is_dir() && root.join("models").is_dir() {
        return Some(root.to_path_buf());
    }

    None
}

fn resolve_minecraft_asset_directory() -> Result<std::path::PathBuf, String> {
    if let Some(root) = std::env::var_os("VIBECRAFT_ASSETS") {
        let root = std::path::PathBuf::from(root);
        return minecraft_asset_directory(&root).ok_or_else(|| {
            format!(
                "asset root {} contains neither assets/minecraft nor a Minecraft asset namespace; set VIBECRAFT_ASSETS to a valid asset checkout",
                root.display()
            )
        });
    }

    // The supplied 26.2 assets normally live next to this repository. Keep
    // the historical CI/developer checkout as a fallback for existing setups.
    let candidates = [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../minecraft-26.2-assets"),
        std::path::PathBuf::from("/tmp/opencode/minecraft-assets"),
    ];
    candidates
        .iter()
        .find_map(|root| minecraft_asset_directory(root))
        .ok_or_else(|| {
            format!(
                "no Minecraft assets found; set VIBECRAFT_ASSETS to an asset checkout (searched {})",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn prepare_new_player_spawn(
    seed: u64,
    generation_profile: WorldGenerationProfile,
    world_spawn: &mut [i32; 3],
    player: &mut Player,
) {
    let (spawn_x, spawn_z) =
        VanillaWorldGenerator::from_seed(seed, generation_profile).suggested_spawn_position();
    world_spawn[0] = spawn_x;
    world_spawn[2] = spawn_z;
    *player = Player::new(spawn_x as f32, world_spawn[1] as f32, spawn_z as f32);
    log::info!("using climate-selected spawn search center at ({spawn_x}, {spawn_z})");
}

fn unique_world_directory(root: &std::path::Path, name: &str) -> Result<std::path::PathBuf, String> {
    let base: String = name
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | ' ' | '-' | '_' | '.' => character,
            _ => '_',
        })
        .collect::<String>()
        .trim()
        .to_string();
    if base.is_empty() || base == "." || base == ".." {
        return Err("World name must contain a usable folder name".to_string());
    }
    for index in 0..10_000 {
        let folder = if index == 0 { base.clone() } else { format!("{base} ({index})") };
        let path = root.join(folder);
        if !path.exists() { return Ok(path); }
    }
    Err("Could not allocate a unique world folder".to_string())
}

/// Convert screen pixel position to inventory slot index.
/// Returns None if the cursor is not over any slot.
// Hardcoded pixel coordinates matching the vanilla container GUI texture (container/inventory.png).
// Slot grid: 9 columns × 3 rows + 1 hotbar row, each slot is 18×18 px with 7px left/top padding
// in a 176×166 px (scaled to 256×256) texture.  These values must stay in sync with the atlas.
fn screen_pos_to_inventory_slot(
    mx: f32,
    my: f32,
    sw: f32,
    sh: f32,
    gui_scale: f32,
) -> Option<usize> {
    ui::inventory_slot_at(sw, sh, gui_scale, mx, my)
}

const MIN_LOCAL_LOAD_DISPLAY: std::time::Duration = std::time::Duration::from_millis(500);
const LOCAL_LOAD_STALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
// Vanilla verifies an 11x11 chunk area around its climate-selected spawn
// suggestion. Keep a comparable block-radius search after this port's
// asynchronous chunk admission step.
const SAFE_SPAWN_SEARCH_RADIUS_CHUNKS: i32 = 5;
const RESPAWN_SEARCH_RADIUS_BLOCKS: i32 = 10;
const SPAWN_SEARCH_CHUNK_LOOKAHEAD: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpawnSearchState {
    Searching,
    Selected([i32; 3]),
    Exhausted,
}

/// Deterministic, bounded state for new-world spawn selection. Chunk admission is
/// deliberately external so this state never performs synchronous generation.
struct SpawnSearch {
    candidates: Vec<(i32, i32)>,
    next_candidate: usize,
    state: SpawnSearchState,
}

impl SpawnSearch {
    fn new(center_x: i32, center_z: i32, radius_chunks: i32) -> Self {
        let radius = radius_chunks.max(0);
        let center_cx = center_x.div_euclid(CHUNK_SIZE as i32);
        let center_cz = center_z.div_euclid(CHUNK_SIZE as i32);
        let mut candidates = Vec::new();
        let mut dx = 0;
        let mut dz = 0;
        let mut step_x = 0;
        let mut step_z = -1;
        for _ in 0..(radius * 2 + 1).pow(2) {
            if (-radius..=radius).contains(&dx) && (-radius..=radius).contains(&dz) {
                candidates.push((center_cx + dx, center_cz + dz));
            }
            if dx == dz || (dx < 0 && dx == -dz) || (dx > 0 && dx == 1 - dz) {
                let old_step_x = step_x;
                step_x = -step_z;
                step_z = old_step_x;
            }
            dx += step_x;
            dz += step_z;
        }
        Self {
            candidates,
            next_candidate: 0,
            state: SpawnSearchState::Searching,
        }
    }

    fn current(&self) -> Option<(i32, i32)> {
        matches!(self.state, SpawnSearchState::Searching)
            .then(|| self.candidates.get(self.next_candidate).copied())
            .flatten()
    }

    fn required_chunks(&self) -> Vec<(i32, i32)> {
        let mut chunks = Vec::new();
        if !matches!(self.state, SpawnSearchState::Searching) {
            return chunks;
        }
        for &chunk in &self.candidates[self.next_candidate..] {
            chunks.push(chunk);
            if chunks.len() == SPAWN_SEARCH_CHUNK_LOOKAHEAD {
                break;
            }
        }
        chunks
    }

    fn reject_current(&mut self) {
        if !matches!(self.state, SpawnSearchState::Searching) {
            return;
        }
        self.next_candidate += 1;
        if self.next_candidate == self.candidates.len() {
            self.state = SpawnSearchState::Exhausted;
        }
    }

    fn select(&mut self, spawn: [i32; 3]) {
        self.state = SpawnSearchState::Selected(spawn);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum LocalLoadStage {
    #[default]
    Missing,
    Requested,
    Generated,
    Lit,
    CpuMesh,
    Uploaded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalLoadFailure {
    Chunk((i32, i32)),
    Stalled,
}

impl LocalLoadStage {
    const fn progress_units(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct LocalLoadSignals {
    requested: bool,
    admitted: bool,
    valid_lighting: bool,
    cpu_mesh: bool,
    uploaded: bool,
}

fn local_load_stage(signals: LocalLoadSignals) -> LocalLoadStage {
    if !signals.admitted {
        return if signals.requested {
            LocalLoadStage::Requested
        } else {
            LocalLoadStage::Missing
        };
    }
    if !signals.valid_lighting {
        return LocalLoadStage::Generated;
    }
    if !signals.cpu_mesh {
        return LocalLoadStage::Lit;
    }
    if signals.uploaded {
        LocalLoadStage::Uploaded
    } else {
        LocalLoadStage::CpuMesh
    }
}

fn mesh_is_uploaded(render_cache_contains_key: bool, mesh: &world::mesh::ChunkMesh) -> bool {
    render_cache_contains_key
        || (mesh.vertices.is_empty() && mesh.transparent_vertices.is_empty())
}

/// Main-thread loading state for the fixed terrain needed to enter a local world.
struct LocalLoadPlan {
    targets: Vec<(i32, i32)>,
    stages: HashMap<(i32, i32), LocalLoadStage>,
    started: std::time::Instant,
    last_progress: std::time::Instant,
}

impl LocalLoadPlan {
    fn new(
        player: &Player,
        world_spawn: [i32; 3],
        fresh_player: bool,
        started: std::time::Instant,
    ) -> Self {
        let (center_x, center_z) = if fresh_player {
            (
                world_spawn[0].div_euclid(CHUNK_SIZE as i32),
                world_spawn[2].div_euclid(CHUNK_SIZE as i32),
            )
        } else {
            (
                (player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                (player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            )
        };
        // 26.2 closes the client loading screen when the player's render
        // section is compiled and visible, not after every nearby chunk has a
        // GPU mesh. Neighbor columns remain streaming dependencies below.
        let targets = vec![(center_x, center_z)];
        let stages = targets
            .iter()
            .copied()
            .map(|key| (key, LocalLoadStage::Missing))
            .collect();
        Self {
            targets,
            stages,
            started,
            last_progress: started,
        }
    }

    fn refresh(
        &mut self,
        chunk_manager: &ChunkManager,
        render_cache: &HashMap<(i32, i32), ChunkRenderData>,
        now: std::time::Instant,
    ) {
        let mut progressed = false;
        for &key in &self.targets {
            let signals = if let Some(chunk) = chunk_manager.get_chunk(key.0, key.1) {
                // A queued remesh can leave an older CPU/GPU mesh in place. It is
                // not a completed load stage until the current task has published.
                let cpu_mesh = chunk.has_mesh
                    && !chunk.is_dirty
                    && !chunk_manager.is_chunk_meshing(key.0, key.1);
                let uploaded = cpu_mesh
                    && chunk_manager.get_chunk_mesh(key.0, key.1).is_some_and(|mesh| {
                        mesh_is_uploaded(render_cache.contains_key(&key), mesh)
                    });
                LocalLoadSignals {
                    admitted: true,
                    valid_lighting: !chunk.light_dirty,
                    cpu_mesh,
                    uploaded,
                    ..LocalLoadSignals::default()
                }
            } else {
                LocalLoadSignals {
                    requested: chunk_manager.is_chunk_pending(key.0, key.1),
                    ..LocalLoadSignals::default()
                }
            };
            let stage = local_load_stage(signals);
            progressed |= self.stages.insert(key, stage).is_some_and(|previous| stage > previous);
        }
        if progressed {
            self.last_progress = now;
            let mut stage_counts = [0usize; 6];
            for stage in self.stages.values() {
                stage_counts[*stage as usize] += 1;
            }
            log::debug!(
                "local terrain load progress: missing/requested/generated/lit/cpu/uploaded={stage_counts:?}; chunk manager={:?}",
                chunk_manager.stats()
            );
        }
    }

    fn progress(&self) -> f32 {
        let total = self.targets.len() as u32 * LocalLoadStage::Uploaded.progress_units();
        if total == 0 {
            return 0.0;
        }
        let complete_units: u32 = self
            .targets
            .iter()
            .map(|key| self.stages.get(key).copied().unwrap_or_default().progress_units())
            .sum();
        complete_units as f32 / total as f32
    }

    fn is_complete(&self) -> bool {
        self.targets
            .iter()
            .all(|key| self.stages.get(key) == Some(&LocalLoadStage::Uploaded))
    }

    fn recovery_failure(
        &self,
        failed_target: Option<(i32, i32)>,
        now: std::time::Instant,
        stall_timeout: std::time::Duration,
    ) -> Option<LocalLoadFailure> {
        failed_target
            .map(LocalLoadFailure::Chunk)
            .or_else(|| {
                (!self.is_complete() && now.duration_since(self.last_progress) >= stall_timeout)
                    .then_some(LocalLoadFailure::Stalled)
            })
    }

    fn retry(&mut self, now: std::time::Instant) {
        self.last_progress = now;
    }

    fn reset_for_fresh_spawn(&mut self, player: &Player, world_spawn: [i32; 3], started: std::time::Instant) {
        *self = Self::new(player, world_spawn, true, started);
    }
}

#[cfg(test)]
mod local_load_plan_tests {
    use super::*;

    #[test]
    fn existing_player_load_accepts_the_player_chunk() {
        let player = Player::new(-17.0, 70.0, 33.0);
        let plan = LocalLoadPlan::new(&player, [0, 75, 0], false, std::time::Instant::now());

        assert_eq!(plan.targets, vec![(-2, 2)]);
    }

    #[test]
    fn fresh_player_load_accepts_the_suggested_spawn_chunk() {
        let player = Player::new(500.0, 70.0, 500.0);
        let plan = LocalLoadPlan::new(&player, [-17, 75, 33], true, std::time::Instant::now());

        assert_eq!(plan.targets, vec![(-2, 2)]);
    }

    #[test]
    fn resetting_for_a_selected_spawn_replaces_the_provisional_load_targets() {
        let player = Player::new(0.0, 75.0, 0.0);
        let mut plan = LocalLoadPlan::new(&player, [0, 75, 0], false, std::time::Instant::now());
        plan.stages.insert((0, 0), LocalLoadStage::Uploaded);
        plan.reset_for_fresh_spawn(&player, [80, 90, -48], std::time::Instant::now());

        assert_eq!(plan.targets, vec![(5, -3)]);
        assert!(plan.stages.values().all(|stage| *stage == LocalLoadStage::Missing));
        assert_eq!(plan.last_progress, plan.started);
    }

    #[test]
    fn spawn_search_orders_columns_and_stops_after_selection_or_exhaustion() {
        let mut search = SpawnSearch::new(0, 0, 1);
        assert_eq!(search.current(), Some((0, 0)));
        assert_eq!(search.required_chunks()[0], (0, 0));
        search.reject_current();
        assert_eq!(search.current(), Some((1, 0)));
        search.select([-1, 80, -1]);
        assert_eq!(search.current(), None);
        assert_eq!(search.state, SpawnSearchState::Selected([-1, 80, -1]));

        let mut exhausted = SpawnSearch::new(0, 0, 0);
        exhausted.reject_current();
        assert_eq!(exhausted.current(), None);
        assert_eq!(exhausted.state, SpawnSearchState::Exhausted);
    }

    #[test]
    fn load_stage_requires_each_accepted_pipeline_step() {
        assert_eq!(local_load_stage(LocalLoadSignals::default()), LocalLoadStage::Missing);
        assert_eq!(
            local_load_stage(LocalLoadSignals { requested: true, ..LocalLoadSignals::default() }),
            LocalLoadStage::Requested,
        );
        assert_eq!(
            local_load_stage(LocalLoadSignals { admitted: true, ..LocalLoadSignals::default() }),
            LocalLoadStage::Generated,
        );
        assert_eq!(
            local_load_stage(LocalLoadSignals {
                admitted: true,
                valid_lighting: true,
                ..LocalLoadSignals::default()
            }),
            LocalLoadStage::Lit,
        );
        assert_eq!(
            local_load_stage(LocalLoadSignals {
                admitted: true,
                valid_lighting: true,
                cpu_mesh: true,
                ..LocalLoadSignals::default()
            }),
            LocalLoadStage::CpuMesh,
        );
        assert_eq!(
            local_load_stage(LocalLoadSignals {
                admitted: true,
                valid_lighting: true,
                cpu_mesh: true,
                uploaded: true,
                ..LocalLoadSignals::default()
            }),
            LocalLoadStage::Uploaded,
        );

        // An old mesh/cache cannot bypass a missing accepted lighting result.
        assert_eq!(
            local_load_stage(LocalLoadSignals {
                admitted: true,
                cpu_mesh: true,
                uploaded: true,
                ..LocalLoadSignals::default()
            }),
            LocalLoadStage::Generated,
        );
    }

    #[test]
    fn final_load_failure_prioritizes_terminal_chunks_and_bounds_stalls() {
        let player = Player::new(0.0, 75.0, 0.0);
        let now = std::time::Instant::now();
        let mut plan = LocalLoadPlan::new(&player, [0, 75, 0], true, now);

        assert_eq!(
            plan.recovery_failure(None, now + LOCAL_LOAD_STALL_TIMEOUT, LOCAL_LOAD_STALL_TIMEOUT),
            Some(LocalLoadFailure::Stalled),
        );

        plan.last_progress = now;
        assert_eq!(
            plan.recovery_failure(None, now + std::time::Duration::from_secs(1), LOCAL_LOAD_STALL_TIMEOUT),
            None,
        );

        let failed_target = plan.targets[0];
        // A failed target must win over the timer, even before it has progressed.
        assert_eq!(
            plan.recovery_failure(Some(failed_target), now + std::time::Duration::from_secs(1), LOCAL_LOAD_STALL_TIMEOUT),
            Some(LocalLoadFailure::Chunk(failed_target)),
        );
    }

    #[test]
    fn empty_cpu_mesh_is_complete_without_a_gpu_cache_entry() {
        let mesh = world::mesh::ChunkMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            transparent_vertices: Vec::new(),
            transparent_indices: Vec::new(),
        };

        assert!(mesh_is_uploaded(false, &mesh));
        assert_eq!(
            local_load_stage(LocalLoadSignals {
                admitted: true,
                valid_lighting: true,
                cpu_mesh: true,
                uploaded: mesh_is_uploaded(false, &mesh),
                ..LocalLoadSignals::default()
            }),
            LocalLoadStage::Uploaded,
        );
    }
}

fn inventory_block_sprites(reader: &vibecraft::assets::reader::AssetReader) -> HashMap<BlockId, String> {
    (0..=413u16)
        .filter_map(|raw_id| {
            let block = BlockId::from_repr(raw_id)?;
            vibecraft::assets::texture_map::inventory_texture_for_block(reader, block)
                .map(|texture| (block, format!("block/{texture}")))
        })
        .collect()
}

fn ui_slot(
    stack: &inventory::ItemStack,
    items: &ItemRegistry,
    block_sprites: &HashMap<BlockId, String>,
    selected: bool,
) -> UiSlot {
    let block = (!stack.is_empty())
        .then(|| items.block_from_item(stack.id))
        .flatten()
        .filter(|block| *block != BlockId::Air);
    let display_name = if stack.is_empty() {
        String::new()
    } else if let Some(block) = block {
        block.name().to_string()
    } else {
        items.name(stack.id).to_string()
    };
    let sprite = if let Some(block) = block {
        block_sprites
            .get(&block)
            .cloned()
            .unwrap_or_else(|| format!("block/{}", gui_asset_stem(&display_name)))
    } else {
        items
            .texture_stem(stack.id)
            .map(|stem| format!("item/{stem}"))
            .unwrap_or_else(|| "_missing".to_string())
    };
    let block_tiles = block.map(|block| {
        use world::block::BlockFace;
        [
            world::mesh::get_face_tile(block, BlockFace::Top),
            world::mesh::get_face_tile(block, BlockFace::Front),
            world::mesh::get_face_tile(block, BlockFace::Right),
        ]
    });
    let hint = block_tiles.map(|t| t[0]).unwrap_or(stack.id as u32);
    UiSlot {
        name: display_name,
        sprite,
        count: stack.count,
        empty: stack.is_empty(),
        selected,
        hint,
        block_tiles,
    }
}

fn resolve_server_address(value: &str) -> Result<std::net::SocketAddr, String> {
    if let Ok(address) = value.parse() {
        return Ok(address);
    }
    value
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| "no address found".to_string())
}

fn gui_asset_stem(name: &str) -> String {
    let normalized = name.to_ascii_lowercase().replace(' ', "_").replace('-', "_");
    match normalized.as_str() {
        // These registry labels are intentionally compact, while the official
        // block assets retain the full vanilla ore names.
        "deepslate_iron" => "deepslate_iron_ore".to_string(),
        "deepslate_coal" => "deepslate_coal_ore".to_string(),
        "deepslate_gold" => "deepslate_gold_ore".to_string(),
        "deepslate_redstone" => "deepslate_redstone_ore".to_string(),
        "deepslate_diamond" => "deepslate_diamond_ore".to_string(),
        "deepslate_emerald" => "deepslate_emerald_ore".to_string(),
        "deepslate_lapis" => "deepslate_lapis_ore".to_string(),
        "deepslate_copper" => "deepslate_copper_ore".to_string(),
        _ => normalized,
    }
}

fn dropped_item_from_data(data: &DroppedItemData, items: &ItemRegistry) -> Option<DroppedItem> {
    let block_id = BlockId::from_repr(data.block_id)?;
    let mut item = DroppedItem::new(data.x, data.y, data.z, block_id);
    let item_id = if data.item_id == inventory::item::AIR {
        items.item_id_from_block(block_id)
    } else {
        data.item_id
    };
    item.stack = inventory::ItemStack::with_damage(item_id, data.count, data.damage);
    item.vx = data.vx;
    item.vy = data.vy;
    item.vz = data.vz;
    item.lifetime = data.lifetime;
    Some(item)
}

fn dropped_item_data(item: &DroppedItem) -> DroppedItemData {
    DroppedItemData {
        x: item.x,
        y: item.y,
        z: item.z,
        vx: item.vx,
        vy: item.vy,
        vz: item.vz,
        block_id: item.block_id as u16,
        item_id: item.stack.id,
        count: item.stack.count,
        damage: item.stack.damage,
        lifetime: item.lifetime,
    }
}

fn xp_orb_from_data(data: &XpOrbData) -> XpOrb {
    let mut orb = XpOrb::new(data.x, data.y, data.z, data.value);
    orb.vx = data.vx;
    orb.vy = data.vy;
    orb.vz = data.vz;
    orb.lifetime = data.lifetime;
    orb
}

fn xp_orb_data(orb: &XpOrb) -> XpOrbData {
    XpOrbData {
        x: orb.x,
        y: orb.y,
        z: orb.z,
        vx: orb.vx,
        vy: orb.vy,
        vz: orb.vz,
        value: orb.value,
        lifetime: orb.lifetime,
    }
}

fn network_face(normal: (i32, i32, i32)) -> Face {
    match normal {
        (0, -1, 0) => Face::Down,
        (0, 1, 0) => Face::Up,
        (0, 0, -1) => Face::North,
        (0, 0, 1) => Face::South,
        (-1, 0, 0) => Face::West,
        _ => Face::East,
    }
}

#[derive(Clone, Debug)]
struct RemotePlayerVisual {
    username: String,
    position: [f32; 3],
    target_position: [f32; 3],
    velocity: [f32; 3],
    yaw: f32,
    walk_phase: f32,
    walk_amount: f32,
}

/// Check a key binding that may have left/right variants (Shift, Ctrl).
fn check_mod(input: &InputState, primary: KeyCode, secondary: KeyCode) -> bool {
    input.is_key_pressed(primary) || input.is_key_pressed(secondary)
}

fn update_local_player_movement(
    player: &mut Player,
    camera: &mut Camera,
    input: &InputState,
    bindings: ResolvedKeyBindings,
    game_mode: GameMode,
    flying: bool,
    chunk_manager: &ChunkManager,
    inventory: &Inventory,
    item_registry: &ItemRegistry,
    difficulty: Difficulty,
    dt: f32,
) {
    let sprinting = check_mod(input, bindings.sprint, KeyCode::ControlRight) && !flying;
    player.sneaking = check_mod(input, bindings.sneak, KeyCode::ShiftRight)
        && game_mode.has_gravity()
        && !flying
        && !game_mode.is_spectator();
    (player.armor_points, player.armor_toughness) = inventory.armor_stats(item_registry);
    let in_water = player.is_in_water(chunk_manager);
    let fully_submerged = player.is_fully_in_water(chunk_manager);
    // In vanilla MC, the swimming pose (0.6 hitbox) activates when fully submerged
    // and sprinting or holding forward.
    player.swimming = fully_submerged
        && (sprinting || input.is_key_pressed(bindings.forward));
    player.on_ground = false;

    if !flying && game_mode.has_gravity() && player.fall_flying {
        let lift = camera.pitch.sin().max(0.0) * 15.0;
        player.vy += (lift - player::GRAVITY * 0.5) * dt;
        player.vy = player.vy.min(5.0).max(-15.0);
    }

    if flying || game_mode.is_spectator() {
        let v_speed = 10.0 * dt;
        let mut h_speed = v_speed;
        if check_mod(input, bindings.sprint, KeyCode::ControlRight) {
            h_speed *= 2.0;
        }
        if input.is_key_pressed(bindings.forward) {
            camera.move_forward(h_speed);
        }
        if input.is_key_pressed(bindings.back) {
            camera.move_forward(-h_speed);
        }
        if input.is_key_pressed(bindings.left) {
            camera.move_right(-h_speed);
        }
        if input.is_key_pressed(bindings.right) {
            camera.move_right(h_speed);
        }
        if input.is_key_pressed(bindings.jump) {
            camera.move_up(v_speed);
        }
        if input.is_key_pressed(bindings.sneak) {
            camera.move_up(-v_speed);
        }
        player.x = camera.position.x;
        player.y = camera.position.y;
        player.z = camera.position.z;
    } else if game_mode.has_gravity() {
        let base_speed = if player.swimming {
            if sprinting { player::SWIM_SPEED } else { player::SURFACE_SWIM_SPEED }
        } else if player.sneaking {
            player::SNEAK_SPEED
        } else {
            player::WALK_SPEED
        };
        let walk_speed = base_speed
            * player.get_speed_multiplier()
            * dt
            * if player.swimming { 1.0 } else if sprinting { player::SPRINT_MULT } else { 1.0 };

        let mut dx = 0.0;
        let mut dz = 0.0;
        if input.is_key_pressed(bindings.forward) {
            dx += camera.yaw.sin() * walk_speed;
            dz += camera.yaw.cos() * walk_speed;
        }
        if input.is_key_pressed(bindings.back) {
            dx -= camera.yaw.sin() * walk_speed;
            dz -= camera.yaw.cos() * walk_speed;
        }
        if input.is_key_pressed(bindings.left) {
            dx += camera.yaw.cos() * walk_speed;
            dz -= camera.yaw.sin() * walk_speed;
        }
        if input.is_key_pressed(bindings.right) {
            dx -= camera.yaw.cos() * walk_speed;
            dz += camera.yaw.sin() * walk_speed;
        }

        let horizontal_length = (dx * dx + dz * dz).sqrt();
        if horizontal_length > 0.0 {
            let scale = walk_speed / horizontal_length;
            dx *= scale;
            dz *= scale;
        }

        let mut touching_climbable = false;
        if in_water {
            let feet_y = player.y.floor() as i32;
            let feet_x = player.x.floor() as i32;
            let feet_z = player.z.floor() as i32;
            let below = chunk_manager.get_block(feet_x, feet_y - 1, feet_z);
            let in_water_block = chunk_manager.get_block(feet_x, feet_y, feet_z).id == BlockId::Water;
            if in_water_block && below.id == BlockId::SoulSand {
                player.vy += 14.0 * dt;
            } else if in_water_block && below.id == BlockId::MagmaBlock {
                player.vy -= 6.0 * dt;
            }
            player.update_water_vertical_velocity(
                input.is_key_pressed(bindings.jump),
                input.is_key_pressed(bindings.sneak),
                dt,
            );
        } else {
            let min_bx = (player.x - player::HALF_WIDTH).floor() as i32;
            let max_bx = (player.x + player::HALF_WIDTH).ceil() as i32;
            let min_by = (player.y - 0.1).floor() as i32;
            let max_by = (player.y + player.current_height() + 0.1).ceil() as i32;
            let min_bz = (player.z - player::HALF_WIDTH).floor() as i32;
            let max_bz = (player.z + player::HALF_WIDTH).ceil() as i32;
            'climb_search: for bx in min_bx..=max_bx {
                for by in min_by..=max_by {
                    for bz in min_bz..=max_bz {
                        if chunk_manager.get_block(bx, by, bz).id.is_climbable() {
                            touching_climbable = true;
                            break 'climb_search;
                        }
                    }
                }
            }
            if touching_climbable {
                player.vy = 0.0;
                if input.is_key_pressed(bindings.forward) || input.is_key_pressed(bindings.jump) {
                    player.vy = player::CLIMB_SPEED;
                }
                if input.is_key_pressed(bindings.back) {
                    player.vy = -player::CLIMB_SPEED;
                }
            } else {
                player.vy += player::GRAVITY * dt;
            }
        }

        let gravity_multiplier = player.get_gravity_multiplier();
        if gravity_multiplier != 1.0 && player.vy < 0.0 {
            player.vy *= gravity_multiplier;
        }
        if !touching_climbable {
            player.vy = player.vy.max(player::TERMINAL_VELOCITY);
        }
        player.try_move_with_difficulty(
            dx,
            player.vy * dt,
            dz,
            chunk_manager,
            difficulty.damage_multiplier(),
        );

        if sprinting {
            player.sprint_exhaustion(dt);
        }
        if !in_water
            && !touching_climbable
            && input.is_key_pressed(bindings.jump)
            && player.on_ground
        {
            player.vy = player::JUMP_SPEED * player.get_jump_multiplier();
            player.jump_exhaustion();
        }
        camera.position = player.eye_position();
    }
}

fn spawn_dropped_stack(
    dropped_items: &mut Vec<DroppedItem>,
    x: f32,
    y: f32,
    z: f32,
    stack: inventory::ItemStack,
    items: &ItemRegistry,
) {
    if let Some(mut dropped) = DroppedItem::from_stack(x, y, z, stack, items) {
        let angle = rand::random::<f32>() * std::f32::consts::TAU;
        let speed = 1.0 + rand::random::<f32>() * 2.0;
        dropped.vx = angle.cos() * speed;
        dropped.vz = angle.sin() * speed;
        dropped.vy = 2.0 + rand::random::<f32>() * 2.0;
        dropped_items.push(dropped);
    }
}

fn click_crafting(
    grid: &mut CraftingGrid,
    slot: Option<usize>,
    result: bool,
    left_click: bool,
    carried: &mut Option<inventory::ItemStack>,
    items: &ItemRegistry,
) -> bool {
    if result {
        if !left_click {
            return false;
        }
        let output = grid.result(items);
        if output.is_empty() {
            return false;
        }
        match carried {
            None => {
                *carried = Some(grid.take_result(items));
                true
            }
            Some(existing) if existing.can_merge_with(&output)
                && existing.count.saturating_add(output.count) <= existing.max_stack(items) as u16 =>
            {
                existing.count += output.count;
                let _ = grid.take_result(items);
                true
            }
            _ => false,
        }
    } else if let Some(index) = slot {
        let Some(mut held) = carried.take() else {
            if let Some(stack) = grid.slots.slots.get_mut(index).filter(|stack| !stack.is_empty()) {
                let take = if left_click { stack.count } else { stack.count.div_ceil(2) };
                let result = inventory::ItemStack::with_damage(stack.id, take, stack.damage);
                stack.count -= take;
                if stack.count == 0 {
                    *stack = inventory::EMPTY_STACK;
                }
                *carried = Some(result);
                return true;
            }
            return false;
        };
        let target = &mut grid.slots.slots[index];
        if target.is_empty() {
            if left_click {
                std::mem::swap(target, &mut held);
            } else {
                *target = inventory::ItemStack::with_damage(held.id, 1, held.damage);
                held.count -= 1;
            }
        } else if target.can_merge_with(&held) {
            let space = (target.max_stack(items) as u16).saturating_sub(target.count);
            let amount = if left_click { held.count.min(space) } else { 1.min(held.count).min(space) };
            target.count += amount;
            held.count -= amount;
        } else if left_click {
            std::mem::swap(target, &mut held);
        }
        if !held.is_empty() {
            *carried = Some(held);
        }
        true
    } else {
        false
    }
}

fn return_crafting_items(
    grid: &mut CraftingGrid,
    inventory: &mut Inventory,
    carried: &mut Option<inventory::ItemStack>,
    dropped_items: &mut Vec<DroppedItem>,
    x: f32,
    y: f32,
    z: f32,
    items: &ItemRegistry,
) {
    if let Some(stack) = carried.take() {
        let remainder = inventory.add_stack(stack, items);
        if !remainder.is_empty() {
            spawn_dropped_stack(dropped_items, x, y, z, remainder, items);
        }
    }
    for stack in &mut grid.slots.slots {
        let held = std::mem::replace(stack, inventory::EMPTY_STACK);
        if !held.is_empty() {
            let remainder = inventory.add_stack(held, items);
            if !remainder.is_empty() {
                spawn_dropped_stack(dropped_items, x, y, z, remainder, items);
            }
        }
    }
}

/// Minimal mouse-driven container interaction until the graphical menu path
/// consumes the shared `SlotContainer` API. Sneaking withdraws; normal use
/// inserts the selected stack. Both directions preserve a full-inventory
/// remainder rather than deleting items.
fn interact_container(
    chunk_manager: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    inventory: &mut Inventory,
    items: &ItemRegistry,
    withdraw: bool,
) -> Option<&'static str> {
    let mut entity = chunk_manager.get_block_entity(x, y, z)?.clone();
    let changed = match &mut entity {
        BlockEntity::Chest { slots } => {
            if withdraw {
                let Some(index) = slots.slots.iter().position(|stack| !stack.is_empty()) else { return Some("Chest is empty"); };
                let stack = slots.take(index, u16::MAX);
                let remainder = inventory.add_stack(stack, items);
                if !remainder.is_empty() {
                    let _ = slots.insert(remainder, items);
                    return Some("Inventory is full");
                }
                true
            } else {
                let stack = inventory.remove_from_hotbar(inventory.held_slot, u16::MAX);
                if stack.is_empty() {
                    return Some("Hold an item to store it");
                }
                let remainder = slots.insert(stack, items);
                if !remainder.is_empty() {
                    let _ = inventory.add_stack(remainder, items);
                }
                true
            }
        }
        BlockEntity::Furnace { state } => {
            if withdraw {
                let stack = state.slots.take(inventory::progression::FURNACE_OUTPUT, u16::MAX);
                if stack.is_empty() {
                    return Some("Furnace output is empty");
                }
                let remainder = inventory.add_stack(stack, items);
                if !remainder.is_empty() {
                    let _ = state.slots.insert(remainder, items);
                    return Some("Inventory is full");
                }
                true
            } else {
                let stack = inventory.remove_from_hotbar(inventory.held_slot, u16::MAX);
                if stack.is_empty() {
                    return Some("Hold an item to smelt or use as fuel");
                }
                let target = if fuel_ticks(stack.id, items) > 0 {
                    inventory::progression::FURNACE_FUEL
                } else {
                    inventory::progression::FURNACE_INPUT
                };
                let slot = &mut state.slots.slots[target];
                if slot.is_empty() {
                    *slot = stack;
                } else if slot.can_merge_with(&stack) && slot.count < slot.max_stack(items) as u16 {
                    let moved = (slot.max_stack(items) as u16 - slot.count).min(stack.count);
                    slot.count += moved;
                    if moved < stack.count {
                        let _ = inventory.add_stack(inventory::ItemStack::with_damage(stack.id, stack.count - moved, stack.damage), items);
                    }
                } else {
                    let _ = inventory.add_stack(stack, items);
                }
                true
            }
        }
    };
    if changed && chunk_manager.set_block_entity(x, y, z, entity) {
        Some(if withdraw { "Retrieved container items" } else { "Stored container items" })
    } else {
        None
    }
}

fn main() {
    env_logger::init();
    match AppConfig::from_env() {
        Ok(config) => pollster::block_on(run(config)),
        Err(vibecraft::config::ConfigError::HelpRequested) => println!("{}", vibecraft::config::usage()),
        Err(error) => {
            log::error!("{error}");
            std::process::exit(2);
        }
    }
}

async fn run(config: AppConfig) {
    let bindings = match config.keybindings.resolve() {
        Ok(bindings) => bindings,
        Err(error) => {
            log::error!("{error}");
            return;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&config.world_dir) {
        log::error!("failed to create world directory {}: {error}", config.world_dir.display());
        return;
    }
    let asset_directory = match resolve_minecraft_asset_directory() {
        Ok(path) => path,
        Err(error) => {
            log::error!("{error}");
            return;
        }
    };
    log::info!("using Minecraft assets from {}", asset_directory.display());
    let asset_reader = vibecraft::assets::reader::AssetReader::new(asset_directory);

    let mut render_distance = config.render_distance;
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            log::error!("failed to create event loop: {error}");
            return;
        }
    };
    let mut window_state = match WindowState::new(&event_loop) {
        Ok(window_state) => window_state,
        Err(error) => {
            log::error!("failed to create window: {error}");
            return;
        }
    };

    let mut grabbed = false;

    let window = window_state.window.clone();

    let inventory_block_sprite_map = inventory_block_sprites(&asset_reader);
    let mut renderer = match Renderer::new(window.clone(), &asset_reader).await {
        Ok(renderer) => renderer,
        Err(error) => {
            log::error!("renderer startup failed: {error}");
            return;
        }
    };

    profiler::reset();

    let mut storage = WorldStorage::new(config.world_dir.clone());
    let requested_seed = config.resolved_seed();
    let level = match storage.load_or_create_level(LevelData {
        name: "New World".to_string(),
        created_at: unix_millis(),
        last_played: unix_millis(),
        coordinate_profile: WorldCoordinateProfile::new_world(),
        generation_profile: WorldGenerationProfile::new_world(),
        seed: requested_seed,
        tick: 0,
        game_time: 9_000,
        spawn: [0, 75, 0],
        gamemode: "survival".to_string(),
        difficulty: "normal".to_string(),
        hardcore: false,
        do_daylight_cycle: true,
        keep_inventory: false,
        experience: 0,
        scheduled_ticks: Vec::new(),
        dropped_items: Vec::new(),
        xp_orbs: Vec::new(),
        players: Vec::new(),
    }) {
        Ok(level) => level,
        Err(error) => {
            log::error!("failed to open world {}: {error}", config.world_dir.display());
            return;
        }
    };
    if config.seed.is_some_and(|seed| seed != level.seed) {
        log::warn!(
            "ignoring requested seed because existing world {} uses seed {}",
            config.world_dir.display(),
            level.seed
        );
    }
    let mut seed = level.seed;
    log::info!(
        "starting world seed={seed} directory={} render_distance={render_distance} graphics={:?}",
        config.world_dir.display(), config.graphics
    );

    let mut game_mode = GameMode::from_str(&level.gamemode).unwrap_or(GameMode::Creative);
    let mut difficulty = Difficulty::from_str(&level.difficulty).unwrap_or(Difficulty::Normal);
    let mut hardcore = level.hardcore;
    let mut game_time = level.game_time as f32 / 20.0;
    let mut simulation_tick = level.tick;
    let mut do_daylight_cycle = level.do_daylight_cycle;
    let mut keep_inventory = level.keep_inventory;
    let mut world_spawn = level.spawn;
    let mut flying = game_mode.can_fly();
    let mut player = Player::new(level.spawn[0] as f32, level.spawn[1] as f32, level.spawn[2] as f32);
    let mut inventory = Inventory::new();
    let mut new_player = false;
    match storage.load_player() {
        Ok(data) => match data.into_runtime() {
            Ok((saved_player, saved_inventory)) => {
                player = saved_player;
                inventory = saved_inventory;
            }
            Err(error) => {
                log::error!("failed to decode player save: {error}");
                return;
            }
        },
        Err(StorageError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            new_player = true;
        }
        Err(error) => {
            log::error!("failed to load player save: {error}");
            return;
        }
    }
    if new_player {
        prepare_new_player_spawn(seed, level.generation_profile, &mut world_spawn, &mut player);
    }
    let mut camera = Camera::new(
        player.eye_position(),
        renderer.size.0 as f32 / renderer.size.1 as f32,
    );

    let mut input = InputState::new();

    let mut chunk_manager = ChunkManager::new(
        seed,
        render_distance,
        level.coordinate_profile,
        level.generation_profile,
    );
    chunk_manager.set_storage(storage.clone());
    let mut network_address = config.server;
    let mut network_username = config.username.clone();
    let mut client_transport = match network_address {
        Some(address) => match ClientTransport::connect(address, config.username.clone()) {
            Ok(transport) => {
                log::info!("connecting to authoritative server at {address}");
                Some(transport)
            }
            Err(error) => {
                log::error!("failed to connect to server {address}: {error}");
                return;
            }
        },
        None => None,
    };
    let mut network_mode = client_transport.is_some();
    if !network_mode {
        // Queue remaining chunks for local singleplayer loading.
        chunk_manager.update_chunks_async(
            (player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            (player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
        );
    }

    let mut chunk_render_data: Vec<(i32, i32, ChunkRenderData)> = Vec::new();
    let mut all_chunk_data: Vec<(i32, i32, ChunkRenderData)> = Vec::new();
    let mut render_cache: HashMap<(i32, i32), ChunkRenderData> = HashMap::new();
    rebuild_render_data(
        &mut chunk_render_data,
        &mut all_chunk_data,
        &mut render_cache,
        &chunk_manager,
        &renderer,
        &camera,
    );

    let item_registry = ItemRegistry::new();
    let mut dropped_items: Vec<DroppedItem> = level
        .dropped_items
        .iter()
        .filter_map(|data| dropped_item_from_data(data, &item_registry))
        .collect();
    let mut xp_orbs: Vec<XpOrb> = level.xp_orbs.iter().map(xp_orb_from_data).collect();
    let mut entities = EntityStore::new();
    if !new_player {
        entities.spawn(
            EntityKind::TrainingDummy,
            Transform::new(nalgebra::Vector3::new(
                world_spawn[0] as f32 + 3.0,
                world_spawn[1] as f32,
                world_spawn[2] as f32,
            )),
        );
    }

    let mut border_data: Option<(wgpu::Buffer, u32)> = None;
    let mut border_needs_rebuild = true;

    let mut show_debug = false;
    let mut show_chunk_borders = false;
    let mut fps_counter = 0u32;
    let mut fps_timer = 0.0f32;
    let mut fps = 0f32;

    // Fill hotbar with starter blocks
    let starter_items = [
        (1u16, 64),
        (2, 64),
        (3, 64),
        (4, 64),
        (15, 64),
        (13, 64),
        (BlockId::OakDoor as u16, 16),
        (BlockId::OakFence as u16, 64),
        (BlockId::RedstoneDust as u16, 64),
    ];
    if new_player {
        for (i, (id, count)) in starter_items.iter().enumerate() {
            inventory.slots[inventory::HOTBAR_START + i] = inventory::ItemStack::new(*id, *count);
        }
    }

    let mut break_progress: f32 = 0.0;
    let mut break_target: Option<(i32, i32, i32)> = None;
    let mut hurt_timer: f32 = 0.0;
    let mut stable_target_hit: Option<world::raycast::RaycastHit> = None;

    let mut experience = level.experience;

    let audio = engine::audio::AudioEngine::new(asset_reader.clone());
    audio.load_common_sounds();

    let mut last_time = std::time::Instant::now();
    let mut last_save = last_time;
    let mut local_load_plan = LocalLoadPlan::new(
        &player,
        world_spawn,
        new_player,
        std::time::Instant::now(),
    );
    let mut spawn_search = new_player.then(|| {
        SpawnSearch::new(world_spawn[0], world_spawn[2], SAFE_SPAWN_SEARCH_RADIUS_CHUNKS)
    });
    let mut simulation_clock = world::simulation::FixedStepClock::new();
    let mut right_was_pressed = false;
    let mut highlight: Option<HighlightData> = None;
    let mut break_overlay: Option<BreakOverlay> = None;
    let mut last_break_overlay_pos: Option<(i32, i32, i32)> = None;
    let mut last_hit_pos: Option<(i32, i32, i32)> = None;
    let mut tick_scheduler = TickScheduler::from_events(level.scheduled_ticks);
    if tick_scheduler.events().is_empty() {
        let chunk = [
            (player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            (player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
        ];
        tick_scheduler.schedule(ScheduledTick {
            due_tick: simulation_tick + 5,
            chunk,
            kind: ScheduledTickKind::Water,
        });
        tick_scheduler.schedule(ScheduledTick {
            due_tick: simulation_tick + 8,
            chunk,
            kind: ScheduledTickKind::Lava,
        });
    }
    let mut render_quality = config.graphics;
    let screen_height = renderer.size.1 as f32;
    let mut ui_state = ui::UiState::new(render_distance, render_quality == GraphicsQuality::Vibrant, screen_height);
    ui_state.glyph_advances.copy_from_slice(renderer.font.glyph_advances_slice());
    ui_state.screen = ui::UiScreen::Title;
    ui_state.connect_username = config.username.clone();
    if let Some(address) = network_address {
        ui_state.server_address = address.to_string();
    }
    let mut command_feedback = String::new();
    let mut command_feedback_timer = 0.0f32;
    let mut chat_state = chat::ChatState::default();
    let mut chat_timer: f32 = 0.0;
    let mut inventory_open = false;
    let mut player_crafting = CraftingGrid::player();
    let mut cursor_x: f32 = 0.0;
    let mut cursor_y: f32 = 0.0;
    let mut carried_item: Option<inventory::ItemStack> = None;
    let mut auto_screenshot_frame: u32 = 60;
    let mut screenshots_taken: u32 = 0;
    let mut save_requested = false;
    let mut quit_requested = false;
    let mut network_session_id = None;
    let mut network_request_id = 1u64;
    let mut network_input_timer = 0.0f32;
    let mut network_inventory_revision = 0u64;
    let mut network_last_held_slot = inventory.held_slot;
    let mut network_reconnect_timer = 0.0f32;
    let mut network_connect_request: Option<String> = None;
    let mut server_connect_timer = 0.0f32;
    let mut remote_players: HashMap<u64, RemotePlayerVisual> = HashMap::new();
    let mut pending_world_open: Option<WorldOpenRequest> = None;
    let mut browser_paths: Vec<std::path::PathBuf> = Vec::new();

    #[allow(deprecated)]
    let _ = event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => {
                input.handle_event(&event);

                match &event {
                    WindowEvent::CloseRequested => {
                        if !network_mode {
                            save_world(
                                &storage, seed, &mut chunk_manager, &player, &inventory,
                                game_mode, difficulty, hardcore, game_time, simulation_tick,
                                world_spawn, do_daylight_cycle, keep_inventory, experience,
                                &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                            );
                        }
                        if let Some(transport) = client_transport.as_mut() {
                            let _ = transport.disconnect();
                        }
                        profiler::save("/tmp/opencode/profiler_output.txt");
                        target.exit();
                    }
                    WindowEvent::Resized(size) => {
                        window_state.resize((size.width, size.height));
                        renderer.resize(window_state.size);
                        renderer.gui_dirty = true;
                        camera.aspect = window_state.size.0 as f32 / window_state.size.1 as f32;
                    }
                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        if key_event.state == ElementState::Pressed {
                            if chat_state.open {
                                match &key_event.logical_key {
                                    Key::Named(NamedKey::Enter) => {
                                        if let Some(cmd) = chat_state.submit() {
                                            if cmd.starts_with('/') {
                                                if network_mode {
                                                    command_feedback = "Commands are server-owned while connected.".to_string();
                                                } else {
                                                    let was_gm = game_mode;
                                                    execute_command(&cmd, &mut chunk_manager, &mut render_cache, &last_hit_pos, &mut command_feedback, &mut save_requested, &mut quit_requested, &mut game_mode, &mut difficulty, &mut hardcore, &mut game_time, &mut do_daylight_cycle, &mut keep_inventory, &mut world_spawn, &mut dropped_items, &mut camera, seed, &mut experience, &mut player, &mut inventory, &item_registry);
                                                    if was_gm != game_mode {
                                                        flying = game_mode.can_fly();
                                                    }
                                                    if save_requested {
                                                        let saved = save_world(
                                                            &storage, seed, &mut chunk_manager, &player, &inventory,
                                                            game_mode, difficulty, hardcore, game_time, simulation_tick,
                                                            world_spawn, do_daylight_cycle, keep_inventory, experience, &tick_scheduler,
                                                             &dropped_items, &xp_orbs, !new_player, true,
                                                        );
                                                        last_save = std::time::Instant::now();
                                                        save_requested = false;
                                                        if quit_requested && saved {
                                                            target.exit();
                                                        } else if quit_requested {
                                                            command_feedback = "Save failed; refusing to quit so world data can be retried.".to_string();
                                                            quit_requested = false;
                                                        } else if saved {
                                                            command_feedback = "World saved.".to_string();
                                                        } else {
                                                            command_feedback = "World save failed; see log for details.".to_string();
                                                        }
                                                    }
                                                }
                                                if !command_feedback.is_empty() {
                                                    chat_state.add_message(command_feedback.clone());
                                                    chat_timer = 5.0;
                                                    command_feedback.clear();
                                                    command_feedback_timer = 0.0;
                                                }
                                            } else {
                                                if network_mode {
                                                    if let Some(transport) = client_transport.as_mut() {
                                                        if let Err(error) = transport.send(vibecraft::network::ClientMessage::Chat { message: cmd.clone() }) {
                                                            command_feedback = format!("Chat send failed: {error}");
                                                            command_feedback_timer = 3.0;
                                                        }
                                                    }
                                                } else {
                                                    chat_state.add_message(format!("<Player> {cmd}"));
                                                    chat_timer = 3.0;
                                                }
                                            }
                                        }
                                        chat_state.close();
                                        if !ui_state.is_menu_open() && !inventory_open {
                                            grabbed = true;
                                            input.mouse_grabbed = true;
                                            window.set_cursor_visible(false);
                                            let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                                        }
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Escape) => {
                                        chat_state.close();
                                        if !ui_state.is_menu_open() && !inventory_open {
                                            grabbed = true;
                                            input.mouse_grabbed = true;
                                            window.set_cursor_visible(false);
                                            let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                                        }
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        chat_state.backspace();
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Delete) => { chat_state.delete(); renderer.gui_dirty = true; }
                                    Key::Named(NamedKey::ArrowLeft) => {
                                        let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                        chat_state.move_cursor(-1, shift);
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::ArrowRight) => {
                                        let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                        chat_state.move_cursor(1, shift);
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Home) => {
                                        let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                        chat_state.move_to_start(shift);
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::End) => {
                                        let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                        chat_state.move_to_end(shift);
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::ArrowUp) => { chat_state.recall_previous(); renderer.gui_dirty = true; }
                                    Key::Named(NamedKey::ArrowDown) => { chat_state.recall_next(); renderer.gui_dirty = true; }
                                    Key::Named(NamedKey::Tab) => {
                                        let suggestions = chat::command_suggestions(&chat_state.text);
                                        if chat_state.complete(&suggestions) { renderer.gui_dirty = true; }
                                    }
                                    Key::Named(NamedKey::Space) => {
                                        chat_state.insert(" ");
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Character(c) => {
                                        chat_state.insert(c.as_ref());
                                        renderer.gui_dirty = true;
                                    }
                                    _ => {}
                                }
                            } else if ui_state.screen == ui::UiScreen::Connect {
                                match &key_event.logical_key {
                                    Key::Named(NamedKey::Enter) => {
                                        if ui_state.handle_key(KeyCode::Enter) == UiAction::ConnectServer {
                                            network_username = ui_state.connect_username.clone();
                                            network_connect_request = Some(ui_state.server_address.clone());
                                            ui_state.screen = ui::UiScreen::Loading;
                                            ui_state.connecting = true;
                                        }
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Escape) => {
                                        ui_state.handle_escape();
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Tab) => {
                                        ui_state.switch_connect_field();
                                        renderer.gui_dirty = true;
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        if ui_state.connect_field == 0 {
                                            ui_state.backspace_server_address();
                                        } else {
                                            ui_state.backspace_connect_username();
                                        }
                                        renderer.gui_dirty = true;
                                    }
                                     Key::Named(NamedKey::Space) => {
                                         if ui_state.connect_field == 0 {
                                             ui_state.append_server_address(" ");
                                         } else {
                                             ui_state.append_connect_username(" ");
                                         }
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Character(value) => {
                                         if ui_state.connect_field == 0 {
                                             ui_state.append_server_address(value.as_ref());
                                         } else {
                                             ui_state.append_connect_username(value.as_ref());
                                         }
                                         renderer.gui_dirty = true;
                                     }
                                     _ => {}
                                 }
                             } else if ui_state.screen == ui::UiScreen::CreateWorld {
                                 match &key_event.logical_key {
                                     Key::Named(NamedKey::Enter) => {
                                         if ui_state.handle_key(KeyCode::Enter) == UiAction::CreateWorld {
                                             pending_world_open = Some(WorldOpenRequest::Create(ui_state.create_options()));
                                         }
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::Escape) => {
                                         ui_state.handle_escape();
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::Tab) => {
                                         ui_state.handle_key(KeyCode::Tab);
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::Backspace) => {
                                         ui_state.backspace_create_text();
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::Delete) => {
                                         ui_state.delete_create_text();
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::ArrowLeft) => {
                                         let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                         ui_state.move_create_cursor(-1, shift);
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::ArrowRight) => {
                                         let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                         ui_state.move_create_cursor(1, shift);
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::Home) => {
                                         let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                         ui_state.move_create_to_start(shift);
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::End) => {
                                         let shift = input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight);
                                         ui_state.move_create_to_end(shift);
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Named(NamedKey::Space) => {
                                         ui_state.insert_create_text(" ");
                                         renderer.gui_dirty = true;
                                     }
                                     Key::Character(value) => {
                                         ui_state.insert_create_text(value.as_ref());
                                         renderer.gui_dirty = true;
                                     }
                                     _ => {}
                                 }
                            } else {
                                match key_event.physical_key {
                                     PhysicalKey::Code(KeyCode::Escape) => {
                                          let was_open = ui_state.is_menu_open();
                                          let action = ui_state.handle_escape();
                                          if action == UiAction::BackToTitle {
                                              spawn_search = None;
                                              ui_state.loading_error = None;
                                              ui_state.open_title();
                                          } else if action == UiAction::Quit {
                                             if !network_mode {
                                                 save_world(
                                                     &storage, seed, &mut chunk_manager, &player, &inventory,
                                                     game_mode, difficulty, hardcore, game_time, simulation_tick,
                                                     world_spawn, do_daylight_cycle, keep_inventory, experience,
                                                      &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                                                 );
                                             }
                                             if let Some(transport) = client_transport.as_mut() {
                                                 let _ = transport.disconnect();
                                             }
                                             target.exit();
                                          } else if inventory_open {
                                              inventory_open = false;
                                              if !network_mode {
                                                  return_crafting_items(
                                                      &mut player_crafting,
                                                      &mut inventory,
                                                      &mut carried_item,
                                                      &mut dropped_items,
                                                      player.x,
                                                      player.y + player.current_eye_height(),
                                                      player.z,
                                                      &item_registry,
                                                  );
                                              } else {
                                                  carried_item = None;
                                              }
                                         } else if !was_open {
                                             grabbed = false;
                                             input.mouse_grabbed = false;
                                             window.set_cursor_visible(true);
                                             let _ = window.set_cursor_grab(CursorGrabMode::None);
                                         } else if !ui_state.is_menu_open() {
                                             grabbed = true;
                                             input.mouse_grabbed = true;
                                             window.set_cursor_visible(false);
                                             let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                                         }
                                         renderer.gui_dirty = true;
                                     }
                                       PhysicalKey::Code(key) if ui_state.is_menu_open() && matches!(key, KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Enter | KeyCode::NumpadEnter) => {
                                           let action = ui_state.handle_key(key);
                                            if action == UiAction::ToggleGraphics {
                                                render_quality = if ui_state.graphics_vibrant { GraphicsQuality::Vibrant } else { GraphicsQuality::Regular };
                                            }
                                             if action == UiAction::RetryLoading {
                                                 chunk_manager.retry_failed_chunks();
                                                 spawn_search = new_player.then(|| SpawnSearch::new(
                                                     world_spawn[0], world_spawn[2], SAFE_SPAWN_SEARCH_RADIUS_CHUNKS,
                                                 ));
                                                 local_load_plan.retry(std::time::Instant::now());
                                                ui_state.loading_error = None;
                                                ui_state.loading_progress = 0.0;
                                                ui_state.screen = ui::UiScreen::Loading;
                                            }
                                            if action == UiAction::BackToTitle {
                                                spawn_search = None;
                                                ui_state.loading_error = None;
                                                ui_state.open_title();
                                            }
                                              if action == UiAction::ConnectServer {
                                                  network_username = ui_state.connect_username.clone();
                                                  network_connect_request = Some(ui_state.server_address.clone());
                                                  ui_state.screen = ui::UiScreen::Loading;
                                                  ui_state.connecting = true;
                                              }
                                            if action == UiAction::OpenWorldSelect {
                                                match discover_worlds(&config.world_dir) {
                                                    Ok(discovery) => {
                                                        for (path, error) in discovery.rejected {
                                                            log::warn!("skipping invalid world {}: {error}", path.display());
                                                        }
                                                        browser_paths = discovery.worlds.iter().map(|world| world.path.clone()).collect();
                                                        ui_state.open_world_select(discovery.worlds.into_iter().map(|world| ui::UiWorld {
                                                            name: world.name,
                                                            gamemode: world.gamemode,
                                                            hardcore: world.hardcore,
                                                            last_played: world.last_played,
                                                        }).collect());
                                                    }
                                                    Err(error) => {
                                                        command_feedback = format!("Could not list worlds: {error}");
                                                        command_feedback_timer = 5.0;
                                                    }
                                                }
                                            }
                                            if action == UiAction::LoadSelectedWorld {
                                                if let Some(path) = ui_state.selected_world.and_then(|index| browser_paths.get(index)).cloned() {
                                                    pending_world_open = Some(WorldOpenRequest::Existing(path));
                                                }
                                            }
                                            if action == UiAction::CreateWorld {
                                                pending_world_open = Some(WorldOpenRequest::Create(ui_state.create_options()));
                                            }
                                            if action == UiAction::DeleteWorld {
                                                if let Some(index) = ui_state.selected_world {
                                                    if let Some(path) = browser_paths.get(index).cloned() {
                                                        match std::fs::remove_dir_all(&path) {
                                                            Err(error) => {
                                                                command_feedback = format!("Failed to delete world: {error}");
                                                                command_feedback_timer = 5.0;
                                                            }
                                                            Ok(()) => {
                                                                command_feedback = "Deleted world".to_string();
                                                                command_feedback_timer = 3.0;
                                                            }
                                                        }
                                                    }
                                                }
                                                match discover_worlds(&config.world_dir) {
                                                    Ok(discovery) => {
                                                        browser_paths = discovery.worlds.iter().map(|world| world.path.clone()).collect();
                                                        ui_state.open_world_select(discovery.worlds.into_iter().map(|world| ui::UiWorld {
                                                            name: world.name,
                                                            gamemode: world.gamemode,
                                                            hardcore: world.hardcore,
                                                            last_played: world.last_played,
                                                        }).collect());
                                                    }
                                                    Err(error) => {
                                                        command_feedback = format!("Failed to refresh world list: {error}");
                                                        command_feedback_timer = 5.0;
                                                    }
                                                }
                                            }
                                            if action == UiAction::QuitToTitle {
                                               if !network_mode {
                                                   save_world(
                                                       &storage, seed, &mut chunk_manager, &player, &inventory,
                                                       game_mode, difficulty, hardcore, game_time, simulation_tick,
                                                       world_spawn, do_daylight_cycle, keep_inventory, experience,
                                                        &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                                                   );
                                               }
                                               if let Some(transport) = client_transport.as_mut() {
                                                   let _ = transport.disconnect();
                                               }
                                               inventory_open = false;
                                               grabbed = false;
                                               input.mouse_grabbed = false;
                                               window.set_cursor_visible(true);
                                               let _ = window.set_cursor_grab(CursorGrabMode::None);
                                                ui_state.open_title();
                                           }
                                           if action == UiAction::Quit {
                                               if !network_mode {
                                                   save_world(
                                                       &storage, seed, &mut chunk_manager, &player, &inventory,
                                                       game_mode, difficulty, hardcore, game_time, simulation_tick,
                                                       world_spawn, do_daylight_cycle, keep_inventory, experience,
                                                        &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                                                   );
                                               }
                                               if let Some(transport) = client_transport.as_mut() {
                                                   let _ = transport.disconnect();
                                               }
                                               target.exit();
                                           }
                                           if !ui_state.is_menu_open() {
                                              inventory_open = false;
                                              grabbed = true;
                                              input.mouse_grabbed = true;
                                              window.set_cursor_visible(false);
                                              let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                                          }
                                          renderer.gui_dirty = true;
                                      }
                                     PhysicalKey::Code(KeyCode::F2) => {
                                        let path = format!("/tmp/opencode/vibecraft_screenshot_{}.png",
                                            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
                                        renderer.request_screenshot(&path);
                                    }
                                    PhysicalKey::Code(KeyCode::F3) => { show_debug = !show_debug; renderer.gui_dirty = true; },
                                    PhysicalKey::Code(KeyCode::F5) => {
                                        if profiler::is_enabled() {
                                            let path = profiler::save("/tmp/opencode/profiler_output.txt");
                                            profiler::set_enabled(false);
                                            let fname = path.rsplit('/').next().unwrap_or(&path);
                                            command_feedback = format!("Profiler saved: {}", fname);
                                            command_feedback_timer = 5.0;
                                        } else {
                                            profiler::reset();
                                            profiler::set_enabled(true);
                                            command_feedback = "Profiler ON (F5 saves)".to_string();
                                            command_feedback_timer = 3.0;
                                        }
                                    }
                                    PhysicalKey::Code(KeyCode::F4) => {
                                        render_quality = match render_quality {
                                            GraphicsQuality::Regular => GraphicsQuality::Vibrant,
                                            GraphicsQuality::Vibrant => GraphicsQuality::Regular,
                                        };
                                    }
                                    PhysicalKey::Code(KeyCode::KeyG) => {
                                        if input.is_key_pressed(KeyCode::F3) { show_chunk_borders = !show_chunk_borders; }
                                    }
                                     PhysicalKey::Code(KeyCode::KeyF) => {
                                         if !ui_state.captures_gameplay_input() && game_mode.can_fly() { flying = !flying; player.vy = 0.0; }
                                    },
                                      PhysicalKey::Code(key) if key == bindings.command => {
                                          if !ui_state.is_menu_open() {
                                              chat_state.open_with("/");
                                             grabbed = false;
                                             input.mouse_grabbed = false;
                                             window.set_cursor_visible(true);
                                             let _ = window.set_cursor_grab(CursorGrabMode::None);
                                             renderer.gui_dirty = true;
                                         }
                                     }
                                      PhysicalKey::Code(key) if key == bindings.chat => {
                                          if !ui_state.is_menu_open() {
                                              chat_state.open_with("");
                                             grabbed = false;
                                             input.mouse_grabbed = false;
                                             window.set_cursor_visible(true);
                                             let _ = window.set_cursor_grab(CursorGrabMode::None);
                                             renderer.gui_dirty = true;
                                         }
                                    }
                                    PhysicalKey::Code(KeyCode::Digit1) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 0.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit2) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 1.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit3) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 2.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit4) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 3.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit5) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 4.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit6) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 5.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit7) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 6.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit8) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 7.min(inventory::HOTBAR_SLOTS - 1); }
                                    PhysicalKey::Code(KeyCode::Digit9) if !ui_state.captures_gameplay_input() => { inventory.held_slot = 8.min(inventory::HOTBAR_SLOTS - 1); }
                                      PhysicalKey::Code(key) if key == bindings.inventory => {
                                           if !chat_state.open {
                                              let was_open = inventory_open;
                                              if was_open || !ui_state.is_menu_open() {
                                                  inventory_open = !inventory_open;
                                                  if inventory_open {
                                                      ui_state.open_inventory();
                                                  } else {
                                                      ui_state.close_to_gameplay();
                                                  }
                                                  renderer.gui_dirty = true;
                                                  if inventory_open {
                                                      grabbed = false;
                                                      input.mouse_grabbed = false;
                                                      window.set_cursor_visible(true);
                                                      let _ = window.set_cursor_grab(CursorGrabMode::None);
                                                  } else {
                                                      grabbed = true;
                                                      input.mouse_grabbed = true;
                                                      window.set_cursor_visible(false);
                                                      let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                                                      if !network_mode {
                                                          return_crafting_items(
                                                              &mut player_crafting,
                                                              &mut inventory,
                                                              &mut carried_item,
                                                              &mut dropped_items,
                                                              player.x,
                                                              player.y + player.current_eye_height(),
                                                              player.z,
                                                              &item_registry,
                                                          );
                                                      } else {
                                                          carried_item = None;
                                                      }
                                                  }
                                              }
                                          }
                                     }
                                      PhysicalKey::Code(key) if key == bindings.drop_item => {
                                           if !ui_state.captures_gameplay_input() && !chat_state.open && !inventory_open {
                                             if network_mode {
                                                 if let Some(transport) = client_transport.as_mut() {
                                                     let result = transport.send(ClientMessage::InventoryActionRequest {
                                                         request_id: network_request_id,
                                                         slot: inventory.held_slot as u16,
                                                         action: vibecraft::network::InventoryAction::Drop { count: 1 },
                                                         expected_revision: network_inventory_revision,
                                                     });
                                                     if result.is_ok() {
                                                         network_request_id = network_request_id.wrapping_add(1).max(1);
                                                     } else if let Err(error) = result {
                                                         command_feedback = format!("Drop failed: {error}");
                                                         command_feedback_timer = 2.0;
                                                     }
                                                 }
                                             } else {
                                                 let dropped = inventory.drop_selected();
                                                 if dropped.id != inventory::item::AIR {
                                                     spawn_dropped_stack(&mut dropped_items, player.x, player.y + player.current_eye_height(), player.z, dropped, &item_registry);
                                                 }
                                             }
                                         }
                                     }
                                    _ => {}
                                }
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        cursor_x = position.x as f32;
                        cursor_y = position.y as f32;
                    }
                        WindowEvent::MouseInput { state, button, .. } => {
                            if ui_state.is_menu_open() && !inventory_open
                                && *state == ElementState::Pressed
                                && *button == MouseButton::Left
                            {
                                let action = ui_state.click(renderer.size.0 as f32, renderer.size.1 as f32, cursor_x, cursor_y);
                                  if action == UiAction::ToggleGraphics {
                                      render_quality = if ui_state.graphics_vibrant { GraphicsQuality::Vibrant } else { GraphicsQuality::Regular };
                                  }
                                   if action == UiAction::RetryLoading {
                                       chunk_manager.retry_failed_chunks();
                                      spawn_search = new_player.then(|| SpawnSearch::new(
                                          world_spawn[0], world_spawn[2], SAFE_SPAWN_SEARCH_RADIUS_CHUNKS,
                                      ));
                                      local_load_plan.retry(std::time::Instant::now());
                                      ui_state.loading_error = None;
                                      ui_state.loading_progress = 0.0;
                                      ui_state.screen = ui::UiScreen::Loading;
                                  }
                                  if action == UiAction::BackToTitle {
                                      spawn_search = None;
                                      ui_state.loading_error = None;
                                      ui_state.open_title();
                                  }
                                 if action == UiAction::OpenWorldSelect {
                                      match discover_worlds(&config.world_dir) {
                                          Ok(discovery) => {
                                              for (path, error) in discovery.rejected {
                                                  log::warn!("skipping invalid world {}: {error}", path.display());
                                              }
                                              browser_paths = discovery.worlds.iter().map(|world| world.path.clone()).collect();
                                              ui_state.open_world_select(discovery.worlds.into_iter().map(|world| ui::UiWorld {
                                                  name: world.name,
                                                  gamemode: world.gamemode,
                                                  hardcore: world.hardcore,
                                                  last_played: world.last_played,
                                              }).collect());
                                          }
                                          Err(error) => {
                                              command_feedback = format!("Could not list worlds: {error}");
                                              command_feedback_timer = 5.0;
                                          }
                                      }
                                  }
                                  if action == UiAction::LoadSelectedWorld {
                                      if let Some(path) = ui_state.selected_world.and_then(|index| browser_paths.get(index)).cloned() {
                                          pending_world_open = Some(WorldOpenRequest::Existing(path));
                                      }
                                  }
                                  if action == UiAction::CreateWorld {
                                      pending_world_open = Some(WorldOpenRequest::Create(ui_state.create_options()));
                                  }
                                  if action == UiAction::DeleteWorld {
                                      if let Some(index) = ui_state.selected_world {
                                          if let Some(path) = browser_paths.get(index).cloned() {
                                              match std::fs::remove_dir_all(&path) {
                                                  Err(error) => {
                                                      command_feedback = format!("Failed to delete world: {error}");
                                                      command_feedback_timer = 5.0;
                                                  }
                                                  Ok(()) => {
                                                      command_feedback = "Deleted world".to_string();
                                                      command_feedback_timer = 3.0;
                                                  }
                                              }
                                          }
                                      }
                                      match discover_worlds(&config.world_dir) {
                                          Ok(discovery) => {
                                              browser_paths = discovery.worlds.iter().map(|world| world.path.clone()).collect();
                                              ui_state.open_world_select(discovery.worlds.into_iter().map(|world| ui::UiWorld {
                                                  name: world.name,
                                                  gamemode: world.gamemode,
                                                  hardcore: world.hardcore,
                                                  last_played: world.last_played,
                                              }).collect());
                                          }
                                          Err(error) => {
                                              command_feedback = format!("Failed to refresh world list: {error}");
                                              command_feedback_timer = 5.0;
                                          }
                                      }
                                  }
                                if action == UiAction::Quit {
                                    if !network_mode {
                                        save_world(
                                            &storage, seed, &mut chunk_manager, &player, &inventory,
                                            game_mode, difficulty, hardcore, game_time, simulation_tick,
                                            world_spawn, do_daylight_cycle, keep_inventory, experience,
                                             &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                                        );
                                    }
                                    if let Some(transport) = client_transport.as_mut() {
                                        let _ = transport.disconnect();
                                    }
                                    target.exit();
                                }
                                if action == UiAction::QuitToTitle {
                                    if !network_mode {
                                        save_world(
                                            &storage, seed, &mut chunk_manager, &player, &inventory,
                                            game_mode, difficulty, hardcore, game_time, simulation_tick,
                                            world_spawn, do_daylight_cycle, keep_inventory, experience,
                                             &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                                        );
                                    }
                                    if let Some(transport) = client_transport.as_mut() {
                                        let _ = transport.disconnect();
                                    }
                                    inventory_open = false;
                                    grabbed = false;
                                    input.mouse_grabbed = false;
                                    window.set_cursor_visible(true);
                                    let _ = window.set_cursor_grab(CursorGrabMode::None);
                                    ui_state.open_title();
                                }
                                 if action == UiAction::ConnectServer {
                                     network_username = ui_state.connect_username.clone();
                                     network_connect_request = Some(ui_state.server_address.clone());
                                     ui_state.screen = ui::UiScreen::Loading;
                                     ui_state.connecting = true;
                                 }
                                if !ui_state.is_menu_open() {
                                    grabbed = true;
                                    input.mouse_grabbed = true;
                                    window.set_cursor_visible(false);
                                    let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                                }
                                renderer.gui_dirty = true;
                            } else if inventory_open
                                && *state == ElementState::Pressed
                                && (*button == MouseButton::Left || *button == MouseButton::Right)
                            {
                            let sw = renderer.size.0 as f32;
                            let sh = renderer.size.1 as f32;
                            let gui_scale = ui_state.gui_scale;
                             let crafting_slot = ui::player_crafting_slot_at(sw, sh, gui_scale, cursor_x, cursor_y);
                             let crafting_result = ui::player_crafting_result_at(sw, sh, gui_scale, cursor_x, cursor_y);
                             if !network_mode && (crafting_slot.is_some() || crafting_result) {
                                 let _ = click_crafting(
                                     &mut player_crafting,
                                     crafting_slot,
                                     crafting_result,
                                     *button == MouseButton::Left,
                                     &mut carried_item,
                                     &item_registry,
                                 );
                             } else if let Some(slot_idx) = screen_pos_to_inventory_slot(cursor_x, cursor_y, sw, sh, gui_scale) {
                                if network_mode {
                                    if let Some(transport) = client_transport.as_mut() {
                                        let click_button = if *button == MouseButton::Right { 1 } else { 0 };
                                        let result = transport.send(ClientMessage::InventoryActionRequest {
                                            request_id: network_request_id,
                                            slot: slot_idx as u16,
                                            action: vibecraft::network::InventoryAction::Click { button: click_button, mode: 0 },
                                            expected_revision: network_inventory_revision,
                                        });
                                        if result.is_ok() {
                                            network_request_id = network_request_id.wrapping_add(1).max(1);
                                        } else if let Err(error) = result {
                                            command_feedback = format!("Inventory action failed: {error}");
                                            command_feedback_timer = 2.0;
                                        }
                                    }
                                } else {
                                    let left_click = *button == MouseButton::Left;
                                    if let Some(mut carried) = carried_item.take() {
                                        if !inventory.can_place_in_slot(slot_idx, &carried, &item_registry) {
                                            carried_item = Some(carried);
                                        } else {
                                            let slot = &mut inventory.slots[slot_idx];
                                            if slot.is_empty() {
                                                if left_click {
                                                    std::mem::swap(slot, &mut carried);
                                                } else {
                                                    *slot = inventory::ItemStack::with_damage(carried.id, 1, carried.damage);
                                                    carried.count -= 1;
                                                }
                                            } else if slot.can_merge_with(&carried) {
                                                let max_stack = slot.max_stack(&item_registry) as u16;
                                                let transfer = if left_click { carried.count.min(max_stack.saturating_sub(slot.count)) } else { 1.min(carried.count).min(max_stack.saturating_sub(slot.count)) };
                                                slot.count += transfer;
                                                carried.count -= transfer;
                                            } else if left_click {
                                                std::mem::swap(slot, &mut carried);
                                            }
                                            if !carried.is_empty() {
                                                carried_item = Some(carried);
                                            }
                                        }
                                    } else {
                                        let slot = &mut inventory.slots[slot_idx];
                                        if !slot.is_empty() {
                                            let take = if left_click { slot.count } else { slot.count.div_ceil(2) };
                                            carried_item = Some(inventory::ItemStack::with_damage(slot.id, take, slot.damage));
                                            slot.count -= take;
                                            if slot.count == 0 {
                                                *slot = inventory::EMPTY_STACK;
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Clicked outside inventory - drop carried item
                                if !network_mode {
                                    if let Some(item) = carried_item.take() {
                                    spawn_dropped_stack(&mut dropped_items, player.x, player.y + player.current_eye_height(), player.z, item, &item_registry);
                                    }
                                }
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        let amount = match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => *y as i32,
                            winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as i32,
                        };
                        if chat_state.open {
                            chat_state.scroll(amount);
                            renderer.gui_dirty = true;
                        } else if ui_state.screen == ui::UiScreen::WorldSelect {
                            ui_state.scroll_world_list(-(amount as isize), ui::world_list_visible_count(renderer.size.1 as f32, ui_state.gui_scale));
                            renderer.gui_dirty = true;
                        } else if !ui_state.captures_gameplay_input() && amount > 0 {
                            inventory.held_slot = (inventory.held_slot + inventory::HOTBAR_SLOTS - 1)
                                % inventory::HOTBAR_SLOTS;
                        } else if !ui_state.captures_gameplay_input() && amount < 0 {
                            inventory.held_slot = (inventory.held_slot + 1) % inventory::HOTBAR_SLOTS;
                        }
                    }
                    WindowEvent::Focused(focused) => {
                        if *focused && !grabbed {
                            if !ui_state.is_menu_open() && !inventory_open && !chat_state.open {
                                grabbed = true;
                                input.mouse_grabbed = true;
                                window.set_cursor_visible(false);
                                let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                            }
                        } else if !*focused && grabbed {
                            grabbed = false;
                            input.mouse_grabbed = false;
                            window.set_cursor_visible(true);
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                            input.clear();
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent { event: dev_event, .. } => {
                input.handle_device_event(&dev_event);
            }
            Event::AboutToWait => {
                profiler::new_frame();
                let _frame_scope = profiler::Scope::new("frame");

                let now = std::time::Instant::now();
                let frame_dt = (now - last_time).as_secs_f32().min(0.25);
                last_time = now;

                if let Some(request) = pending_world_open.take() {
                    if !network_mode && inventory_open {
                        return_crafting_items(
                            &mut player_crafting,
                            &mut inventory,
                            &mut carried_item,
                            &mut dropped_items,
                            player.x,
                            player.y + player.current_eye_height(),
                            player.z,
                            &item_registry,
                        );
                    }
                    if !network_mode && !save_world(
                        &storage, seed, &mut chunk_manager, &player, &inventory,
                        game_mode, difficulty, hardcore, game_time, simulation_tick,
                        world_spawn, do_daylight_cycle, keep_inventory, experience,
                        &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                    ) {
                        command_feedback = "World save failed; current world remains loaded.".to_string();
                        command_feedback_timer = 5.0;
                    } else {
                        let opened = match request {
                            WorldOpenRequest::Existing(path) => {
                                let storage = WorldStorage::new(path);
                                storage.load_level().map(|level| (storage, level)).map_err(|error| error.to_string())
                            }
                            WorldOpenRequest::Create(options) => {
                                let name = options.name.trim().to_string();
                                let path = unique_world_directory(&config.world_dir, &name);
                                path.and_then(|path| {
                                    std::fs::create_dir(&path).map_err(|error| format!("failed to create {}: {error}", path.display()))?;
                                    let storage = WorldStorage::new(path);
                                    let now = unix_millis();
                                    let level = LevelData {
                                        name,
                                        created_at: now,
                                        last_played: now,
                                        coordinate_profile: WorldCoordinateProfile::new_world(),
                                        generation_profile: WorldGenerationProfile::new_world(),
                                        seed: options.seed.unwrap_or_else(|| config.resolved_seed()),
                                        tick: 0,
                                        game_time: 9_000,
                                        spawn: [0, 75, 0],
                                        gamemode: options.gamemode.level_gamemode().to_string(),
                                        difficulty: if options.gamemode.hardcore() { "hard" } else { "normal" }.to_string(),
                                        hardcore: options.gamemode.hardcore(),
                                        do_daylight_cycle: true,
                                        keep_inventory: false,
                                        experience: 0,
                                        scheduled_ticks: Vec::new(),
                                        dropped_items: Vec::new(),
                                        xp_orbs: Vec::new(),
                                        players: Vec::new(),
                                    };
                                    storage.load_or_create_level(level).map(|level| (storage, level)).map_err(|error| error.to_string())
                                })
                            }
                        };
                        match opened {
                            Err(error) => {
                                command_feedback = format!("Could not open world: {error}");
                                command_feedback_timer = 5.0;
                            }
                            Ok((next_storage, mut next_level)) => {
                                next_level.last_played = unix_millis();
                                if let Err(error) = next_storage.save_level(&next_level) {
                                    command_feedback = format!("Could not update world metadata: {error}");
                                    command_feedback_timer = 5.0;
                                    return;
                                }
                                let (mut next_player, next_inventory, is_new_player) = match next_storage.load_player() {
                                    Ok(data) => match data.into_runtime() {
                                        Ok((player, inventory)) => (player, inventory, false),
                                        Err(error) => {
                                            command_feedback = format!("Could not load player: {error}");
                                            command_feedback_timer = 5.0;
                                            return;
                                        }
                                    },
                                    Err(StorageError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => (
                                        Player::new(next_level.spawn[0] as f32, next_level.spawn[1] as f32, next_level.spawn[2] as f32),
                                        Inventory::new(),
                                        true,
                                    ),
                                    Err(error) => {
                                        command_feedback = format!("Could not load player: {error}");
                                        command_feedback_timer = 5.0;
                                        return;
                                    }
                                };
                                if let Some(transport) = client_transport.as_mut() { let _ = transport.disconnect(); }
                                client_transport = None;
                                network_address = None;
                                network_mode = false;
                                network_session_id = None;
                                network_reconnect_timer = f32::INFINITY;
                                network_connect_request = None;
                                server_connect_timer = 0.0;
                                remote_players.clear();
                                network_inventory_revision = 0;
                                network_request_id = 1;

                                storage = next_storage;
                                seed = next_level.seed;
                                game_mode = GameMode::from_str(&next_level.gamemode).unwrap_or(GameMode::Survival);
                                difficulty = Difficulty::from_str(&next_level.difficulty).unwrap_or(Difficulty::Normal);
                                hardcore = next_level.hardcore;
                                game_time = next_level.game_time as f32 / 20.0;
                                simulation_tick = next_level.tick;
                                do_daylight_cycle = next_level.do_daylight_cycle;
                                keep_inventory = next_level.keep_inventory;
                                world_spawn = next_level.spawn;
                                if is_new_player {
                                    prepare_new_player_spawn(
                                        seed,
                                        next_level.generation_profile,
                                        &mut world_spawn,
                                        &mut next_player,
                                    );
                                }
                                flying = game_mode.can_fly();
                                player = next_player;
                                inventory = next_inventory;
                                if is_new_player {
                                    for (index, (id, count)) in [(1u16, 64), (2, 64), (3, 64), (4, 64), (15, 64), (13, 64), (BlockId::OakDoor as u16, 16), (BlockId::OakFence as u16, 64), (BlockId::RedstoneDust as u16, 64)].iter().enumerate() {
                                        inventory.slots[inventory::HOTBAR_START + index] = inventory::ItemStack::new(*id, *count);
                                    }
                                }
                                new_player = is_new_player;
                                camera = Camera::new(player.eye_position(), renderer.size.0 as f32 / renderer.size.1 as f32);
                                chunk_manager = ChunkManager::new(
                                    seed,
                                    render_distance,
                                    next_level.coordinate_profile,
                                    next_level.generation_profile,
                                );
                                chunk_manager.set_storage(storage.clone());
                                chunk_manager.update_chunks_async(
                                    (player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                                    (player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                                );
                                dropped_items = next_level.dropped_items.iter().filter_map(|data| dropped_item_from_data(data, &item_registry)).collect();
                                xp_orbs = next_level.xp_orbs.iter().map(xp_orb_from_data).collect();
                                entities = EntityStore::new();
                                if !is_new_player {
                                    entities.spawn(EntityKind::TrainingDummy, Transform::new(nalgebra::Vector3::new(world_spawn[0] as f32 + 3.0, world_spawn[1] as f32, world_spawn[2] as f32)));
                                }
                                experience = next_level.experience;
                                tick_scheduler = TickScheduler::from_events(next_level.scheduled_ticks);
                                simulation_clock = world::simulation::FixedStepClock::new();
                                player_crafting = CraftingGrid::player();
                                carried_item = None;
                                inventory_open = false;
                                chat_state.close();
                                break_target = None;
                                break_progress = 0.0;
                                right_was_pressed = false;
                                stable_target_hit = None;
                                highlight = None;
                                break_overlay = None;
                                last_break_overlay_pos = None;
                                last_hit_pos = None;
                                chunk_render_data.clear();
                                all_chunk_data.clear();
                                render_cache.clear();
                                border_data = None;
                                border_needs_rebuild = true;
                                local_load_plan = LocalLoadPlan::new(
                                    &player,
                                    world_spawn,
                                    new_player,
                                    now,
                                );
                                spawn_search = is_new_player.then(|| {
                                    SpawnSearch::new(world_spawn[0], world_spawn[2], SAFE_SPAWN_SEARCH_RADIUS_CHUNKS)
                                });
                                last_save = now;
                                ui_state.screen = ui::UiScreen::Loading;
                                ui_state.loading_progress = 0.0;
                                ui_state.connecting = false;
                                input.clear();
                                grabbed = false;
                                input.mouse_grabbed = false;
                                window.set_cursor_visible(true);
                                let _ = window.set_cursor_grab(CursorGrabMode::None);
                            }
                        }
                    }
                    renderer.gui_dirty = true;
                }

                if let Some(address_text) = network_connect_request.take() {
                    let address_text = address_text.trim();
                    match resolve_server_address(address_text) {
                        Err(error) => {
                            ui_state.screen = ui::UiScreen::Connect;
                            ui_state.connecting = false;
                            command_feedback = format!("Invalid server address: {error}");
                            command_feedback_timer = 5.0;
                        }
                        Ok(address) => match ClientTransport::connect(address, network_username.clone()) {
                            Err(error) => {
                                ui_state.screen = ui::UiScreen::Connect;
                                ui_state.connecting = false;
                                command_feedback = format!("Could not connect: {error}");
                                command_feedback_timer = 5.0;
                            }
                            Ok(transport) => {
                                if !network_mode {
                                    save_world(
                                        &storage, seed, &mut chunk_manager, &player, &inventory,
                                        game_mode, difficulty, hardcore, game_time, simulation_tick,
                                        world_spawn, do_daylight_cycle, keep_inventory, experience,
                                        &tick_scheduler, &dropped_items, &xp_orbs, !new_player, true,
                                    );
                                }
                                if let Some(previous) = client_transport.as_mut() {
                                    let _ = previous.disconnect();
                                }
                                client_transport = Some(transport);
                                network_address = Some(address);
                                network_mode = true;
                                network_session_id = None;
                                network_reconnect_timer = 0.0;
                                remote_players.clear();
                                chunk_manager.reset_authoritative_session();
                                render_cache.clear();
                                chunk_render_data.clear();
                                all_chunk_data.clear();
                                border_data = None;
                                border_needs_rebuild = true;
                                ui_state.loading_progress = 0.0;
                                server_connect_timer = 5.0;
                                command_feedback_timer = 5.0;
                            }
                        },
                    }
                    renderer.gui_dirty = true;
                }

                if server_connect_timer > 0.0 {
                    server_connect_timer = (server_connect_timer - frame_dt).max(0.0);
                    ui_state.loading_progress = 1.0 - (server_connect_timer / 5.0);
                    if server_connect_timer == 0.0 {
                        ui_state.connecting = false;
                        ui_state.close_to_gameplay();
                        grabbed = true;
                        input.mouse_grabbed = true;
                        window.set_cursor_visible(false);
                        let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                    }
                }

                if ui_state.render_distance != render_distance {
                    render_distance = ui_state.render_distance;
                    chunk_manager.set_render_distance(render_distance);
                    if !network_mode {
                        chunk_manager.update_chunks_async(
                            (player.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                            (player.z.floor() as i32).div_euclid(CHUNK_SIZE as i32),
                        );
                    }
                }
                ui_state.graphics_vibrant = render_quality == GraphicsQuality::Vibrant;

                if let Some(transport) = client_transport.as_mut() {
                    match transport.poll() {
                        Ok(messages) => {
                            for message in messages {
                                match message {
                                    ServerMessage::Welcome {
                                        session_id,
                                        username,
                                        world_seed,
                                        coordinate_profile,
                                        generation_profile,
                                        spawn,
                                        ..
                                    } => {
                                        network_username = username;
                                        chunk_manager = ChunkManager::new(
                                            world_seed,
                                            render_distance,
                                            coordinate_profile,
                                            generation_profile,
                                        );
                                        render_cache.clear();
                                        chunk_render_data.clear();
                                        all_chunk_data.clear();
                                        border_data = None;
                                        network_inventory_revision = 0;
                                        network_last_held_slot = inventory.held_slot;
                                        carried_item = None;
                                        remote_players.clear();
                                        network_session_id = Some(session_id);
                                        game_mode = GameMode::Survival;
                                        flying = false;
                                        player.x = spawn[0] as f32;
                                        player.y = spawn[1] as f32;
                                        player.z = spawn[2] as f32;
                                        player.vy = 0.0;
                                        new_player = false;
                                        camera.position = player.eye_position();
                                        command_feedback = "Connected to authoritative server".to_string();
                                        command_feedback_timer = 3.0;
                                    }
                                    ServerMessage::ChunkData { cx, cz, revision, data } => {
                                        match decode_chunk_data(&data) {
                                            Ok(chunk) if chunk.cx == cx && chunk.cz == cz => {
                                                if let Err(error) = chunk_manager.apply_chunk_data(chunk, revision) {
                                                    log::warn!("failed to apply server chunk ({cx}, {cz}): {error}");
                                                } else {
                                                    border_needs_rebuild = true;
                                                }
                                            }
                                            Ok(_) => log::warn!("server chunk coordinates do not match its envelope"),
                                            Err(error) => log::warn!("failed to decode server chunk ({cx}, {cz}): {error}"),
                                        }
                                    }
                                    ServerMessage::ChunkUnload { cx, cz } => {
                                        if chunk_manager.unload_authoritative_chunk(cx, cz) {
                                            render_cache.remove(&(cx, cz));
                                            border_needs_rebuild = true;
                                        }
                                    }
                                    ServerMessage::PlayerSpawn { player_id, username, position } => {
                                        if Some(player_id) != network_session_id {
                                            let position = [position[0] as f32, position[1] as f32, position[2] as f32];
                                            remote_players.insert(player_id, RemotePlayerVisual {
                                                username,
                                                position,
                                                target_position: position,
                                                velocity: [0.0; 3],
                                                yaw: 0.0,
                                                walk_phase: 0.0,
                                                walk_amount: 0.0,
                                            });
                                        }
                                    }
                                    ServerMessage::PlayerDespawn { player_id } => {
                                        remote_players.remove(&player_id);
                                    }
                                     ServerMessage::PlayerUpdate { player_id, position, velocity, yaw, pitch: _, .. } => {
                                        if Some(player_id) == network_session_id {
                                            player.x = position[0] as f32;
                                            player.y = position[1] as f32;
                                            player.z = position[2] as f32;
                                            player.vy = velocity[1];
                                             // Orientation is local presentation state. Applying
                                             // delayed server echoes here makes mouse panning snap
                                             // backward and forward every authoritative tick.
                                         } else if let Some(remote) = remote_players.get_mut(&player_id) {
                                             remote.target_position = [position[0] as f32, position[1] as f32, position[2] as f32];
                                             remote.velocity = velocity;
                                             remote.yaw = yaw;
                                        }
                                    }
                                    ServerMessage::BlockUpdate { position, state, revision } => {
                                        if let Some(block_id) = BlockId::from_repr(state.block_id) {
                                            let changed = chunk_manager.apply_block_state(
                                                position[0],
                                                position[1],
                                                position[2],
                                                Block { id: block_id, state: state.state, data: state.data },
                                                revision,
                                            );
                                            if changed {
                                                border_needs_rebuild = true;
                                            }
                                        }
                                    }
                                    ServerMessage::InventorySnapshot { revision, slots, held_slot, cursor } => {
                                        let valid = slots.len() == inventory::TOTAL_SLOTS
                                            && slots.iter().all(|stack| {
                                                if stack.count == 0 {
                                                    stack.item_id == inventory::item::AIR && stack.damage == 0
                                                } else if item_registry.is_valid(stack.item_id) {
                                                    let definition = item_registry.def(stack.item_id);
                                                    stack.count <= definition.max_stack as u16
                                                        && (definition.max_damage == 0 || stack.damage < definition.max_damage)
                                                } else {
                                                    false
                                                }
                                            })
                                            && (cursor.count == 0
                                                || (item_registry.is_valid(cursor.item_id)
                                                    && cursor.count <= item_registry.def(cursor.item_id).max_stack as u16
                                                    && (item_registry.def(cursor.item_id).max_damage == 0
                                                        || cursor.damage < item_registry.def(cursor.item_id).max_damage)));
                                        if valid {
                                            inventory.slots = slots
                                                .into_iter()
                                                .map(|stack| inventory::ItemStack::with_damage(stack.item_id, stack.count, stack.damage))
                                                .collect();
                                            inventory.held_slot = (held_slot as usize).min(inventory::HOTBAR_SLOTS - 1);
                                            network_inventory_revision = revision;
                                            network_last_held_slot = inventory.held_slot;
                                            carried_item = if cursor.count == 0 {
                                                None
                                            } else {
                                                Some(inventory::ItemStack::with_damage(cursor.item_id, cursor.count, cursor.damage))
                                            };
                                            renderer.gui_dirty = true;
                                        } else {
                                            log::warn!("ignored invalid authoritative inventory snapshot");
                                        }
                                    }
                                    ServerMessage::Chat { sender, message, .. } => {
                                        chat_state.add_message(format!("<{sender}> {message}"));
                                        chat_timer = 3.0;
                                        renderer.gui_dirty = true;
                                    }
                                    ServerMessage::KeepAlive { nonce } => {
                                        if let Err(error) = transport.send(ClientMessage::KeepAlive { nonce }) {
                                            log::warn!("failed to answer server keep-alive: {error}");
                                        }
                                    }
                                    ServerMessage::ActionAccepted { .. } => {}
                                    ServerMessage::Reject { message, .. } => {
                                        command_feedback = format!("Server rejected action: {message}");
                                        command_feedback_timer = 2.0;
                                    }
                                    ServerMessage::Disconnect { message, .. } => {
                                        command_feedback = format!("Disconnected: {message}");
                                        command_feedback_timer = 10.0;
                                    }
                                }
                            }
                        }
                        Err(ClientError::ServerDisconnected { code, message }) => {
                            log::warn!("server disconnected client ({code:?}): {message}");
                            client_transport = None;
                            network_session_id = None;
                            remote_players.clear();
                            command_feedback = format!("Disconnected: {message}");
                            command_feedback_timer = 10.0;
                            if matches!(code, DisconnectCode::ServerFull | DisconnectCode::UnsupportedVersion | DisconnectCode::MalformedMessage | DisconnectCode::ProtocolError | DisconnectCode::Kicked) {
                                network_reconnect_timer = f32::INFINITY;
                            } else {
                                network_reconnect_timer = 2.0;
                            }
                        }
                        Err(ClientError::Closed) => {
                            log::warn!("server closed the client connection");
                            client_transport = None;
                            network_session_id = None;
                            remote_players.clear();
                            network_reconnect_timer = 2.0;
                        }
                        Err(error) => {
                            log::warn!("network client error: {error}");
                            client_transport = None;
                            network_session_id = None;
                            remote_players.clear();
                            network_reconnect_timer = 2.0;
                        }
                    }
                }
                if network_mode && client_transport.is_none() {
                    network_reconnect_timer = (network_reconnect_timer - frame_dt).max(0.0);
                    if network_reconnect_timer == 0.0 {
                        if let Some(address) = network_address {
                            match ClientTransport::connect(address, network_username.clone()) {
                                Ok(transport) => {
                                    log::info!("reconnected to authoritative server at {address}");
                                    client_transport = Some(transport);
                                    command_feedback = "Reconnecting to server".to_string();
                                    command_feedback_timer = 3.0;
                                }
                                Err(error) => {
                                    log::debug!("server reconnect failed: {error}");
                                    network_reconnect_timer = 2.0;
                                }
                            }
                        }
                    }
                }

                fps_counter += 1;
                fps_timer += frame_dt;
                if fps_timer >= 1.0 {
                    fps = fps_counter as f32 / fps_timer;
                    fps_counter = 0;
                    fps_timer = 0.0;
                }

                let gameplay_input = !ui_state.captures_gameplay_input() && !chat_state.open;
                let (mouse_dx, mouse_dy) = input.consume_mouse_delta();
                if gameplay_input {
                    camera.rotate(-mouse_dx * 0.003, mouse_dy * 0.003);
                }
                network_input_timer = (network_input_timer - frame_dt).max(0.0);
                if network_mode && gameplay_input && network_input_timer <= 0.0 {
                    if let Some(transport) = client_transport.as_mut() {
                        if transport.phase() == ClientPhase::Active {
                            if inventory.held_slot != network_last_held_slot {
                                let result = transport.send(ClientMessage::InventoryActionRequest {
                                    request_id: network_request_id,
                                    slot: inventory.held_slot as u16,
                                    action: vibecraft::network::InventoryAction::SwapHotbar {
                                        hotbar_slot: inventory.held_slot as u8,
                                    },
                                    expected_revision: network_inventory_revision,
                                });
                                if result.is_ok() {
                                    network_last_held_slot = inventory.held_slot;
                                    network_request_id = network_request_id.wrapping_add(1).max(1);
                                }
                            }
                            let mut movement = [0.0f32, 0.0, 0.0];
                            if input.is_key_pressed(bindings.forward) {
                                movement[0] += camera.yaw.sin();
                                movement[2] += camera.yaw.cos();
                            }
                            if input.is_key_pressed(bindings.back) {
                                movement[0] -= camera.yaw.sin();
                                movement[2] -= camera.yaw.cos();
                            }
                            if input.is_key_pressed(bindings.left) {
                                movement[0] += camera.yaw.cos();
                                movement[2] -= camera.yaw.sin();
                            }
                            if input.is_key_pressed(bindings.right) {
                                movement[0] -= camera.yaw.cos();
                                movement[2] += camera.yaw.sin();
                            }
                            let length = (movement[0] * movement[0] + movement[2] * movement[2]).sqrt();
                            if length > 1.0 {
                                movement[0] /= length;
                                movement[2] /= length;
                            }
                            let result = transport.send(ClientMessage::Input {
                                sequence: network_request_id,
                                movement,
                                yaw: camera.yaw,
                                pitch: camera.pitch,
                                jump: input.is_key_pressed(bindings.jump),
                                sprint: input.is_key_pressed(bindings.sprint),
                                sneak: input.is_key_pressed(bindings.sneak),
                            });
                            network_request_id = network_request_id.wrapping_add(1).max(1);
                            network_input_timer = 0.05;
                            if let Err(error) = result {
                                log::warn!("failed to send authoritative input: {error}");
                            }
                        }
                    }
                }
                let simulation_steps = if !network_mode && !gameplay_input {
                    0
                } else {
                    simulation_clock.advance(frame_dt)
                };
                if ui_state.screen == ui::UiScreen::Loading && !network_mode {
                    let pcx = (camera.position.x.floor() as i32).div_euclid(CHUNK_SIZE as i32);
                    let pcz = (camera.position.z.floor() as i32).div_euclid(CHUNK_SIZE as i32);
                    let mut required_chunks = local_load_plan.targets.clone();
                    required_chunks.extend(spawn_search
                        .as_ref()
                        .map_or_else(Vec::new, SpawnSearch::required_chunks));
                    required_chunks.sort_unstable();
                    required_chunks.dedup();
                    let mut loading_chunks = required_chunks.clone();
                    for &(cx, cz) in &required_chunks {
                        for dx in -1..=1 {
                            for dz in -1..=1 {
                                loading_chunks.push((cx + dx, cz + dz));
                            }
                        }
                    }
                    loading_chunks.sort_unstable();
                    loading_chunks.dedup();
                    chunk_manager.update_required_chunks_async((pcx, pcz), &loading_chunks);
                    chunk_manager.process_loaded_chunks();
                    if new_player {
                        let mut selected_spawn = None;
                        let mut failed_chunk = None;
                        let search = spawn_search.get_or_insert_with(|| {
                            SpawnSearch::new(world_spawn[0], world_spawn[2], SAFE_SPAWN_SEARCH_RADIUS_CHUNKS)
                        });
                        while let Some((cx, cz)) = search.current() {
                            if chunk_manager.is_chunk_failed(cx, cz) {
                                failed_chunk = Some((cx, cz));
                                break;
                            }
                            if chunk_manager.get_chunk(cx, cz).is_none() {
                                break;
                            }
                            if let Some(spawn) = chunk_manager.find_safe_spawn_in_chunk(cx, cz) {
                                search.select(spawn);
                                selected_spawn = Some(spawn);
                                break;
                            }
                            search.reject_current();
                        }

                        if selected_spawn.is_none()
                            && search.state == SpawnSearchState::Exhausted
                        {
                            selected_spawn = chunk_manager
                                .find_spawn_fallback(world_spawn[0], world_spawn[2]);
                            if let Some(spawn) = selected_spawn {
                                search.select(spawn);
                                log::warn!(
                                    "no safe column in the 26.2 11x11 initial-spawn search; using the preliminary surface spawn"
                                );
                            }
                        }

                        if let Some(spawn) = selected_spawn {
                            let selected_player = Player::new(spawn[0] as f32, spawn[1] as f32, spawn[2] as f32);
                            if save_world(
                                &storage, seed, &mut chunk_manager, &selected_player, &inventory,
                                game_mode, difficulty, hardcore, game_time, simulation_tick,
                                spawn, do_daylight_cycle, keep_inventory, experience,
                                &tick_scheduler, &dropped_items, &xp_orbs, true, false,
                            ) {
                                world_spawn = spawn;
                                player = selected_player;
                                camera.position = player.eye_position();
                                new_player = false;
                                spawn_search = None;
                                local_load_plan.reset_for_fresh_spawn(&player, world_spawn, now);
                                entities.spawn(EntityKind::TrainingDummy, Transform::new(nalgebra::Vector3::new(
                                    world_spawn[0] as f32 + 3.0,
                                    world_spawn[1] as f32,
                                    world_spawn[2] as f32,
                                )));
                                log::info!("selected world spawn at ({}, {}, {})", spawn[0], spawn[1], spawn[2]);
                            } else {
                                ui_state.loading_error = Some("Could not save the selected spawn. Retry to try again.".to_string());
                                ui_state.screen = ui::UiScreen::LoadFailed;
                            }
                        } else if let Some((cx, cz)) = failed_chunk {
                            ui_state.loading_error = Some(format!("Chunk ({cx}, {cz}) could not be loaded."));
                            ui_state.screen = ui::UiScreen::LoadFailed;
                        } else if spawn_search.as_ref().is_some_and(|search| search.state == SpawnSearchState::Exhausted) {
                            ui_state.loading_error = Some("The preliminary spawn chunk has no usable surface.".to_string());
                            ui_state.screen = ui::UiScreen::LoadFailed;
                        }
                    }
                    for key in chunk_manager.rebuild_dirty_meshes() {
                        render_cache.remove(&key);
                        border_needs_rebuild = true;
                    }
                    rebuild_render_data(&mut chunk_render_data, &mut all_chunk_data, &mut render_cache, &chunk_manager, &renderer, &camera);

                    local_load_plan.refresh(&chunk_manager, &render_cache, now);
                    ui_state.loading_progress = local_load_plan.progress();
                    let failed_target = local_load_plan
                        .targets
                        .iter()
                        .copied()
                        .find(|&(cx, cz)| chunk_manager.is_chunk_failed(cx, cz));
                    if !new_player {
                        if let Some(failure) = local_load_plan.recovery_failure(
                            failed_target,
                            now,
                            LOCAL_LOAD_STALL_TIMEOUT,
                        ) {
                            let mut stage_counts = [0usize; 6];
                            for stage in local_load_plan.stages.values() {
                                stage_counts[*stage as usize] += 1;
                            }
                            log::warn!(
                                "local terrain load failed: {failure:?}; stage counts \
                                 missing/requested/generated/lit/cpu/uploaded={stage_counts:?}; \
                                 chunk manager={:?}",
                                chunk_manager.stats()
                            );
                            ui_state.loading_error = Some(match failure {
                                LocalLoadFailure::Chunk((cx, cz)) => {
                                    format!("Chunk ({cx}, {cz}) could not finish loading.")
                                }
                                LocalLoadFailure::Stalled => {
                                    "Terrain loading stalled. Retry to continue.".to_string()
                                }
                            });
                            ui_state.screen = ui::UiScreen::LoadFailed;
                            input.clear();
                            renderer.gui_dirty = true;
                        }
                    }
                    if ui_state.screen == ui::UiScreen::Loading
                        && local_load_plan.is_complete()
                        && now.duration_since(local_load_plan.started) >= MIN_LOCAL_LOAD_DISPLAY
                        && !new_player
                    {
                        log::info!(
                            "local terrain load complete: {} target chunks in {:.2}s",
                            local_load_plan.targets.len(),
                            now.duration_since(local_load_plan.started).as_secs_f32()
                        );
                        break_target = None;
                        break_progress = 0.0;
                        right_was_pressed = false;
                        input.clear();
                        ui_state.close_to_gameplay();
                        grabbed = true;
                        input.mouse_grabbed = true;
                        window.set_cursor_visible(false);
                        let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                        renderer.gui_dirty = true;
                    }
                }
                if !network_mode && gameplay_input {
                    profiler::begin("player_movement");
                    update_local_player_movement(
                        &mut player,
                        &mut camera,
                        &input,
                        bindings,
                        game_mode,
                        flying,
                        &chunk_manager,
                        &inventory,
                        &item_registry,
                        difficulty,
                        frame_dt,
                    );
                    profiler::end("player_movement");
                }
                let mut hit = stable_target_hit.clone();
                for _ in 0..simulation_steps {
                    let dt = world::simulation::SIMULATION_DT;
                    simulation_tick = simulation_tick.wrapping_add(1);
                    player.tick_animation_timers(dt);
                if do_daylight_cycle {
                    game_time = (game_time + dt).rem_euclid(1200.0);
                }

                profiler::begin("player_physics");

                if !network_mode && ui_state.screen != ui::UiScreen::Loading {

                    // Attack cooldown tick (uses held weapon's attack speed)
                    if game_mode.takes_damage() {
                        let held_id = inventory.selected_id();
                        let weapon_speed = item_registry.def(held_id).attack_speed;
                        player.tick_attack_cooldown(dt, weapon_speed);
                    }

                    // Track health for hurt flash/shake
                    let health_before = player.health;

                    // Status effect tick
                    if game_mode.takes_damage() && player.is_alive() {
                        let prev_health = player.health;
                        player.tick_effects(dt, difficulty.damage_multiplier());
                        if (player.health - prev_health).abs() > 0.001 {
                            renderer.gui_dirty = true;
                        }
                    }

                    // Natural health regen (requires hunger >= 18)
                    if game_mode.takes_damage() && player.is_alive() {
                        let prev_health = player.health;
                        player.tick_regen(dt, difficulty.natural_regen_allowed(), difficulty == Difficulty::Peaceful);
                        if (player.health - prev_health).abs() > 0.001 {
                            renderer.gui_dirty = true;
                        }
                    }

                    // Hunger tick (starvation damage)
                    if game_mode.takes_damage() && player.is_alive() {
                        player.tick_hunger(dt, difficulty.damage_multiplier());
                    }

                    // Drowning, lava, suffocation damage
                    if game_mode.takes_damage() && player.is_alive() {
                        player.tick_damage(dt, &chunk_manager, difficulty.damage_multiplier());
                        renderer.gui_dirty = true;
                    }

                    // Hurt flash/shake on damage taken
                    if player.health < health_before {
                        hurt_timer = 0.5;
                    } else if hurt_timer > 0.0 {
                        hurt_timer = (hurt_timer - dt).max(0.0);
                    }

                    // Respawn if dead
                    if !player.is_alive() {
                        if hardcore {
                            flying = false;
                        } else if let Some(spawn) = chunk_manager.find_safe_spawn(
                            world_spawn[0],
                            world_spawn[2],
                            RESPAWN_SEARCH_RADIUS_BLOCKS,
                        ) {
                            if !keep_inventory {
                                for stack in inventory.slots.clone() {
                                    if !stack.is_empty() {
                                        spawn_dropped_stack(
                                            &mut dropped_items,
                                            player.x,
                                            player.y + player.current_eye_height() * 0.5,
                                            player.z,
                                            stack,
                                            &item_registry,
                                        );
                                    }
                                }
                                inventory.clear();
                            }
                            player = Player::new(spawn[0] as f32, spawn[1] as f32, spawn[2] as f32);
                            camera.position = player.eye_position();
                            flying = game_mode.can_fly();
                        }
                    }

                } else {
                    // Adventure mode: no gravity, no flight — static camera
                    // (just allow mouse look and raycasting)
                }

                profiler::end("player_physics");

                profiler::begin("raycast");
                // Raycast for block targeting and highlight
                let (origin, dir) = camera.get_ray();
                let reach = if game_mode == GameMode::Creative { 5.0 } else { 4.5 };
                let raw_hit = chunk_manager.raycast(origin, dir, reach);
                // Hysteresis: hold the last stable hit so the "Looking at" text
                // does not flicker between a block name and "nothing" when
                // sub-pixel camera movement causes the DDA ray to land exactly
                // on a block corner or gap.
                if raw_hit.is_some() {
                    stable_target_hit = raw_hit.clone();
                } else {
                    // Only clear the stable cache when the camera has moved
                    // far enough that the cached position is clearly stale.
                    if let Some(ref cached) = stable_target_hit {
                        let dx = cached.x as f32 - origin.x;
                        let dy = cached.y as f32 - origin.y;
                        let dz = cached.z as f32 - origin.z;
                        if dx * dx + dy * dy + dz * dz > reach * reach {
                            stable_target_hit = None;
                        }
                    }
                }
                hit = if raw_hit.is_some() { raw_hit } else { stable_target_hit.clone() };
                profiler::end("raycast");

                // Update highlight and break overlay (only recreate when target changes)
                let hit_pos = hit.as_ref().map(|h| (h.x, h.y, h.z));
                if hit_pos != last_hit_pos {
                    highlight = hit.as_ref().map(|h| {
                        let (bmin, bmax) = h.block.selection_box();
                        renderer.create_cube_outline(h.x as f32, h.y as f32, h.z as f32, bmin, bmax)
                    });
                    last_hit_pos = hit_pos;
                }

                // Update break overlay (recreate when target or visibility changes)
                if break_target != last_break_overlay_pos || break_progress <= 0.0 {
                    last_break_overlay_pos = break_target;
                    if break_target.is_some() && break_progress > 0.0 && !game_mode.instant_break() {
                        if let Some((bx, by, bz)) = break_target {
                            break_overlay = Some(renderer.create_break_overlay(bx as f32, by as f32, bz as f32));
                        }
                    } else {
                        break_overlay = None;
                    }
                }

                profiler::begin("block_interaction");
                // Block interaction
                let left_down = input.is_mouse_pressed(MouseButton::Left);
                let right_down = input.is_mouse_pressed(MouseButton::Right);

                // Block breaking with hold time
                let hit_pos = hit.as_ref().map(|h| (h.x, h.y, h.z));
                if left_down && gameplay_input && hit_pos.is_some() && game_mode.can_break() {
                    if hit_pos == break_target {
                        let h = hit.as_ref().unwrap();
                        // Don't allow breaking fluids or bedrock
                        if h.block.id == BlockId::Water || h.block.id == BlockId::Lava || h.block.id == BlockId::Bedrock {
                            break_target = None;
                            break_progress = 0.0;
                        } else if game_mode.instant_break() {
                            if network_mode {
                                if let Some(transport) = client_transport.as_mut() {
                                    let expected_revision = chunk_manager
                                        .chunk_revision(h.x.div_euclid(CHUNK_SIZE as i32), h.z.div_euclid(CHUNK_SIZE as i32))
                                        .unwrap_or(0);
                                    let _ = transport.send(ClientMessage::BlockEditRequest {
                                        request_id: network_request_id,
                                        position: [h.x, h.y, h.z],
                                        face: network_face(h.normal),
                                        action: BlockEditAction::Break,
                                        expected_revision,
                                    });
                                    network_request_id = network_request_id.wrapping_add(1).max(1);
                                }
                            } else {
                                chunk_manager.set_block(h.x, h.y, h.z, Block::air());
                                mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, h.x, h.y, h.z);
                            }
                            break_target = None;
                            break_progress = 0.0;
                        } else {
                            let outcome = mining_outcome(h.block.id, inventory.selected_stack(), &item_registry);
                            if !outcome.break_seconds.is_finite() {
                                break_progress = 0.0;
                                break_target = None;
                            }
                            // Haste: +20% per level, Mining Fatigue: -70% per level.
                            let haste_level = player.effects.get_level(player::StatusEffect::Haste);
                            let fatigue_level = player.effects.get_level(player::StatusEffect::MiningFatigue);
                            let effect_speed = (1.0 + 0.2 * haste_level) * (0.3_f32).powf(fatigue_level);
                            break_progress += dt / outcome.break_seconds * effect_speed;
                            if break_progress >= 1.0 {
                                if !network_mode {
                                    if !outcome.drop.is_empty() {
                                        spawn_dropped_stack(&mut dropped_items, h.x as f32 + 0.5, h.y as f32 + 0.5, h.z as f32 + 0.5, outcome.drop, &item_registry);
                                    }
                                    if outcome.experience > 0 {
                                        for _ in 0..outcome.experience.min(5) {
                                            xp_orbs.push(XpOrb::new(
                                                h.x as f32 + 0.5, h.y as f32 + 0.5, h.z as f32 + 0.5,
                                                (outcome.experience / outcome.experience.min(5).max(1)).max(1),
                                            ));
                                        }
                                    }
                                    if outcome.damages_tool {
                                        inventory.hotbar_slot_mut(inventory.held_slot).damage_once(&item_registry);
                                    }
                                }
                                if network_mode {
                                    if let Some(transport) = client_transport.as_mut() {
                                        let expected_revision = chunk_manager
                                            .chunk_revision(h.x.div_euclid(CHUNK_SIZE as i32), h.z.div_euclid(CHUNK_SIZE as i32))
                                            .unwrap_or(0);
                                        let _ = transport.send(ClientMessage::BlockEditRequest {
                                            request_id: network_request_id,
                                            position: [h.x, h.y, h.z],
                                            face: network_face(h.normal),
                                            action: BlockEditAction::Break,
                                            expected_revision,
                                        });
                                        network_request_id = network_request_id.wrapping_add(1).max(1);
                                    }
                                } else {
                                    chunk_manager.set_block(h.x, h.y, h.z, Block::air());
                                    mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, h.x, h.y, h.z);
                                }
                                break_progress = 0.0;
                                break_target = None;
                            }
                        }
                    } else {
                        // Started targeting a new block
                        break_target = hit_pos;
                        break_progress = 0.0;
                    }
                } else if left_down && gameplay_input && hit_pos.is_none() && game_mode.takes_damage() {
                    let (origin, direction) = camera.get_ray();
                    if player.get_attack_cooldown() >= player::CRITICAL_COOLDOWN_THRESHOLD {
                        if let Some(target) = entities.raycast(origin, direction, 3.0) {
                            let weapon = item_registry.def(inventory.selected_id());
                            let damage = player.get_attack_damage(weapon.attack_damage);
                            let knockback = direction.normalize() * player.get_sprint_knockback();
                            if entities.melee_damage(target.entity, damage, knockback) {
                                player.reset_attack_cooldown();
                                player.attack_exhaustion();
                                command_feedback = "Hit target".to_string();
                                command_feedback_timer = 0.5;
                            }
                        }
                    }
                    break_progress = 0.0;
                    break_target = None;
                } else {
                    break_progress = 0.0;
                    break_target = None;
                }

                if right_down && !right_was_pressed && gameplay_input && game_mode.can_place() {
                    if network_mode {
                        if let Some(target) = hit.as_ref() {
                            let placed_id = inventory.selected_id();
                            if let Some(block_id) = item_registry.block_from_item(placed_id) {
                                let expected_revision = chunk_manager
                                    .chunk_revision(target.x.div_euclid(CHUNK_SIZE as i32), target.z.div_euclid(CHUNK_SIZE as i32))
                                    .unwrap_or(0);
                                if let Some(transport) = client_transport.as_mut() {
                                    let _ = transport.send(ClientMessage::BlockEditRequest {
                                        request_id: network_request_id,
                                        position: [target.x, target.y, target.z],
                                        face: network_face(target.normal),
                                        action: BlockEditAction::Place {
                                            state: WireBlockState { block_id: block_id as u16, state: 0, data: 0 },
                                        },
                                        expected_revision,
                                    });
                                    network_request_id = network_request_id.wrapping_add(1).max(1);
                                }
                            }
                        }
                    } else {
                    let placed_id = inventory.selected_id();
                    let container_message = hit.as_ref().and_then(|target| {
                        interact_container(
                            &mut chunk_manager,
                            target.x,
                            target.y,
                            target.z,
                            &mut inventory,
                            &item_registry,
                            player.sneaking,
                        )
                    });
                    if let Some(message) = container_message {
                        command_feedback = message.to_string();
                        command_feedback_timer = 1.0;
                    } else if inventory.equip_selected_armor(&item_registry) {
                        command_feedback = "Equipped armor".to_string();
                        command_feedback_timer = 1.0;
                    } else if item_registry.is_food(placed_id) && game_mode.takes_damage() {
                        player.hunger = (player.hunger + item_registry.def(placed_id).food_value).min(player::MAX_FOOD);
                        player.saturation = (player.saturation + item_registry.def(placed_id).food_value * item_registry.def(placed_id).saturation_ratio).min(player.hunger);
                        inventory.remove_from_hotbar(inventory.held_slot, 1);
                        if item_registry.is_golden_apple(placed_id) {
                            player.effects.apply(player::StatusEffect::Absorption, 120.0, 0);
                            player.effects.apply(player::StatusEffect::Regeneration, 5.0, 1);
                        }
                        command_feedback = format!("Ate {}", item_registry.name(placed_id));
                        command_feedback_timer = 2.0;
                    } else if let Some(ref h) = hit {
                        if let Some(block_id) = item_registry.block_from_item(placed_id) {
                                let px = h.x + h.normal.0;
                                let py = h.y + h.normal.1;
                                let pz = h.z + h.normal.2;
                                let existing = chunk_manager.get_block(px, py, pz);
                                // Don't place inside the player's AABB
                                let place_inside_player = player.x + player::HALF_WIDTH > px as f32
                                    && player.x - player::HALF_WIDTH < (px + 1) as f32
                                    && player.y + player.current_height() > py as f32
                                    && player.y < (py + 1) as f32
                                    && player.z + player::HALF_WIDTH > pz as f32
                                    && player.z - player::HALF_WIDTH < (pz + 1) as f32;
                                if existing.is_air() && !matches!(block_id, BlockId::Air | BlockId::Water | BlockId::Lava) && !place_inside_player {
                                        if chunk_manager.place_block(px, py, pz, block_id) {
                                            mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, px, py, pz);
                                            // Survival: consume item
                                            if game_mode.takes_damage() {
                                                inventory.remove_from_hotbar(inventory.held_slot, 1);
                                            }
                                            // Sponge water absorption
                                            if block_id == BlockId::Sponge {
                                                if chunk_manager.absorb_water_sponge(px, py, pz) {
                                                    chunk_manager.set_block(px, py, pz, Block::new(BlockId::WetSponge));
                                                    mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, px, py, pz);
                                                }
                                            }
                                        }
                                }
                            }
                        }
                    }
                }
                right_was_pressed = right_down;
                profiler::end("block_interaction");

                profiler::begin("dropped_items");
                // Update dropped items
                dropped_items.retain(|item| item.is_alive(&chunk_manager));
                for item in &mut dropped_items {
                    item.update(dt, &chunk_manager);
                    // Bubble columns: soul sand → upward, magma → downward
                    if player.swimming || player.is_in_lava(&chunk_manager) {
                        let ix = item.x.floor() as i32;
                        let iy = (item.y - 0.5).floor() as i32;
                        let iz = item.z.floor() as i32;
                        let below = chunk_manager.get_block(ix, iy, iz);
                        let above_water = chunk_manager.get_block(ix, iy + 1, iz).id == BlockId::Water;
                        if above_water && below.id == BlockId::SoulSand {
                            item.vy += 14.0 * dt;
                        } else if above_water && below.id == BlockId::MagmaBlock {
                            item.vy -= 6.0 * dt;
                        }
                    }
                }
                // Item merging uses stack compatibility and preserves any remainder.
                let mut merge_positions: std::collections::HashMap<(i32, i32, i32), usize> = std::collections::HashMap::new();
                let mut i = 0;
                while i < dropped_items.len() {
                    if dropped_items[i].lifetime < 290.0 {
                        i += 1;
                        continue;
                    }
                    let cell = (
                        dropped_items[i].x.floor() as i32,
                        dropped_items[i].y.floor() as i32,
                        dropped_items[i].z.floor() as i32,
                    );
                    if let Some(&prev) = merge_positions.get(&cell) {
                        let merged = {
                            let (before, current) = dropped_items.split_at_mut(i);
                            before[prev].try_merge(&mut current[0], &item_registry)
                        };
                        if merged && dropped_items[i].stack.is_empty() {
                            dropped_items.remove(i);
                            continue;
                        }
                    }
                    merge_positions.insert(cell, i);
                    i += 1;
                }

                // Item pickup first inserts into the inventory and retains any
                // remainder when the inventory is full.
                let px = if flying { camera.position.x } else { player.x };
                let py = if flying { camera.position.y } else { player.y + player.current_eye_height() * 0.5 };
                let pz = if flying { camera.position.z } else { player.z };
                dropped_items.retain_mut(|item| {
                    let dx = item.x - px;
                    let dy = item.y - py;
                    let dz = item.z - pz;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist < 2.0 && item.pickup_delay <= 0.0 {
                        item.stack = inventory.add_stack(item.stack.clone(), &item_registry);
                        item.stack.is_empty()
                    } else {
                        true
                    }
                });

                // Update XP orbs and attract toward player
                xp_orbs.retain(|orb| orb.is_alive(&chunk_manager));
                for orb in &mut xp_orbs {
                    orb.update(dt, &chunk_manager, px, py, pz);
                    // Bubble columns for XP orbs
                    let ox = orb.x.floor() as i32;
                    let oy = (orb.y - 0.5).floor() as i32;
                    let oz = orb.z.floor() as i32;
                    let below = chunk_manager.get_block(ox, oy, oz);
                    let above_water = chunk_manager.get_block(ox, oy + 1, oz).id == BlockId::Water;
                    if above_water && below.id == BlockId::SoulSand {
                        orb.vy += 14.0 * dt;
                    } else if above_water && below.id == BlockId::MagmaBlock {
                        orb.vy -= 6.0 * dt;
                    }
                }
                // Collect XP orbs within 2 blocks
                xp_orbs.retain(|orb| {
                    let dx = orb.x - px;
                    let dy = orb.y - py;
                    let dz = orb.z - pz;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist < 2.0 {
                        experience += orb.value;
                        false
                    } else {
                        true
                    }
                });

                profiler::end("dropped_items");

                let pcx = (camera.position.x.floor() as i32).div_euclid(CHUNK_SIZE as i32);
                let pcz = (camera.position.z.floor() as i32).div_euclid(CHUNK_SIZE as i32);
                if !network_mode {
                    profiler::begin("update_chunks");
                    if !player.is_alive() && !hardcore {
                        let spawn_chunk = (
                            world_spawn[0].div_euclid(CHUNK_SIZE as i32),
                            world_spawn[2].div_euclid(CHUNK_SIZE as i32),
                        );
                        chunk_manager.update_chunks_async_for_centers(&[(pcx, pcz), spawn_chunk]);
                    } else {
                        chunk_manager.update_chunks_async(pcx, pcz);
                    }
                    profiler::end("update_chunks");
                    profiler::begin("process_chunks");
                    chunk_manager.process_loaded_chunks();
                    profiler::end("process_chunks");
                }
                if !network_mode {
                profiler::begin("scheduled_ticks");
                for event in tick_scheduler.drain_due(simulation_tick) {
                    let (kind, interval) = match event.kind {
                        ScheduledTickKind::Water => {
                            chunk_manager.tick_water(pcx, pcz);
                            (ScheduledTickKind::Water, 5)
                        }
                        ScheduledTickKind::Lava => {
                            chunk_manager.tick_lava(pcx, pcz);
                            (ScheduledTickKind::Lava, 8)
                        }
                        // No random-ticking block family is supported yet. Keep
                        // this persistent event kind for future blocks without
                        // allowing an unloaded chunk to be mutated today.
                        ScheduledTickKind::Random => (ScheduledTickKind::Random, 20),
                    };
                    tick_scheduler.schedule(ScheduledTick {
                        due_tick: simulation_tick + interval,
                        chunk: [pcx, pcz],
                        kind,
                    });
                }
                profiler::end("scheduled_ticks");
                }
                // Furnace work is deterministic fixed-step simulation and only
                // touches loaded block entities; completed recipes award XP.
                if !network_mode {
                    experience = experience.saturating_add(chunk_manager.tick_block_entities(&item_registry));
                    let _projectile_hits = entities.tick(&chunk_manager);
                }
                profiler::begin("rebuild_meshes");
                for key in chunk_manager.rebuild_dirty_meshes() {
                    render_cache.remove(&key);
                    border_needs_rebuild = true;
                }
                profiler::end("rebuild_meshes");
                profiler::begin("rebuild_render_data");
    rebuild_render_data(&mut chunk_render_data, &mut all_chunk_data, &mut render_cache, &chunk_manager, &renderer, &camera);
                profiler::end("rebuild_render_data");
                }
                if network_mode {
                    // Smoothly interpolate the camera between server position
                    // updates so the view remains fluid despite 20 TPS physics.
                    let target_eye = player.eye_position();
                    let delta = target_eye - camera.position;
                    let dist = delta.norm();
                    if dist > 8.0 {
                        camera.position = target_eye;
                    } else {
                        let blend = 1.0 - (-frame_dt * 18.0).exp();
                        camera.position = camera.position + delta * blend;
                    }
                }
                // Vanilla's dynamic FOV is presentation state, so update it
                // with the render cadence rather than the simulation tick.
                let sprinting = input.is_key_pressed(bindings.sprint) && !flying;
                let base_fov = 70.0_f32.to_radians();
                let moving = input.is_key_pressed(bindings.forward)
                    || input.is_key_pressed(bindings.left)
                    || input.is_key_pressed(bindings.back)
                    || input.is_key_pressed(bindings.right);
                let target_fov = if sprinting && moving { base_fov * 1.05 } else { base_fov };
                // Exponential smoothing — frame-rate-independent, ~0.4s to settle
                let lerp_factor = 1.0 - (-6.0 * frame_dt).exp();
                camera.fov += (target_fov - camera.fov) * lerp_factor;

                let world_billboards: Vec<_> = dropped_items
                    .iter()
                    .filter(|item| !item.stack.is_empty())
                    .map(|item| {
                        let slot = ui_slot(
                            &item.stack,
                            &item_registry,
                            &inventory_block_sprite_map,
                            false,
                        );
                        engine::renderer::WorldBillboard {
                            position: [item.x, item.y, item.z],
                            sprite: slot.sprite,
                        }
                    })
                    .collect();

                // XP orbs, temporary entities, and remote players retain the
                // terrain-mesh path. Dropped item stacks render separately as
                // ItemAtlas billboards in Renderer::render.
                {
                    let _dropped_mesh_scope = profiler::Scope::new("dropped_items_mesh");
                    for remote in remote_players.values_mut() {
                        let delta = [
                            remote.target_position[0] - remote.position[0],
                            remote.target_position[1] - remote.position[1],
                            remote.target_position[2] - remote.position[2],
                        ];
                        let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
                        if distance > 8.0 {
                            remote.position = remote.target_position;
                        } else {
                            let blend = 1.0 - (-frame_dt * 18.0).exp();
                            remote.position[0] += delta[0] * blend;
                            remote.position[1] += delta[1] * blend;
                            remote.position[2] += delta[2] * blend;
                        }
                        let horizontal_speed = (remote.velocity[0] * remote.velocity[0]
                            + remote.velocity[2] * remote.velocity[2]).sqrt();
                        let target_walk = (horizontal_speed / 4.3).clamp(0.0, 1.0);
                        let animation_blend = 1.0 - (-frame_dt * 12.0).exp();
                        remote.walk_amount += (target_walk - remote.walk_amount) * animation_blend;
                        remote.walk_phase = (remote.walk_phase + horizontal_speed * frame_dt * 5.0)
                            .rem_euclid(std::f32::consts::TAU);
                    }
                    let xp_mesh = xp_orbs_to_mesh(&xp_orbs);
                    let entity_data: Vec<_> = entities
                        .entities()
                        .map(|entity| (
                            entity.transform.position.x,
                            entity.transform.position.y,
                            entity.transform.position.z,
                            BlockId::Target,
                        ))
                        .collect();
                    let entity_mesh = build_item_cube_mesh(&entity_data);
                    let player_instances: Vec<_> = remote_players.values().map(|remote| PlayerMeshInstance {
                        position: remote.position,
                        yaw: remote.yaw,
                        walk_phase: remote.walk_phase,
                        walk_amount: remote.walk_amount,
                    }).collect();
                    let player_mesh = build_player_mesh(&player_instances);
                    let mut combined = xp_mesh;
                    let vert_offset = combined.vertices.len() as u32;
                    combined.vertices.extend(entity_mesh.vertices);
                    combined.indices.extend(entity_mesh.indices.iter().map(|i| i.saturating_add(vert_offset)));
                    let vert_offset = combined.vertices.len() as u32;
                    combined.vertices.extend(player_mesh.vertices);
                    combined.indices.extend(player_mesh.indices.iter().map(|i| i.saturating_add(vert_offset)));
                    if !combined.vertices.is_empty() {
                        let vert_len = combined.vertices.len();
                        let idx_len = combined.indices.len();
                        let need_recreate = renderer.item_vb_cap < vert_len
                            || renderer.item_ib_cap < idx_len;
                        let _old_item_vb;
                        let _old_item_ib;
                        if need_recreate {
                            renderer.item_vb_cap = (vert_len * 2).next_power_of_two();
                            renderer.item_ib_cap = (idx_len * 2).next_power_of_two();
                            let new_vb = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("item_vb"),
                                size: (renderer.item_vb_cap as u64) * std::mem::size_of::<crate::engine::renderer::Vertex>() as u64,
                                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            });
                            _old_item_vb = std::mem::replace(&mut renderer.item_vb, new_vb);
                            let new_ib = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("item_ib"),
                                size: (renderer.item_ib_cap as u64) * std::mem::size_of::<u32>() as u64,
                                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            });
                            _old_item_ib = std::mem::replace(&mut renderer.item_ib, new_ib);
                        }
                        renderer.queue.write_buffer(&renderer.item_vb, 0,
                            bytemuck::cast_slice(&combined.vertices));
                        renderer.queue.write_buffer(&renderer.item_ib, 0,
                            bytemuck::cast_slice(&combined.indices));
                        chunk_render_data.push((i32::MAX, i32::MAX, crate::engine::renderer::ChunkRenderData {
                            vertex_buffer: renderer.item_vb.clone(),
                            index_buffer: renderer.item_ib.clone(),
                            num_indices: combined.indices.len() as u32,
                            transparent_vertex_buffer: renderer.item_vb.clone(),
                            transparent_index_buffer: renderer.item_ib.clone(),
                            transparent_num_indices: 0,
                        }));
                    }
                }

                // Show feedback line when visible, even without F3 debug overlay
                let feedback_visible = command_feedback_timer > 0.0 && !command_feedback.is_empty();

                let capture_workload = show_debug || profiler::is_enabled();
                let chunk_stats = capture_workload.then(|| chunk_manager.stats());
                let visible_chunks = chunk_render_data
                    .iter()
                    .filter(|(cx, cz, _)| (*cx, *cz) != (i32::MAX, i32::MAX))
                    .count();
                let (opaque_triangles, transparent_triangles, draw_calls) = chunk_render_data
                    .iter()
                    .fold((0u64, 0u64, 0usize), |(ot, tt, dc), (_, _, data)| {
                        (
                            ot + (data.num_indices / 3) as u64,
                            tt + (data.transparent_num_indices / 3) as u64,
                            dc + (data.num_indices > 0) as usize + (data.transparent_num_indices > 0) as usize,
                        )
                    });
                if profiler::is_enabled() {
                    if let Some(stats) = chunk_stats {
                        profiler::set_gauge("chunks_loaded", stats.loaded as f64);
                        profiler::set_gauge("chunks_meshed", stats.meshed as f64);
                        profiler::set_gauge("chunks_pending", stats.pending as f64);
                        profiler::set_gauge("mesh_queue", stats.dirty_queue as f64);
                        profiler::set_gauge("light_queue", stats.light_dirty_queue as f64);
                    }
                    profiler::set_gauge("visible_chunks", visible_chunks as f64);
                    profiler::set_gauge("draw_calls", draw_calls as f64);
                    profiler::set_gauge("opaque_triangles", opaque_triangles as f64);
                    profiler::set_gauge("transparent_triangles", transparent_triangles as f64);
                    profiler::set_gauge("dropped_items", dropped_items.len() as f64);
                }

                // Build debug lines for F3 overlay
                let debug_lines: Vec<String> = if show_debug {
                    let pos = format!("XYZ: {:.1} / {:.1} / {:.1}", camera.position.x, camera.position.y, camera.position.z);
                    let block = match &hit {
                        Some(h) => format!("Looking at: {} ({},{},{})", h.block.id.name(), h.x, h.y, h.z),
                        None => "Looking at: nothing".to_string(),
                    };
                    let biome = format!("Biome: {}", chunk_manager.get_biome_name(camera.position.x as f64, camera.position.z as f64));
                    let facing = format!("Facing: {:.1}° / {:.1}°", camera.yaw.to_degrees(), camera.pitch.to_degrees());
                    let break_info = if break_target.is_some() {
                        format!("Break: {:.0}%", break_progress * 100.0)
                    } else {
                        String::new()
                    };
                    let hunger_bar = format!("Food: {:.0}/{} [{:.0}]", player.hunger, player::MAX_FOOD, player.saturation);
                    let armor_bar = if player.armor_points > 0.0 {
                        format!("  Armor: {}", player.armor_points)
                    } else {
                        String::new()
                    };
                    let absorption_bar = if player.absorption_health > 0.0 {
                        format!("  Abs: {:.0}", player.absorption_health)
                    } else {
                        String::new()
                    };
                    let cooldown_pct = if game_mode.takes_damage() {
                        format!("  CD: {:.0}%", player.get_attack_cooldown() * 100.0)
                    } else {
                        String::new()
                    };
                    let mut lines = vec![
                        format!("Vibecraft  FPS: {:.0}", fps),
                        pos,
                        block,
                        biome,
                        facing,
                        format!("Time: {:.0}s  HP: {:.0}/{}  Mode: {}  Diff: {}{}  Flying: {}  Borders: {}", game_time, player.health, player::MAX_HEALTH, game_mode.name(), difficulty.name(), if hardcore { " (HC)" } else { "" }, if flying { "yes" } else { "no" }, if show_chunk_borders { "ON" } else { "OFF" }),
                        format!("Oxygen: {:.1}/{}  EXP: {}", player.oxygen, player::MAX_OXYGEN, experience),
                        format!("{}{}{}{}", hunger_bar, armor_bar, absorption_bar, cooldown_pct),
                    ];
                    if network_mode {
                        for remote in remote_players.values() {
                            lines.push(format!("Remote {}: {:.1} / {:.1} / {:.1}", remote.username, remote.position[0], remote.position[1], remote.position[2]));
                        }
                    }
                    if let Some(stats) = chunk_stats {
                        let target_chunks = ((render_distance * 2 + 1) as usize).pow(2);
                        lines.push(format!(
                            "Chunks: {}/{} loaded  {} meshed  {} generating  queues: mesh {} / light {}",
                            stats.loaded,
                            target_chunks,
                            stats.meshed,
                            stats.pending,
                            stats.dirty_queue,
                            stats.light_dirty_queue,
                        ));
                        lines.push(format!(
                            "Render workload: {} visible chunks  {} draws  {} opaque + {} transparent triangles  {} items",
                            visible_chunks,
                            draw_calls,
                            opaque_triangles,
                            transparent_triangles,
                            dropped_items.len(),
                        ));
                    }
                    if profiler::is_enabled() {
                        if let Some(snapshot) = profiler::snapshot() {
                            lines.push(format!(
                                "Profiler ON (F5 saves): {:.2}ms latest, {:.2}ms avg, {:.2}ms P95 over {} frames, {} recent stutters",
                                snapshot.latest_frame_ms,
                                snapshot.average_frame_ms,
                                snapshot.p95_frame_ms,
                                snapshot.frames,
                                snapshot.recent_stutters,
                            ));
                            if !snapshot.top_scopes.is_empty() {
                                let top_scopes = snapshot.top_scopes.iter()
                                    .map(|(label, time)| format!("{label} {time:.2}ms"))
                                    .collect::<Vec<_>>()
                                    .join("  ");
                                lines.push(format!("Previous frame CPU: {top_scopes}"));
                            }
                        } else {
                            lines.push(String::from("Profiler ON (F5 saves): collecting first frame"));
                        }
                    }
                    if !break_info.is_empty() {
                        lines.push(break_info);
                    }
                    lines
                } else {
                    Vec::new()
                };

                // Command feedback timer
                command_feedback_timer = (command_feedback_timer - frame_dt).max(0.0);

                // Build hotbar / chat / inventory text
                let hotbar: String;
                let mut chat_lines: Vec<String> = Vec::new();
                if chat_state.open {
                    chat_lines.extend(chat_state.visible_messages(10).iter().cloned());
                    let suggestions = chat::command_suggestions(&chat_state.text);
                    if !suggestions.is_empty() {
                        chat_lines.push(format!("[{}]", suggestions.into_iter().take(5).collect::<Vec<_>>().join(", ")));
                    }
                    if chat_state.unread_messages > 0 {
                        chat_lines.push(format!("{} new message(s)", chat_state.unread_messages));
                    }
                    chat_lines.push(chat_state.display_text().to_string());
                    hotbar = String::new();
                } else if inventory_open {
                    // Inventory rendered graphically in the overlay pass
                    chat_lines.clear();
                    hotbar = String::new();
                } else {
                    chat_timer = (chat_timer - frame_dt).max(0.0);
                    if chat_timer > 0.0 {
                        chat_lines.extend(chat_state.recent_messages(5).iter().cloned());
                    }
                    // Build graphical hotbar with slot boxes
                    let mut h = String::new();
                    for i in 0..inventory::HOTBAR_SLOTS {
                        let stack = inventory.hotbar_slot(i);
                        if i > 0 { h.push(' '); }
                        if i == inventory.held_slot {
                            h.push('[');
                        }
                        if stack.is_empty() {
                            h.push_str("_");
                        } else {
                            let name = item_registry.name(stack.id);
                            let short = if name.len() > 3 { &name[..3] } else { name };
                            h.push_str(short);
                            if stack.count > 1 {
                                h.push_str(&stack.count.to_string());
                            }
                        }
                        if i == inventory.held_slot {
                            h.push(']');
                        }
                    }
                    hotbar = h;
                };

                // Chunk border debug lines (only rebuild when chunks change)
                if show_chunk_borders && border_needs_rebuild {
                    let mut verts: Vec<[f32; 3]> = Vec::new();
                    for (&(cx, cz), _chunk) in &chunk_manager.chunks {
                        let x0 = (cx * CHUNK_SIZE as i32) as f32;
                        let z0 = (cz * CHUNK_SIZE as i32) as f32;
                        let x1 = x0 + CHUNK_SIZE as i32 as f32;
                        let z1 = z0 + CHUNK_SIZE as i32 as f32;
                        // 4 vertical corner lines
                        for &(x, z) in &[(x0, z0), (x1, z0), (x1, z1), (x0, z1)] {
                            verts.push([x, chunk_manager.coordinate_profile().min_y() as f32, z]);
                            verts.push([x, chunk_manager.coordinate_profile().max_y_exclusive() as f32, z]);
                        }
                        // 4 bottom edges
                        let min_y = chunk_manager.coordinate_profile().min_y() as f32;
                        let max_y = chunk_manager.coordinate_profile().max_y_exclusive() as f32;
                        verts.push([x0, min_y, z0]); verts.push([x1, min_y, z0]);
                        verts.push([x1, min_y, z0]); verts.push([x1, min_y, z1]);
                        verts.push([x1, min_y, z1]); verts.push([x0, min_y, z1]);
                        verts.push([x0, min_y, z1]); verts.push([x0, min_y, z0]);
                        // 4 top edges
                        verts.push([x0, max_y, z0]); verts.push([x1, max_y, z0]);
                        verts.push([x1, max_y, z0]); verts.push([x1, max_y, z1]);
                        verts.push([x1, max_y, z1]); verts.push([x0, max_y, z1]);
                        verts.push([x0, max_y, z1]); verts.push([x0, max_y, z0]);
                    }
                    if !verts.is_empty() {
                        let vb = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("border_vb"),
                            size: (verts.len() as u64) * std::mem::size_of::<[f32; 3]>() as u64,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        renderer.queue.write_buffer(&vb, 0, bytemuck::cast_slice(&verts));
                        border_data = Some((vb, verts.len() as u32));
                    } else {
                        border_data = None;
                    }
                    border_needs_rebuild = false;
                }

                // Minecraft 26.2 Overworld SKY_LIGHT_FACTOR timeline. The
                // stored clock is seconds, while the reference keyframes are
                // ticks in a 24,000-tick day.
                let day_tick = (game_time * 20.0).rem_euclid(24_000.0);
                let sky_light_factor = if day_tick < 730.0 {
                    let t = (day_tick + 1_140.0) / 1_870.0;
                    0.24 + 0.76 * t
                } else if day_tick <= 11_270.0 {
                    1.0
                } else if day_tick < 13_140.0 {
                    1.0 - 0.76 * ((day_tick - 11_270.0) / 1_870.0)
                } else if day_tick <= 22_860.0 {
                    0.24
                } else {
                    0.24 + 0.76 * ((day_tick - 22_860.0) / 1_870.0)
                };
                let night_factor = sky_light_factor;

                // Dynamic light direction: sun arcs east→overhead→west during the day
                // game_time 300 = dawn, 450 = noon, 600 = dusk
                let sun_angle = (game_time - 450.0) / 600.0 * std::f32::consts::TAU;
                let mut light_dir = nalgebra::Vector3::new(sun_angle.sin(), sun_angle.cos(), 0.0);
                if light_dir.norm() > 0.0 {
                    light_dir = light_dir.normalize();
                }
                let light_vp = camera.light_vp_matrix(&light_dir);
                let shadow_vp: [[f32; 4]; 4] = *light_vp.as_ref();
                let view_distance = render_distance as f32 * 16.0;
                // Vanilla fog is primarily a horizon cue; starting it close to
                // the camera washed out the terrain long before the horizon.
                let display_brightness = if render_quality == GraphicsQuality::Vibrant {
                    0.8
                } else {
                    0.5
                };
                let eye_block = chunk_manager.get_block(
                    camera.position.x.floor() as i32,
                    camera.position.y.floor() as i32,
                    camera.position.z.floor() as i32,
                ).id;
                let underwater = eye_block == BlockId::Water;
                let fog_params = if underwater {
                    // 26.2 defaults to -8..96 and ramps water vision over time.
                    // Use half range until that player-side timer is modeled.
                    [-8.0, 48.0, display_brightness, 1.0]
                } else {
                    [view_distance * 0.72, view_distance, display_brightness, 0.0]
                };

                if auto_screenshot_frame > 0 {
                    auto_screenshot_frame -= 1;
                    if auto_screenshot_frame == 0 && screenshots_taken < 5 {
                        let path = format!("/tmp/opencode/vibecraft_auto_{}.png", screenshots_taken);
                        renderer.request_screenshot(&path);
                        screenshots_taken += 1;
                        auto_screenshot_frame = 120; // Take multiple screenshots 120 frames apart
                    }
                }

                {
                    let _render_scope = profiler::Scope::new("render");
                    // Build hotbar items for graphical rendering
                    let hotbar_items: Vec<engine::renderer::HotbarItem> = if !inventory_open && !chat_state.open {
                        (0..inventory::HOTBAR_SLOTS).map(|i| {
                            let stack = inventory.hotbar_slot(i);
                             let tile = if !stack.is_empty() {
                                 item_registry.block_from_item(stack.id).map(|bid| {
                                     world::mesh::get_face_tile(bid, world::block::BlockFace::Top)
                                 }).unwrap_or(0)
                             } else { 0 };
                            engine::renderer::HotbarItem {
                                name: if stack.is_empty() { String::new() } else { item_registry.name(stack.id).to_string() },
                                count: stack.count,
                                selected: i == inventory.held_slot,
                                is_empty: stack.is_empty(),
                                tex_tile: tile,
                            }
                        }).collect()
                    } else {
                        Vec::new()
                    };
                    // Build inventory items for graphical inventory rendering
                    let inventory_items: Vec<engine::renderer::InventorySlot> = if inventory_open {
                        (0..inventory::TOTAL_SLOTS).map(|i| {
                            let stack = &inventory.slots[i];
                             let tile = if !stack.is_empty() {
                                 item_registry.block_from_item(stack.id).map(|bid| {
                                     world::mesh::get_face_tile(bid, world::block::BlockFace::Top)
                                 }).unwrap_or(0)
                             } else { 0 };
                            engine::renderer::InventorySlot {
                                name: if stack.is_empty() { String::new() } else { item_registry.name(stack.id).to_string() },
                                count: stack.count,
                                tex_tile: tile,
                                is_empty: stack.is_empty(),
                            }
                        }).collect()
                    } else {
                        Vec::new()
                    };
                    let carried_slot = carried_item.as_ref().map(|ci| engine::renderer::InventorySlot {
                        name: if ci.is_empty() { String::new() } else { item_registry.name(ci.id).to_string() },
                        count: ci.count,
                        tex_tile: if !ci.is_empty() {
                             item_registry.block_from_item(ci.id).map(|bid| world::mesh::get_face_tile(bid, world::block::BlockFace::Top)).unwrap_or(0)
                        } else { 0 },
                        is_empty: ci.is_empty(),
                    });
                    let cursor = if inventory_open || ui_state.is_menu_open() { Some((cursor_x, cursor_y)) } else { None };
                    let ui_hotbar: Vec<UiSlot> = (0..inventory::HOTBAR_SLOTS)
                        .map(|index| ui_slot(inventory.hotbar_slot(index), &item_registry, &inventory_block_sprite_map, index == inventory.held_slot))
                        .collect();
                    let ui_inventory: Vec<UiSlot> = if inventory_open {
                        inventory.slots.iter().enumerate().map(|(index, stack)| ui_slot(stack, &item_registry, &inventory_block_sprite_map, index == inventory.held_slot)).collect()
                    } else {
                        Vec::new()
                    };
                    let ui_carried = carried_item.as_ref().map(|stack| ui_slot(stack, &item_registry, &inventory_block_sprite_map, false));
                    let ui_crafting: Vec<UiSlot> = if inventory_open {
                        player_crafting.slots.slots.iter().map(|stack| ui_slot(stack, &item_registry, &inventory_block_sprite_map, false)).collect()
                    } else {
                        Vec::new()
                    };
                    let craft_result = player_crafting.result(&item_registry);
                    let ui_craft_result = if inventory_open { Some(ui_slot(&craft_result, &item_registry, &inventory_block_sprite_map, false)) } else { None };
                    let toast = if feedback_visible && !show_debug { Some(command_feedback.as_str()) } else { None };
                    let selected_stack = inventory.selected_stack();
                    let selected_item_name = if selected_stack.is_empty() { String::new() } else { item_registry.name(selected_stack.id).to_string() };
                    ui_state.tick();
                    let chat_cursor_info = if chat_state.open {
                        Some((chat_state.cursor_char_index(), chat_state.selection_range()))
                    } else {
                        None
                    };
                    let show_chat_cursor = chat_state.open && ui_state.frame_count % 60 < 30;
                    if ui_state.screen == ui::UiScreen::WorldSelect {
                        let visible = ui::world_list_visible_count(renderer.size.1.max(1) as f32, ui_state.gui_scale);
                        ui_state.clamp_world_scroll(visible);
                    }
                    let ui_frame = ui_state.frame(
                        renderer.size.0.max(1) as f32,
                        renderer.size.1.max(1) as f32,
                        &ui_hotbar,
                        if inventory_open { Some(&ui_inventory) } else { None },
                        if inventory_open { Some(&ui_crafting) } else { None },
                        ui_craft_result.as_ref(),
                        ui_carried.as_ref(),
                        player.health,
                        player.hunger,
                        player.saturation,
                        player.absorption_health,
                        player.armor_points,
                        experience as f32 / 100.0,
                        &selected_item_name,
                        &chat_lines,
                        toast,
                        cursor,
                        gameplay_input,
                        hurt_timer,
                        player.health_blink_timer,
                        player.hunger_shake_timer,
                        player.effects.has(player::StatusEffect::Regeneration),
                        player.effects.has(player::StatusEffect::Poison),
                        player.effects.has(player::StatusEffect::Wither),
                        player.effects.has(player::StatusEffect::Hunger),
                        simulation_tick,
                        chat_cursor_info,
                        show_chat_cursor,
                    );
                    // Build nametags: project remote player head positions to screen
                    let mut nametags = Vec::new();
                    let vp = camera.vp_matrix();
                    let sw = renderer.size.0.max(1) as f32;
                    let sh = renderer.size.1.max(1) as f32;
                    for remote in remote_players.values() {
                        let world_pos = nalgebra::Vector4::new(
                            remote.position[0],
                            remote.position[1] + 2.4,
                            remote.position[2],
                            1.0,
                        );
                        let clip = vp * world_pos;
                        if clip.z < 0.0 || clip.w <= 0.0 { continue; }
                        let ndc_x = clip.x / clip.w;
                        let ndc_y = clip.y / clip.w;
                        if ndc_x.abs() > 1.5 || ndc_y.abs() > 1.5 { continue; }
                        let sx = (ndc_x * 0.5 + 0.5) * sw;
                        let sy = (1.0 - (ndc_y * 0.5 + 0.5)) * sh;
                        nametags.push(engine::renderer::NametagRender {
                            screen_x: sx,
                            screen_y: sy,
                            text: remote.username.clone(),
                        });
                    }
                    let ctx = RenderContext {
                        camera: &camera,
                        chunk_data: &chunk_render_data,
                        all_chunk_data: &all_chunk_data,
                        highlight: highlight.as_ref(),
                        break_overlay: break_overlay.as_ref(),
                        break_progress,
                        chunk_borders: border_data.as_ref(),
                        debug_overlay: if show_debug { Some(&debug_lines) } else { None },
                        hotbar_text: &hotbar,
                        chat_lines: &chat_lines,
                        feedback_line: if feedback_visible && !show_debug { Some(&command_feedback) } else { None },
                        night_factor,
                        fog_params,
                        shadow_vp: &shadow_vp,
                        light_dir: &light_dir,
                        game_time,
                        vibrant: render_quality == GraphicsQuality::Vibrant,
                        hotbar_items: if hotbar_items.is_empty() { None } else { Some(&hotbar_items) },
                        inventory_open,
                        inventory_items: if inventory_items.is_empty() { None } else { Some(&inventory_items) },
                        cursor_pos: cursor,
                        carried_item: carried_slot.as_ref(),
                        health: player.health,
                        hunger: player.hunger,
                        ui_frame: Some(&ui_frame),
                        ui_captures_gameplay: ui_state.captures_gameplay_input() || chat_state.open,
                        nametags: &nametags,
                        world_billboards: &world_billboards,
                        blur_enabled: ui_state.screen == ui::UiScreen::Pause || ui_state.screen == ui::UiScreen::Options || ui_state.screen == ui::UiScreen::Controls || ui_state.screen == ui::UiScreen::Accessibility,
                        blur_intensity: ui_state.blur_intensity,
                    };
                    renderer.blur_enabled = ctx.blur_enabled;
                    renderer.blur_intensity = ctx.blur_intensity;
                    match renderer.render(&ctx) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            log::warn!("Surface lost, resizing");
                            renderer.resize(renderer.size);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                        Err(_) => {}
                    }
                }

                if !network_mode && last_save.elapsed().as_secs_f32() >= 30.0 {
                    save_world(
                        &storage,
                        seed,
                        &mut chunk_manager,
                        &player,
                        &inventory,
                        game_mode,
                        difficulty,
                        hardcore,
                        game_time,
                        simulation_tick,
                        world_spawn,
                        do_daylight_cycle,
                        keep_inventory,
                        experience,
                        &tick_scheduler,
                        &dropped_items,
                        &xp_orbs,
                        !new_player,
                        false,
                    );
                    last_save = std::time::Instant::now();
                }

                input.end_frame();
            }
            _ => {}
        }
    });
}

fn save_world(
    storage: &WorldStorage,
    seed: u64,
    chunk_manager: &mut ChunkManager,
    player: &Player,
    inventory: &Inventory,
    game_mode: GameMode,
    difficulty: Difficulty,
    hardcore: bool,
    game_time: f32,
    simulation_tick: u64,
    world_spawn: [i32; 3],
    do_daylight_cycle: bool,
    keep_inventory: bool,
    experience: u32,
    tick_scheduler: &TickScheduler,
    dropped_items: &[DroppedItem],
    xp_orbs: &[XpOrb],
    persist_player: bool,
    wait_for_chunks: bool,
) -> bool {
    let chunks_saved = if wait_for_chunks {
        chunk_manager.flush_saved_chunks()
    } else {
        chunk_manager.queue_saved_chunks();
        true
    };
    let prior_level = storage.load_level().ok();
    let level_saved = storage.save_level(&LevelData {
        name: prior_level
            .as_ref()
            .map(|level| level.name.clone())
            .unwrap_or_else(|| "New World".to_string()),
        created_at: prior_level.as_ref().map_or_else(unix_millis, |level| level.created_at),
        last_played: unix_millis(),
        coordinate_profile: prior_level
            .as_ref()
            .map_or(WorldCoordinateProfile::new_world(), |level| level.coordinate_profile),
        generation_profile: chunk_manager.generation_profile(),
        seed,
        tick: simulation_tick,
        game_time: (game_time.rem_euclid(1200.0) * 20.0).round() as u64,
        spawn: world_spawn,
        gamemode: match game_mode {
            GameMode::Survival => "survival",
            GameMode::Creative => "creative",
            GameMode::Adventure => "adventure",
            GameMode::Spectator => "spectator",
        }
        .to_string(),
        difficulty: match difficulty {
            Difficulty::Peaceful => "peaceful",
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
        .to_string(),
        hardcore,
        do_daylight_cycle,
        keep_inventory,
        experience,
        scheduled_ticks: tick_scheduler.events(),
        dropped_items: dropped_items.iter().map(dropped_item_data).collect(),
        xp_orbs: xp_orbs.iter().map(xp_orb_data).collect(),
        players: Vec::new(),
    });
    // Persist spawn metadata before a new player's position so a crash cannot
    // leave player data pointing at an uncommitted provisional world spawn.
    let player_saved = if persist_player && level_saved.is_ok() {
        Some(storage.save_player(&PlayerData::from_runtime(player, inventory)))
    } else {
        None
    };
    let save_succeeded = chunks_saved
        && level_saved.is_ok()
        && player_saved.as_ref().map_or(true, Result::is_ok);
    if !chunks_saved {
        log::error!("some changed chunks could not be saved; keeping them loaded for retry");
    }
    if let Some(Err(error)) = player_saved {
        log::error!("failed to save player data: {error}");
    }
    if let Err(error) = level_saved {
        log::error!("failed to save level metadata: {error}");
    }
    save_succeeded
}

fn execute_command(
    cmd: &str,
    cm: &mut ChunkManager,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
    target: &Option<(i32, i32, i32)>,
    feedback: &mut String,
    save_requested: &mut bool,
    quit_requested: &mut bool,
    game_mode: &mut GameMode,
    difficulty: &mut Difficulty,
    hardcore: &mut bool,
    game_time: &mut f32,
    do_daylight_cycle: &mut bool,
    keep_inventory: &mut bool,
    world_spawn: &mut [i32; 3],
    _dropped_items: &mut Vec<DroppedItem>,
    camera: &mut Camera,
    seed: u64,
    experience: &mut u32,
    player: &mut Player,
    inventory: &mut Inventory,
    item_registry: &ItemRegistry,
) {
    // Extract the command parts:
    //   /<action> [<subj> [<args>...]]
    let parts: Vec<&str> = cmd.trim_start_matches('/').split_whitespace().collect();
    let action = parts.first().copied().unwrap_or("").to_ascii_lowercase();
    let action = action.as_str();
    let subj = parts.get(1).copied().unwrap_or("");
    let rest: Vec<&str> = parts.get(2..).map(|s| s.to_vec()).unwrap_or_default();

    if action == "save" {
        *save_requested = true;
        *feedback = "Saving world...".to_string();
        return;
    }
    if action == "quit" {
        *save_requested = true;
        *quit_requested = true;
        *feedback = "Saving world and quitting...".to_string();
        return;
    }

    if action == "tp" || action == "teleport" {
        let coordinates = if subj == "@s" { &rest[..] } else { &parts[1..] };
        if coordinates.len() != 3 {
            *feedback = "Usage: /teleport [@s] <x> <y> <z>".to_string();
            return;
        }
        let position = [
            parse_command_coordinate(coordinates[0], player.x),
            parse_command_coordinate(coordinates[1], player.y),
            parse_command_coordinate(coordinates[2], player.z),
        ];
        if let [Some(x), Some(y), Some(z)] = position {
            if !cm.contains_world_y(y) {
                *feedback = "Position is outside the world's build height.".to_string();
                return;
            }
            player.x = x as f32;
            player.y = y as f32;
            player.z = z as f32;
            player.vy = 0.0;
            camera.position = player.eye_position();
            *feedback = format!("Teleported to {x}, {y}, {z}");
        } else {
            *feedback = "Invalid position. Use absolute numbers or ~ relative coordinates.".to_string();
        }
        return;
    }

    if action == "setblock" {
        if parts.len() < 5 {
            *feedback = "Usage: /setblock <x> <y> <z> <block>".to_string();
            return;
        }
        let position = [
            parse_command_coordinate(parts[1], player.x),
            parse_command_coordinate(parts[2], player.y),
            parse_command_coordinate(parts[3], player.z),
        ];
        let block = command_block_id(parts[4]);
        if let ([Some(x), Some(y), Some(z)], Some(block)) = (position, block) {
            if !cm.contains_world_y(y) {
                *feedback = "Position is outside the world's build height.".to_string();
                return;
            }
            cm.set_block(x, y, z, Block::new(block));
            mark_neighbors_dirty(cm, cache, x, y, z);
            *feedback = format!("Set block at {x}, {y}, {z}");
        } else {
            *feedback = "Invalid position or unsupported block identifier.".to_string();
        }
        return;
    }

    if action == "fill" {
        if parts.len() < 8 {
            *feedback = "Usage: /fill <from> <to> <block> [replace|keep|destroy]".to_string();
            return;
        }
        let coordinates = [
            parse_command_coordinate(parts[1], player.x), parse_command_coordinate(parts[2], player.y), parse_command_coordinate(parts[3], player.z),
            parse_command_coordinate(parts[4], player.x), parse_command_coordinate(parts[5], player.y), parse_command_coordinate(parts[6], player.z),
        ];
        let Some(block) = command_block_id(parts[7]) else {
            *feedback = "Unsupported block identifier.".to_string();
            return;
        };
        let [Some(x0), Some(y0), Some(z0), Some(x1), Some(y1), Some(z1)] = coordinates else {
            *feedback = "Invalid position. Use absolute numbers or ~ relative coordinates.".to_string();
            return;
        };
        let (min_x, max_x) = (x0.min(x1), x0.max(x1));
        let (min_y, max_y) = (y0.min(y1), y0.max(y1));
        let (min_z, max_z) = (z0.min(z1), z0.max(z1));
        let volume = (max_x - min_x + 1) as i64 * (max_y - min_y + 1) as i64 * (max_z - min_z + 1) as i64;
        if !cm.contains_world_y(min_y) || !cm.contains_world_y(max_y) || volume > 32_768 {
            *feedback = "Fill region is outside build height or exceeds 32768 blocks.".to_string();
            return;
        }
        let keep = parts.get(8).is_some_and(|mode| *mode == "keep");
        let mut dirty = HashSet::new();
        let mut changed = 0;
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for z in min_z..=max_z {
                    if keep && !cm.get_block(x, y, z).is_air() {
                        continue;
                    }
                    cm.set_block(x, y, z, Block::new(block));
                    dirty.insert((x.div_euclid(CHUNK_SIZE as i32), z.div_euclid(CHUNK_SIZE as i32)));
                    changed += 1;
                }
            }
        }
        for key in cm.rebuild_chunks_now(&dirty) {
            cache.remove(&key);
        }
        *feedback = format!("Filled {changed} block(s).");
        return;
    }

    // Handle game mode commands (no target needed)
    if action == "gamemode" || action == "gm" {
        if let Some(mode) = GameMode::from_str(subj) {
            if *hardcore && mode != GameMode::Survival {
                *feedback = "Cannot change game mode in hardcore mode!".to_string();
                return;
            }
            *game_mode = mode;
            *feedback = format!("Set game mode to {}", mode.name());
        } else if subj.is_empty() {
            *feedback = format!("Current game mode: {}", game_mode.name());
        } else {
            *feedback = format!(
                "Unknown game mode: {}. Use: survival, creative, adventure, spectator",
                subj
            );
        }
        return;
    }

    // Handle difficulty command
    if action == "difficulty" || action == "d" {
        if *hardcore {
            *feedback = "Cannot change difficulty in hardcore mode (locked to Hard).".to_string();
            return;
        }
        if let Some(d) = Difficulty::from_str(subj) {
            *difficulty = d;
            *feedback = format!("Set difficulty to {}", d.name());
        } else if subj.is_empty() {
            *feedback = format!("Current difficulty: {}", difficulty.name());
        } else {
            *feedback = format!(
                "Unknown difficulty: {}. Use: peaceful, easy, normal, hard",
                subj
            );
        }
        return;
    }

    // Handle hardcore command
    if action == "hardcore" || action == "hc" {
        *hardcore = true;
        *difficulty = Difficulty::Hard;
        *game_mode = GameMode::Survival;
        *feedback =
            "Hardcore mode enabled! Difficulty locked to Hard, permanent death.".to_string();
        return;
    }

    // Handle time set command
    if action == "time" && subj == "query" {
        match rest.first().copied() {
            Some("daytime") => *feedback = format!("The time is {:.0}", game_time.rem_euclid(1200.0)),
            Some("gametime") => *feedback = format!("The game time is {:.0}", *game_time),
            _ => *feedback = "Usage: /time query <daytime|gametime>".to_string(),
        }
        return;
    }

    if action == "time" && subj == "set" {
        let arg = rest.first().copied().unwrap_or("");
        let new_time = match arg.to_lowercase().as_str() {
            "day" => Some(300.0),
            "noon" => Some(450.0),
            "night" => Some(900.0),
            "midnight" => Some(0.0),
            _ => arg.parse::<f32>().ok(),
        };
        if let Some(t) = new_time {
            *game_time = t.rem_euclid(1200.0);
            *feedback = format!("Set time to {:.0}s ({})", *game_time, arg);
        } else {
            *feedback = format!("Usage: /time set <0-1200|day|night>");
        }
        return;
    }

    if action == "setworldspawn" {
        let position = if parts.len() == 1 {
            Some([camera.position.x.floor() as i32, camera.position.y.floor() as i32, camera.position.z.floor() as i32])
        } else if parts.len() == 4 {
            match (
                parse_command_coordinate(parts[1], player.x),
                parse_command_coordinate(parts[2], player.y),
                parse_command_coordinate(parts[3], player.z),
            ) {
                (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                _ => None,
            }
        } else {
            None
        };
        let Some(position) = position else {
            *feedback = "Usage: /setworldspawn [x y z]".to_string();
            return;
        };
        if !cm.contains_world_y(position[1]) {
            *feedback = "Position is outside the world's build height.".to_string();
            return;
        }
        *world_spawn = position;
        *feedback = format!(
            "World spawn set to ({}, {}, {})",
            world_spawn[0], world_spawn[1], world_spawn[2]
        );
        return;
    }

    // Handle give command (adds to inventory)
    if action == "give" {
        let item_name = subj;
        let count = rest
            .first()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1)
            .min(64);
        // Try as block first, then as item
        let item_id: Option<ItemId> = if let Some(bid) = command_block_id(item_name) {
            Some(item_registry.item_id_from_block(bid))
        } else {
            item_id_from_command(item_registry, item_name)
        };
        if let Some(id) = item_id {
            let remaining = inventory.add_item(id, count, item_registry);
            let given = count - remaining;
            *feedback = format!("Gave {} x {}", given, item_registry.name(id));
        } else {
            *feedback = format!(
                "Unknown item: {}. Try /give stone, /give dirt, etc.",
                item_name
            );
        }
        return;
    }

    // Handle seed command
    if action == "seed" {
        *feedback = format!("Seed: {}", seed);
        return;
    }

    // Handle xp command
    if action == "xp" || action == "experience" {
        let amount_text = if subj == "add" { rest.first().copied().unwrap_or("") } else { subj };
        if let Ok(amount) = amount_text.parse::<u32>() {
            *experience += amount;
            *feedback = format!("Gave {} experience (total: {})", amount, experience);
        } else if subj.is_empty() {
            *feedback = format!("Experience: {}", experience);
        } else {
            *feedback = "Usage: /experience add <amount> [points]".to_string();
        }
        return;
    }

    // Handle /effect command
    if action == "effect" || action == "ef" {
        let effect_name = subj;
        let duration: f32 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(30.0);
        let amplifier: u32 = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let effect = match effect_name.to_lowercase().as_str() {
            "speed" => Some(player::StatusEffect::Speed),
            "slowness" | "slow" => Some(player::StatusEffect::Slowness),
            "haste" => Some(player::StatusEffect::Haste),
            "mining_fatigue" | "fatigue" => Some(player::StatusEffect::MiningFatigue),
            "strength" | "str" => Some(player::StatusEffect::Strength),
            "jump_boost" | "jump" => Some(player::StatusEffect::JumpBoost),
            "regeneration" | "regen" => Some(player::StatusEffect::Regeneration),
            "resistance" | "resist" => Some(player::StatusEffect::Resistance),
            "fire_resistance" | "fire_resist" => Some(player::StatusEffect::FireResistance),
            "water_breathing" | "water_breath" => Some(player::StatusEffect::WaterBreathing),
            "night_vision" | "nv" => Some(player::StatusEffect::NightVision),
            "invisibility" | "invis" => Some(player::StatusEffect::Invisibility),
            "absorption" | "abs" => Some(player::StatusEffect::Absorption),
            "slow_falling" | "slowfall" => Some(player::StatusEffect::SlowFalling),
            "dolphin_grace" | "dolphin" => Some(player::StatusEffect::DolphinGrace),
            "weakness" => Some(player::StatusEffect::Weakness),
            "poison" => Some(player::StatusEffect::Poison),
            "wither" => Some(player::StatusEffect::Wither),
            "hunger" => Some(player::StatusEffect::Hunger),
            "nausea" => Some(player::StatusEffect::Nausea),
            "blindness" | "blind" => Some(player::StatusEffect::Blindness),
            "levitation" | "levi" => Some(player::StatusEffect::Levitation),
            "darkness" | "dark" => Some(player::StatusEffect::Darkness),
            "instant_health" | "insta_heal" => Some(player::StatusEffect::InstantHealth),
            "instant_damage" | "insta_dmg" => Some(player::StatusEffect::InstantDamage),
            "health_boost" | "hp_boost" => Some(player::StatusEffect::HealthBoost),
            "saturation" | "sat" => Some(player::StatusEffect::SaturationEffect),
            "fatal_poison" | "fatal" => Some(player::StatusEffect::FatalPoison),
            "bad_omen" | "omen" => Some(player::StatusEffect::BadOmen),
            "hero_of_the_village" | "hero" => Some(player::StatusEffect::HeroOfTheVillage),
            "wind_charged" | "wind" => Some(player::StatusEffect::WindCharged),
            "infested" => Some(player::StatusEffect::Infested),
            "oozing" | "ooze" => Some(player::StatusEffect::Oozing),
            "weaving" | "weave" => Some(player::StatusEffect::Weaving),
            "clear" | "remove_all" => {
                player.effects.clear();
                *feedback = "All effects cleared.".to_string();
                return;
            }
            _ => None,
        };
        if let Some(ef) = effect {
            player.effects.apply(ef, duration, amplifier);
            *feedback = format!("Applied {} ({}s, amp {})", ef.name(), duration, amplifier);
        } else {
            *feedback = format!("Unknown effect: {}.", effect_name);
        }
        return;
    }

    // Handle /armor command
    if action == "armor" {
        if let Ok(pts) = subj.parse::<f32>() {
            player.armor_points = pts;
            if let Some(toughness_str) = rest.first() {
                if let Ok(t) = toughness_str.parse::<f32>() {
                    player.armor_toughness = t;
                    *feedback = format!("Set armor to {} pts, toughness to {}", pts, t);
                } else {
                    *feedback = format!("Set armor to {} points (toughness unchanged: {})", pts, player.armor_toughness);
                }
            } else {
                *feedback = format!("Set armor to {} points", pts);
            }
        } else if subj.is_empty() {
            *feedback = format!("Armor: {} pts, toughness: {}", player.armor_points, player.armor_toughness);
        } else {
            *feedback = "Usage: /armor <points> [toughness]".to_string();
        }
        return;
    }

    // Handle /heal command
    if action == "heal" {
        player.health = player::MAX_HEALTH;
        player.hunger = player::MAX_FOOD;
        player.saturation = player::MAX_SATURATION;
        player.absorption_health = 0.0;
        *feedback = "Fully healed.".to_string();
        return;
    }

    // Handle /feed command
    if action == "feed" || action == "eat" {
        player.hunger = player::MAX_FOOD;
        player.saturation = player::MAX_SATURATION;
        *feedback = "Hunger and saturation restored.".to_string();
        return;
    }

    // Handle /clearinventory command
    if action == "clear" || action == "clearinventory" || action == "ci" {
        let item_name = if subj == "@s" { rest.first().copied() } else { (!subj.is_empty()).then_some(subj) };
        if let Some(item_name) = item_name {
            let Some(item_id) = command_block_id(item_name).map(|block| item_registry.item_id_from_block(block)).or_else(|| item_id_from_command(item_registry, item_name)) else {
                *feedback = format!("Unknown item: {item_name}");
                return;
            };
            let mut removed = 0u32;
            for slot in &mut inventory.slots {
                if slot.id == item_id {
                    removed += slot.count as u32;
                    *slot = inventory::EMPTY_STACK;
                }
            }
            *feedback = format!("Cleared {removed} item(s).");
        } else {
            inventory.clear();
            *feedback = "Cleared the inventory.".to_string();
        }
        return;
    }

    // Handle /hotbar command - fill hotbar with items from name
    if action == "hotbar" || action == "hb" {
        let item_name = subj;
        if let Some(bid) = BlockId::from_name(item_name) {
            let item_id = item_registry.item_id_from_block(bid);
            for i in 0..inventory::HOTBAR_SLOTS {
                inventory.slots[inventory::HOTBAR_START + i] =
                    inventory::ItemStack::new(item_id, 64);
            }
            *feedback = format!("Filled hotbar with {}", item_registry.name(item_id));
        } else {
            *feedback = format!("Unknown item: {}", item_name);
        }
        return;
    }

    // Handle /weather command
    if action == "weather" {
        match subj {
            "clear" => {
                *feedback = "Set weather to clear.".to_string();
            }
            "rain" | "rainy" => {
                *feedback = "Set weather to rain.".to_string();
            }
            "thunder" | "storm" => {
                *feedback = "Set weather to thunderstorm.".to_string();
            }
            _ => {
                *feedback = "Usage: /weather <clear|rain|thunder>".to_string();
            }
        }
        return;
    }

    // Handle /kill command
    if action == "kill" {
        if subj.is_empty() || subj == "@s" || subj == "player" {
            player.health = 0.0;
            *feedback = "Ouch. You killed yourself.".to_string();
        } else {
            *feedback = format!("Can't find entity: {}", subj);
        }
        return;
    }

    // Handle /gamerule command
    if action == "gamerule" || action == "g" {
        let rule = subj;
        let value = rest.first().copied().unwrap_or("");
        match rule {
            "doDaylightCycle" | "daylightCycle" | "dodaylightcycle" => {
                if value == "false" || value == "true" {
                    *do_daylight_cycle = value == "true";
                    *feedback = format!("doDaylightCycle set to {}", *do_daylight_cycle);
                } else if value.is_empty() {
                    *feedback = format!("doDaylightCycle = {}", *do_daylight_cycle);
                } else {
                    *feedback = "Usage: /gamerule doDaylightCycle <true|false>".to_string();
                }
            }
            "keepInventory" | "keepinventory" => {
                if value == "false" || value == "true" {
                    *keep_inventory = value == "true";
                    *feedback = format!("keepInventory set to {}", *keep_inventory);
                } else if value.is_empty() {
                    *feedback = format!("keepInventory = {}", *keep_inventory);
                } else {
                    *feedback = "Usage: /gamerule keepInventory <true|false>".to_string();
                }
            }
            "" => {
                *feedback = "Usage: /gamerule <rule> [value]".to_string();
            }
            _ => {
                *feedback = format!("Unknown gamerule: {}. Supported: doDaylightCycle, keepInventory", rule);
            }
        }
        return;
    }

    // Handle /help (no target needed)
    if action == "help" || action == "?" || action == "h" {
        *feedback = "Commands: /teleport, /setblock, /fill, /clear, /experience, /gamemode, /difficulty, /time, /weather, /gamerule, /give, /effect, /seed, /save, /quit. Tab completes supported syntax.".to_string();
        return;
    }

    // Structure commands require a target
    let pos = match target {
        Some(p) => *p,
        None => {
            *feedback = "No block targeted. Look at a block first.".to_string();
            return;
        }
    };

    let mut rng = rand::rng();

    // Determine what structure to spawn
    let structure = match (action, subj) {
        ("summon", name) | ("place", name) => name,
        _ => action,
    };

    match structure {
        "dungeon" | "d" => {
            spawn_dungeon(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned dungeon at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "portal" | "ruined_portal" | "p" => {
            spawn_ruined_portal(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!(
                "Summoned ruined portal at ({}, {}, {})",
                pos.0, pos.1, pos.2
            );
        }
        "lava" | "lava_pool" | "l" => {
            spawn_lava_pool(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned lava pool at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "mushroom" | "giant_mushroom" | "m" => {
            spawn_giant_mushroom(cm, pos.0, pos.1, pos.2, rng.random_bool(0.5), &mut rng, cache);
            *feedback = format!(
                "Summoned giant mushroom at ({}, {}, {})",
                pos.0, pos.1, pos.2
            );
        }
        "tree" | "oak" | "t" => {
            spawn_tree(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned oak tree at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "igloo" | "i" => {
            spawn_igloo_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned igloo at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "swamp_hut" | "hut" | "sh" => {
            spawn_swamp_hut_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned swamp hut at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "well" | "desert_well" | "w" => {
            spawn_desert_well_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned desert well at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "ruin" | "ocean_ruin" | "r" => {
            spawn_ocean_ruin_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned ocean ruin at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        _ => {
            *feedback = format!("Unknown: /{}. Try /help", structure);
        }
    }
}

fn parse_command_coordinate(value: &str, origin: f32) -> Option<i32> {
    if let Some(offset) = value.strip_prefix('~') {
        let offset = if offset.is_empty() { 0.0 } else { offset.parse::<f32>().ok()? };
        return Some((origin + offset).floor() as i32);
    }
    value.parse::<f32>().ok().map(|coordinate| coordinate.floor() as i32)
}

fn command_block_id(value: &str) -> Option<BlockId> {
    let name = value
        .strip_prefix("minecraft:")
        .unwrap_or(value)
        .split_once('[')
        .map_or(value.strip_prefix("minecraft:").unwrap_or(value), |(name, _)| name);
    BlockId::from_name(name)
}

fn item_id_from_command(items: &ItemRegistry, value: &str) -> Option<ItemId> {
    let requested = value.strip_prefix("minecraft:").unwrap_or(value).replace([' ', '-'], "_").to_ascii_lowercase();
    (0..2048u16).find(|&id| {
        items.is_valid(id)
            && items.name(id).replace([' ', '-'], "_").to_ascii_lowercase() == requested
    })
}

fn mark_block(cm: &mut ChunkManager, x: i32, y: i32, z: i32, id: BlockId) {
    if !cm.contains_world_y(y) {
        return;
    }
    cm.set_block(x, y, z, Block::new(id));
}

fn mark_area_dirty(
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
    x: i32,
    z: i32,
    rx: i32,
    rz: i32,
) {
    for dx in -rx..=rx {
        for dz in -rz..=rz {
            let cx = (x + dx).div_euclid(CHUNK_SIZE as i32);
            let cz = (z + dz).div_euclid(CHUNK_SIZE as i32);
            cache.remove(&(cx, cz));
        }
    }
}

fn spawn_dungeon(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let hw = 3i32;
    let hh = 2i32;
    let hd = 3i32;
    for dx in -hw - 1..=hw + 1 {
        for dz in -hd - 1..=hd + 1 {
            for dy in -hh - 1..=hh + 2 {
                let bx = x + dx;
                let by = y + dy;
                let bz = z + dz;
                let is_wall = dx == -hw - 1
                    || dx == hw + 1
                    || dz == -hd - 1
                    || dz == hd + 1
                    || dy == -hh - 1
                    || dy == hh + 2;
                if is_wall {
                    let id = if rng.random_bool(0.3) {
                        BlockId::MossyCobblestone
                    } else {
                        BlockId::Cobblestone
                    };
                    mark_block(cm, bx, by, bz, id);
                } else {
                    mark_block(cm, bx, by, bz, BlockId::Air);
                }
            }
        }
    }
    // Spawner in center
    mark_block(cm, x, y, z, BlockId::Spawner);
    // Chests
    for _ in 0..1 + rng.random_range(0..2) as usize {
        let (cx, cz) = match rng.random_range(0..4) {
            0 => (x - hw + 1 + rng.random_range(0..hw * 2 - 1), z - hd),
            1 => (x - hw + 1 + rng.random_range(0..hw * 2 - 1), z + hd),
            2 => (x - hw, z - hd + 1 + rng.random_range(0..hd * 2 - 1)),
            _ => (x + hw, z - hd + 1 + rng.random_range(0..hd * 2 - 1)),
        };
        if cm.get_block(cx, y, cz).is_air() {
            mark_block(cm, cx, y, cz, BlockId::Chest);
        }
    }
    mark_area_dirty(cache, x, z, hw + 2, hd + 2);
}

fn spawn_ruined_portal(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let fw = 4i32;
    let fh = 5i32;
    let mut missing = 2 + rng.random_range(0..5) as i32;
    for dy in 0i32..fh {
        for dx in 0i32..fw {
            let bx = x - fw / 2 + dx;
            let by = y + dy;
            let bz = z;
            let is_frame = dx == 0 || dx == fw - 1 || dy == 0 || dy == fh - 1;
            let is_top_corner = (dx == 0 || dx == fw - 1) && dy == fh - 1;
            if !is_frame || is_top_corner {
                continue;
            }
            if missing > 0 && !(dx == 0 || dx == fw - 1) && dy > 0 {
                missing -= 1;
                continue;
            }
            let id = if dy == 0 && rng.random_bool(0.4) {
                BlockId::StoneBricks
            } else if rng.random_bool(0.1) {
                BlockId::CryingObsidian
            } else {
                BlockId::Obsidian
            };
            mark_block(cm, bx, by, bz, id);
        }
    }
    // Stone bricks around base
    for dx in -2i32..=fw + 1 {
        for dz in -2i32..=2 {
            let bx = x - fw / 2 + dx;
            let bz = z + dz;
            if dx < 0 || dx >= fw || dz < -1 || dz > 1 {
                if rng.random_bool(0.3) {
                    mark_block(cm, bx, y, bz, BlockId::StoneBricks);
                }
            }
        }
    }
    // Vines
    for _ in 0..3 {
        let vx = x - fw / 2 + rng.random_range(0..fw);
        if rng.random_bool(0.5) {
            let side = if rng.random_bool(0.5) { -1 } else { 1 };
            let target = cm.get_block(vx + side, y + 2, z);
            if target.is_air() {
                mark_block(cm, vx + side, y + 2, z, BlockId::Vine);
            }
        }
    }
    mark_area_dirty(cache, x, z, fw + 3, 3);
}

fn spawn_lava_pool(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let radius_i = 1 + rng.random_range(0..2) as i32;
    for dx in -radius_i..=radius_i {
        for dz in -radius_i..=radius_i {
            let bx = x + dx;
            let bz = z + dz;
            let dist = (dx as f64).powi(2) + (dz as f64).powi(2);
            if dist > (radius_i as f64 + 0.5).powi(2) {
                continue;
            }
            mark_block(cm, bx, y, bz, BlockId::Stone);
            let is_lava = dx.abs() <= radius_i - 1 && dz.abs() <= radius_i - 1;
            if is_lava {
                mark_block(cm, bx, y + 1, bz, BlockId::Lava);
            }
        }
    }
    mark_area_dirty(cache, x, z, radius_i + 1, radius_i + 1);
}

fn spawn_giant_mushroom(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    is_red: bool,
    rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let stem_h = 2i32 + rng.random_range(0..3) as i32;
    let cap_id = if is_red {
        BlockId::RedMushroomBlock
    } else {
        BlockId::BrownMushroomBlock
    };
    for dy in 1i32..=stem_h {
        mark_block(cm, x, y + dy, z, BlockId::MushroomStem);
    }
    let cy = y + stem_h + 1;
    for dx in -2i32..=2i32 {
        for dz in -2i32..=2i32 {
            for dy in 0i32..=2i32 {
                let bx = x + dx;
                let bz = z + dz;
                let by = cy + dy;
                let d = dx.abs().max(dz.abs());
                let in_range = match dy {
                    0i32 => d <= 2i32 && d > 0i32,
                    1i32 => d <= 2i32,
                    2i32 => d <= 1i32,
                    _ => false,
                };
                if in_range {
                    mark_block(cm, bx, by, bz, cap_id);
                }
            }
        }
    }
    mark_block(cm, x, cy + 2i32, z, cap_id);
    mark_area_dirty(cache, x, z, 3i32, 3i32);
}

fn spawn_tree(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let trunk_h: i32 = 5 + rng.random_range(0..3) as i32;
    for dy in 1..=trunk_h.saturating_sub(1) {
        mark_block(cm, x, y + dy, z, BlockId::OakLog);
    }
    let leaf_start = y + trunk_h - 2;
    for dy in 0..=2i32 {
        let radius: i32 = if dy == 0 {
            2
        } else if dy == 1 {
            2
        } else {
            1
        };
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() == radius && dz.abs() == radius && dy > 0 {
                    continue;
                }
                let bx = x + dx;
                let bz = z + dz;
                let by = leaf_start + dy;
                if cm.get_block(bx, by, bz).is_air() {
                    mark_block(cm, bx, by, bz, BlockId::OakLeaves);
                }
            }
        }
    }
    mark_area_dirty(cache, x, z, 3, 3);
}

fn spawn_igloo_command(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    _rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let gy = y.max(1);
    // Snow floor
    let floor_y = gy;
    for dy in 0..3 {
        let r = match dy {
            0 => 2i32,
            1 => 1i32,
            _ => 0i32,
        };
        for dx in -(r as i32)..=r as i32 {
            for dz in -(r as i32)..=r as i32 {
                let bx = x + dx;
                let bz = z + dz;
                let by = floor_y + dy;
                if (dx.abs() == r as i32 || dz.abs() == r as i32 || dy == 2) && r > 0 {
                    mark_block(cm, bx, by, bz, BlockId::SnowBlock);
                } else if dy == 0 {
                    mark_block(
                        cm,
                        bx,
                        by,
                        bz,
                        if dx == 0 && dz == 0 {
                            BlockId::RedCarpet
                        } else {
                            BlockId::WhiteCarpet
                        },
                    );
                } else {
                    mark_block(cm, bx, by, bz, BlockId::Air);
                }
            }
        }
    }
    mark_block(cm, x, floor_y + 1, z, BlockId::Furnace);
    if x + 1 < 1_000_000 {
        mark_block(cm, x + 1, floor_y, z, BlockId::RedWool);
    }
    mark_area_dirty(cache, x, z, 3, 3);
}

fn spawn_swamp_hut_command(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    _rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let gy = y;
    let floor_y = gy;
    // 3x3 oak plank floor
    for dx in 0..3 {
        for dz in 0..3 {
            mark_block(cm, x + dx, floor_y, z + dz, BlockId::OakPlanks);
        }
    }
    // Walls 2 high
    for dy in 1..=2 {
        for dx in 0..3 {
            for dz in 0..3 {
                let is_wall = dx == 0 || dx == 2 || dz == 0 || dz == 2;
                let is_door = dx == 1 && dz == 0 && dy == 1;
                if is_wall && !is_door {
                    mark_block(cm, x + dx, floor_y + dy, z + dz, BlockId::OakPlanks);
                }
            }
        }
    }
    // Roof
    for dx in -1i32..=3 {
        for dz in -1i32..=3 {
            let d = dx.abs().max(dz.abs());
            if d <= 2 {
                mark_block(cm, x + dx, floor_y + 3, z + dz, BlockId::OakPlanks);
            }
        }
    }
    // Mushroom inside
    mark_block(cm, x + 1, floor_y + 1, z + 1, BlockId::BrownMushroom);
    mark_area_dirty(cache, x, z, 3, 3);
}

fn spawn_desert_well_command(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    _rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let gy = y;
    for dx in 0..2 {
        for dz in 0..2 {
            mark_block(cm, x + dx, gy + 1, z + dz, BlockId::Water);
        }
    }
    for (rx, rz) in &[
        (-1i32, -1i32),
        (-1, 0),
        (-1, 1),
        (-1, 2),
        (2, -1),
        (2, 0),
        (2, 1),
        (2, 2),
        (0, -1),
        (1, -1),
        (0, 2),
        (1, 2),
    ] {
        mark_block(cm, x + rx, gy + 1, z + rz, BlockId::StoneBricks);
    }
    mark_area_dirty(cache, x, z, 3, 3);
}

fn spawn_ocean_ruin_command(
    cm: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    rng: &mut impl rand::Rng,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
) {
    let gy = y;
    for dx in -1i32..=1 {
        for dz in -1i32..=1 {
            mark_block(cm, x + dx, gy, z + dz, BlockId::StoneBricks);
            if dx.abs() == 1 && dz.abs() == 1 && rng.random_bool(0.6) {
                mark_block(cm, x + dx, gy + 1, z + dz, BlockId::StoneBricks);
            }
        }
    }
    mark_area_dirty(cache, x, z, 2, 2);
}

fn mark_neighbors_dirty(
    cm: &mut ChunkManager,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
    x: i32,
    _y: i32,
    z: i32,
) {
    let cx0 = x.div_euclid(CHUNK_SIZE as i32);
    let cz0 = z.div_euclid(CHUNK_SIZE as i32);
    let keys = HashSet::from([(cx0, cz0)]);
    for key in cm.rebuild_chunks_now(&keys) {
        cache.remove(&key);
    }
}

fn rebuild_render_data(
    data: &mut Vec<(i32, i32, ChunkRenderData)>,
    all_data: &mut Vec<(i32, i32, ChunkRenderData)>,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
    manager: &ChunkManager,
    renderer: &Renderer,
    camera: &Camera,
) {
    cache.retain(|key, _| manager.chunks.contains_key(key));

    for (&(cx, cz), chunk) in &manager.chunks {
        if chunk.has_mesh && !cache.contains_key(&(cx, cz)) {
            if let Some(mesh) = manager.get_chunk_mesh(cx, cz) {
                if mesh.vertices.is_empty() && mesh.transparent_vertices.is_empty() {
                    continue;
                }
                let rd = renderer.create_chunk_data(mesh);
                cache.insert((cx, cz), rd);
            }
        }
    }

    data.clear();
    all_data.clear();
    let vp = camera.vp_matrix();
    for (&(cx, cz), rd) in cache.iter() {
        let min_x = (cx * CHUNK_SIZE as i32) as f32;
        let min_z = (cz * CHUNK_SIZE as i32) as f32;
        let max_x = min_x + CHUNK_SIZE as i32 as f32;
        let max_z = min_z + CHUNK_SIZE as i32 as f32;
        // Note: ChunkRenderData contains wgpu::Buffer handles which are reference-counted
        // (similar to Arc), so cloning is cheap — it only increments refcounts.
        all_data.push((cx, cz, rd.clone()));
        let profile = manager.coordinate_profile();
        if camera.is_aabb_visible(
            &vp,
            min_x,
            profile.min_y() as f32,
            min_z,
            max_x,
            profile.max_y_exclusive() as f32,
            max_z,
        ) {
            data.push((cx, cz, rd.clone()));
        }
    }
}
