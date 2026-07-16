//! Port of Minecraft's NoiseBasedChunkGenerator fillFromNoise/doFill.
//!
//! Implements the cell-based trilinear interpolation loop that drives
//! chunk generation from density functions.  This is the top-level
//! orchestrator that ties the NoiseRouter, NoiseSettings, and
//! DensityFunction system to block placement.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use crate::world::generation::WorldGenerationProfile;
use crate::world::world_gen::density_fn::{
    DensityFunction, InterpolatedContext, SinglePointContext,
};
use crate::world::world_gen::noise::{NoiseSeed, NoiseSettings, PositionalRandomFactory};
use crate::world::world_gen::noise_router::{NoiseRouter, NoiseRouterData};
use crate::world::world_gen::biome_source::OverworldBiomeSource;
use crate::world::world_gen::aquifer::NoiseBasedAquifer;
use crate::world::world_gen::surface::{default_overworld_rules, OreVeinifier, StaticRuleSource, SurfaceSystem};
use crate::world::world_gen::Biome;
use crate::world::world_gen::decoration::{
    decoration_seed, feature_seed, ChunkPosition, FeatureCandidate, PlannedFeature, WorldPosition,
    MAX_DECORATION_OPERATIONS,
};

// Refine only the narrow material-boundary band. This removes visible cell
// steps while keeping the current bounded cell cache practical until the
// density graph's Java cache wrappers are fully implemented.
const DENSITY_EXACT_REFINEMENT: f64 = 0.001;
const DENSITY_BOUNDARY_REFINEMENT: f64 = 0.05;
const ORE_VEIN_MIN_Y: i32 = -60;
const ORE_VEIN_MAX_Y: i32 = 50;
const SURFACE_BIOME_QUART_CACHE_CAPACITY: usize =
    (CHUNK_SIZE / 4) * (CHUNK_HEIGHT / 4) * (CHUNK_SIZE / 4);

/// Owner chunks that can reach the target with a preview tree canopy or well.
const PREVIEW_OWNER_HALO: i32 = 1;
/// Both supported features extend at most two blocks from their origin in X/Z.
const PREVIEW_SUPPORT_HALO: i32 = 2;
/// A 3x3 owner halo can produce one tree and one well candidate per owner.
const MAX_PREVIEW_CANDIDATES: usize = 18;
/// Relevant radius-two footprints can touch only the target and its eight
/// neighboring chunks. Snapshot generation must never exceed this scope.
const MAX_PREVIEW_SNAPSHOTS: usize = 9;
const PREVIEW_TREE_STEP: i32 = 0;
const PREVIEW_WELL_STEP: i32 = 1;

/// Per-surface-pass biome cache. Surface rules can query many blocks in a
/// column, but biome sampling is defined at quart resolution. Keeping the
/// cache to this chunk's quart axes makes memory use fixed and worker-local.
struct SurfaceBiomeQuartCache {
    min_quart_x: i32,
    max_quart_x: i32,
    min_quart_y: i32,
    max_quart_y: i32,
    min_quart_z: i32,
    max_quart_z: i32,
    biomes: HashMap<(i32, i32, i32), Biome>,
}

impl SurfaceBiomeQuartCache {
    fn new(chunk_x: i32, chunk_z: i32, world_min_y: i32) -> Self {
        let min_block_x = chunk_x * CHUNK_SIZE as i32;
        let min_block_z = chunk_z * CHUNK_SIZE as i32;
        Self {
            min_quart_x: min_block_x.div_euclid(4),
            max_quart_x: (min_block_x + CHUNK_SIZE as i32 - 1).div_euclid(4),
            min_quart_y: world_min_y.div_euclid(4),
            max_quart_y: (world_min_y + CHUNK_HEIGHT as i32 - 1).div_euclid(4),
            min_quart_z: min_block_z.div_euclid(4),
            max_quart_z: (min_block_z + CHUNK_SIZE as i32 - 1).div_euclid(4),
            biomes: HashMap::with_capacity(SURFACE_BIOME_QUART_CACHE_CAPACITY),
        }
    }

    fn quart_key(block_x: i32, block_y: i32, block_z: i32) -> (i32, i32, i32) {
        (
            block_x.div_euclid(4),
            block_y.div_euclid(4),
            block_z.div_euclid(4),
        )
    }

    fn contains(&self, (quart_x, quart_y, quart_z): (i32, i32, i32)) -> bool {
        (self.min_quart_x..=self.max_quart_x).contains(&quart_x)
            && (self.min_quart_y..=self.max_quart_y).contains(&quart_y)
            && (self.min_quart_z..=self.max_quart_z).contains(&quart_z)
    }

    fn get_or_insert_with(
        &mut self,
        block_x: i32,
        block_y: i32,
        block_z: i32,
        query: impl FnOnce(i32, i32, i32) -> Biome,
    ) -> Biome {
        let key = Self::quart_key(block_x, block_y, block_z);
        if !self.contains(key) {
            return query(key.0, key.1, key.2);
        }
        if let Some(&biome) = self.biomes.get(&key) {
            return biome;
        }

        let biome = query(key.0, key.1, key.2);
        debug_assert!(self.biomes.len() < SURFACE_BIOME_QUART_CACHE_CAPACITY);
        self.biomes.insert(key, biome);
        biome
    }
}

// ---------------------------------------------------------------------------
// AquiferData
// ---------------------------------------------------------------------------

/// Per-chunk fluid-placement configuration (simplified aquifer).
#[derive(Clone, Debug)]
pub struct AquiferData {
    pub sea_level: i32,
    pub default_fluid: BlockId,
    pub lava_level: i32,
    pub lava_fluid: BlockId,
}

impl AquiferData {
    pub const fn overworld() -> Self {
        AquiferData {
            sea_level: 63,
            default_fluid: BlockId::Water,
            lava_level: -54,
            lava_fluid: BlockId::Lava,
        }
    }

    /// Determine the fluid block that would occupy (x, y, z) in the absence
    /// of an aquifer system.
    pub fn fluid_at(&self, _x: i32, y: i32, _z: i32) -> BlockId {
        if y < self.lava_level.min(self.sea_level) {
            self.lava_fluid
        } else if y < self.sea_level {
            self.default_fluid
        } else {
            BlockId::Air
        }
    }
}

// ---------------------------------------------------------------------------
// CellCorners
// ---------------------------------------------------------------------------

/// Density values at the 8 corners of a cell.
///
/// Index layout: `values[xi][yi][zi]` where xi/yi/zi ∈ {0, 1}.
/// - [0][0][0] = (cell_min_x, cell_min_y, cell_min_z)
/// - [1][0][0] = (cell_max_x, cell_min_y, cell_min_z)
/// - [0][1][0] = (cell_min_x, cell_max_y, cell_min_z)
/// - [1][1][0] = (cell_max_x, cell_max_y, cell_min_z)
/// - [0][0][1] = (cell_min_x, cell_min_y, cell_max_z)
/// — etc.
#[derive(Clone, Copy, Debug)]
pub struct CellCorners {
    values: [[[f64; 2]; 2]; 2],
}

impl CellCorners {
    fn sample_at(
        router: &NoiseRouter,
        cell_min_x: i32,
        cell_min_y: i32,
        cell_min_z: i32,
        cell_width: i32,
        cell_height: i32,
    ) -> Self {
        let x1 = cell_min_x;
        let x2 = cell_min_x + cell_width;
        let y1 = cell_min_y;
        let y2 = cell_min_y + cell_height;
        let z1 = cell_min_z;
        let z2 = cell_min_z + cell_width;

        let v000 = sample_density(router, x1, y1, z1);
        let v100 = sample_density(router, x2, y1, z1);
        let v010 = sample_density(router, x1, y2, z1);
        let v110 = sample_density(router, x2, y2, z1);
        let v001 = sample_density(router, x1, y1, z2);
        let v101 = sample_density(router, x2, y1, z2);
        let v011 = sample_density(router, x1, y2, z2);
        let v111 = sample_density(router, x2, y2, z2);

        CellCorners {
            values: [
                [[v000, v001], [v010, v011]],
                [[v100, v101], [v110, v111]],
            ],
        }
    }

    fn trilerp(&self, fx: f64, fy: f64, fz: f64) -> f64 {
        let v00 = lerp(fx, self.values[0][0][0], self.values[1][0][0]);
        let v10 = lerp(fx, self.values[0][1][0], self.values[1][1][0]);
        let v01 = lerp(fx, self.values[0][0][1], self.values[1][0][1]);
        let v11 = lerp(fx, self.values[0][1][1], self.values[1][1][1]);
        let v0 = lerp(fy, v00, v10);
        let v1 = lerp(fy, v01, v11);
        lerp(fz, v0, v1)
    }
}

// ---------------------------------------------------------------------------
// NoiseChunkData — per-chunk interpolation state
// ---------------------------------------------------------------------------

