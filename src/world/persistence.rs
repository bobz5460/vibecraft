//! Native M2 persistence files are JSON envelopes with explicit format and data versions.
//!
//! Chunk cells deliberately contain raw block ID, registry state ordinal, and legacy data.
//! Those values are implementation-specific rather than Java-compatible; changing their
//! meaning requires a `DATA_VERSION` migration before old worlds may be loaded.

use crate::inventory::item::ItemRegistry;
use crate::inventory::{Inventory, ItemStack, TOTAL_SLOTS};
use crate::player::{EffectManager, Player, StatusEffect};
use crate::world::block::{Block, BlockId};
use crate::world::chunk::{BlockEntity, Chunk, CHEST_SLOTS, CHUNK_SIZE, CHUNK_VOLUME};
use crate::world::coordinates::WorldCoordinateProfile;
use crate::world::generation::WorldGenerationProfile;
use crate::inventory::progression::FurnaceState;
use crate::inventory::SlotContainer;
use crate::world::simulation::ScheduledTick;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Native on-disk envelope format. This is unrelated to Minecraft's save format.
pub const FORMAT_VERSION: u32 = 1;
/// Version of the native game data encoded inside each envelope.
pub const DATA_VERSION: u32 = 10;

const LEVEL_FILE: &str = "level.json";
const PLAYER_FILE: &str = "player.json";
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LevelData {
    /// User-facing save name. The directory remains a separate safe identifier.
    #[serde(default = "default_world_name")]
    pub name: String,
    /// UTC milliseconds since the Unix epoch. Zero identifies migrated legacy worlds.
    #[serde(default)]
    pub created_at: u64,
    /// UTC milliseconds since the Unix epoch. Updated after a world is entered.
    #[serde(default)]
    pub last_played: u64,
    /// Defines how public Y coordinates map onto fixed local chunk storage.
    pub coordinate_profile: WorldCoordinateProfile,
    /// Selects immutable generator behavior independently from coordinates.
    pub generation_profile: WorldGenerationProfile,
    pub seed: u64,
    pub tick: u64,
    pub game_time: u64,
    pub spawn: [i32; 3],
    /// One of `survival`, `creative`, `adventure`, or `spectator`.
    pub gamemode: String,
    /// One of `peaceful`, `easy`, `normal`, or `hard`.
    pub difficulty: String,
    pub hardcore: bool,
    pub do_daylight_cycle: bool,
    #[serde(default)]
    pub keep_inventory: bool,
    #[serde(default)]
    pub experience: u32,
    #[serde(default)]
    pub scheduled_ticks: Vec<ScheduledTick>,
    #[serde(default)]
    pub dropped_items: Vec<DroppedItemData>,
    #[serde(default)]
    pub xp_orbs: Vec<XpOrbData>,
    #[serde(default)]
    pub players: Vec<NamedPlayerData>,
}