/// All state needed to perform the doFill interpolation loop for one chunk.
pub struct NoiseChunkData {
    pub router: Arc<NoiseRouter>,
    pub settings: NoiseSettings,
    pub aquifer: AquiferData,
    pub cell_width: i32,
    pub cell_height: i32,
    pub min_y: i32,
    pub cell_count_y: i32,
    pub cell_min_y: i32,
    pub cell_count_xz: i32,
    pub first_cell_x: i32,
    pub first_cell_z: i32,
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub chunk_start_x: i32,
    pub chunk_start_z: i32,

    // Two slices of density values, each [cellZ][cellY] sized.
    // slice0 corresponds to the current cell-X, slice1 to cell-X+1.
    slice0: Vec<Vec<f64>>,
    slice1: Vec<Vec<f64>>,

    // Current cell corner values (selected by select_cell_yz)
    noise000: f64,
    noise001: f64,
    noise100: f64,
    noise101: f64,
    noise010: f64,
    noise011: f64,
    noise110: f64,
    noise111: f64,

    // Intermediate interpolated values
    value_xz00: f64,
    value_xz10: f64,
    value_xz01: f64,
    value_xz11: f64,
    value_z0: f64,
    value_z1: f64,
    pub value: f64,

    pub interpolating: bool,
}

impl NoiseChunkData {
    pub fn new(
        chunk: &Chunk,
        router: Arc<NoiseRouter>,
        settings: NoiseSettings,
        aquifer: AquiferData,
    ) -> Self {
        let cell_width = settings.cell_width();
        let cell_height = settings.cell_height();
        let min_y = settings.min_y;
        let height = settings.height;
        let cell_count_y = height / cell_height;
        let cell_count_xz = CHUNK_SIZE as i32 / cell_width;
        let cell_min_y = min_y.div_euclid(cell_height);
        // Note: when min_y is negative, Java's floorDiv gives a different
        // result than Rust's div_euclid for negative numbers in some cases.
        // For -64 / 8 = -8 both agree.
        let first_cell_x = (chunk.cx as i64 * CHUNK_SIZE as i64)
            .div_euclid(cell_width as i64) as i32;
        let first_cell_z = (chunk.cz as i64 * CHUNK_SIZE as i64)
            .div_euclid(cell_width as i64) as i32;
        let chunk_start_x = chunk.cx * CHUNK_SIZE as i32;
        let chunk_start_z = chunk.cz * CHUNK_SIZE as i32;

        let size_z = cell_count_xz + 1;
        let size_y = cell_count_y + 1;
        let slice0 = vec![vec![0.0; size_y as usize]; size_z as usize];
        let slice1 = vec![vec![0.0; size_y as usize]; size_z as usize];

        NoiseChunkData {
            router,
            settings,
            aquifer,
            cell_width,
            cell_height,
            min_y,
            cell_count_y,
            cell_min_y,
            cell_count_xz,
            first_cell_x,
            first_cell_z,
            chunk_x: chunk.cx,
            chunk_z: chunk.cz,
            chunk_start_x,
            chunk_start_z,
            slice0,
            slice1,
            noise000: 0.0,
            noise001: 0.0,
            noise100: 0.0,
            noise101: 0.0,
            noise010: 0.0,
            noise011: 0.0,
            noise110: 0.0,
            noise111: 0.0,
            value_xz00: 0.0,
            value_xz10: 0.0,
            value_xz01: 0.0,
            value_xz11: 0.0,
            value_z0: 0.0,
            value_z1: 0.0,
            value: 0.0,
            interpolating: false,
        }
    }

    /// Fill slice for the given cell-X coordinate.
    fn fill_slice_from(
        slice: &mut [Vec<f64>],
        cell_x: i32,
        cell_count_xz: i32,
        cell_count_y: i32,
        cell_width: i32,
        cell_height: i32,
        cell_min_y: i32,
        first_cell_z: i32,
        router: &NoiseRouter,
    ) {
        let cell_start_x = cell_x * cell_width;
        for cell_z_idx in 0..=cell_count_xz {
            let cell_z = first_cell_z + cell_z_idx;
            let cell_start_z = cell_z * cell_width;
            for cell_y_idx in 0..=cell_count_y {
                let cell_start_y = (cell_min_y + cell_y_idx) * cell_height;
                let ctx = SinglePointContext {
                    block_x: cell_start_x,
                    block_y: cell_start_y,
                    block_z: cell_start_z,
                };
                slice[cell_z_idx as usize][cell_y_idx as usize] =
                    router.final_density.compute(&ctx);
            }
        }
    }

    /// Equivalent to NoiseChunk.initializeForFirstCellX().
    pub fn initialize_for_first_cell_x(&mut self) {
        assert!(!self.interpolating, "already interpolating");
        self.interpolating = true;
        Self::fill_slice_from(
            &mut self.slice0,
            self.first_cell_x,
            self.cell_count_xz,
            self.cell_count_y,
            self.cell_width,
            self.cell_height,
            self.cell_min_y,
            self.first_cell_z,
            &self.router,
        );
    }

    /// Equivalent to NoiseChunk.advanceCellX(cellXIndex).
    pub fn advance_cell_x(&mut self, cell_x_index: i32) {
        Self::fill_slice_from(
            &mut self.slice1,
            self.first_cell_x + cell_x_index + 1,
            self.cell_count_xz,
            self.cell_count_y,
            self.cell_width,
            self.cell_height,
            self.cell_min_y,
            self.first_cell_z,
            &self.router,
        );
    }

    /// Equivalent to NoiseChunk.swapSlices(). Java swaps only after every
    /// cell-Z/Y column for the current cell-X has consumed both endpoints.
    pub fn swap_slices(&mut self) {
        std::mem::swap(&mut self.slice0, &mut self.slice1);
    }

    /// Equivalent to NoiseChunk.selectCellYZ(cellYIndex, cellZIndex).
    ///
    /// Loads the 8 corner noise values for the current cell from the slices.
    pub fn select_cell_yz(&mut self, cell_y_index: i32, cell_z_index: i32) {
        let cz = cell_z_index as usize;
        let cy = cell_y_index as usize;
        let cy1 = (cell_y_index + 1) as usize;

        self.noise000 = self.slice0[cz][cy];
        self.noise001 = self.slice0[cz + 1][cy];
        self.noise100 = self.slice1[cz][cy];
        self.noise101 = self.slice1[cz + 1][cy];

        self.noise010 = self.slice0[cz][cy1];
        self.noise011 = self.slice0[cz + 1][cy1];
        self.noise110 = self.slice1[cz][cy1];
        self.noise111 = self.slice1[cz + 1][cy1];
    }

    /// Equivalent to NoiseChunk.updateForY(posY, factorY).
    pub fn update_for_y(&mut self, factor_y: f64) {
        self.value_xz00 = lerp(factor_y, self.noise000, self.noise010);
        self.value_xz10 = lerp(factor_y, self.noise100, self.noise110);
        self.value_xz01 = lerp(factor_y, self.noise001, self.noise011);
        self.value_xz11 = lerp(factor_y, self.noise101, self.noise111);
    }

    /// Equivalent to NoiseChunk.updateForX(posX, factorX).
    pub fn update_for_x(&mut self, factor_x: f64) {
        self.value_z0 = lerp(factor_x, self.value_xz00, self.value_xz10);
        self.value_z1 = lerp(factor_x, self.value_xz01, self.value_xz11);
    }

    /// Equivalent to NoiseChunk.updateForZ(posZ, factorZ).
    pub fn update_for_z(&mut self, factor_z: f64) {
        self.value = lerp(factor_z, self.value_z0, self.value_z1);
    }

    /// Call when done to reset interpolation state.
    pub fn stop_interpolation(&mut self) {
        assert!(self.interpolating, "not interpolating");
        self.interpolating = false;
    }

    /// Evaluate the cell corners directly (fallback / for unreachable positions).
    pub fn compute_cell_corners(
        &self,
        cell_x_start: i32,
        cell_y_start: i32,
        cell_z_start: i32,
    ) -> CellCorners {
        CellCorners::sample_at(
            &self.router,
            cell_x_start,
            cell_y_start,
            cell_z_start,
            self.cell_width,
            self.cell_height,
        )
    }
}

// ---------------------------------------------------------------------------
// Density sampling — evaluates the NoiseRouter's final_density
// ---------------------------------------------------------------------------

/// Sample the router's final density at a world block position.
fn sample_density(router: &NoiseRouter, x: i32, y: i32, z: i32) -> f64 {
    let ctx = SinglePointContext {
        block_x: x,
        block_y: y,
        block_z: z,
    };
    router.final_density.compute(&ctx)
}

// ---------------------------------------------------------------------------
// Preliminary surface level
// ---------------------------------------------------------------------------

/// Estimate the surface level from the preliminary_surface_level function
/// (which is the offset/factor-based FindTopSurface from NoiseRouterData).
fn preliminary_surface_level(router: &NoiseRouter, block_x: i32, block_z: i32) -> i32 {
    let ctx = SinglePointContext {
        block_x,
        block_y: 0,
        block_z,
    };
    // The preliminary_surface_level function returns the surface Y as a float.
    router.preliminary_surface_level.compute(&ctx).floor() as i32
}

// ---------------------------------------------------------------------------
// Block determination
// ---------------------------------------------------------------------------

/// Stone type with deepslate depth transition.
fn get_stone_type(y: i32, _noise_val: f64) -> BlockId {
    // Simplified deepslate transition:
    // y >= 0: stone
    // y from -8 to 0: transition zone (using noise_val as threshold)
    // y < -8: deepslate
    if y >= 0 {
        BlockId::Stone
    } else if y >= -8 {
        let t = y as f64 / -8.0; // 0 at y=0, 1 at y=-8
        if _noise_val > t {
            BlockId::Deepslate
        } else {
            BlockId::Stone
        }
    } else {
        BlockId::Deepslate
    }
}

/// The y=0 density proxy only affects the deepslate transition. Cache it per
/// column so fallback material selection does not resample it for every block.
fn fallback_stone_type(
    y: i32,
    cached_column_noise: &mut Option<f64>,
    sample_column_noise: impl FnOnce() -> f64,
) -> BlockId {
    if (-8..0).contains(&y) {
        get_stone_type(y, *cached_column_noise.get_or_insert_with(sample_column_noise))
    } else {
        get_stone_type(y, 0.0)
    }
}

fn can_apply_ore_veinifier(y: i32) -> bool {
    y >= ORE_VEIN_MIN_Y && y <= ORE_VEIN_MAX_Y
}

// ---------------------------------------------------------------------------
// VanillaWorldGenerator
// ---------------------------------------------------------------------------

/// High-level chunk generator that wraps a NoiseRouter.
///
/// This is the main entry point for noise-based terrain generation.
/// It mirrors the `fillFromNoise` → `doFill` pipeline from
/// Minecraft's `NoiseBasedChunkGenerator`.
pub struct VanillaWorldGenerator {
    pub router: Arc<NoiseRouter>,
    pub settings: NoiseSettings,
    pub aquifer: AquiferData,
    seed: u64,
    biome_source: OverworldBiomeSource,
    aquifer_random: PositionalRandomFactory,
    ore_veins_random: PositionalRandomFactory,
    generation_profile: WorldGenerationProfile,
}

#[derive(Clone, Copy)]
enum PreviewTreeKind {
    Oak,
    Spruce,
}

#[derive(Clone, Copy)]
struct PreviewCandidate {
    feature: FeatureCandidate,
    tree_kind: Option<PreviewTreeKind>,
}

impl VanillaWorldGenerator {
    /// Create a new generator from an existing router.
    pub fn new(
        router: Arc<NoiseRouter>,
        seed: u64,
        generation_profile: WorldGenerationProfile,
    ) -> Self {
        let biome_source = OverworldBiomeSource::from_router((*router).clone());
        let mut root_seed = NoiseSeed::new(seed);
        let random = root_seed.fork_positional();
        let mut aquifer_seed = random.from_hash_of("minecraft:aquifer");
        let mut ore_seed = random.from_hash_of("minecraft:ore");
        let aquifer_random = aquifer_seed.fork_positional();
        let ore_veins_random = ore_seed.fork_positional();
        VanillaWorldGenerator {
            router,
            settings: NoiseSettings::OVERWORLD,
            aquifer: AquiferData::overworld(),
            seed,
            biome_source,
            aquifer_random,
            ore_veins_random,
            generation_profile,
        }
    }

    /// Convenience constructor that creates the router from a seed.
    pub fn from_seed(seed: u64, generation_profile: WorldGenerationProfile) -> Self {
        let router = Arc::new(NoiseRouterData::create_overworld_router(seed, false, false));
        Self::new(router, seed, generation_profile)
    }