fn default_world_name() -> String {
    "New World".to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldSummary {
    pub path: PathBuf,
    pub name: String,
    pub gamemode: String,
    pub hardcore: bool,
    pub last_played: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldDiscovery {
    pub worlds: Vec<WorldSummary>,
    pub rejected: Vec<(PathBuf, String)>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NamedPlayerData {
    pub username: String,
    pub player: PlayerData,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DroppedItemData {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub block_id: u16,
    #[serde(default)]
    pub item_id: u16,
    #[serde(default = "one")]
    pub count: u16,
    #[serde(default)]
    pub damage: u16,
    pub lifetime: f32,
}

const fn one() -> u16 { 1 }

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct XpOrbData {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub value: u32,
    pub lifetime: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerData {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vy: f32,
    pub on_ground: bool,
    pub health: f32,
    pub last_vy: f32,
    pub sneaking: bool,
    pub swimming: bool,
    pub oxygen: f32,
    pub hunger: f32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub attack_cooldown: f32,
    pub armor_points: f32,
    pub armor_toughness: f32,
    pub absorption_health: f32,
    pub damage_cooldown: f32,
    pub fall_flying: bool,
    pub effects: Vec<StatusEffectData>,
    pub inventory: InventoryData,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StatusEffectData {
    /// Canonical, stable native status-effect name, such as `fire_resistance`.
    pub effect: String,
    pub duration: f32,
    pub amplifier: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct InventoryData {
    pub slots: Vec<ItemStackData>,
    pub held_slot: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ItemStackData {
    pub id: u16,
    pub count: u16,
    #[serde(default)]
    pub damage: u16,
}

impl PlayerData {
    pub fn from_runtime(player: &Player, inventory: &Inventory) -> Self {
        Self {
            x: player.x,
            y: player.y,
            z: player.z,
            vy: player.vy,
            on_ground: player.on_ground,
            health: player.health,
            last_vy: player.last_vy,
            sneaking: player.sneaking,
            swimming: player.swimming,
            oxygen: player.oxygen,
            hunger: player.hunger,
            saturation: player.saturation,
            exhaustion: player.exhaustion,
            attack_cooldown: player.attack_cooldown,
            armor_points: player.armor_points,
            armor_toughness: player.armor_toughness,
            absorption_health: player.absorption_health,
            damage_cooldown: player.damage_cooldown,
            fall_flying: player.fall_flying,
            effects: player
                .effects
                .effects
                .iter()
                .map(|effect| StatusEffectData {
                    effect: status_effect_name(effect.effect).to_string(),
                    duration: effect.duration,
                    amplifier: effect.amplifier,
                })
                .collect(),
            inventory: InventoryData::from_runtime(inventory),
        }
    }

    pub fn into_runtime(self) -> Result<(Player, Inventory), StorageError> {
        self.validate(Path::new("<player data>"))?;
        let mut effects = EffectManager::new();
        for effect in &self.effects {
            effects.apply(
                status_effect_from_name(&effect.effect).ok_or_else(|| StorageError::Corrupt {
                    path: PathBuf::from("<player data>"),
                    message: format!("unknown status effect `{}`", effect.effect),
                })?,
                effect.duration,
                effect.amplifier,
            );
        }

        Ok((
            Player {
                x: self.x,
                y: self.y,
                z: self.z,
                vy: self.vy,
                on_ground: self.on_ground,
                health: self.health,
                prev_health: self.health,
                last_vy: self.last_vy,
                sneaking: self.sneaking,
                swimming: self.swimming,
                oxygen: self.oxygen,
                hunger: self.hunger,
                prev_hunger: self.hunger,
                saturation: self.saturation,
                exhaustion: self.exhaustion,
                health_blink_timer: 0.0,
                hunger_shake_timer: 0.0,
                attack_cooldown: self.attack_cooldown,
                armor_points: self.armor_points,
                armor_toughness: self.armor_toughness,
                absorption_health: self.absorption_health,
                damage_cooldown: self.damage_cooldown,
                fall_flying: self.fall_flying,
                effects,
            },
            self.inventory.into_runtime(),
        ))
    }

    fn validate(&self, path: &Path) -> Result<(), StorageError> {
        let floats = [
            self.x,
            self.y,
            self.z,
            self.vy,
            self.health,
            self.last_vy,
            self.oxygen,
            self.hunger,
            self.saturation,
            self.exhaustion,
            self.attack_cooldown,
            self.armor_points,
            self.armor_toughness,
            self.absorption_health,
            self.damage_cooldown,
        ];
        if floats.iter().any(|value| !value.is_finite()) {
            return Err(corrupt(path, "player contains a non-finite value"));
        }
        for effect in &self.effects {
            if !effect.duration.is_finite() || effect.duration < 0.0 {
                return Err(corrupt(path, "status effect has an invalid duration"));
            }
            if status_effect_from_name(&effect.effect).is_none() {
                return Err(corrupt(
                    path,
                    format!("unknown status effect `{}`", effect.effect),
                ));
            }
        }
        self.inventory.validate(path)
    }
}

impl InventoryData {
    pub fn from_runtime(inventory: &Inventory) -> Self {
        Self {
            slots: inventory
                .slots
                .iter()
                .map(|stack| ItemStackData {
                    id: stack.id,
                    count: stack.count,
                    damage: stack.damage,
                })
                .collect(),
            held_slot: inventory.held_slot,
        }
    }

    pub fn into_runtime(self) -> Inventory {
        Inventory {
            slots: self
                .slots
                .into_iter()
                .map(|stack| ItemStack::with_damage(stack.id, stack.count, stack.damage))
                .collect(),
            held_slot: self.held_slot,
        }
    }

    fn validate(&self, path: &Path) -> Result<(), StorageError> {
        if self.slots.len() != TOTAL_SLOTS {
            return Err(corrupt(
                path,
                format!("inventory has {} slots; expected {TOTAL_SLOTS}", self.slots.len()),
            ));
        }
        if self.held_slot >= 9 {
            return Err(corrupt(path, "inventory selected slot is outside the hotbar"));
        }
        let items = ItemRegistry::new();
        for (index, stack) in self.slots.iter().enumerate() {
            if stack.count == 0 {
                if stack.id != 0 || stack.damage != 0 {
                    return Err(corrupt(path, format!("inventory slot {index} has a non-canonical empty stack")));
                }
                continue;
            }
            if !items.is_valid(stack.id) || stack.count > items.def(stack.id).max_stack as u16 {
                return Err(corrupt(path, format!("inventory slot {index} has an invalid stack")));
            }
            let max_damage = items.def(stack.id).max_damage;
            if (max_damage == 0 && stack.damage != 0) || (max_damage > 0 && stack.damage >= max_damage) {
                return Err(corrupt(path, format!("inventory slot {index} has invalid durability")));
            }
        }
        Ok(())
    }
}

/// A compact JSON tuple: `[raw_block_id, registry_state, legacy_data]`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BlockCell(pub u16, pub u16, pub u8);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ChunkData {
    pub cx: i32,
    pub cz: i32,
    pub cells: Vec<BlockCell>,
    pub block_entities: Vec<BlockEntityData>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockEntityData {
    Chest {
        x: u8,
        y: u16,
        z: u8,
        slots: Vec<ItemStackData>,
    },
    Furnace {
        x: u8,
        y: u16,
        z: u8,
        slots: Vec<ItemStackData>,
        burn_time: u16,
        burn_total: u16,
        cook_time: u16,
    },
}

impl ChunkData {
    pub fn from_chunk(chunk: &Chunk) -> Self {
        Self {
            cx: chunk.cx,
            cz: chunk.cz,
            cells: chunk
                .blocks
                .iter()
                .map(|block| BlockCell(block.id as u16, block.state, block.data))
                .collect(),
            block_entities: {
                let mut entities: Vec<_> = chunk
                    .block_entities
                    .iter()
                    .map(|(&index, entity)| BlockEntityData::from_runtime(index, entity))
                    .collect();
                entities.sort_by_key(BlockEntityData::position);
                entities
            },
        }
    }

    /// Rebuilds fluid bookkeeping and intentionally discards saved lighting and mesh state.
    pub fn into_chunk(self) -> Result<Chunk, StorageError> {
        self.validate(Path::new("<chunk data>"))?;
        let mut chunk = Chunk::new(self.cx, self.cz);
        for (target, BlockCell(id, state, data)) in chunk.blocks.iter_mut().zip(self.cells) {
            *target = Block {
                id: BlockId::from_repr(id).ok_or_else(|| StorageError::Corrupt {
                    path: PathBuf::from("<chunk data>"),
                    message: format!("unknown block ID {id}"),
                })?,
                state,
                data,
            };
        }
        for entity in self.block_entities {
            let (index, entity) = entity.into_runtime();
            chunk.block_entities.insert(index, entity);
        }
        chunk.reconcile_block_entities();
        chunk.recount_fluids();
        chunk.is_dirty = true;
        chunk.light_dirty = true;
        chunk.has_mesh = false;
        Ok(chunk)
    }

    fn validate(&self, path: &Path) -> Result<(), StorageError> {
        if self.cells.len() != CHUNK_VOLUME {
            return Err(corrupt(
                path,
                format!("chunk has {} cells; expected {CHUNK_VOLUME}", self.cells.len()),
            ));
        }
        for (index, BlockCell(id, _, _)) in self.cells.iter().copied().enumerate() {
            if BlockId::from_repr(id).is_none() {
                return Err(corrupt(path, format!("cell {index} has unknown block ID {id}")));
            }
        }
        let mut positions = std::collections::HashSet::new();
        for entity in &self.block_entities {
            let (x, y, z, expected_block, slots, label) = match entity {
                BlockEntityData::Chest { x, y, z, slots } => {
                    (*x, *y, *z, BlockId::Chest, slots.as_slice(), "chest")
                }
                BlockEntityData::Furnace { x, y, z, slots, .. } => {
                    (*x, *y, *z, BlockId::Furnace, slots.as_slice(), "furnace")
                }
            };
            if x as usize >= CHUNK_SIZE || y as usize >= crate::world::chunk::CHUNK_HEIGHT || z as usize >= CHUNK_SIZE {
                return Err(corrupt(path, format!("{label} block entity is outside chunk bounds")));
            }
            let index = Chunk::index(x as usize, y as usize, z as usize);
            if !positions.insert(index) {
                return Err(corrupt(path, "chunk contains duplicate block-entity positions"));
            }
            if self.cells[index].0 != expected_block as u16 {
                return Err(corrupt(path, format!("{label} block entity does not match its block cell")));
            }
            let slot_count = if expected_block == BlockId::Chest { CHEST_SLOTS } else { 3 };
            validate_item_stacks(slots, slot_count, label, path)?;
        }
        Ok(())
    }
}

impl BlockEntityData {
    fn position(&self) -> (u8, u16, u8) {
        match self {
            Self::Chest { x, y, z, .. } | Self::Furnace { x, y, z, .. } => (*x, *y, *z),
        }
    }

    fn from_runtime(index: usize, entity: &BlockEntity) -> Self {
        let x = (index % CHUNK_SIZE) as u8;
        let y = (index / (CHUNK_SIZE * CHUNK_SIZE)) as u16;
        let z = ((index / CHUNK_SIZE) % CHUNK_SIZE) as u8;
        match entity {
            BlockEntity::Chest { slots } => Self::Chest {
                x,
                y,
                z,
                slots: item_stacks_from_runtime(&slots.slots),
            },
            BlockEntity::Furnace { state } => Self::Furnace {
                x,
                y,
                z,
                slots: item_stacks_from_runtime(&state.slots.slots),
                burn_time: state.burn_time,
                burn_total: state.burn_total,
                cook_time: state.cook_time,
            },
        }
    }

    fn into_runtime(self) -> (usize, BlockEntity) {
        match self {
            Self::Chest { x, y, z, slots } => (
                Chunk::index(x as usize, y as usize, z as usize),
                BlockEntity::Chest {
                    slots: SlotContainer {
                        slots: item_stacks_into_runtime(slots),
                    },
                },
            ),
            Self::Furnace {
                x,
                y,
                z,
                slots,
                burn_time,
                burn_total,
                cook_time,
            } => (
                Chunk::index(x as usize, y as usize, z as usize),
                BlockEntity::Furnace {
                    state: FurnaceState {
                        slots: SlotContainer {
                            slots: item_stacks_into_runtime(slots),
                        },
                        burn_time,
                        burn_total,
                        cook_time,
                    },
                },
            ),
        }
    }
}

fn item_stacks_from_runtime(stacks: &[ItemStack]) -> Vec<ItemStackData> {
    stacks
        .iter()
        .map(|stack| ItemStackData {
            id: stack.id,
            count: stack.count,
            damage: stack.damage,
        })
        .collect()
}

fn item_stacks_into_runtime(stacks: Vec<ItemStackData>) -> Vec<ItemStack> {
    stacks
        .into_iter()
        .map(|stack| ItemStack::with_damage(stack.id, stack.count, stack.damage))
        .collect()
}

fn validate_item_stacks(
    stacks: &[ItemStackData],
    expected_slots: usize,
    label: &str,
    path: &Path,
) -> Result<(), StorageError> {
    if stacks.len() != expected_slots {
        return Err(corrupt(
            path,
            format!("{label} has {} slots; expected {expected_slots}", stacks.len()),
        ));
    }
    let items = ItemRegistry::new();
    for (index, stack) in stacks.iter().enumerate() {
        if stack.count == 0 {
            if stack.id != 0 || stack.damage != 0 {
                return Err(corrupt(path, format!("{label} slot {index} has a non-canonical empty stack")));
            }
            continue;
        }
        if !items.is_valid(stack.id) || stack.count > items.def(stack.id).max_stack as u16 {
            return Err(corrupt(path, format!("{label} slot {index} has an invalid stack")));
        }
        let max_damage = items.def(stack.id).max_damage;
        if (max_damage == 0 && stack.damage != 0) || (max_damage > 0 && stack.damage >= max_damage) {
            return Err(corrupt(path, format!("{label} slot {index} has invalid durability")));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum StorageError {
    Io { path: PathBuf, source: std::io::Error },
    Json { path: PathBuf, source: serde_json::Error },
    Version {
        path: PathBuf,
        format_version: u32,
        data_version: u32,
    },
    Corrupt { path: PathBuf, message: String },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "I/O error at {}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "invalid JSON at {}: {source}", path.display()),
            Self::Version {
                path,
                format_version,
                data_version,
            } => write!(
                f,
                "unsupported save versions at {}: format {format_version}, data {data_version}",
                path.display()
            ),
            Self::Corrupt { path, message } => {
                write!(f, "corrupt save data at {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for StorageError {}

#[derive(Clone, Debug)]
pub struct WorldStorage {
    root: PathBuf,
}

impl WorldStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn level_path(&self) -> PathBuf {
        self.root.join(LEVEL_FILE)
    }

    pub fn player_path(&self) -> PathBuf {
        self.root.join(PLAYER_FILE)
    }

    pub fn chunk_path(&self, cx: i32, cz: i32) -> PathBuf {
        self.root.join("chunks").join(format!("c.{cx}.{cz}.json"))
    }

    /// Loads an existing level or atomically writes `new_level` only when no level file exists.
    /// A corrupt existing level is always returned as an error and is never replaced.
    pub fn load_or_create_level(&self, new_level: LevelData) -> Result<LevelData, StorageError> {
        let path = self.level_path();
        if path.exists() {
            return self.load_level();
        }
        self.save_level(&new_level)?;
        Ok(new_level)
    }

    pub fn load_level(&self) -> Result<LevelData, StorageError> {
        self.read(self.level_path(), FileKind::Level)
    }

    pub fn save_level(&self, data: &LevelData) -> Result<(), StorageError> {
        self.write(&self.level_path(), FileKind::Level, data)
    }

    pub fn load_player(&self) -> Result<PlayerData, StorageError> {
        self.read(self.player_path(), FileKind::Player)
    }

    pub fn save_player(&self, data: &PlayerData) -> Result<(), StorageError> {
        self.write(&self.player_path(), FileKind::Player, data)
    }

    pub fn load_chunk(&self, cx: i32, cz: i32) -> Result<ChunkData, StorageError> {
        let path = self.chunk_path(cx, cz);
        let chunk: ChunkData = self.read(path.clone(), FileKind::Chunk)?;
        if (chunk.cx, chunk.cz) != (cx, cz) {
            return Err(corrupt(
                &path,
                format!(
                    "chunk coordinates ({}, {}) do not match requested ({cx}, {cz})",
                    chunk.cx, chunk.cz
                ),
            ));
        }
        Ok(chunk)
    }

    /// Returns `Ok(None)` only when this chunk has not been saved yet.
    /// Invalid or unreadable existing files remain errors so callers never
    /// replace player data with generated terrain.
    pub fn load_chunk_if_present(&self, cx: i32, cz: i32) -> Result<Option<ChunkData>, StorageError> {
        match self.load_chunk(cx, cz) {
            Ok(chunk) => Ok(Some(chunk)),
            Err(StorageError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn save_chunk(&self, data: &ChunkData) -> Result<(), StorageError> {
        self.write(&self.chunk_path(data.cx, data.cz), FileKind::Chunk, data)
    }

    fn read<T: DeserializeOwned + Validate>(
        &self,
        path: PathBuf,
        expected_kind: FileKind,
    ) -> Result<T, StorageError> {
        let bytes = fs::read(&path).map_err(|source| StorageError::Io {
            path: path.clone(),
            source,
        })?;
        let file: SaveFile<Value> = serde_json::from_slice(&bytes).map_err(|source| StorageError::Json {
            path: path.clone(),
            source,
        })?;
        if file.format_version != FORMAT_VERSION || file.data_version > DATA_VERSION {
            return Err(StorageError::Version {
                path,
                format_version: file.format_version,
                data_version: file.data_version,
            });
        }
        if file.kind != expected_kind {
            return Err(corrupt(&path, "save file kind does not match its path"));
        }
        let data = migrate_data(file.data, file.data_version, expected_kind, &path)?;
        let data: T = serde_json::from_value(data).map_err(|source| StorageError::Json {
            path: path.clone(),
            source,
        })?;
        data.validate(&path)?;
        Ok(data)
    }

    fn write<T: Serialize + Validate>(
        &self,
        path: &Path,
        kind: FileKind,
        data: &T,
    ) -> Result<(), StorageError> {
        data.validate(path)?;
        let file = SaveFile {
            format_version: FORMAT_VERSION,
            data_version: DATA_VERSION,
            kind,
            data,
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|source| StorageError::Json {
            path: path.to_path_buf(),
            source,
        })?;
        atomic_write(path, &bytes)
    }
}

/// Lists the configured legacy root when it is a valid world, followed by valid
/// immediate child worlds. Discovery never creates or repairs save data.
pub fn discover_worlds(root: &Path) -> Result<WorldDiscovery, StorageError> {
    let mut discovery = WorldDiscovery::default();
    let root_storage = WorldStorage::new(root);
    if root_storage.level_path().is_file() {
        match root_storage.load_level() {
            Ok(level) => discovery.worlds.push(world_summary(root.to_path_buf(), level)),
            Err(error) => discovery.rejected.push((root.to_path_buf(), error.to_string())),
        }
    }

    let entries = fs::read_dir(root).map_err(|source| StorageError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| StorageError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| StorageError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let storage = WorldStorage::new(&path);
        if !storage.level_path().is_file() {
            continue;
        }
        match storage.load_level() {
            Ok(level) => discovery.worlds.push(world_summary(path, level)),
            Err(error) => discovery.rejected.push((path, error.to_string())),
        }
    }
    discovery.worlds.sort_by(|a, b| b.last_played.cmp(&a.last_played).then_with(|| a.name.cmp(&b.name)));
    Ok(discovery)
}

fn world_summary(path: PathBuf, level: LevelData) -> WorldSummary {
    WorldSummary {
        path,
        name: level.name,
        gamemode: level.gamemode,
        hardcore: level.hardcore,
        last_played: level.last_played,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum FileKind {
    Level,
    Player,
    Chunk,
}

#[derive(Deserialize, Serialize)]
struct SaveFile<T> {
    format_version: u32,
    data_version: u32,
    kind: FileKind,
    data: T,
}

trait Validate {
    fn validate(&self, path: &Path) -> Result<(), StorageError>;
}

impl Validate for LevelData {
    fn validate(&self, path: &Path) -> Result<(), StorageError> {
        if !matches!(
            self.gamemode.as_str(),
            "survival" | "creative" | "adventure" | "spectator"
        ) {
            return Err(corrupt(path, format!("unknown gamemode `{}`", self.gamemode)));
        }
        if !matches!(
            self.difficulty.as_str(),
            "peaceful" | "easy" | "normal" | "hard"
        ) {
            return Err(corrupt(
                path,
                format!("unknown difficulty `{}`", self.difficulty),
            ));
        }
        if self.scheduled_ticks.len() > 65_536 {
            return Err(corrupt(path, "level contains too many scheduled ticks"));
        }
        if !self.coordinate_profile.contains_world_y(self.spawn[1]) {
            return Err(corrupt(path, "world spawn is outside the world's build height"));
        }
        let min_live_y = self.coordinate_profile.min_y() as f32;
        let max_live_y = self.coordinate_profile.max_y_exclusive() as f32 + 16.0;
        for item in &self.dropped_items {
            let values = [item.x, item.y, item.z, item.vx, item.vy, item.vz, item.lifetime];
            if values.iter().any(|value| !value.is_finite()) || item.lifetime < 0.0 {
                return Err(corrupt(path, "dropped item contains an invalid value"));
            }
            if item.y <= min_live_y || item.y >= max_live_y {
                return Err(corrupt(path, "dropped item is outside this world's live height"));
            }
            if BlockId::from_repr(item.block_id).is_none() {
                return Err(corrupt(
                    path,
                    format!("dropped item has unknown block ID {}", item.block_id),
                ));
            }
            let items = ItemRegistry::new();
            let item_id = if item.item_id == 0 { items.item_id_from_block(BlockId::from_repr(item.block_id).unwrap()) } else { item.item_id };
            if item.count == 0 || !items.is_valid(item_id) || item.count > items.def(item_id).max_stack as u16 {
                return Err(corrupt(path, "dropped item has an invalid item stack"));
            }
        }
        for orb in &self.xp_orbs {
            let values = [orb.x, orb.y, orb.z, orb.vx, orb.vy, orb.vz, orb.lifetime];
            if values.iter().any(|value| !value.is_finite()) || orb.lifetime < 0.0 {
                return Err(corrupt(path, "XP orb contains an invalid value"));
            }
            if orb.y <= min_live_y || orb.y >= max_live_y {
                return Err(corrupt(path, "XP orb is outside this world's live height"));
            }
        }
        let mut usernames = std::collections::HashSet::new();
        for named in &self.players {
            if named.username.is_empty()
                || named.username.len() > 16
                || named.username.chars().any(char::is_control)
                || !usernames.insert(named.username.clone())
            {
                return Err(corrupt(path, "level contains an invalid or duplicate player name"));
            }
            named.player.validate(path)?;
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 64 || self.name.chars().any(char::is_control) {
            return Err(corrupt(path, "level has an invalid display name"));
        }
        Ok(())
    }
}

impl Validate for PlayerData {
    fn validate(&self, path: &Path) -> Result<(), StorageError> {
        Self::validate(self, path)
    }
}

impl Validate for ChunkData {
    fn validate(&self, path: &Path) -> Result<(), StorageError> {
        Self::validate(self, path)
    }
}

/// Migrates only explicitly known native layouts. Unknown future versions are
/// rejected before this function so an old binary cannot overwrite newer data.
fn migrate_data(
    mut data: Value,
    mut version: u32,
    kind: FileKind,
    path: &Path,
) -> Result<Value, StorageError> {
    while version < DATA_VERSION {
        match (version, kind) {
            (1, FileKind::Level) => {
                let object = data
                    .as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?;
                object.entry("experience".to_string()).or_insert(Value::from(0));
                object
                    .entry("scheduled_ticks".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                object
                    .entry("dropped_items".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                object
                    .entry("xp_orbs".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
            }
            (1, FileKind::Player | FileKind::Chunk) => {}
            (2, FileKind::Player) => {
                let object = data
                    .as_object_mut()
                    .ok_or_else(|| corrupt(path, "player data must be a JSON object"))?;
                let inventory = object
                    .get_mut("inventory")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| corrupt(path, "player inventory must be an object"))?;
                let slots = inventory
                    .get_mut("slots")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| corrupt(path, "player inventory slots must be an array"))?;
                for slot in slots {
                    slot.as_object_mut()
                        .ok_or_else(|| corrupt(path, "player inventory stack must be an object"))?
                        .entry("damage".to_string())
                        .or_insert(Value::from(0));
                }
            }
            (2, FileKind::Level) => {
                let object = data
                    .as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?;
                let drops = object
                    .get_mut("dropped_items")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| corrupt(path, "level dropped items must be an array"))?;
                let items = ItemRegistry::new();
                for drop in drops {
                    let drop = drop.as_object_mut().ok_or_else(|| corrupt(path, "dropped item must be an object"))?;
                    let block_id = drop.get("block_id").and_then(Value::as_u64)
                        .and_then(|id| BlockId::from_repr(id as u16))
                        .ok_or_else(|| corrupt(path, "dropped item has an invalid block ID"))?;
                    drop.entry("item_id".to_string()).or_insert(Value::from(items.item_id_from_block(block_id)));
                    drop.entry("count".to_string()).or_insert(Value::from(1));
                    drop.entry("damage".to_string()).or_insert(Value::from(0));
                }
            }
            (2, FileKind::Chunk) => {}
            (3, FileKind::Chunk) => migrate_v3_chunk_block_entities(&mut data, path)?,
            (3, FileKind::Level | FileKind::Player) => {}
            (4, FileKind::Level) => {
                data.as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?
                    .entry("keep_inventory".to_string())
                    .or_insert(Value::Bool(false));
            }
            (4, FileKind::Player | FileKind::Chunk) => {}
            (5, FileKind::Level) => {
                let object = data
                    .as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?;
                let legacy_name = path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("New World");
                object
                    .entry("name".to_string())
                    .or_insert_with(|| Value::String(legacy_name.to_string()));
                object.entry("created_at".to_string()).or_insert(Value::from(0));
                object.entry("last_played".to_string()).or_insert(Value::from(0));
            }
            (5, FileKind::Player | FileKind::Chunk) => {}
            (6, FileKind::Level) => {
                data.as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?
                    .insert(
                        "coordinate_profile".to_string(),
                        Value::String("legacy_local".to_string()),
                    );
            }
            (6, FileKind::Player | FileKind::Chunk) => {}
            (7, FileKind::Level) => {
                data.as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?
                    .insert(
                        "generation_profile".to_string(),
                        Value::String("legacy_pre_corrected_interpolation".to_string()),
                    );
            }
            (7, FileKind::Player | FileKind::Chunk) => {}
            // Version 9 introduces a new-world-only decoration profile. Version
            // 8 could only persist the legacy or undecorated base profiles.
            (8, FileKind::Level) => {
                let object = data
                    .as_object_mut()
                    .ok_or_else(|| corrupt(path, "level data must be a JSON object"))?;
                if !matches!(
                    object.get("generation_profile").and_then(Value::as_str),
                    Some("legacy_pre_corrected_interpolation" | "minecraft26_base")
                ) {
                    object.insert(
                        "generation_profile".to_string(),
                        Value::String("minecraft26_base".to_string()),
                    );
                }
            }
            (8, FileKind::Player | FileKind::Chunk) => {}
            // Version 10 adds a new explicit generation-profile enum value.
            // Existing version-9 worlds retain their persisted profile exactly.
            (9, FileKind::Level | FileKind::Player | FileKind::Chunk) => {}
            _ => {
                return Err(StorageError::Version {
                    path: path.to_path_buf(),
                    format_version: FORMAT_VERSION,
                    data_version: version,
                });
            }
        }
        version += 1;
    }
    Ok(data)
}

fn migrate_v3_chunk_block_entities(data: &mut Value, path: &Path) -> Result<(), StorageError> {
    let object = data
        .as_object_mut()
        .ok_or_else(|| corrupt(path, "chunk data must be a JSON object"))?;
    if object.contains_key("block_entities") {
        return Ok(());
    }
    let cells = object
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| corrupt(path, "chunk cells must be an array"))?;
    let mut block_entities = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let block_id = cell
            .as_array()
            .and_then(|values| values.first())
            .and_then(Value::as_u64)
            .and_then(|id| u16::try_from(id).ok())
            .ok_or_else(|| corrupt(path, format!("chunk cell {index} is invalid")))?;
        let (kind, slot_count) = match BlockId::from_repr(block_id) {
            Some(BlockId::Chest) => ("chest", CHEST_SLOTS),
            Some(BlockId::Furnace) => ("furnace", 3),
            _ => continue,
        };
        let x = index % CHUNK_SIZE;
        let y = index / (CHUNK_SIZE * CHUNK_SIZE);
        let z = (index / CHUNK_SIZE) % CHUNK_SIZE;
        let slots: Vec<_> = (0..slot_count)
            .map(|_| serde_json::json!({ "id": 0, "count": 0, "damage": 0 }))
            .collect();
        block_entities.push(if kind == "furnace" {
            serde_json::json!({
                "type": kind,
                "x": x,
                "y": y,
                "z": z,
                "slots": slots,
                "burn_time": 0,
                "burn_total": 0,
                "cook_time": 0,
            })
        } else {
            serde_json::json!({
                "type": kind,
                "x": x,
                "y": y,
                "z": z,
                "slots": slots,
            })
        });
    }
    object.insert("block_entities".to_string(), Value::Array(block_entities));
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let parent = path.parent().ok_or_else(|| corrupt(path, "save path has no parent directory"))?;
    fs::create_dir_all(parent).map_err(|source| StorageError::Io {
        path: parent.to_path_buf(),
        source,
    })?;

    let file_name = path.file_name().ok_or_else(|| corrupt(path, "save path has no file name"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut temp_path = None;
    let mut temp_file = None;
    for _ in 0..16 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            stamp + counter as u128
        ));
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(StorageError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    }
    let (temp_path, mut file) = match (temp_path, temp_file) {
        (Some(temp_path), Some(file)) => (temp_path, file),
        _ => return Err(corrupt(path, "could not allocate a temporary save file")),
    };
    let write_result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| StorageError::Io {
            path: temp_path.clone(),
            source,
        });
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(source) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(StorageError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| StorageError::Io {
                path: path.to_path_buf(),
                source,
            })?;
    }
    Ok(())
}

fn corrupt(path: &Path, message: impl Into<String>) -> StorageError {
    StorageError::Corrupt {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn status_effect_name(effect: StatusEffect) -> &'static str {
    match effect {
        StatusEffect::Speed => "speed",
        StatusEffect::Slowness => "slowness",
        StatusEffect::Haste => "haste",
        StatusEffect::MiningFatigue => "mining_fatigue",
        StatusEffect::Strength => "strength",
        StatusEffect::JumpBoost => "jump_boost",
        StatusEffect::Regeneration => "regeneration",
        StatusEffect::Resistance => "resistance",
        StatusEffect::FireResistance => "fire_resistance",
        StatusEffect::WaterBreathing => "water_breathing",
        StatusEffect::NightVision => "night_vision",
        StatusEffect::Invisibility => "invisibility",
        StatusEffect::Absorption => "absorption",
        StatusEffect::SlowFalling => "slow_falling",
        StatusEffect::DolphinGrace => "dolphins_grace",
        StatusEffect::Weakness => "weakness",
        StatusEffect::Poison => "poison",
        StatusEffect::Wither => "wither",
        StatusEffect::Hunger => "hunger",
        StatusEffect::Nausea => "nausea",
        StatusEffect::Blindness => "blindness",
        StatusEffect::Levitation => "levitation",
        StatusEffect::Darkness => "darkness",
        StatusEffect::InstantHealth => "instant_health",
        StatusEffect::InstantDamage => "instant_damage",
        StatusEffect::HealthBoost => "health_boost",
        StatusEffect::SaturationEffect => "saturation",
        StatusEffect::FatalPoison => "fatal_poison",
        StatusEffect::BadOmen => "bad_omen",
        StatusEffect::HeroOfTheVillage => "hero_of_the_village",
        StatusEffect::WindCharged => "wind_charged",
        StatusEffect::Infested => "infested",
        StatusEffect::Oozing => "oozing",
        StatusEffect::Weaving => "weaving",
    }
}

fn status_effect_from_name(name: &str) -> Option<StatusEffect> {
    Some(match name {
        "speed" => StatusEffect::Speed,
        "slowness" => StatusEffect::Slowness,
        "haste" => StatusEffect::Haste,
        "mining_fatigue" => StatusEffect::MiningFatigue,
        "strength" => StatusEffect::Strength,
        "jump_boost" => StatusEffect::JumpBoost,
        "regeneration" => StatusEffect::Regeneration,
        "resistance" => StatusEffect::Resistance,
        "fire_resistance" => StatusEffect::FireResistance,
        "water_breathing" => StatusEffect::WaterBreathing,
        "night_vision" => StatusEffect::NightVision,
        "invisibility" => StatusEffect::Invisibility,
        "absorption" => StatusEffect::Absorption,
        "slow_falling" => StatusEffect::SlowFalling,
        "dolphins_grace" => StatusEffect::DolphinGrace,
        "weakness" => StatusEffect::Weakness,
        "poison" => StatusEffect::Poison,
        "wither" => StatusEffect::Wither,
        "hunger" => StatusEffect::Hunger,
        "nausea" => StatusEffect::Nausea,
        "blindness" => StatusEffect::Blindness,
        "levitation" => StatusEffect::Levitation,
        "darkness" => StatusEffect::Darkness,
        "instant_health" => StatusEffect::InstantHealth,
        "instant_damage" => StatusEffect::InstantDamage,
        "health_boost" => StatusEffect::HealthBoost,
        "saturation" => StatusEffect::SaturationEffect,
        "fatal_poison" => StatusEffect::FatalPoison,
        "bad_omen" => StatusEffect::BadOmen,
        "hero_of_the_village" => StatusEffect::HeroOfTheVillage,
        "wind_charged" => StatusEffect::WindCharged,
        "infested" => StatusEffect::Infested,
        "oozing" => StatusEffect::Oozing,
        "weaving" => StatusEffect::Weaving,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempWorld {
        path: PathBuf,
    }

    impl TempWorld {
        fn new() -> Self {
            let id = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "vibecraft-persistence-{}-{}",
                std::process::id(),
                id
            ));
            fs::create_dir(&path).expect("temporary world directory should be created");
            Self { path }
        }
    }

    impl Drop for TempWorld {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn level(seed: u64) -> LevelData {
        LevelData {
            name: "Fixture World".to_string(),
            created_at: 100,
            last_played: 200,
            coordinate_profile: WorldCoordinateProfile::LegacyLocal,
            generation_profile: WorldGenerationProfile::LegacyPreCorrectedInterpolation,
            seed,
            tick: 42,
            game_time: 84,
            spawn: [1, 72, -3],
            gamemode: "survival".to_string(),
            difficulty: "normal".to_string(),
            hardcore: false,
            do_daylight_cycle: true,
            keep_inventory: false,
            experience: 17,
            scheduled_ticks: vec![ScheduledTick {
                due_tick: 50,
                chunk: [-2, 7],
                kind: crate::world::simulation::ScheduledTickKind::Water,
            }],
            dropped_items: vec![DroppedItemData {
                x: 1.0,
                y: 72.0,
                z: -3.0,
                vx: 0.1,
                vy: 0.2,
                vz: 0.3,
                block_id: BlockId::Stone as u16,
                item_id: BlockId::Stone as u16,
                count: 1,
                damage: 0,
                lifetime: 299.0,
            }],
            xp_orbs: vec![XpOrbData {
                x: 2.0,
                y: 73.0,
                z: -4.0,
                vx: 0.4,
                vy: 0.5,
                vz: 0.6,
                value: 3,
                lifetime: 59.0,
            }],
            players: Vec::new(),
        }
    }

    #[test]
    fn level_and_chunk_round_trip() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let expected_level = level(1234);
        storage.save_level(&expected_level).unwrap();
        assert_eq!(storage.load_level().unwrap(), expected_level);

        let mut chunk = Chunk::new(-2, 7);
        chunk.set_block(1, 10, 2, Block::new(BlockId::Water));
        chunk.set_block(3, 11, 4, Block::new(BlockId::Lava));
        chunk.set_block(5, 12, 6, Block {
            id: BlockId::StoneStairs,
            state: 3,
            data: 6,
        });
        chunk.set_block(7, 13, 8, Block::new(BlockId::Chest));
        let mut chest = chunk.get_block_entity(7, 13, 8).unwrap().clone();
        let BlockEntity::Chest { slots } = &mut chest else { unreachable!() };
        slots.slots[0] = ItemStack::new(1, 12);
        assert!(chunk.set_block_entity(7, 13, 8, chest));
        chunk.set_block(9, 13, 8, Block::new(BlockId::Furnace));
        let mut furnace = chunk.get_block_entity(9, 13, 8).unwrap().clone();
        let BlockEntity::Furnace { state } = &mut furnace else { unreachable!() };
        state.slots.slots[0] = ItemStack::new(1, 3);
        state.burn_time = 17;
        state.burn_total = 100;
        state.cook_time = 4;
        assert!(chunk.set_block_entity(9, 13, 8, furnace));
        let expected_chunk = ChunkData::from_chunk(&chunk);
        storage.save_chunk(&expected_chunk).unwrap();
        let restored = storage.load_chunk(-2, 7).unwrap().into_chunk().unwrap();

        assert_eq!(ChunkData::from_chunk(&restored), expected_chunk);
        assert_eq!(restored.water_count, 1);
        assert_eq!(restored.lava_count, 1);
        assert!(restored.is_dirty && restored.light_dirty && !restored.has_mesh);
        assert_eq!(restored.get_light_at(1, 10, 2), (15, 0));
    }

    #[test]
    fn version_three_chunk_migrates_default_block_entities() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(1, 20, 2, Block::new(BlockId::Chest));
        chunk.set_block(3, 20, 4, Block::new(BlockId::Furnace));
        let legacy = ChunkData::from_chunk(&chunk);
        let old_file = serde_json::json!({
            "format_version": FORMAT_VERSION,
            "data_version": 3,
            "kind": "chunk",
            "data": {
                "cx": legacy.cx,
                "cz": legacy.cz,
                "cells": legacy.cells,
            },
        });
        let path = storage.chunk_path(0, 0);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec(&old_file).unwrap()).unwrap();

        let migrated = storage.load_chunk(0, 0).unwrap();
        assert_eq!(migrated.block_entities.len(), 2);
        assert!(matches!(
            &migrated.block_entities[0],
            BlockEntityData::Chest { slots, .. } if slots.len() == CHEST_SLOTS
        ));
        assert!(matches!(
            &migrated.block_entities[1],
            BlockEntityData::Furnace { slots, burn_time: 0, burn_total: 0, cook_time: 0, .. } if slots.len() == 3
        ));

        storage.save_chunk(&migrated).unwrap();
        let rewritten: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(rewritten["data_version"], DATA_VERSION);
    }

    #[test]
    fn version_one_level_migrates_to_version_two_defaults() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let old_level = serde_json::json!({
            "format_version": FORMAT_VERSION,
            "data_version": 1,
            "kind": "level",
            "data": {
                "seed": 9,
                "tick": 12,
                "game_time": 24,
                "spawn": [0, 75, 0],
                "gamemode": "survival",
                "difficulty": "normal",
                "hardcore": false,
                "do_daylight_cycle": true
            }
        });
        fs::write(storage.level_path(), serde_json::to_vec(&old_level).unwrap()).unwrap();

        let migrated = storage.load_level().unwrap();
        assert_eq!(migrated.experience, 0);
        assert!(migrated.scheduled_ticks.is_empty());
        assert!(migrated.dropped_items.is_empty());
        assert!(migrated.xp_orbs.is_empty());
        assert_eq!(migrated.coordinate_profile, WorldCoordinateProfile::LegacyLocal);
        assert_eq!(
            migrated.generation_profile,
            WorldGenerationProfile::LegacyPreCorrectedInterpolation
        );
    }

    #[test]
    fn version_six_level_defaults_to_legacy_coordinates_without_shifting_data() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut data = serde_json::to_value(level(12)).unwrap();
        let object = data.as_object_mut().unwrap();
        object.insert(
            "coordinate_profile".to_string(),
            Value::String("java_overworld".to_string()),
        );
        object.insert(
            "generation_profile".to_string(),
            Value::String("minecraft26_native_decoration_preview".to_string()),
        );
        let old_level = serde_json::json!({
            "format_version": FORMAT_VERSION,
            "data_version": 6,
            "kind": "level",
            "data": data,
        });
        fs::write(storage.level_path(), serde_json::to_vec(&old_level).unwrap()).unwrap();

        let migrated = storage.load_level().unwrap();
        assert_eq!(migrated.coordinate_profile, WorldCoordinateProfile::LegacyLocal);
        assert_eq!(
            migrated.generation_profile,
            WorldGenerationProfile::LegacyPreCorrectedInterpolation
        );
        assert_eq!(migrated.spawn, [1, 72, -3]);
        assert_eq!(migrated.dropped_items[0].y, 72.0);
    }

    #[test]
    fn version_seven_level_defaults_to_legacy_generation_without_shifting_data() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut data = serde_json::to_value(level(12)).unwrap();
        let object = data.as_object_mut().unwrap();
        object.insert(
            "coordinate_profile".to_string(),
            Value::String("java_overworld".to_string()),
        );
        object.insert(
            "generation_profile".to_string(),
            Value::String("minecraft26_native_decoration_preview".to_string()),
        );
        let old_level = serde_json::json!({
            "format_version": FORMAT_VERSION,
            "data_version": 7,
            "kind": "level",
            "data": data,
        });
        fs::write(storage.level_path(), serde_json::to_vec(&old_level).unwrap()).unwrap();

        let migrated = storage.load_level().unwrap();
        assert_eq!(
            migrated.generation_profile,
            WorldGenerationProfile::LegacyPreCorrectedInterpolation
        );
        assert_eq!(migrated.coordinate_profile, WorldCoordinateProfile::JavaOverworld);
        assert_eq!(migrated.spawn, [1, 72, -3]);
        assert_eq!(migrated.dropped_items[0].y, 72.0);
    }

    #[test]
    fn version_eight_level_normalizes_a_forged_decoration_profile_to_base() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut data = serde_json::to_value(level(12)).unwrap();
        data.as_object_mut()
            .unwrap()
            .insert(
                "generation_profile".to_string(),
                Value::String("minecraft26_native_decoration_preview".to_string()),
            );
        let old_level = serde_json::json!({
            "format_version": FORMAT_VERSION,
            "data_version": 8,
            "kind": "level",
            "data": data,
        });
        fs::write(storage.level_path(), serde_json::to_vec(&old_level).unwrap()).unwrap();

        let migrated = storage.load_level().unwrap();
        assert_eq!(migrated.generation_profile, WorldGenerationProfile::Minecraft26Base);
        storage.save_level(&migrated).unwrap();
        let rewritten: Value = serde_json::from_slice(&fs::read(storage.level_path()).unwrap()).unwrap();
        assert_eq!(rewritten["data_version"], DATA_VERSION);
        assert_eq!(
            rewritten["data"]["generation_profile"],
            "minecraft26_base",
        );
    }

    #[test]
    fn version_nine_level_retains_its_existing_generation_profile() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut data = serde_json::to_value(level(12)).unwrap();
        data.as_object_mut().unwrap().insert(
            "generation_profile".to_string(),
            Value::String("minecraft26_native_decoration_preview".to_string()),
        );
        let old_level = serde_json::json!({
            "format_version": FORMAT_VERSION,
            "data_version": 9,
            "kind": "level",
            "data": data,
        });
        fs::write(storage.level_path(), serde_json::to_vec(&old_level).unwrap()).unwrap();

        let migrated = storage.load_level().unwrap();
        assert_eq!(
            migrated.generation_profile,
            WorldGenerationProfile::Minecraft26NativeDecorationPreview
        );
    }

    #[test]
    fn current_level_round_trips_minecraft26_geometry_profile() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut data = level(13);
        data.generation_profile = WorldGenerationProfile::Minecraft26Geometry;
        storage.save_level(&data).unwrap();
        assert_eq!(storage.load_level().unwrap().generation_profile, data.generation_profile);
    }

    #[test]
    fn current_version_level_requires_explicit_world_profiles() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);

        for field in ["coordinate_profile", "generation_profile"] {
            let mut data = serde_json::to_value(level(12)).unwrap();
            data.as_object_mut().unwrap().remove(field);
            let current_level = serde_json::json!({
                "format_version": FORMAT_VERSION,
                "data_version": DATA_VERSION,
                "kind": "level",
                "data": data,
            });
            fs::write(storage.level_path(), serde_json::to_vec(&current_level).unwrap()).unwrap();

            assert!(matches!(storage.load_level(), Err(StorageError::Json { .. })));
        }
    }

    #[test]
    fn writes_replace_files_without_leaving_temporary_data() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        storage.save_level(&level(1)).unwrap();
        storage.save_level(&level(2)).unwrap();

        assert_eq!(storage.load_level().unwrap().seed, 2);
        let names: Vec<_> = fs::read_dir(&world.path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from(LEVEL_FILE)]);
    }

    #[test]
    fn corrupt_level_is_rejected_without_replacing_it() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let path = storage.level_path();
        let corrupt_json = b"{ not valid json";
        fs::write(&path, corrupt_json).unwrap();

        assert!(matches!(
            storage.load_or_create_level(level(99)),
            Err(StorageError::Json { .. })
        ));
        assert_eq!(fs::read(path).unwrap(), corrupt_json);
    }

    #[test]
    fn discovery_lists_valid_child_worlds_and_rejects_corrupt_ones() {
        let root = TempWorld::new();
        let legacy = WorldStorage::new(&root.path);
        legacy.save_level(&level(1)).unwrap();
        let child_path = root.path.join("child");
        let child = WorldStorage::new(&child_path);
        child.save_level(&level(2)).unwrap();
        let corrupt_path = root.path.join("corrupt");
        fs::create_dir(&corrupt_path).unwrap();
        fs::write(corrupt_path.join(LEVEL_FILE), b"invalid").unwrap();

        let discovery = discover_worlds(&root.path).unwrap();
        assert_eq!(discovery.worlds.len(), 2);
        assert_eq!(discovery.worlds[0].name, "Fixture World");
        assert_eq!(discovery.rejected.len(), 1);
        assert_eq!(discovery.rejected[0].0, corrupt_path);
    }

    #[test]
    fn player_and_inventory_round_trip() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut player = Player::new(4.0, 70.0, -9.0);
        player.health = 13.5;
        player.vy = -2.0;
        player.effects.apply(StatusEffect::NightVision, 120.0, 1);
        let mut inventory = Inventory::new();
        inventory.slots[3] = ItemStack::new(12, 42);
        inventory.held_slot = 3;

        storage
            .save_player(&PlayerData::from_runtime(&player, &inventory))
            .unwrap();
        let (loaded_player, loaded_inventory) = storage
            .load_player()
            .unwrap()
            .into_runtime()
            .unwrap();

        assert_eq!((loaded_player.x, loaded_player.y, loaded_player.z), (4.0, 70.0, -9.0));
        assert_eq!(loaded_player.health, 13.5);
        assert_eq!(loaded_player.effects.get_amplifier(StatusEffect::NightVision), Some(1));
        assert_eq!(loaded_inventory.slots[3], ItemStack::new(12, 42));
        assert_eq!(loaded_inventory.held_slot, 3);
    }

    #[test]
    fn player_coordinates_are_not_limited_by_block_coordinate_profiles() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let player = Player::new(0.0, 512.0, 0.0);
        let inventory = Inventory::new();
        let data = PlayerData::from_runtime(&player, &inventory);

        storage.save_player(&data).unwrap();
        let (restored, _) = storage.load_player().unwrap().into_runtime().unwrap();
        assert_eq!(restored.y, 512.0);

        let mut level = level(1);
        level.coordinate_profile = WorldCoordinateProfile::JavaOverworld;
        level.players.push(NamedPlayerData {
            username: "Alex".to_string(),
            player: data,
        });
        storage.save_level(&level).unwrap();
        assert_eq!(storage.load_level().unwrap().players[0].player.y, 512.0);
    }

    #[test]
    fn dropped_entities_must_fit_their_profile_live_height() {
        let world = TempWorld::new();
        let storage = WorldStorage::new(&world.path);
        let mut legacy_level = level(1);
        legacy_level.dropped_items[0].y = -1.0;
        assert!(matches!(storage.save_level(&legacy_level), Err(StorageError::Corrupt { .. })));

        let mut java_level = level(1);
        java_level.coordinate_profile = WorldCoordinateProfile::JavaOverworld;
        java_level.xp_orbs[0].y = 336.0;
        assert!(matches!(storage.save_level(&java_level), Err(StorageError::Corrupt { .. })));
    }
}