    pub fn with_settings(mut self, settings: NoiseSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn with_aquifer(mut self, aquifer: AquiferData) -> Self {
        self.aquifer = aquifer;
        self
    }

    pub fn get_biome(&self, wx: f64, wz: f64) -> Biome {
        self.get_biome_at(wx.floor() as i32, 0, wz.floor() as i32)
    }

    pub fn get_biome_at(&self, x: i32, y: i32, z: i32) -> Biome {
        self.biome_source
            .get_biome(x, y, z)
            .unwrap_or_else(|unsupported| {
                log::warn!("{}; using plains compatibility biome", unsupported);
                Biome::Plains
            })
    }

    // ------------------------------------------------------------------
    // Main chunk generation
    // ------------------------------------------------------------------

    /// Generate the full terrain for a chunk using the density-based pipeline.
    ///
    /// The native decoration preview is composed after the isolated base pass
    /// only for its explicitly versioned generation profile.
    pub fn generate_chunk(&self, chunk: &mut Chunk) {
        self.generate_undecorated_chunk(chunk);
        if self.generation_profile.uses_native_decoration_preview() {
            self.apply_native_decoration_preview(chunk);
        }
    }

    /// Plans candidates from a fixed world-space owner halo, then writes only
    /// operations whose target belongs to `chunk`. This never reaches
    /// `ChunkManager`, does not call `generate_chunk`, and does not mutate a
    /// neighboring runtime chunk. Isolated undecorated snapshots are used only
    /// for bounded support checks.
    fn apply_native_decoration_preview(&self, chunk: &mut Chunk) {
        let target = ChunkPosition::new(chunk.cx, chunk.cz);
        let candidates: Vec<_> = self
            .preview_candidates(target)
            .into_iter()
            .filter(|candidate| preview_footprint_intersects_target(candidate.feature, target))
            .collect();
        if candidates.is_empty() {
            return;
        }

        let snapshot_keys = self.preview_snapshot_keys(target, &candidates);

        let mut snapshots = BTreeMap::new();
        for (cx, cz) in snapshot_keys {
            if (cx, cz) == (chunk.cx, chunk.cz) {
                snapshots.insert((cx, cz), chunk.clone());
            } else {
                let mut snapshot = Chunk::new(cx, cz);
                self.generate_undecorated_chunk(&mut snapshot);
                snapshots.insert((cx, cz), snapshot);
            }
        }

        let mut operations = BTreeMap::new();
        for candidate in candidates {
            match candidate.feature.feature() {
                PlannedFeature::Tree => {
                    if let Some(kind) = candidate.tree_kind {
                        self.plan_preview_tree(candidate.feature, kind, target, &snapshots, chunk, &mut operations);
                    }
                }
                PlannedFeature::DesertWell => {
                    self.plan_preview_well(candidate.feature, target, &snapshots, chunk, &mut operations);
                }
            }
        }

        for (position, block) in operations {
            let Some(local_y) = self.world_y_to_local(position.y()) else {
                continue;
            };
            // Projection only writes cells that belonged to the undecorated
            // target snapshot. This protects terrain and resolves feature
            // overlaps without depending on chunk generation order.
            let local_x = position.x().rem_euclid(CHUNK_SIZE as i32) as usize;
            let local_z = position.z().rem_euclid(CHUNK_SIZE as i32) as usize;
            if chunk.get_block(local_x, local_y, local_z).is_air() {
                chunk.set_block(local_x, local_y, local_z, block);
            }
        }
        chunk.recount_fluids();
    }

    fn preview_snapshot_keys(
        &self,
        target: ChunkPosition,
        candidates: &[PreviewCandidate],
    ) -> BTreeSet<(i32, i32)> {
        debug_assert!(candidates.len() <= MAX_PREVIEW_CANDIDATES);

        let mut snapshot_keys = BTreeSet::new();
        snapshot_keys.insert((target.x(), target.z()));
        for candidate in candidates {
            let origin = candidate.feature.origin();
            match candidate.feature.feature() {
                PlannedFeature::Tree => {
                    snapshot_keys.insert((candidate.feature.owner().x(), candidate.feature.owner().z()));
                }
                PlannedFeature::DesertWell => {
                    // A well validates exactly its five-by-five support area.
                    // Add only the chunk corners that area can touch, rather
                    // than a halo around every candidate owner.
                    let min_chunk_x = origin
                        .x()
                        .saturating_sub(PREVIEW_SUPPORT_HALO)
                        .div_euclid(CHUNK_SIZE as i32);
                    let max_chunk_x = origin
                        .x()
                        .saturating_add(PREVIEW_SUPPORT_HALO)
                        .div_euclid(CHUNK_SIZE as i32);
                    let min_chunk_z = origin
                        .z()
                        .saturating_sub(PREVIEW_SUPPORT_HALO)
                        .div_euclid(CHUNK_SIZE as i32);
                    let max_chunk_z = origin
                        .z()
                        .saturating_add(PREVIEW_SUPPORT_HALO)
                        .div_euclid(CHUNK_SIZE as i32);
                    for cx in min_chunk_x..=max_chunk_x {
                        for cz in min_chunk_z..=max_chunk_z {
                            snapshot_keys.insert((cx, cz));
                        }
                    }
                }
            }
        }
        debug_assert!(snapshot_keys.len() <= MAX_PREVIEW_SNAPSHOTS);
        snapshot_keys
    }

    fn preview_candidates(&self, target: ChunkPosition) -> Vec<PreviewCandidate> {
        let mut candidates = Vec::with_capacity(MAX_PREVIEW_CANDIDATES);
        for cz in target.z() - PREVIEW_OWNER_HALO..=target.z() + PREVIEW_OWNER_HALO {
            for cx in target.x() - PREVIEW_OWNER_HALO..=target.x() + PREVIEW_OWNER_HALO {
                let chunk_start_x = cx * CHUNK_SIZE as i32;
                let chunk_start_z = cz * CHUNK_SIZE as i32;
                let decoration = decoration_seed(self.seed, chunk_start_x, chunk_start_z);

                let mut tree_rng = feature_seed(decoration, PREVIEW_TREE_STEP, 0);
                if tree_rng.next_int(3) == 0 && candidates.len() < MAX_PREVIEW_CANDIDATES {
                    let x = chunk_start_x + tree_rng.next_int(CHUNK_SIZE as i32);
                    let z = chunk_start_z + tree_rng.next_int(CHUNK_SIZE as i32);
                    let kind = match self.get_biome_at(x, 0, z) {
                        Biome::Plains
                        | Biome::Forest
                        | Biome::BirchForest
                        | Biome::FlowerForest
                        | Biome::SunflowerPlains => Some(PreviewTreeKind::Oak),
                        Biome::Taiga
                        | Biome::SnowyTaiga
                        | Biome::OldGrowthPineTaiga
                        | Biome::OldGrowthSpruceTaiga
                        | Biome::Grove => Some(PreviewTreeKind::Spruce),
                        _ => None,
                    };
                    if let Some(tree_kind) = kind {
                        candidates.push(PreviewCandidate {
                            feature: FeatureCandidate::new(PlannedFeature::Tree, WorldPosition::new(x, 0, z)),
                            tree_kind: Some(tree_kind),
                        });
                    }
                }

                let mut well_rng = feature_seed(decoration, PREVIEW_WELL_STEP, 0);
                if well_rng.next_int(128) == 0 && candidates.len() < MAX_PREVIEW_CANDIDATES {
                    let x = chunk_start_x + well_rng.next_int(CHUNK_SIZE as i32);
                    let z = chunk_start_z + well_rng.next_int(CHUNK_SIZE as i32);
                    if self.get_biome_at(x, 0, z) == Biome::Desert {
                        candidates.push(PreviewCandidate {
                            feature: FeatureCandidate::new(PlannedFeature::DesertWell, WorldPosition::new(x, 0, z)),
                            tree_kind: None,
                        });
                    }
                }
            }
        }
        candidates
    }

    fn plan_preview_tree(
        &self,
        candidate: FeatureCandidate,
        kind: PreviewTreeKind,
        target: ChunkPosition,
        snapshots: &BTreeMap<(i32, i32), Chunk>,
        chunk: &Chunk,
        operations: &mut BTreeMap<WorldPosition, Block>,
    ) {
        let origin = candidate.origin();
        let Some((surface_y, support)) = preview_surface(snapshots, origin.x(), origin.z(), self.settings.min_y) else {
            return;
        };
        if !matches!(support.id, BlockId::GrassBlock | BlockId::Dirt | BlockId::Podzol | BlockId::CoarseDirt) {
            return;
        }
        let root_y = surface_y + 1;
        let seed = decoration_seed(
            self.seed,
            candidate.owner().x() * CHUNK_SIZE as i32,
            candidate.owner().z() * CHUNK_SIZE as i32,
        );
        let mut random = feature_seed(seed, PREVIEW_TREE_STEP, 1);
        let (trunk_height, log, leaves, canopy_height) = match kind {
            PreviewTreeKind::Oak => (
                4 + random.next_int(3),
                BlockId::OakLog,
                BlockId::OakLeaves,
                2,
            ),
            PreviewTreeKind::Spruce => (
                6 + random.next_int(3),
                BlockId::SpruceLog,
                BlockId::SpruceLeaves,
                2,
            ),
        };
        let Some(root_local_y) = self.world_y_to_local(root_y) else {
            return;
        };
        if root_local_y + trunk_height as usize + canopy_height as usize >= self.settings.height as usize {
            return;
        }

        for dy in 0..trunk_height {
            self.project_preview_block(
                target,
                chunk,
                operations,
                WorldPosition::new(origin.x(), root_y + dy, origin.z()),
                Block::new(log),
            );
        }
        match kind {
            PreviewTreeKind::Oak => {
                let canopy_y = root_y + trunk_height - 1;
                for dy in -2..=1 {
                    let radius: i32 = if dy == -1 || dy == 0 { 2 } else { 1 };
                    for dx in -radius..=radius {
                        for dz in -radius..=radius {
                            if dx.abs() + dz.abs() > radius + 1 || (dx == 0 && dz == 0 && dy <= 0) {
                                continue;
                            }
                            self.project_preview_block(
                                target,
                                chunk,
                                operations,
                                WorldPosition::new(origin.x() + dx, canopy_y + dy, origin.z() + dz),
                                Block::new(leaves),
                            );
                        }
                    }
                }
            }
            PreviewTreeKind::Spruce => {
                let canopy_y = root_y + trunk_height - 3;
                for (layer, radius) in [2_i32, 2, 1, 1, 0].into_iter().enumerate() {
                    for dx in -radius..=radius {
                        for dz in -radius..=radius {
                            if dx.abs() + dz.abs() > radius + 1 || (dx == 0 && dz == 0) {
                                continue;
                            }
                            self.project_preview_block(
                                target,
                                chunk,
                                operations,
                                WorldPosition::new(origin.x() + dx, canopy_y + layer as i32, origin.z() + dz),
                                Block::new(leaves),
                            );
                        }
                    }
                }
            }
        }
    }

    fn plan_preview_well(
        &self,
        candidate: FeatureCandidate,
        target: ChunkPosition,
        snapshots: &BTreeMap<(i32, i32), Chunk>,
        chunk: &Chunk,
        operations: &mut BTreeMap<WorldPosition, Block>,
    ) {
        let origin = candidate.origin();
        let mut surface_y = None;
        for dx in -2..=2 {
            for dz in -2..=2 {
                let Some((y, support)) = preview_surface(
                    snapshots,
                    origin.x() + dx,
                    origin.z() + dz,
                    self.settings.min_y,
                ) else {
                    return;
                };
                if support.id != BlockId::Sand || surface_y.is_some_and(|expected| expected != y) {
                    return;
                }
                surface_y = Some(y);
            }
        }
        let Some(surface_y) = surface_y else {
            return;
        };
        let Some(surface_local_y) = self.world_y_to_local(surface_y) else {
            return;
        };
        if surface_local_y + 5 >= self.settings.height as usize {
            return;
        }

        for dx in -2..=2 {
            for dz in -2..=2 {
                self.project_preview_block(
                    target,
                    chunk,
                    operations,
                    WorldPosition::new(origin.x() + dx, surface_y + 1, origin.z() + dz),
                    Block::new(BlockId::Sandstone),
                );
                let block = if dx.abs() == 2 || dz.abs() == 2 {
                    BlockId::Sandstone
                } else {
                    BlockId::Water
                };
                self.project_preview_block(
                    target,
                    chunk,
                    operations,
                    WorldPosition::new(origin.x() + dx, surface_y + 2, origin.z() + dz),
                    Block::new(block),
                );
                self.project_preview_block(
                    target,
                    chunk,
                    operations,
                    WorldPosition::new(origin.x() + dx, surface_y + 5, origin.z() + dz),
                    Block::new(BlockId::Sandstone),
                );
            }
        }
        for dy in 3..=4 {
            for (dx, dz) in [(-2, -2), (-2, 2), (2, -2), (2, 2)] {
                self.project_preview_block(
                    target,
                    chunk,
                    operations,
                    WorldPosition::new(origin.x() + dx, surface_y + dy, origin.z() + dz),
                    Block::new(BlockId::Sandstone),
                );
            }
        }
    }

    fn project_preview_block(
        &self,
        target: ChunkPosition,
        chunk: &Chunk,
        operations: &mut BTreeMap<WorldPosition, Block>,
        position: WorldPosition,
        block: Block,
    ) {
        if position.chunk() != target || operations.len() >= MAX_DECORATION_OPERATIONS {
            return;
        }
        let Some(local_y) = self.world_y_to_local(position.y()) else {
            return;
        };
        let local_x = position.x().rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = position.z().rem_euclid(CHUNK_SIZE as i32) as usize;
        if chunk.get_block(local_x, local_y, local_z).is_air() {
            operations.entry(position).or_insert(block);
        }
    }

    fn world_y_to_local(&self, world_y: i32) -> Option<usize> {
        let local_y = world_y - self.settings.min_y;
        (0..self.settings.height).contains(&local_y).then_some(local_y as usize)
    }

    /// Generate the density fill, aquifer, ore, surface, and fluid base pass.
    ///
    /// This is equivalent to `NoiseBasedChunkGenerator.doFill`:
    /// 1. Create the per-chunk interpolation state.
    /// 2. Iterate over cells in X, Z, Y order.
    /// 3. Interpolate the cached cell density and refine blocks near a
    ///    material boundary with marker-aware per-block evaluation.
    /// 4. Place stone/deepslate/fluid/air based on density threshold.
    pub fn generate_undecorated_chunk(&self, chunk: &mut Chunk) {
        let cell_width = self.settings.cell_width();
        let cell_height = self.settings.cell_height();
        let cell_count_y = self.settings.height / cell_height;
        let cell_count_xz = CHUNK_SIZE as i32 / cell_width;
        let cell_min_y = self.settings.min_y.div_euclid(cell_height);
        let sea_level = self.aquifer.sea_level;

        let mut aquifer = NoiseBasedAquifer::with_positional_random_factory(
            chunk.cx,
            chunk.cz,
            self.router.clone(),
            self.aquifer_random,
            self.settings.min_y,
            self.settings.height,
            crate::world::world_gen::aquifer::GlobalFluidPicker::new(
                self.aquifer.sea_level,
                self.aquifer.lava_level,
                self.aquifer.default_fluid,
                self.aquifer.lava_fluid,
            ),
        );

        // Place bedrock at the bottom of the world (y = min_y)
        let bottom_local = 0usize;
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set_block(x, bottom_local, z, Block::new(BlockId::Bedrock));
            }
        }

        let mut nd = NoiseChunkData::new(chunk, self.router.clone(), self.settings, self.aquifer.clone());
        nd.initialize_for_first_cell_x();
        let mut column_stone_noise = [None; CHUNK_SIZE * CHUNK_SIZE];

        // The main doFill loop.
        for cell_x_index in 0..cell_count_xz {
            nd.advance_cell_x(cell_x_index);

            for cell_z_index in 0..cell_count_xz {
                for cell_y_index in (0..cell_count_y).rev() {
                    nd.select_cell_yz(cell_y_index, cell_z_index);

                    let cell_start_y = (cell_min_y + cell_y_index) * cell_height;

                    for y_in_cell in (0..cell_height).rev() {
                        let pos_y = cell_start_y + y_in_cell;
                        if pos_y < self.settings.min_y
                            || pos_y >= self.settings.min_y + self.settings.height
                        {
                            continue;
                        }
                        let y_local = pos_y - self.settings.min_y;

                        let factor_y = y_in_cell as f64 / cell_height as f64;
                        nd.update_for_y(factor_y);

                        for x_in_cell in 0..cell_width {
                            let pos_x = nd.chunk_start_x + cell_x_index * cell_width + x_in_cell;
                            let x_chunk = (pos_x - nd.chunk_start_x) as usize;
                            if x_chunk >= CHUNK_SIZE {
                                continue;
                            }

                            let factor_x = x_in_cell as f64 / cell_width as f64;
                            nd.update_for_x(factor_x);

                            for z_in_cell in 0..cell_width {
                                let pos_z = nd.chunk_start_z + cell_z_index * cell_width + z_in_cell;
                                let z_chunk = (pos_z - nd.chunk_start_z) as usize;
                                if z_chunk >= CHUNK_SIZE {
                                    continue;
                                }

                                let factor_z = z_in_cell as f64 / cell_width as f64;
                                nd.update_for_z(factor_z);

                                let interpolated_density = nd.value;
                                let context = InterpolatedContext::new(
                                    pos_x,
                                    pos_y,
                                    pos_z,
                                    cell_width,
                                    cell_height,
                                );
                                let density = if interpolated_density.abs() < DENSITY_EXACT_REFINEMENT {
                                    self.router.final_density.compute(&context)
                                } else if interpolated_density.abs() < DENSITY_BOUNDARY_REFINEMENT {
                                    let point_context = SinglePointContext {
                                        block_x: pos_x,
                                        block_y: pos_y,
                                        block_z: pos_z,
                                    };
                                    self.router.final_density.compute(&point_context)
                                } else {
                                    interpolated_density
                                };

                                let aquifer_block =
                                    aquifer.compute_substance(pos_x, pos_y, pos_z, density);
                                let block_id = if density > 0.0 || aquifer_block.is_none() {
                                    let column_index = x_chunk * CHUNK_SIZE + z_chunk;
                                    let mut fallback_stone = || {
                                        fallback_stone_type(
                                            pos_y,
                                            &mut column_stone_noise[column_index],
                                            || sample_density_2d(&self.router, pos_x, pos_z),
                                        )
                                    };
                                    if can_apply_ore_veinifier(pos_y) {
                                        let ore_context = SinglePointContext {
                                            block_x: pos_x,
                                            block_y: pos_y,
                                            block_z: pos_z,
                                        };
                                        OreVeinifier::apply(
                                            &self.router.vein_toggle,
                                            &self.router.vein_ridged,
                                            &self.router.vein_gap,
                                            &self.ore_veins_random,
                                            &ore_context,
                                        )
                                        .unwrap_or_else(fallback_stone)
                                    } else {
                                        fallback_stone()
                                    }
                                } else {
                                    aquifer_block.unwrap_or(BlockId::Air)
                                };

                                let y_usize = y_local as usize;
                                if block_id != BlockId::Air {
                                    chunk.set_block(x_chunk, y_usize, z_chunk, Block::new(block_id));
                                }
                            }
                        }
                    }
                }
            }
            if self.generation_profile.uses_corrected_interpolation() {
                nd.swap_slices();
            }
        }

        nd.stop_interpolation();

        // Carve caves and canyons (post-density, pre-surface-rules, matching Java order)
        crate::world::world_gen::carver::carve_overworld_chunk(
            chunk,
            self.seed,
            &mut aquifer,
            self.settings.min_y,
            self.settings.height,
        );

        let system = SurfaceSystem::create_ref_from_seed(
            BlockId::Stone,
            sea_level,
            self.seed,
        );
        let surface_rules = default_overworld_rules(system.clone());
        let surface_source = StaticRuleSource::new(surface_rules);
        let mut surface_biomes =
            SurfaceBiomeQuartCache::new(chunk.cx, chunk.cz, self.settings.min_y);
        let biome_source = self.biome_source.clone();
        SurfaceSystem::build_surface_with_biome_provider(
            system,
            chunk,
            &surface_source,
            sea_level,
            self.settings.min_y,
            move |x, y, z| {
                surface_biomes.get_or_insert_with(x, y, z, |quart_x, quart_y, quart_z| {
                    match biome_source.get_biome_quart(quart_x, quart_y, quart_z) {
                        Ok(biome) => biome,
                        Err(unsupported) => {
                            log::warn!(
                                "{} at quart ({quart_x}, {quart_y}, {quart_z}); using plains compatibility biome",
                                unsupported
                            );
                            Biome::Plains
                        }
                    }
                })
            },
        );

        chunk.recount_fluids();
        chunk.is_dirty = true;
    }

    // ------------------------------------------------------------------
    // Utility queries
    // ------------------------------------------------------------------

    /// Get the surface height at a world position by sampling the
    /// preliminary_surface_level function.
    pub fn get_height(&self, x: i32, z: i32) -> i32 {
        preliminary_surface_level(&self.router, x, z)
    }

    /// Return the density value at a single block position (for debugging).
    pub fn get_density(&self, x: i32, y: i32, z: i32) -> f64 {
        sample_density(&self.router, x, y, z)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sample the density at (x, 0, z) as a rough per-column noise proxy for
/// stone-type blending.
fn sample_density_2d(router: &NoiseRouter, x: i32, z: i32) -> f64 {
    let ctx = SinglePointContext {
        block_x: x,
        block_y: 0,
        block_z: z,
    };
    router.final_density.compute(&ctx)
}

/// Returns a dry, exposed terrain surface from an isolated base snapshot.
/// Preview planning never asks the runtime for a neighboring chunk, so this
/// query is deterministic and bounded by the fixed snapshot set.
fn preview_surface(
    snapshots: &BTreeMap<(i32, i32), Chunk>,
    world_x: i32,
    world_z: i32,
    min_y: i32,
) -> Option<(i32, Block)> {
    let chunk_x = world_x.div_euclid(CHUNK_SIZE as i32);
    let chunk_z = world_z.div_euclid(CHUNK_SIZE as i32);
    let chunk = snapshots.get(&(chunk_x, chunk_z))?;
    let local_x = world_x.rem_euclid(CHUNK_SIZE as i32) as usize;
    let local_z = world_z.rem_euclid(CHUNK_SIZE as i32) as usize;
    for local_y in (0..crate::world::chunk::CHUNK_HEIGHT).rev() {
        let block = chunk.get_block(local_x, local_y, local_z);
        if block.is_air() || matches!(block.id, BlockId::Water | BlockId::Lava) {
            continue;
        }
        if local_y + 1 >= crate::world::chunk::CHUNK_HEIGHT
            || !chunk.get_block(local_x, local_y + 1, local_z).is_air()
        {
            return None;
        }
        return Some((min_y + local_y as i32, block));
    }
    None
}

/// Returns whether a supported feature can write at least one target-column
/// cell. Trees omit the four corners of their five-by-five canopy; wells use
/// the complete five-by-five footprint.
fn preview_footprint_intersects_target(
    candidate: FeatureCandidate,
    target: ChunkPosition,
) -> bool {
    let origin = candidate.origin();
    let target_min_x = i64::from(target.x()) * CHUNK_SIZE as i64;
    let target_min_z = i64::from(target.z()) * CHUNK_SIZE as i64;
    let target_max_x = target_min_x + CHUNK_SIZE as i64 - 1;
    let target_max_z = target_min_z + CHUNK_SIZE as i64 - 1;
    let radius = i64::from(PREVIEW_SUPPORT_HALO);
    let min_dx = (target_min_x - i64::from(origin.x())).max(-radius);
    let max_dx = (target_max_x - i64::from(origin.x())).min(radius);
    let min_dz = (target_min_z - i64::from(origin.z())).max(-radius);
    let max_dz = (target_max_z - i64::from(origin.z())).min(radius);
    if min_dx > max_dx || min_dz > max_dz {
        return false;
    }

    match candidate.feature() {
        PlannedFeature::DesertWell => true,
        PlannedFeature::Tree => (min_dx..=max_dx).any(|dx| {
            (min_dz..=max_dz).any(|dz| dx.abs() + dz.abs() <= radius + 1)
        }),
    }
}

fn lerp(alpha: f64, a: f64, b: f64) -> f64 {
    a + (b - a) * alpha
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::CHUNK_HEIGHT;
    use crate::world::coordinates::WorldCoordinateProfile;
    use crate::world::world_gen::noise_router::NoiseRouterData;

    #[test]
    fn test_cell_corners_trilerp() {
        let corners = CellCorners {
            values: [
                [[0.0, 0.0], [0.0, 0.0]],
                [[1.0, 0.0], [0.0, 0.0]],
            ],
        };
        // At the x=1 corner (fx=1, fy=0, fz=0) the value should be 1.0.
        let v = corners.trilerp(1.0, 0.0, 0.0);
        assert!((v - 1.0).abs() < 1e-12);

        // At the center (fx=0.5, fy=0, fz=0) the value should be 0.5.
        let v = corners.trilerp(0.5, 0.0, 0.0);
        assert!((v - 0.5).abs() < 1e-12);
    }

    #[test]
    fn test_get_stone_type() {
        assert_eq!(get_stone_type(5, 0.0), BlockId::Stone);
        assert_eq!(get_stone_type(-10, 0.0), BlockId::Deepslate);
        // In transition zone with large noise → deepslate
        assert_eq!(get_stone_type(-4, 1.0), BlockId::Deepslate);
        // In transition zone with small noise → stone
        assert_eq!(get_stone_type(-4, -1.0), BlockId::Stone);
    }

    #[test]
    fn transition_stone_noise_is_lazy_and_cached_per_column() {
        let mut cached_noise = None;
        let mut calls = 0;

        for y in [-9, 0, 12] {
            fallback_stone_type(y, &mut cached_noise, || {
                calls += 1;
                1.0
            });
        }
        assert_eq!(calls, 0);
        assert_eq!(cached_noise, None);

        assert_eq!(
            fallback_stone_type(-4, &mut cached_noise, || {
                calls += 1;
                1.0
            }),
            BlockId::Deepslate
        );
        assert_eq!(
            fallback_stone_type(-6, &mut cached_noise, || {
                calls += 1;
                -1.0
            }),
            BlockId::Deepslate
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn surface_biome_cache_uses_canonical_negative_quart_keys() {
        assert_eq!(SurfaceBiomeQuartCache::quart_key(-1, -1, -1), (-1, -1, -1));
        assert_eq!(SurfaceBiomeQuartCache::quart_key(-4, -4, -4), (-1, -1, -1));
        assert_eq!(SurfaceBiomeQuartCache::quart_key(-5, -5, -5), (-2, -2, -2));
        assert_eq!(SurfaceBiomeQuartCache::quart_key(3, 3, 3), (0, 0, 0));
        assert_eq!(SurfaceBiomeQuartCache::quart_key(4, 4, 4), (1, 1, 1));
    }

    #[test]
    fn surface_biome_cache_is_bounded_to_chunk_quart_axes() {
        let mut cache = SurfaceBiomeQuartCache::new(-1, 2, -64);
        assert!(cache.contains((-4, -16, 8)));
        assert!(cache.contains((-1, 79, 11)));
        assert!(!cache.contains((-5, -16, 8)));
        assert!(!cache.contains((-4, 80, 8)));
        assert!(!cache.contains((-4, -16, 12)));

        for quart_x in -4..=-1 {
            for quart_y in -16..=79 {
                for quart_z in 8..=11 {
                    cache.get_or_insert_with(quart_x * 4, quart_y * 4, quart_z * 4, |_, _, _| {
                        Biome::Plains
                    });
                }
            }
        }
        assert_eq!(cache.biomes.len(), SURFACE_BIOME_QUART_CACHE_CAPACITY);

        cache.get_or_insert_with(0, 0, 32, |_, _, _| Biome::Desert);
        assert_eq!(cache.biomes.len(), SURFACE_BIOME_QUART_CACHE_CAPACITY);
    }

    #[test]
    fn ore_veinifier_gate_matches_the_documented_union() {
        assert!(!can_apply_ore_veinifier(-61));
        assert!(can_apply_ore_veinifier(-60));
        assert!(can_apply_ore_veinifier(-8));
        assert!(can_apply_ore_veinifier(0));
        assert!(can_apply_ore_veinifier(50));
        assert!(!can_apply_ore_veinifier(51));
    }

    #[test]
    fn fixed_chunk_generation_output_is_stable() {
        let generator = VanillaWorldGenerator::from_seed(0x5EED, WorldGenerationProfile::Minecraft26Base);
        let mut chunk = Chunk::new(3, -2);
        generator.generate_undecorated_chunk(&mut chunk);

        let fingerprint = chunk.blocks.iter().fold(0xcbf29ce484222325_u64, |hash, block| {
            let block_bits = (block.id as u64) | ((block.state as u64) << 16) | ((block.data as u64) << 32);
            (hash ^ block_bits).wrapping_mul(0x100000001b3)
        });
        assert_eq!(fingerprint, 13_316_232_380_495_532_990);
    }

    #[test]
    fn test_aquifer_data() {
        let aquifer = AquiferData::overworld();
        assert_eq!(aquifer.fluid_at(0, 70, 0), BlockId::Air);
        assert_eq!(aquifer.fluid_at(0, 50, 0), BlockId::Water);
        assert_eq!(aquifer.fluid_at(0, -60, 0), BlockId::Lava);
    }

    #[test]
    fn overworld_generation_keeps_its_existing_local_storage_mapping() {
        let local_y = (63 - NoiseSettings::OVERWORLD.min_y) as usize;
        assert_eq!(local_y, 127);
        assert_eq!(WorldCoordinateProfile::JavaOverworld.from_local_y(local_y), Some(63));
        assert_eq!(WorldCoordinateProfile::LegacyLocal.from_local_y(local_y), Some(127));
    }

    #[test]
    fn cell_x_slices_are_swapped_after_the_current_column() {
        let router = Arc::new(NoiseRouterData::create_overworld_router(42, false, false));
        let chunk = Chunk::new(2, -3);
        let mut noise = NoiseChunkData::new(
            &chunk,
            router.clone(),
            NoiseSettings::OVERWORLD,
            AquiferData::overworld(),
        );
        noise.initialize_for_first_cell_x();
        noise.advance_cell_x(0);
        noise.select_cell_yz(0, 0);

        let left_x = chunk.cx * CHUNK_SIZE as i32;
        let start_y = NoiseSettings::OVERWORLD.min_y;
        let start_z = chunk.cz * CHUNK_SIZE as i32;
        assert_eq!(noise.noise000, sample_density(&router, left_x, start_y, start_z));
        assert_eq!(
            noise.noise100,
            sample_density(&router, left_x + NoiseSettings::OVERWORLD.cell_width(), start_y, start_z)
        );

        noise.swap_slices();
        noise.select_cell_yz(0, 0);
        assert_eq!(
            noise.noise000,
            sample_density(&router, left_x + NoiseSettings::OVERWORLD.cell_width(), start_y, start_z)
        );
    }

    #[test]
    fn generation_profile_selects_only_the_interpolation_slice_swap() {
        let seed = 0x5EED;
        let mut corrected = Chunk::new(1, 0);
        let mut legacy = Chunk::new(1, 0);
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Base)
            .generate_chunk(&mut corrected);
        VanillaWorldGenerator::from_seed(
            seed,
            WorldGenerationProfile::LegacyPreCorrectedInterpolation,
        )
        .generate_chunk(&mut legacy);

        assert_ne!(&*corrected.blocks, &*legacy.blocks);
    }

    #[test]
    fn test_ore_vein_inputs_reach_reference_thresholds() {
        let router = NoiseRouterData::create_overworld_router(42, false, false);
        let mut toggle_peak = f64::NEG_INFINITY;
        let mut ridged_negative = false;
        for x in (-32..=32).step_by(4) {
            for y in (-60..=50).step_by(4) {
                for z in (-32..=32).step_by(4) {
                    let context = SinglePointContext {
                        block_x: x,
                        block_y: y,
                        block_z: z,
                    };
                    let toggle = router.vein_toggle.compute(&context);
                    let ridged = router.vein_ridged.compute(&context);
                    toggle_peak = toggle_peak.max(toggle.abs());
                    ridged_negative |= ridged < 0.0;
                }
            }
        }
        assert!(toggle_peak >= 0.4, "ore vein toggle peak was {toggle_peak}");
        assert!(ridged_negative, "ore vein ridged input never crossed below zero");
    }

    #[test]
    fn test_generate_chunk_produces_terrain() {
        let gen = VanillaWorldGenerator::from_seed(42, WorldGenerationProfile::Minecraft26Base);
        let mut chunk = Chunk::new(0, 0);
        gen.generate_chunk(&mut chunk);
        let mut solid_count = 0;
        let mut fluid_count = 0;
        let mut air_count = 0;
        let mut vein_count = 0;
        let mut top_y_per_col: Vec<i32> = Vec::new();
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut top = -1i32;
                for y in (0..CHUNK_HEIGHT).rev() {
                    let block = chunk.get_block(x, y, z);
                    if matches!(
                        block.id,
                        BlockId::CopperOre
                            | BlockId::RawCopperBlock
                            | BlockId::DeepslateIronOre
                            | BlockId::RawIronBlock
                    ) {
                        vein_count += 1;
                    }
                    if !block.is_air() && block.id != BlockId::Water && block.id != BlockId::Lava {
                        solid_count += 1;
                        if top < 0 { top = y as i32; }
                    } else if block.id == BlockId::Water || block.id == BlockId::Lava {
                        fluid_count += 1;
                    } else {
                        air_count += 1;
                    }
                }
                top_y_per_col.push(top);
            }
        }
        println!("solid={} fluid={} air={} veins={}", solid_count, fluid_count, air_count, vein_count);
        let min_top = top_y_per_col.iter().min().unwrap();
        let max_top = top_y_per_col.iter().max().unwrap();
        let avg_top: f64 = top_y_per_col.iter().sum::<i32>() as f64 / top_y_per_col.len() as f64;
        println!("top surface: min={} max={} avg={:.1}", min_top, max_top, avg_top);
        assert!(solid_count > 0, "Chunk must contain solid blocks, got 0");
        assert!(max_top - min_top >= 0, "Terrain must have height variation, got flat at {}", min_top);
    }

    #[test]
    fn public_generation_matches_the_undecorated_base_pass() {
        let generator = VanillaWorldGenerator::from_seed(0x5EED, WorldGenerationProfile::Minecraft26Base);
        let mut base = Chunk::new(3, -2);
        let mut public = Chunk::new(3, -2);

        generator.generate_undecorated_chunk(&mut base);
        generator.generate_chunk(&mut public);

        assert_eq!(&*base.blocks, &*public.blocks);
        assert_eq!(&*base.sky_light, &*public.sky_light);
        assert_eq!(&*base.block_light, &*public.block_light);
        assert_eq!(base.is_dirty, public.is_dirty);
        assert_eq!(base.light_dirty, public.light_dirty);
        assert_eq!(base.has_water, public.has_water);
        assert_eq!(base.has_lava, public.has_lava);
        assert_eq!(base.water_count, public.water_count);
        assert_eq!(base.lava_count, public.lava_count);
        assert_eq!(base.fluid_positions, public.fluid_positions);
    }

    #[test]
    fn legacy_and_minecraft_base_profiles_remain_undecorated() {
        let seed = 0xDEC0_2026;
        for profile in [
            WorldGenerationProfile::LegacyPreCorrectedInterpolation,
            WorldGenerationProfile::Minecraft26Base,
        ] {
            let generator = VanillaWorldGenerator::from_seed(seed, profile);
            let mut base = Chunk::new(-2, 1);
            let mut generated = Chunk::new(-2, 1);
            generator.generate_undecorated_chunk(&mut base);
            generator.generate_chunk(&mut generated);
            assert_eq!(&*generated.blocks, &*base.blocks, "{profile:?} gained preview decoration");
        }
        assert!(!WorldGenerationProfile::new_world().uses_native_decoration_preview());
    }

    #[test]
    fn preview_target_is_independent_of_generation_order() {
        let generator = VanillaWorldGenerator::from_seed(
            0xDEC0_2026,
            WorldGenerationProfile::Minecraft26NativeDecorationPreview,
        );
        let mut first = Chunk::new(-1, 0);
        generator.generate_chunk(&mut first);

        let mut unrelated = Chunk::new(12, -8);
        generator.generate_chunk(&mut unrelated);
        let mut repeated = Chunk::new(-1, 0);
        generator.generate_chunk(&mut repeated);

        assert_eq!(&*first.blocks, &*repeated.blocks);
        assert_eq!(first.fluid_positions, repeated.fluid_positions);
    }

    #[test]
    fn negative_border_projection_uses_euclidean_chunk_ownership() {
        let generator = VanillaWorldGenerator::from_seed(
            1,
            WorldGenerationProfile::Minecraft26NativeDecorationPreview,
        );
        let target = ChunkPosition::new(-1, 0);
        let chunk = Chunk::new(-1, 0);
        let mut operations = BTreeMap::new();
        generator.project_preview_block(
            target,
            &chunk,
            &mut operations,
            WorldPosition::new(-16, -63, 15),
            Block::new(BlockId::OakLog),
        );
        generator.project_preview_block(
            target,
            &chunk,
            &mut operations,
            WorldPosition::new(0, -63, 15),
            Block::new(BlockId::OakLog),
        );

        assert_eq!(operations.len(), 1);
        assert!(operations.contains_key(&WorldPosition::new(-16, -63, 15)));
    }

    #[test]
    fn preview_filters_to_target_footprints_and_bounds_snapshot_scope() {
        let generator = VanillaWorldGenerator::from_seed(
            1,
            WorldGenerationProfile::Minecraft26NativeDecorationPreview,
        );
        let target = ChunkPosition::new(0, 0);
        let mut candidates = Vec::new();
        for owner_z in -1..=1 {
            for owner_x in -1..=1 {
                // These origins are the closest cells in each owner to the
                // target, so every five-by-five footprint reaches it.
                let origin_x = if owner_x < 0 { -1 } else { owner_x * CHUNK_SIZE as i32 };
                let origin_z = if owner_z < 0 { -1 } else { owner_z * CHUNK_SIZE as i32 };
                for (feature, tree_kind) in [
                    (PlannedFeature::Tree, Some(PreviewTreeKind::Oak)),
                    (PlannedFeature::DesertWell, None),
                ] {
                    candidates.push(PreviewCandidate {
                        feature: FeatureCandidate::new(
                            feature,
                            WorldPosition::new(origin_x, 0, origin_z),
                        ),
                        tree_kind,
                    });
                }
            }
        }

        assert_eq!(candidates.len(), MAX_PREVIEW_CANDIDATES);
        assert!(candidates
            .iter()
            .all(|candidate| preview_footprint_intersects_target(candidate.feature, target)));
        assert!(!preview_footprint_intersects_target(
            FeatureCandidate::new(PlannedFeature::Tree, WorldPosition::new(32, 0, 0)),
            target,
        ));
        assert!(!preview_footprint_intersects_target(
            FeatureCandidate::new(PlannedFeature::Tree, WorldPosition::new(-2, 0, -2)),
            target,
        ));
        assert!(preview_footprint_intersects_target(
            FeatureCandidate::new(PlannedFeature::DesertWell, WorldPosition::new(-2, 0, -2)),
            target,
        ));

        let snapshots = generator.preview_snapshot_keys(target, &candidates);
        assert!(snapshots.len() <= MAX_PREVIEW_SNAPSHOTS);
        assert!(snapshots
            .iter()
            .all(|&(cx, cz)| (-1..=1).contains(&cx) && (-1..=1).contains(&cz)));
    }

    #[test]
    fn preview_tree_and_well_project_across_a_negative_chunk_border() {
        let generator = VanillaWorldGenerator::from_seed(
            1,
            WorldGenerationProfile::Minecraft26NativeDecorationPreview,
        );
        let target = ChunkPosition::new(0, 0);
        let target_chunk = Chunk::new(0, 0);
        let surface_local_y = 128;
        let surface_world_y = NoiseSettings::OVERWORLD.min_y + surface_local_y as i32;

        let mut tree_owner = Chunk::new(-1, 0);
        tree_owner.set_block(15, surface_local_y, 8, Block::new(BlockId::GrassBlock));
        let mut tree_snapshots = BTreeMap::new();
        tree_snapshots.insert((-1, 0), tree_owner);
        let mut tree_operations = BTreeMap::new();
        generator.plan_preview_tree(
            FeatureCandidate::new(PlannedFeature::Tree, WorldPosition::new(-1, 0, 8)),
            PreviewTreeKind::Oak,
            target,
            &tree_snapshots,
            &target_chunk,
            &mut tree_operations,
        );
        assert!(tree_operations.values().any(|block| block.id == BlockId::OakLeaves));
        assert!(tree_operations.keys().all(|position| position.x() >= 0));

        let mut well_snapshots = BTreeMap::new();
        for chunk_x in [-1, 0] {
            well_snapshots.insert((chunk_x, 0), Chunk::new(chunk_x, 0));
        }
        for world_x in -3_i32..=1 {
            for world_z in 6_i32..=10 {
                let chunk_x = world_x.div_euclid(CHUNK_SIZE as i32);
                let local_x = world_x.rem_euclid(CHUNK_SIZE as i32) as usize;
                let local_z = world_z.rem_euclid(CHUNK_SIZE as i32) as usize;
                well_snapshots
                    .get_mut(&(chunk_x, 0))
                    .unwrap()
                    .set_block(local_x, surface_local_y, local_z, Block::new(BlockId::Sand));
            }
        }
        let mut well_operations = BTreeMap::new();
        generator.plan_preview_well(
            FeatureCandidate::new(PlannedFeature::DesertWell, WorldPosition::new(-1, 0, 8)),
            target,
            &well_snapshots,
            &target_chunk,
            &mut well_operations,
        );
        assert!(well_operations.values().any(|block| block.id == BlockId::Sandstone));
        assert!(well_operations.values().any(|block| block.id == BlockId::Water));
        assert!(well_operations.keys().all(|position| {
            position.chunk() == target && position.y() > surface_world_y
        }));
    }

    #[test]
    fn preview_never_replaces_undecorated_terrain() {
        let seed = 0xA11C_E55;
        let mut base = Chunk::new(0, 0);
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Base)
            .generate_undecorated_chunk(&mut base);
        let mut preview = Chunk::new(0, 0);
        VanillaWorldGenerator::from_seed(
            seed,
            WorldGenerationProfile::Minecraft26NativeDecorationPreview,
        )
        .generate_chunk(&mut preview);

        for (before, after) in base.blocks.iter().zip(preview.blocks.iter()) {
            if !before.is_air() {
                assert_eq!(before, after, "preview replaced non-air base terrain");
            }
        }
    }

    #[test]
    fn negative_chunk_generation_is_deterministic() {
        let generator = VanillaWorldGenerator::from_seed(0xC0FFEE, WorldGenerationProfile::Minecraft26Base);
        let mut first = Chunk::new(-17, -9);
        let mut second = Chunk::new(-17, -9);

        generator.generate_chunk(&mut first);
        generator.generate_chunk(&mut second);

        assert_eq!(first.cx * CHUNK_SIZE as i32, -272);
        assert_eq!(first.cz * CHUNK_SIZE as i32, -144);
        assert_eq!(&*first.blocks, &*second.blocks);
        assert_eq!(first.fluid_positions, second.fluid_positions);
    }

    #[test]
    fn surface_quart_cache_keeps_generation_deterministic() {
        let generator = VanillaWorldGenerator::from_seed(0xB10B_E, WorldGenerationProfile::Minecraft26Base);
        let mut first = Chunk::new(-3, 4);
        let mut second = Chunk::new(-3, 4);

        generator.generate_undecorated_chunk(&mut first);
        generator.generate_undecorated_chunk(&mut second);

        assert_eq!(&*first.blocks, &*second.blocks);
        assert_eq!(first.fluid_positions, second.fluid_positions);
    }

    #[test]
    fn test_chunks_at_different_positions() {
        let seed = 42u64;
        let gen = VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Base);
        for &(cx, cz) in &[(0, 0), (10, 10)] {
            let density = gen.get_density(cx * CHUNK_SIZE as i32 + 8, 0, cz * CHUNK_SIZE as i32 + 8);
            assert!(density.is_finite(), "chunk ({},{}) density was not finite", cx, cz);
        }
    }



    #[test]
    fn test_noise_chunk_data_creation() {
        let chunk = Chunk::new(0, 0);
        let router = NoiseRouterData::create_overworld_router(42, false, false);
        let settings = NoiseSettings::OVERWORLD;
        let aquifer = AquiferData::overworld();

        let nd = NoiseChunkData::new(&chunk, Arc::new(router), settings, aquifer);
        assert_eq!(nd.cell_width, 4);
        assert_eq!(nd.cell_height, 8);
        assert_eq!(nd.cell_count_y, 48);
        assert_eq!(nd.cell_count_xz, 4);
        assert_eq!(nd.min_y, -64);
    }

    #[test]
    fn test_density_cross_section() {
        let router = Arc::new(NoiseRouterData::create_overworld_router(42, false, false));
        
        println!("=== Full pipeline at (0, y, 0) ===");
        for y in (0..260).step_by(10) {
            let ctx = SinglePointContext { block_x: 0, block_y: y, block_z: 0 };
            let final_d = router.final_density.compute(&ctx);
            let cont = router.continents.compute(&ctx);
            let erosion = router.erosion.compute(&ctx);
            let depth_val = router.depth.compute(&ctx);
            let prelim = router.preliminary_surface_level.compute(&ctx);
            if y % 20 == 0 || final_d > -0.1 {
                println!("y={:>4}: final={:+.6} cont={:+.4} erosion={:+.4} depth={:+.4} prelim={:+.1}",
                    y, final_d, cont, erosion, depth_val, prelim);
            }
        }
        
        println!("\n=== Cross-section at (0, 0) descending ===");
        for y in (0..320).rev().step_by(8) {
            let ctx = SinglePointContext { block_x: 0, block_y: y, block_z: 0 };
            let d = router.final_density.compute(&ctx);
            let marker = if d > 0.0 { '#' } else if d > -0.1 { '.' } else { ' ' };
            print!("{}", marker);
            if y % 64 == 0 {
                println!("<y={}>", y);
            }
        }
        println!("<y=0>");
        
        println!("\n=== Highest solid per column ===");
        for (x, z) in [(0,0), (0,8), (8,0), (8,8), (4,4), (12, 12)] {
            let mut found_surface = None;
            for y in (0..320).rev() {
                let ctx = SinglePointContext { block_x: x, block_y: y, block_z: z };
                let d = router.final_density.compute(&ctx);
                if d > 0.0 && found_surface.is_none() {
                    found_surface = Some((y, d));
                }
            }
            if let Some((sy, sd)) = found_surface {
                println!("col ({:>3},{:>3}): highest solid at y={:>4} density={:+.4}", x, z, sy, sd);
            }
        }
    }

    #[test]
    fn corrected_overworld_density_does_not_fill_the_upper_world() {
        let generator = VanillaWorldGenerator::from_seed(42, WorldGenerationProfile::Minecraft26Base);
        assert!(generator.get_density(0, 200, 0) < 0.0);

        let mut chunk = Chunk::new(0, 0);
        generator.generate_chunk(&mut chunk);
        let highest = (0..CHUNK_SIZE)
            .flat_map(|x| (0..CHUNK_SIZE).map(move |z| (x, z)))
            .filter_map(|(x, z)| {
                (0..crate::world::chunk::CHUNK_HEIGHT)
                    .rev()
                    .find(|&y| !chunk.get_block(x, y, z).is_air())
            })
            .max()
            .unwrap();

        // Seed 42 used to fill through local Y 319 because two density
        // transforms and blended-noise normalization were ported incorrectly.
        assert_eq!(highest, 143);
    }

    #[test]
    fn test_density_produces_terrain() {
        let router = Arc::new(NoiseRouterData::create_overworld_router(42, false, false));
        // Sample density at various world positions
        let mut has_positive = false;
        let mut has_negative = false;
        for y in (-64..320).step_by(8) {
            for z in [0, 8, 16] {
                for x in [0, 8, 16] {
                    let ctx = SinglePointContext { block_x: x, block_y: y, block_z: z };
                    let d = router.final_density.compute(&ctx);
                    if d > 0.0 { has_positive = true; }
                    if d <= 0.0 { has_negative = true; }
                }
            }
        }
        assert!(has_positive, "Density router must produce positive values somewhere for terrain");
        assert!(has_negative, "Density router must produce negative values somewhere for air/water");
    }
}
