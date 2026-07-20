//! Port of Minecraft's SurfaceRules and SurfaceSystem.
//!
//! Corresponding Java classes:
//! - `net.minecraft.world.level.levelgen.SurfaceRules`
//! - `net.minecraft.world.level.levelgen.SurfaceSystem`
//! - `net.minecraft.world.level.levelgen.OreVeinifier`

#![allow(dead_code)]

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use crate::world::world_gen::density_fn::{DensityFunction, FunctionContext, NoiseHandle};
use crate::world::world_gen::noise::{NoiseKey, NormalNoise, NoiseSeed, PositionalRandomFactory};
use crate::world::world_gen::noise_router::NoiseRouter;
use crate::world::world_gen::structures::JavaLegacyRandom;
use crate::world::world_gen::Biome;
use std::sync::{Arc, Once, OnceLock};

/// Reference branches whose Java output cannot be represented by the current
/// native block/state model. They are preserved in-place rather than replaced
/// with a visually similar block.
pub const MINECRAFT26_REFERENCE_SURFACE_UNSUPPORTED: &[&str] = &[
    "minecraft:sulfur_caves surface bands require minecraft:cinnabar and minecraft:sulfur",
    "badlands cave ceilings require minecraft:red_sandstone (only smooth_red_sandstone is registered)",
    "minecraft:blue_ice is a placed-feature output, not a SurfaceRuleData/SurfaceSystem surface branch",
];

static REPORT_REFERENCE_SURFACE_UNSUPPORTED: Once = Once::new();

pub fn report_minecraft26_reference_surface_unsupported() {
    REPORT_REFERENCE_SURFACE_UNSUPPORTED.call_once(|| {
        for branch in MINECRAFT26_REFERENCE_SURFACE_UNSUPPORTED {
            log::warn!("unsupported Minecraft 26.2 reference surface branch: {branch}");
        }
    });
}

/// Ordinary fallback materials may reach this far below the preliminary
/// surface. The current preliminary surface comes from the chunk heightmap;
/// a later data-driven cave rule can use `DeepCaveException` instead.
const ORDINARY_SURFACE_MAX_DEPTH_BELOW_PRELIMINARY: i32 = 8;

// ============================================================================
// CaveSurface
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaveSurface {
    Floor,
    Ceiling,
}

/// Controls whether a surface replacement is constrained to the preliminary
/// terrain surface. Deep cave-specific rules are intentionally a separate
/// scope so they are not permanently hidden by the ordinary fallback gate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SurfaceRuleScope {
    Ordinary,
    DeepCaveException,
}

// ============================================================================
// VerticalAnchor
// ============================================================================

#[derive(Clone, Copy, Debug)]
pub enum VerticalAnchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
}

impl VerticalAnchor {
    pub fn resolve(&self, min_y: i32, height: i32) -> i32 {
        match self {
            VerticalAnchor::Absolute(v) => *v,
            VerticalAnchor::AboveBottom(v) => min_y + v,
            VerticalAnchor::BelowTop(v) => min_y + height - 1 + v,
        }
    }
}

// ============================================================================
// Core traits (Ex versions — receive &mut SurfaceContext)
// ============================================================================

pub trait ConditionEx: Send + Sync {
    fn test(&self, ctx: &mut SurfaceContext) -> bool;
    fn clone_box(&self) -> Box<dyn ConditionEx>;
}

pub trait SurfaceRuleEx: Send + Sync {
    fn try_apply(&self, ctx: &mut SurfaceContext, x: i32, y: i32, z: i32) -> Option<BlockId>;
    fn clone_box(&self) -> Box<dyn SurfaceRuleEx>;
}

pub trait ConditionSourceEx: Send + Sync {
    fn create(&self) -> Box<dyn ConditionEx>;
}

pub trait RuleSourceEx: Send + Sync {
    fn create(&self) -> Box<dyn SurfaceRuleEx>;
}

/// Adapts an already-built rule tree to the source interface used by the
/// surface pipeline. This keeps rule construction separate from chunk-local
/// biome callbacks.
pub struct StaticRuleSource {
    rule: Box<dyn SurfaceRuleEx>,
}

impl StaticRuleSource {
    pub fn new(rule: Box<dyn SurfaceRuleEx>) -> Self {
        Self { rule }
    }
}

impl RuleSourceEx for StaticRuleSource {
    fn create(&self) -> Box<dyn SurfaceRuleEx> {
        self.rule.clone()
    }
}

// ============================================================================
// Clone for trait-object boxes
// ============================================================================

impl Clone for Box<dyn ConditionEx> {
    fn clone(&self) -> Self {
        ConditionEx::clone_box(self.as_ref())
    }
}

impl Clone for Box<dyn SurfaceRuleEx> {
    fn clone(&self) -> Self {
        SurfaceRuleEx::clone_box(self.as_ref())
    }
}

// ============================================================================
// SurfaceContext
// ============================================================================

pub struct SurfaceContext {
    pub(crate) system: Arc<SurfaceSystemRef>,

    // Per-column state (updated via update_xz)
    pub(crate) block_x: i32,
    pub(crate) block_z: i32,
    pub(crate) surface_depth: i32,
    pub(crate) surface_secondary: f64,
    pub(crate) min_surface_level: i32,
    pub(crate) last_update_xz: u64,
    pub(crate) last_surface_secondary_xz: u64,
    pub(crate) last_min_surface_level_xz: u64,

    // Per-block state (updated via update_y)
    pub(crate) block_y: i32,
    pub(crate) water_height: i32,
    pub(crate) stone_depth_above: i32,
    pub(crate) stone_depth_below: i32,
    pub(crate) current_block: BlockId,
    pub(crate) biome: Option<Biome>,
    pub(crate) last_update_y: u64,

    // Sea level
    pub(crate) sea_level: i32,
    pub(crate) world_min_y: i32,
    pub(crate) world_height: i32,
    // Height getter (wx, wy, wz) -> biome
    pub(crate) get_height: Box<dyn FnMut(i32, i32, i32) -> Biome + Send>,
    // Preliminary surface level getter (x, z) -> y
    pub(crate) get_preliminary_surface: Box<dyn Fn(i32, i32) -> i32 + Send + Sync>,
    pub(crate) reference_preliminary_interpolation: bool,
    pub(crate) world_surface_heightmap: Option<Vec<i32>>,
}

impl SurfaceContext {
    pub fn new(
        system: Arc<SurfaceSystemRef>,
        sea_level: i32,
        get_height: Box<dyn FnMut(i32, i32, i32) -> Biome + Send>,
        get_preliminary_surface: Box<dyn Fn(i32, i32) -> i32 + Send + Sync>,
    ) -> Self {
        Self::new_with_height(system, sea_level, 0, CHUNK_HEIGHT as i32, get_height, get_preliminary_surface)
    }

    pub fn new_with_height(
        system: Arc<SurfaceSystemRef>,
        sea_level: i32,
        world_min_y: i32,
        world_height: i32,
        get_height: Box<dyn FnMut(i32, i32, i32) -> Biome + Send>,
        get_preliminary_surface: Box<dyn Fn(i32, i32) -> i32 + Send + Sync>,
    ) -> Self {
        SurfaceContext {
            system,
            block_x: 0,
            block_z: 0,
            surface_depth: 0,
            surface_secondary: 0.0,
            min_surface_level: 0,
            last_update_xz: 0,
            last_surface_secondary_xz: u64::MAX,
            last_min_surface_level_xz: u64::MAX,
            block_y: 0,
            water_height: i32::MIN,
            stone_depth_above: 0,
            stone_depth_below: 0,
            current_block: BlockId::Stone,
            biome: None,
            last_update_y: 0,
            sea_level,
            world_min_y,
            world_height,
            get_height,
            get_preliminary_surface,
            reference_preliminary_interpolation: false,
            world_surface_heightmap: None,
        }
    }

    pub fn update_xz(&mut self, block_x: i32, block_z: i32) {
        self.last_update_xz += 1;
        self.block_x = block_x;
        self.block_z = block_z;
        self.surface_depth = self.system.get_surface_depth(block_x, block_z);
        self.last_surface_secondary_xz = self.last_update_xz.wrapping_sub(1);
        self.last_min_surface_level_xz = self.last_update_xz.wrapping_sub(1);
    }

    pub fn update_y(&mut self, stone_depth_above: i32, stone_depth_below: i32, water_height: i32, block_y: i32) {
        self.last_update_y += 1;
        self.biome = None;
        self.block_y = block_y;
        self.water_height = water_height;
        self.stone_depth_above = stone_depth_above;
        self.stone_depth_below = stone_depth_below;
    }

    fn update_y_with_block(
        &mut self,
        stone_depth_above: i32,
        stone_depth_below: i32,
        water_height: i32,
        block_y: i32,
        current_block: BlockId,
    ) {
        self.update_y(stone_depth_above, stone_depth_below, water_height, block_y);
        self.current_block = current_block;
    }

    pub fn get_surface_secondary(&mut self) -> f64 {
        if self.last_surface_secondary_xz != self.last_update_xz {
            self.last_surface_secondary_xz = self.last_update_xz;
            self.surface_secondary = self.system.get_surface_secondary(self.block_x, self.block_z);
        }
        self.surface_secondary
    }

    pub fn get_biome(&mut self, y: i32) -> Biome {
        if self.biome.is_none() {
            self.biome = Some((self.get_height)(self.block_x, y, self.block_z));
        }
        self.biome.unwrap()
    }

    pub fn get_min_surface_level(&mut self) -> i32 {
        if self.last_min_surface_level_xz != self.last_update_xz {
            self.last_min_surface_level_xz = self.last_update_xz;
            let prelim = if self.reference_preliminary_interpolation {
                let cell_x = self.block_x.div_euclid(16);
                let cell_z = self.block_z.div_euclid(16);
                let x0 = cell_x * 16;
                let z0 = cell_z * 16;
                let fx = (self.block_x.rem_euclid(16) as f32 / 16.0) as f64;
                let fz = (self.block_z.rem_euclid(16) as f32 / 16.0) as f64;
                let p00 = (self.get_preliminary_surface)(x0, z0) as f64;
                let p10 = (self.get_preliminary_surface)(x0 + 16, z0) as f64;
                let p01 = (self.get_preliminary_surface)(x0, z0 + 16) as f64;
                let p11 = (self.get_preliminary_surface)(x0 + 16, z0 + 16) as f64;
                let north = p00 + fx * (p10 - p00);
                let south = p01 + fx * (p11 - p01);
                (north + fz * (south - north)).floor() as i32
            } else {
                (self.get_preliminary_surface)(self.block_x, self.block_z)
            };
            self.min_surface_level = prelim + self.surface_depth - 8;
        }
        self.min_surface_level
    }

    fn permits_surface_rule(&mut self, scope: SurfaceRuleScope) -> bool {
        match scope {
            SurfaceRuleScope::Ordinary => {
                let preliminary_surface = (self.get_preliminary_surface)(self.block_x, self.block_z);
                self.block_y >= preliminary_surface - ORDINARY_SURFACE_MAX_DEPTH_BELOW_PRELIMINARY
            }
            SurfaceRuleScope::DeepCaveException => true,
        }
    }
}

// ============================================================================
// Concrete ConditionEx implementations
// ============================================================================

// --- BiomeCondition ---

#[derive(Clone)]
pub struct BiomeConditionEx {
    pub biomes: Vec<Biome>,
}

impl BiomeConditionEx {
    pub fn new(biomes: Vec<Biome>) -> Self {
        BiomeConditionEx { biomes }
    }
}

impl ConditionEx for BiomeConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let biome = ctx.get_biome(ctx.block_y);
        self.biomes.contains(&biome)
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- StoneDepthCondition ---

#[derive(Clone)]
pub struct StoneDepthConditionEx {
    offset: i32,
    add_surface_depth: bool,
    secondary_depth_range: i32,
    surface_type: CaveSurface,
}

impl StoneDepthConditionEx {
    pub fn new(offset: i32, add_surface_depth: bool, secondary_depth_range: i32, surface_type: CaveSurface) -> Self {
        StoneDepthConditionEx {
            offset,
            add_surface_depth,
            secondary_depth_range,
            surface_type,
        }
    }
}

impl ConditionEx for StoneDepthConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let ceiling = self.surface_type == CaveSurface::Ceiling;
        let stone_depth = if ceiling { ctx.stone_depth_below } else { ctx.stone_depth_above };
        let surface_depth = if self.add_surface_depth { ctx.surface_depth } else { 0 };
        let secondary = if self.secondary_depth_range == 0 {
            0
        } else {
            clamped_map(ctx.get_surface_secondary(), -1.0, 1.0, 0.0, self.secondary_depth_range as f64) as i32
        };
        stone_depth <= 1 + self.offset + surface_depth + secondary
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- YCondition ---

#[derive(Clone)]
pub struct YConditionEx {
    anchor: VerticalAnchor,
    surface_depth_multiplier: i32,
    add_stone_depth: bool,
}

impl YConditionEx {
    pub fn new(anchor: VerticalAnchor, surface_depth_multiplier: i32, add_stone_depth: bool) -> Self {
        YConditionEx {
            anchor,
            surface_depth_multiplier,
            add_stone_depth,
        }
    }
}

impl ConditionEx for YConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let y = ctx.block_y + if self.add_stone_depth { ctx.stone_depth_above } else { 0 };
        let anchor_y = self.anchor.resolve(ctx.world_min_y, ctx.world_height);
        y >= anchor_y + ctx.surface_depth * self.surface_depth_multiplier
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- WaterCondition ---

#[derive(Clone)]
pub struct WaterConditionEx {
    offset: i32,
    surface_depth_multiplier: i32,
    add_stone_depth: bool,
}

impl WaterConditionEx {
    pub fn new(offset: i32, surface_depth_multiplier: i32, add_stone_depth: bool) -> Self {
        WaterConditionEx {
            offset,
            surface_depth_multiplier,
            add_stone_depth,
        }
    }
}

impl ConditionEx for WaterConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        if ctx.water_height == i32::MIN {
            return true;
        }
        let y = ctx.block_y + if self.add_stone_depth { ctx.stone_depth_above } else { 0 };
        y >= ctx.water_height + self.offset + ctx.surface_depth * self.surface_depth_multiplier
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- NoiseThresholdCondition ---

#[derive(Clone)]
pub struct NoiseThresholdConditionEx {
    sampler_2d: bool,
    noise: NoiseHandle,
    min_threshold: f64,
    max_threshold: f64,
}

impl NoiseThresholdConditionEx {
    pub fn new_2d(noise: NoiseHandle, min_threshold: f64, max_threshold: f64) -> Self {
        NoiseThresholdConditionEx {
            sampler_2d: true,
            noise,
            min_threshold,
            max_threshold,
        }
    }

    pub fn new_3d(noise: NoiseHandle, min_threshold: f64, max_threshold: f64) -> Self {
        NoiseThresholdConditionEx {
            sampler_2d: false,
            noise,
            min_threshold,
            max_threshold,
        }
    }
}

impl ConditionEx for NoiseThresholdConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let value = if self.sampler_2d {
            self.noise.sample(ctx.block_x as f64, 0.0, ctx.block_z as f64)
        } else {
            self.noise.sample(ctx.block_x as f64, ctx.block_y as f64, ctx.block_z as f64)
        };
        value >= self.min_threshold && value <= self.max_threshold
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- VerticalGradientCondition ---

#[derive(Clone)]
pub struct VerticalGradientConditionEx {
    true_at_and_below: VerticalAnchor,
    false_at_and_above: VerticalAnchor,
    random_factory: PositionalRandomFactory,
}

impl VerticalGradientConditionEx {
    pub fn new(
        seed: u64,
        random_name: &str,
        true_at_and_below: VerticalAnchor,
        false_at_and_above: VerticalAnchor,
    ) -> Self {
        let mut world_random = NoiseSeed::new(seed);
        let positional = world_random.fork_positional();
        let mut named = positional.from_hash_of(random_name);
        VerticalGradientConditionEx {
            true_at_and_below,
            false_at_and_above,
            random_factory: named.fork_positional(),
        }
    }
}

impl ConditionEx for VerticalGradientConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let true_y = self.true_at_and_below.resolve(ctx.world_min_y, ctx.world_height);
        let false_y = self.false_at_and_above.resolve(ctx.world_min_y, ctx.world_height);
        if ctx.block_y <= true_y {
            return true;
        }
        if ctx.block_y >= false_y {
            return false;
        }
        let probability = 1.0 - (ctx.block_y - true_y) as f32 / (false_y - true_y) as f32;
        self.random_factory.at(ctx.block_x, ctx.block_y, ctx.block_z).next_float() < probability
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- TemperatureCondition ---

// Biome's static temperature noises use WorldgenRandom(LegacyRandomSource),
// not the Xoroshiro RandomState noise factory used by terrain generation.
struct LegacySimplexNoise {
    permutation: [i32; 256],
}

impl LegacySimplexNoise {
    const GRADIENT: [[i32; 3]; 12] = [
        [1, 1, 0], [-1, 1, 0], [1, -1, 0], [-1, -1, 0],
        [1, 0, 1], [-1, 0, 1], [1, 0, -1], [-1, 0, -1],
        [0, 1, 1], [0, -1, 1], [0, 1, -1], [0, -1, -1],
    ];

    fn new(random: &mut JavaLegacyRandom) -> Self {
        random.next_double();
        random.next_double();
        random.next_double();
        let mut permutation = [0; 256];
        for (index, value) in permutation.iter_mut().enumerate() {
            *value = index as i32;
        }
        for index in 0..256 {
            let offset = random.next_int_bound(256 - index as i32) as usize;
            permutation.swap(index, index + offset);
        }
        Self { permutation }
    }

    fn permutation(&self, index: i32) -> i32 {
        self.permutation[index as usize & 0xff]
    }

    fn corner(&self, index: i32, x: f64, z: f64) -> f64 {
        let mut falloff = 0.5 - x * x - z * z;
        if falloff < 0.0 {
            return 0.0;
        }
        falloff *= falloff;
        let gradient = Self::GRADIENT[index as usize];
        falloff * falloff * (gradient[0] as f64 * x + gradient[1] as f64 * z)
    }

    fn sample(&self, x: f64, z: f64) -> f64 {
        const F2: f64 = 0.3660254037844386;
        const G2: f64 = 0.21132486540518713;
        let skew = (x + z) * F2;
        let cell_x = (x + skew).floor() as i32;
        let cell_z = (z + skew).floor() as i32;
        let unskew = (cell_x + cell_z) as f64 * G2;
        let local_x = x - (cell_x as f64 - unskew);
        let local_z = z - (cell_z as f64 - unskew);
        let (step_x, step_z) = if local_x > local_z { (1, 0) } else { (0, 1) };
        let middle_x = local_x - step_x as f64 + G2;
        let middle_z = local_z - step_z as f64 + G2;
        let far_x = local_x - 1.0 + 2.0 * G2;
        let far_z = local_z - 1.0 + 2.0 * G2;
        let cell_x = cell_x & 0xff;
        let cell_z = cell_z & 0xff;
        let first = self.permutation(cell_x + self.permutation(cell_z)) % 12;
        let middle = self.permutation(cell_x + step_x + self.permutation(cell_z + step_z)) % 12;
        let last = self.permutation(cell_x + 1 + self.permutation(cell_z + 1)) % 12;
        70.0
            * (self.corner(first, local_x, local_z)
                + self.corner(middle, middle_x, middle_z)
                + self.corner(last, far_x, far_z))
    }
}

struct BiomeTemperatureNoises {
    temperature: LegacySimplexNoise,
    frozen: [LegacySimplexNoise; 3],
    biome_info: LegacySimplexNoise,
}

fn biome_temperature_noises() -> &'static BiomeTemperatureNoises {
    static NOISES: OnceLock<BiomeTemperatureNoises> = OnceLock::new();
    NOISES.get_or_init(|| {
        let mut temperature = JavaLegacyRandom::new(1234);
        let mut frozen = JavaLegacyRandom::new(3456);
        let mut biome_info = JavaLegacyRandom::new(2345);
        BiomeTemperatureNoises {
            temperature: LegacySimplexNoise::new(&mut temperature),
            frozen: [
                LegacySimplexNoise::new(&mut frozen),
                LegacySimplexNoise::new(&mut frozen),
                LegacySimplexNoise::new(&mut frozen),
            ],
            biome_info: LegacySimplexNoise::new(&mut biome_info),
        }
    })
}

/// Minecraft's process-wide `Biome.BIOME_INFO_NOISE`, used by vegetation
/// placement count modifiers as well as temperature edge variation.
pub(crate) fn biome_info_noise(block_x: i32, block_z: i32) -> f64 {
    biome_temperature_noises()
        .biome_info
        .sample(block_x as f64 / 200.0, block_z as f64 / 200.0)
}

fn frozen_ocean_temperature(block_x: i32, block_y: i32, block_z: i32, sea_level: i32) -> f32 {
    let noises = biome_temperature_noises();
    let x = block_x as f64;
    let z = block_z as f64;
    let mut frozen_value = 0.0;
    let mut input_factor = 1.0;
    let mut value_factor = 1.0 / 7.0;
    for noise in &noises.frozen {
        frozen_value += noise.sample(x * 0.05 * input_factor, z * 0.05 * input_factor) * value_factor;
        input_factor /= 2.0;
        value_factor *= 2.0;
    }
    let large_variation = frozen_value * 7.0;
    let edge_variation = noises.biome_info.sample(x * 0.2, z * 0.2);
    let mut temperature = if large_variation + edge_variation < 0.3
        && noises.biome_info.sample(x * 0.09, z * 0.09) < 0.8
    {
        0.2_f32
    } else {
        0.0_f32
    };
    let snow_level = sea_level + 17;
    if block_y > snow_level {
        let height_noise = (noises.temperature.sample(x / 8.0, z / 8.0) * 8.0) as f32;
        temperature -= (height_noise + (block_y - snow_level) as f32) * 0.05_f32 / 40.0_f32;
    }
    temperature
}

fn frozen_ocean_iceberg_melts_slightly(block_x: i32, block_z: i32, sea_level: i32) -> bool {
    frozen_ocean_temperature(block_x, sea_level, block_z, sea_level) > 0.1
}

#[derive(Clone)]
pub struct TemperatureConditionEx;

impl ConditionEx for TemperatureConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let biome = ctx.get_biome(ctx.block_y);
        if matches!(biome, Biome::FrozenOcean | Biome::DeepFrozenOcean) {
            return frozen_ocean_temperature(ctx.block_x, ctx.block_y, ctx.block_z, ctx.sea_level) < 0.15;
        }
        matches!(
            biome,
            Biome::SnowyPlains
                | Biome::Taiga
                | Biome::FrozenPeaks
                | Biome::JaggedPeaks
                | Biome::SnowySlopes
                | Biome::Grove
        )
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- SteepCondition ---

#[derive(Clone)]
pub struct SteepConditionEx;

impl ConditionEx for SteepConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let Some(heightmap) = &ctx.world_surface_heightmap else {
            return false;
        };
        let x = ctx.block_x.rem_euclid(16) as usize;
        let z = ctx.block_z.rem_euclid(16) as usize;
        let north = z.saturating_sub(1);
        let south = (z + 1).min(15);
        if heightmap[x * CHUNK_SIZE + south] >= heightmap[x * CHUNK_SIZE + north] + 4 {
            return true;
        }
        let west = x.saturating_sub(1);
        let east = (x + 1).min(15);
        heightmap[west * CHUNK_SIZE + z] >= heightmap[east * CHUNK_SIZE + z] + 4
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- HoleCondition ---

#[derive(Clone)]
pub struct HoleConditionEx;

impl ConditionEx for HoleConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        ctx.surface_depth <= 0
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- AbovePreliminarySurfaceCondition ---

#[derive(Clone)]
pub struct AbovePreliminarySurfaceConditionEx;

impl ConditionEx for AbovePreliminarySurfaceConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        ctx.block_y >= ctx.get_min_surface_level()
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- NotCondition ---

#[derive(Clone)]
pub struct NotConditionEx {
    inner: Box<dyn ConditionEx>,
}

impl NotConditionEx {
    pub fn new(inner: Box<dyn ConditionEx>) -> Self {
        NotConditionEx { inner }
    }
}

impl ConditionEx for NotConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        !self.inner.test(ctx)
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// ============================================================================
// SurfaceRuleEx implementations
// ============================================================================

// --- StateRule ---

#[derive(Clone)]
pub struct StateRuleEx {
    state: BlockId,
}

#[derive(Clone)]
struct PreserveRuleEx;

impl SurfaceRuleEx for PreserveRuleEx {
    fn try_apply(&self, ctx: &mut SurfaceContext, _x: i32, _y: i32, _z: i32) -> Option<BlockId> {
        Some(ctx.current_block)
    }

    fn clone_box(&self) -> Box<dyn SurfaceRuleEx> {
        Box::new(self.clone())
    }
}

impl StateRuleEx {
    pub fn new(state: BlockId) -> Self {
        StateRuleEx { state }
    }
}

impl SurfaceRuleEx for StateRuleEx {
    fn try_apply(&self, _ctx: &mut SurfaceContext, _x: i32, _y: i32, _z: i32) -> Option<BlockId> {
        Some(self.state)
    }

    fn clone_box(&self) -> Box<dyn SurfaceRuleEx> {
        Box::new(self.clone())
    }
}

// --- TestRule ---

pub struct TestRuleEx {
    condition: Box<dyn ConditionEx>,
    followup: Box<dyn SurfaceRuleEx>,
}

impl TestRuleEx {
    pub fn new(condition: Box<dyn ConditionEx>, followup: Box<dyn SurfaceRuleEx>) -> Self {
        TestRuleEx { condition, followup }
    }
}

impl Clone for TestRuleEx {
    fn clone(&self) -> Self {
        TestRuleEx {
            condition: self.condition.clone(),
            followup: self.followup.clone(),
        }
    }
}

impl ConditionEx for TestRuleEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        self.condition.test(ctx)
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

impl SurfaceRuleEx for TestRuleEx {
    fn try_apply(&self, ctx: &mut SurfaceContext, x: i32, y: i32, z: i32) -> Option<BlockId> {
        if !self.condition.test(ctx) {
            return None;
        }
        self.followup.try_apply(ctx, x, y, z)
    }

    fn clone_box(&self) -> Box<dyn SurfaceRuleEx> {
        Box::new(self.clone())
    }
}

// --- SequenceRule ---

pub struct SequenceRuleEx {
    rules: Vec<Box<dyn SurfaceRuleEx>>,
}

impl SequenceRuleEx {
    pub fn new(rules: Vec<Box<dyn SurfaceRuleEx>>) -> Self {
        SequenceRuleEx { rules }
    }
}

impl Clone for SequenceRuleEx {
    fn clone(&self) -> Self {
        SequenceRuleEx {
            rules: self.rules.iter().map(|r| r.clone_box()).collect(),
        }
    }
}

impl SurfaceRuleEx for SequenceRuleEx {
    fn try_apply(&self, ctx: &mut SurfaceContext, x: i32, y: i32, z: i32) -> Option<BlockId> {
        for rule in &self.rules {
            if let Some(state) = rule.try_apply(ctx, x, y, z) {
                return Some(state);
            }
        }
        None
    }

    fn clone_box(&self) -> Box<dyn SurfaceRuleEx> {
        Box::new(self.clone())
    }
}

// --- BandlandsRule ---

#[derive(Clone)]
pub struct BandlandsRuleEx {
    system: Arc<SurfaceSystemRef>,
}

impl BandlandsRuleEx {
    pub fn new(system: Arc<SurfaceSystemRef>) -> Self {
        BandlandsRuleEx { system }
    }
}

impl SurfaceRuleEx for BandlandsRuleEx {
    fn try_apply(&self, _ctx: &mut SurfaceContext, x: i32, y: i32, z: i32) -> Option<BlockId> {
        Some(self.system.get_band(x, y, z))
    }

    fn clone_box(&self) -> Box<dyn SurfaceRuleEx> {
        Box::new(self.clone())
    }
}

// ============================================================================
// SurfaceSystemRef — shared data behind Arc, used by both SurfaceSystem and rules
// ============================================================================

pub struct SurfaceSystemRef {
    pub default_block: BlockId,
    pub sea_level: i32,
    pub clay_bands: Vec<BlockId>,
    pub clay_bands_offset_noise: NoiseHandle,
    pub surface_noise: NoiseHandle,
    pub surface_secondary_noise: NoiseHandle,
    pub badlands_pillar_noise: NoiseHandle,
    pub badlands_pillar_roof_noise: NoiseHandle,
    pub badlands_surface_noise: NoiseHandle,
    pub iceberg_pillar_noise: NoiseHandle,
    pub iceberg_pillar_roof_noise: NoiseHandle,
    pub iceberg_surface_noise: NoiseHandle,
    pub noise_random_seed: u64,
}

impl SurfaceSystemRef {
    pub fn get_surface_depth(&self, block_x: i32, block_z: i32) -> i32 {
        let noise_value = self.surface_noise.sample(block_x as f64, 0.0, block_z as f64);
        // SurfaceSystem uses the same world-seed positional factory for both
        // noise initialization and its per-column random offset.
        let mut world_random = NoiseSeed::new(self.noise_random_seed);
        let positional = world_random.fork_positional();
        let mut rng = positional.at(block_x, 0, block_z);
        let random_offset = rng.next_double() * 0.25;
        (noise_value * 2.75 + 3.0 + random_offset) as i32
    }

    pub fn get_surface_secondary(&self, block_x: i32, block_z: i32) -> f64 {
        self.surface_secondary_noise.sample(block_x as f64, 0.0, block_z as f64)
    }

    pub fn get_band(&self, world_x: i32, y: i32, world_z: i32) -> BlockId {
        let offset =
            (self.clay_bands_offset_noise.sample(world_x as f64, 0.0, world_z as f64) * 4.0).round() as i32;
        let len = self.clay_bands.len() as i32;
        let index = ((y + offset) % len + len) as usize % self.clay_bands.len();
        self.clay_bands[index]
    }
}

// ============================================================================
// SurfaceSystem
// ============================================================================

pub struct SurfaceSystem;

impl SurfaceSystem {
    pub fn create_ref_from_seed(default_block: BlockId, sea_level: i32, seed: u64) -> Arc<SurfaceSystemRef> {
        let mut world_random = NoiseSeed::new(seed);
        let positional = world_random.fork_positional();

        Self::create_ref(
            default_block,
            sea_level,
            seed,
            seeded_surface_noise(&positional, NoiseKey::Surface),
            seeded_surface_noise(&positional, NoiseKey::SurfaceSecondary),
            seeded_surface_noise(&positional, NoiseKey::ClayBandsOffset),
            seeded_surface_noise(&positional, NoiseKey::BadlandsPillar),
            seeded_surface_noise(&positional, NoiseKey::BadlandsPillarRoof),
            seeded_surface_noise(&positional, NoiseKey::BadlandsSurface),
            seeded_surface_noise(&positional, NoiseKey::IcebergPillar),
            seeded_surface_noise(&positional, NoiseKey::IcebergPillarRoof),
            seeded_surface_noise(&positional, NoiseKey::IcebergSurface),
        )
    }

    pub fn create_ref(
        default_block: BlockId,
        sea_level: i32,
        seed: u64,
        surface_noise: NoiseHandle,
        surface_secondary_noise: NoiseHandle,
        clay_bands_offset_noise: NoiseHandle,
        badlands_pillar_noise: NoiseHandle,
        badlands_pillar_roof_noise: NoiseHandle,
        badlands_surface_noise: NoiseHandle,
        iceberg_pillar_noise: NoiseHandle,
        iceberg_pillar_roof_noise: NoiseHandle,
        iceberg_surface_noise: NoiseHandle,
    ) -> Arc<SurfaceSystemRef> {
        let mut world_random = NoiseSeed::new(seed);
        let positional = world_random.fork_positional();
        let mut rng = positional.from_hash_of("minecraft:clay_bands");
        let clay_bands = generate_bands(&mut rng);

        Arc::new(SurfaceSystemRef {
            default_block,
            sea_level,
            clay_bands,
            clay_bands_offset_noise,
            surface_noise,
            surface_secondary_noise,
            badlands_pillar_noise,
            badlands_pillar_roof_noise,
            badlands_surface_noise,
            iceberg_pillar_noise,
            iceberg_pillar_roof_noise,
            iceberg_surface_noise,
            noise_random_seed: seed,
        })
    }

    pub fn build_surface(
        system: Arc<SurfaceSystemRef>,
        chunk: &mut Chunk,
        rule_source: &dyn RuleSourceEx,
        sea_level: i32,
    ) {
        Self::build_surface_with_biome_provider(
            system,
            chunk,
            rule_source,
            sea_level,
            0,
            |_, _, _| Biome::Plains,
        );
    }

    pub fn build_surface_with_biome_provider(
        system: Arc<SurfaceSystemRef>,
        chunk: &mut Chunk,
        rule_source: &dyn RuleSourceEx,
        sea_level: i32,
        world_min_y: i32,
        biome_provider: impl FnMut(i32, i32, i32) -> Biome + Send + 'static,
    ) {
        let heightmap = build_heightmap(chunk);

        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;

        let get_preliminary_surface: Box<dyn Fn(i32, i32) -> i32 + Send + Sync> = {
            let hm = heightmap.clone();
            Box::new(move |wx: i32, wz: i32| {
                let lx = (wx as i64 - base_x) as usize;
                let lz = (wz as i64 - base_z) as usize;
                if lx < CHUNK_SIZE && lz < CHUNK_SIZE {
                    hm[lx * CHUNK_SIZE + lz] + world_min_y
                } else {
                    sea_level
                }
            })
        };

        let mut ctx = SurfaceContext::new_with_height(
            system.clone(),
            sea_level,
            world_min_y,
            CHUNK_HEIGHT as i32,
            Box::new(biome_provider),
            get_preliminary_surface,
        );

        let rule = rule_source.create();

        let min_x = base_x as i32;
        let min_z = base_z as i32;

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let block_x = min_x + x as i32;
                let block_z = min_z + z as i32;

                let mut starting_height = heightmap[x * CHUNK_SIZE + z] + 1;
                if starting_height >= CHUNK_HEIGHT as i32 {
                    starting_height = CHUNK_HEIGHT as i32 - 1;
                }

                ctx.update_xz(block_x, block_z);

                let mut stone_above_depth = 0;
                let mut water_height = i32::MIN;
                let end_y = 0;

                // The Java implementation tracks the nearest opening below
                // each stone block. Precompute that depth bottom-up once per
                // column instead of rescanning the entire lower column for
                // every block (which turns a 384-high column into O(n^2)).
                let mut stone_below_depth = vec![0i32; (starting_height + 1) as usize];
                let mut nearest_opening = None;
                for local_y in end_y..=starting_height {
                    let block = chunk.get_block(x, local_y as usize, z);
                    if block.is_air() || block.id == BlockId::Water || block.id == BlockId::Lava {
                        nearest_opening = Some(local_y);
                    } else {
                        stone_below_depth[local_y as usize] = nearest_opening
                            .map(|opening| local_y - opening)
                            .unwrap_or(1);
                    }
                }

                for y in (end_y..=starting_height).rev() {
                    let blk = chunk.get_block(x, y as usize, z);
                    let world_y = y + world_min_y;

                    if blk.is_air() {
                        stone_above_depth = 0;
                        water_height = i32::MIN;
                    } else if blk.id == BlockId::Water || blk.id == BlockId::Lava {
                        if water_height == i32::MIN {
                            water_height = world_y + 1;
                        }
                    } else {
                        stone_above_depth += 1;
                        let stone_below_depth = stone_below_depth[y as usize];
                        ctx.update_y(stone_above_depth, stone_below_depth, water_height, world_y);

                        // The simplified rules have unconditional fallback
                        // states (for example grass, dirt, sandstone, and
                        // terracotta). The depth context handles the exposed
                        // face; this preliminary-heightmap gate prevents those
                        // ordinary materials from reaching deep cave openings.
                        // Deep-cave-specific rules must be evaluated with
                        // `DeepCaveException` before this ordinary fallback.
                        let surface_rule_depth = 1 + ctx.surface_depth.max(0);
                        if blk.id == system.default_block
                            && stone_above_depth <= surface_rule_depth
                            && ctx.permits_surface_rule(SurfaceRuleScope::Ordinary)
                        {
                            if let Some(new_state) = rule.try_apply(&mut ctx, block_x, world_y, block_z) {
                                chunk.set_block(x, y as usize, z, Block::new(new_state));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Geometry-only Minecraft 26.2 surface entry point. Unlike the
    /// compatibility builder, this uses WORLD_SURFACE_WG (all non-air), the
    /// density router's preliminary surface, and the complete reference rule
    /// scan without an additional heuristic depth gate.
    pub fn build_reference_overworld_surface(
        system: Arc<SurfaceSystemRef>,
        chunk: &mut Chunk,
        sea_level: i32,
        world_min_y: i32,
        mut biome_provider: impl FnMut(i32, i32, i32) -> Biome + Send + 'static,
        preliminary_surface: impl Fn(i32, i32) -> i32 + Send + Sync + 'static,
    ) {
        report_minecraft26_reference_surface_unsupported();
        let mut heightmap = build_world_surface_wg_heightmap(chunk, world_min_y);
        let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
        let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;
        let mut ctx = SurfaceContext::new_with_height(
            system.clone(),
            sea_level,
            world_min_y,
            CHUNK_HEIGHT as i32,
            Box::new(move |x, y, z| biome_provider(x, y, z)),
            Box::new(preliminary_surface),
        );
        ctx.reference_preliminary_interpolation = true;
        ctx.world_surface_heightmap = Some(heightmap.clone());
        let rule = minecraft26_reference_overworld_rules(system.clone());

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let index = x * CHUNK_SIZE + z;
                let block_x = (base_x + x as i64) as i32;
                let block_z = (base_z + z as i64) as i32;
                ctx.update_xz(block_x, block_z);
                let initial_top = heightmap[index];
                let surface_biome = ctx.get_biome(initial_top + 1);

                if surface_biome == Biome::ErodedBadlands {
                    reference_eroded_badlands_extension(
                        &system,
                        chunk,
                        x,
                        z,
                        block_x,
                        block_z,
                        initial_top + 1,
                        world_min_y,
                    );
                    heightmap[index] = reference_column_top(chunk, x, z, world_min_y, true);
                    if let Some(context_heightmap) = &mut ctx.world_surface_heightmap {
                        context_heightmap[index] = heightmap[index];
                    }
                }

                let top = heightmap[index];
                let starting_local = (top + 1 - world_min_y).clamp(0, CHUNK_HEIGHT as i32 - 1);
                let mut stone_below_depth = vec![0i32; starting_local as usize + 1];
                let mut solid_run = 0;
                for local_y in 0..=starting_local {
                    let block = chunk.get_block(x, local_y as usize, z);
                    if block.is_air() || matches!(block.id, BlockId::Water | BlockId::Lava) {
                        solid_run = 0;
                    } else {
                        solid_run += 1;
                        stone_below_depth[local_y as usize] = solid_run;
                    }
                }

                let mut stone_above_depth = 0;
                let mut water_height = i32::MIN;
                for local_y in (0..=starting_local).rev() {
                    let block = chunk.get_block(x, local_y as usize, z);
                    let world_y = world_min_y + local_y;
                    if block.is_air() {
                        stone_above_depth = 0;
                        water_height = i32::MIN;
                    } else if matches!(block.id, BlockId::Water | BlockId::Lava) {
                        if water_height == i32::MIN {
                            water_height = world_y + 1;
                        }
                    } else {
                        stone_above_depth += 1;
                        ctx.update_y_with_block(
                            stone_above_depth,
                            stone_below_depth[local_y as usize],
                            water_height,
                            world_y,
                            block.id,
                        );
                        // Density fill preclassifies deepslate for this profile;
                        // treating it as factored default stone lets the Java
                        // bedrock/deepslate surface rules retain their ordering.
                        if matches!(block.id, BlockId::Stone | BlockId::Deepslate) {
                            if let Some(new_state) = rule.try_apply(&mut ctx, block_x, world_y, block_z) {
                                chunk.set_block(x, local_y as usize, z, Block::new(new_state));
                            }
                        }
                    }
                }

                if matches!(surface_biome, Biome::FrozenOcean | Biome::DeepFrozenOcean) {
                    reference_frozen_ocean_extension(
                        &system,
                        chunk,
                        x,
                        z,
                        block_x,
                        block_z,
                        top + 1,
                        ctx.get_min_surface_level(),
                        world_min_y,
                    );
                    heightmap[index] = reference_column_top(chunk, x, z, world_min_y, true);
                    if let Some(context_heightmap) = &mut ctx.world_surface_heightmap {
                        context_heightmap[index] = heightmap[index];
                    }
                }
            }
        }
    }
}

fn reference_column_top(chunk: &Chunk, x: usize, z: usize, world_min_y: i32, include_fluids: bool) -> i32 {
    for y in (0..CHUNK_HEIGHT).rev() {
        let block = chunk.get_block(x, y, z);
        if !block.is_air() && (include_fluids || !matches!(block.id, BlockId::Water | BlockId::Lava)) {
            return world_min_y + y as i32;
        }
    }
    world_min_y - 1
}

fn build_world_surface_wg_heightmap(chunk: &Chunk, world_min_y: i32) -> Vec<i32> {
    let mut heightmap = vec![world_min_y - 1; CHUNK_SIZE * CHUNK_SIZE];
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            heightmap[x * CHUNK_SIZE + z] = reference_column_top(chunk, x, z, world_min_y, true);
        }
    }
    heightmap
}

fn reference_eroded_badlands_extension(
    system: &SurfaceSystemRef,
    chunk: &mut Chunk,
    x: usize,
    z: usize,
    block_x: i32,
    block_z: i32,
    height: i32,
    world_min_y: i32,
) {
    let pillar_buffer = (system.badlands_surface_noise.sample(block_x as f64, 0.0, block_z as f64) * 8.25)
        .abs()
        .min(system.badlands_pillar_noise.sample(block_x as f64 * 0.2, 0.0, block_z as f64 * 0.2) * 15.0);
    if pillar_buffer <= 0.0 {
        return;
    }
    let pillar_floor = (system
        .badlands_pillar_roof_noise
        .sample(block_x as f64 * 0.75, 0.0, block_z as f64 * 0.75)
        * 1.5)
        .abs();
    let start_y =
        (64.0 + (pillar_buffer * pillar_buffer * 2.5).min((pillar_floor * 50.0).ceil() + 24.0)).floor() as i32;
    if height > start_y {
        return;
    }
    for world_y in (world_min_y..=start_y).rev() {
        let local_y = world_y - world_min_y;
        if !(0..CHUNK_HEIGHT as i32).contains(&local_y) {
            continue;
        }
        let block = chunk.get_block(x, local_y as usize, z);
        if block.id == system.default_block {
            break;
        }
        if block.id == BlockId::Water {
            return;
        }
    }
    for world_y in (world_min_y..=start_y).rev() {
        let local_y = world_y - world_min_y;
        if !(0..CHUNK_HEIGHT as i32).contains(&local_y) {
            continue;
        }
        if !chunk.get_block(x, local_y as usize, z).is_air() {
            break;
        }
        chunk.set_block(x, local_y as usize, z, Block::new(system.default_block));
    }
}

fn reference_frozen_ocean_extension(
    system: &SurfaceSystemRef,
    chunk: &mut Chunk,
    x: usize,
    z: usize,
    block_x: i32,
    block_z: i32,
    height: i32,
    min_surface_level: i32,
    world_min_y: i32,
) {
    let iceberg = (system.iceberg_surface_noise.sample(block_x as f64, 0.0, block_z as f64) * 8.25)
        .abs()
        .min(system.iceberg_pillar_noise.sample(block_x as f64 * 1.28, 0.0, block_z as f64 * 1.28) * 15.0);
    if iceberg <= 1.8 {
        return;
    }
    let roof = (system
        .iceberg_pillar_roof_noise
        .sample(block_x as f64 * 1.17, 0.0, block_z as f64 * 1.17)
        * 1.5)
        .abs();
    let Some((top, bottom)) = frozen_ocean_extension_bounds(
        iceberg,
        roof,
        system.sea_level,
        frozen_ocean_iceberg_melts_slightly(block_x, block_z, system.sea_level),
    ) else {
        return;
    };
    let mut world_random = NoiseSeed::new(system.noise_random_seed);
    let positional = world_random.fork_positional();
    let mut random = positional.at(block_x, 0, block_z);
    let max_snow_depth = 2 + random.next_int(4);
    let min_snow_height = system.sea_level + 18 + random.next_int(10);
    let mut snow_depth = 0;
    let start = height.max(top as i32 + 1).min(world_min_y + CHUNK_HEIGHT as i32 - 1);
    let end = min_surface_level.max(world_min_y);
    for world_y in (end..=start).rev() {
        let local_y = (world_y - world_min_y) as usize;
        let block = chunk.get_block(x, local_y, z);
        let replace = (block.is_air() && world_y < top as i32 && random.next_double() > 0.01)
            || (block.id == BlockId::Water
                && world_y > bottom as i32
                && world_y < system.sea_level
                && bottom != 0.0
                && random.next_double() > 0.15);
        if replace {
            let state = if snow_depth <= max_snow_depth && world_y > min_snow_height {
                snow_depth += 1;
                BlockId::SnowBlock
            } else {
                BlockId::PackedIce
            };
            chunk.set_block(x, local_y, z, Block::new(state));
        }
    }
}

fn frozen_ocean_extension_bounds(
    iceberg: f64,
    roof: f64,
    sea_level: i32,
    melts_slightly: bool,
) -> Option<(f64, f64)> {
    let mut top = (iceberg * iceberg * 1.2).min((roof * 40.0).ceil() + 14.0);
    if melts_slightly {
        top -= 2.0;
    }
    if top <= 2.0 {
        return None;
    }
    let bottom = sea_level as f64 - top - 7.0;
    Some((top + sea_level as f64, bottom))
}

fn generate_bands(random: &mut NoiseSeed) -> Vec<BlockId> {
    let mut bands = vec![BlockId::Terracotta; 192];

    let mut i = 0;
    while i < bands.len() {
        i += 1 + random.next_int(5) as usize;
        if i < bands.len() {
            bands[i] = BlockId::OrangeTerracotta;
        }
    }

    make_bands(random, &mut bands, 1, BlockId::YellowTerracotta);
    make_bands(random, &mut bands, 2, BlockId::BrownTerracotta);
    make_bands(random, &mut bands, 1, BlockId::RedTerracotta);

    let white_band_count = 9 + random.next_int(7);
    let mut start = 0;
    for _ in 0..white_band_count {
        if start >= bands.len() {
            break;
        }
        bands[start] = BlockId::WhiteTerracotta;
        if start > 0 && random.next_boolean() {
            bands[start - 1] = BlockId::LightGrayTerracotta;
        }
        if start + 1 < bands.len() && random.next_boolean() {
            bands[start + 1] = BlockId::LightGrayTerracotta;
        }
        start += 4 + random.next_int(16) as usize;
    }

    bands
}

fn make_bands(random: &mut NoiseSeed, bands: &mut [BlockId], base_width: usize, state: BlockId) {
    let band_count = 6 + random.next_int(10);
    for _ in 0..band_count {
        let width = base_width + random.next_int(3) as usize;
        let start = random.next_int(bands.len() as i32) as usize;
        for p in 0..width {
            if start + p < bands.len() {
                bands[start + p] = state;
            }
        }
    }
}

fn build_heightmap(chunk: &Chunk) -> Vec<i32> {
    let mut heightmap = vec![0i32; CHUNK_SIZE * CHUNK_SIZE];
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let mut top = -1i32;
            for y in (1..CHUNK_HEIGHT).rev() {
                let block = chunk.get_block(x, y, z);
                if !block.is_air() && block.id != BlockId::Water && block.id != BlockId::Lava {
                    top = y as i32;
                    break;
                }
            }
            if top < 0 {
                top = 1;
            }
            heightmap[x * CHUNK_SIZE + z] = top;
        }
    }
    heightmap
}

fn positional_hash(base_seed: u64, x: i32, y: i32, z: i32) -> u64 {
    let mut h = base_seed;
    h = h.wrapping_mul(0x9e3779b97f4a7c15);
    h ^= x as u64;
    h = h.wrapping_mul(0x9e3779b97f4a7c15);
    h ^= y as u64;
    h = h.wrapping_mul(0x9e3779b97f4a7c15);
    h ^= z as u64;
    h = h.wrapping_mul(0x9e3779b97f4a7c15);
    h
}

fn seeded_surface_noise(positional: &PositionalRandomFactory, key: NoiseKey) -> NoiseHandle {
    let mut random = positional.from_hash_of(&format!("minecraft:{}", key.name()));
    let parameters = crate::world::world_gen::noise_router::create_noise_parameters(&key);
    NoiseHandle::new(NormalNoise::create(&mut random, &parameters))
}

fn clamped_map(value: f64, from_start: f64, from_end: f64, to_start: f64, to_end: f64) -> f64 {
    let t = ((value - from_start) / (from_end - from_start)).clamp(0.0, 1.0);
    to_start + (to_end - to_start) * t
}

// ============================================================================
// OreVeinifier
// ============================================================================

#[allow(dead_code)]
pub struct OreVeinifier;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VeinType {
    Copper,
    Iron,
}

impl VeinType {
    fn ore(&self) -> BlockId {
        match self {
            VeinType::Copper => BlockId::CopperOre,
            VeinType::Iron => BlockId::DeepslateIronOre,
        }
    }

    fn raw_ore_block(&self) -> BlockId {
        match self {
            VeinType::Copper => BlockId::RawCopperBlock,
            VeinType::Iron => BlockId::RawIronBlock,
        }
    }

    fn filler(&self) -> BlockId {
        match self {
            VeinType::Copper => BlockId::Granite,
            VeinType::Iron => BlockId::Tuff,
        }
    }

    fn min_y(&self) -> i32 {
        match self {
            VeinType::Copper => 0,
            VeinType::Iron => -60,
        }
    }

    fn max_y(&self) -> i32 {
        match self {
            VeinType::Copper => 50,
            VeinType::Iron => -8,
        }
    }
}

impl OreVeinifier {
    pub const VEININESS_THRESHOLD: f64 = 0.4;
    pub const EDGE_ROUNDOFF_BEGIN: i32 = 20;
    pub const MAX_EDGE_ROUNDOFF: f64 = 0.2;
    pub const VEIN_SOLIDNESS: f64 = 0.7;
    pub const MIN_RICHNESS: f64 = 0.1;
    pub const MAX_RICHNESS: f64 = 0.3;
    pub const MAX_RICHNESS_THRESHOLD: f64 = 0.6;
    pub const CHANCE_OF_RAW_ORE_BLOCK: f64 = 0.02;
    pub const SKIP_ORE_IF_GAP_NOISE_IS_BELOW: f64 = -0.3;

    pub fn apply(
        vein_toggle: &dyn DensityFunction,
        vein_ridged: &dyn DensityFunction,
        vein_gap: &dyn DensityFunction,
        ore_veins_random: &PositionalRandomFactory,
        pos: &dyn FunctionContext,
    ) -> Option<BlockId> {
        let ore_veininess_noise = vein_toggle.compute(pos);
        let pos_y = pos.block_y();

        let vein_type = if ore_veininess_noise > 0.0 {
            VeinType::Copper
        } else {
            VeinType::Iron
        };

        let veininess_ridged = ore_veininess_noise.abs();

        let distance_from_top = vein_type.max_y() - pos_y;
        let distance_from_bottom = pos_y - vein_type.min_y();

        if distance_from_bottom < 0 || distance_from_top < 0 {
            return None;
        }

        let distance_from_edge = distance_from_top.min(distance_from_bottom);
        let edge_roundoff = clamped_map(distance_from_edge as f64, 0.0, 20.0, -0.2, 0.0);

        if veininess_ridged + edge_roundoff < 0.4 {
            return None;
        }

        let mut rng = ore_veins_random.at(pos.block_x(), pos.block_y(), pos.block_z());

        if rng.next_float() > 0.7 {
            return None;
        }

        if vein_ridged.compute(pos) >= 0.0 {
            return None;
        }

        let richness = clamped_map(veininess_ridged, 0.4, 0.6, 0.1, 0.3);

        if rng.next_float() as f64 <= richness && vein_gap.compute(pos) > -0.3 {
            if rng.next_float() < 0.02 {
                Some(vein_type.raw_ore_block())
            } else {
                Some(vein_type.ore())
            }
        } else {
            Some(vein_type.filler())
        }
    }
}

// ============================================================================
// Default Overworld Rules
// ============================================================================

fn reference_state(state: BlockId) -> Box<dyn SurfaceRuleEx> {
    Box::new(StateRuleEx::new(state))
}

fn reference_preserve() -> Box<dyn SurfaceRuleEx> {
    Box::new(PreserveRuleEx)
}

fn reference_if(condition: Box<dyn ConditionEx>, rule: Box<dyn SurfaceRuleEx>) -> Box<dyn SurfaceRuleEx> {
    Box::new(TestRuleEx::new(condition, rule))
}

fn reference_sequence(rules: Vec<Box<dyn SurfaceRuleEx>>) -> Box<dyn SurfaceRuleEx> {
    Box::new(SequenceRuleEx::new(rules))
}

fn reference_biome(biomes: &[Biome]) -> Box<dyn ConditionEx> {
    Box::new(BiomeConditionEx::new(biomes.to_vec()))
}

fn reference_stone_depth(
    offset: i32,
    add_surface_depth: bool,
    secondary_depth_range: i32,
    surface: CaveSurface,
) -> Box<dyn ConditionEx> {
    Box::new(StoneDepthConditionEx::new(offset, add_surface_depth, secondary_depth_range, surface))
}

fn reference_y(anchor: i32, multiplier: i32, add_stone_depth: bool) -> Box<dyn ConditionEx> {
    Box::new(YConditionEx::new(VerticalAnchor::Absolute(anchor), multiplier, add_stone_depth))
}

fn reference_water(offset: i32, multiplier: i32, add_stone_depth: bool) -> Box<dyn ConditionEx> {
    Box::new(WaterConditionEx::new(offset, multiplier, add_stone_depth))
}

fn reference_noise(system: &SurfaceSystemRef, key: NoiseKey, min: f64, max: f64) -> Box<dyn ConditionEx> {
    let mut world_random = NoiseSeed::new(system.noise_random_seed);
    let positional = world_random.fork_positional();
    Box::new(NoiseThresholdConditionEx::new_2d(
        seeded_surface_noise(&positional, key),
        min,
        max,
    ))
}

fn reference_noise_3d(system: &SurfaceSystemRef, key: NoiseKey, min: f64, max: f64) -> Box<dyn ConditionEx> {
    let mut world_random = NoiseSeed::new(system.noise_random_seed);
    let positional = world_random.fork_positional();
    Box::new(NoiseThresholdConditionEx::new_3d(
        seeded_surface_noise(&positional, key),
        min,
        max,
    ))
}

fn reference_surface_noise_above(system: &SurfaceSystemRef, threshold: f64) -> Box<dyn ConditionEx> {
    reference_noise(system, NoiseKey::Surface, threshold / 8.25, f64::MAX)
}

fn reference_sulfur_cave_bands(system: &SurfaceSystemRef) -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(
            reference_noise_3d(system, NoiseKey::SulfurCaveGradient, -0.4, -0.1),
            reference_preserve(),
        ),
        reference_if(
            reference_noise_3d(system, NoiseKey::SulfurCaveGradient, 0.0, 0.4),
            reference_preserve(),
        ),
        reference_if(
            reference_noise_3d(system, NoiseKey::SulfurCaveGradient, 0.4, f64::MAX),
            reference_preserve(),
        ),
    ])
}

fn reference_not(condition: Box<dyn ConditionEx>) -> Box<dyn ConditionEx> {
    Box::new(NotConditionEx::new(condition))
}

fn reference_on_floor() -> Box<dyn ConditionEx> {
    reference_stone_depth(0, false, 0, CaveSurface::Floor)
}

fn reference_under_floor() -> Box<dyn ConditionEx> {
    reference_stone_depth(0, true, 0, CaveSurface::Floor)
}

fn reference_on_ceiling() -> Box<dyn ConditionEx> {
    reference_stone_depth(0, false, 0, CaveSurface::Ceiling)
}

fn reference_grass_or_dirt() -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(reference_water(0, 0, false), reference_state(BlockId::GrassBlock)),
        reference_state(BlockId::Dirt),
    ])
}

fn reference_sand_or_sandstone() -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(reference_on_ceiling(), reference_state(BlockId::Sandstone)),
        reference_state(BlockId::Sand),
    ])
}

fn reference_gravel_or_stone() -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(reference_on_ceiling(), reference_state(BlockId::Stone)),
        reference_state(BlockId::Gravel),
    ])
}

fn reference_common_surface_rules(system: &Arc<SurfaceSystemRef>) -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(
            reference_biome(&[Biome::StonyPeaks]),
            reference_sequence(vec![
                reference_if(reference_noise(system, NoiseKey::Calcite, -0.0125, 0.0125), reference_state(BlockId::Calcite)),
                reference_state(BlockId::Stone),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::StonyShore]),
            reference_sequence(vec![
                reference_if(reference_noise(system, NoiseKey::Gravel, -0.05, 0.05), reference_gravel_or_stone()),
                reference_state(BlockId::Stone),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::WindsweptHills]),
            reference_if(reference_surface_noise_above(system, 1.0), reference_state(BlockId::Stone)),
        ),
        reference_if(
            reference_biome(&[Biome::WarmOcean, Biome::Beach, Biome::SnowyBeach]),
            reference_sand_or_sandstone(),
        ),
        reference_if(reference_biome(&[Biome::Desert]), reference_sand_or_sandstone()),
        reference_if(reference_biome(&[Biome::DripstoneCaves]), reference_state(BlockId::Stone)),
        reference_if(
            reference_biome(&[Biome::SulfurCaves]),
            reference_sequence(vec![reference_sulfur_cave_bands(system), reference_state(BlockId::Stone)]),
        ),
    ])
}

fn reference_powder_snow(system: &Arc<SurfaceSystemRef>, min: f64, max: f64) -> Box<dyn SurfaceRuleEx> {
    reference_if(
        reference_noise(system, NoiseKey::PowderSnow, min, max),
        reference_if(reference_water(0, 0, false), reference_state(BlockId::PowderSnow)),
    )
}

fn reference_biome_under_surface_rule(system: &Arc<SurfaceSystemRef>) -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(
            reference_biome(&[Biome::FrozenPeaks]),
            reference_sequence(vec![
                reference_if(Box::new(SteepConditionEx), reference_state(BlockId::PackedIce)),
                reference_if(reference_noise(system, NoiseKey::PackedIce, -0.5, 0.2), reference_state(BlockId::PackedIce)),
                reference_if(reference_noise(system, NoiseKey::Ice, -0.0625, 0.025), reference_state(BlockId::Ice)),
                reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::SnowySlopes]),
            reference_sequence(vec![
                reference_if(Box::new(SteepConditionEx), reference_state(BlockId::Stone)),
                reference_powder_snow(system, 0.45, 0.58),
                reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
            ]),
        ),
        reference_if(reference_biome(&[Biome::JaggedPeaks]), reference_state(BlockId::Stone)),
        reference_if(
            reference_biome(&[Biome::Grove]),
            reference_sequence(vec![reference_powder_snow(system, 0.45, 0.58), reference_state(BlockId::Dirt)]),
        ),
        reference_common_surface_rules(system),
        reference_if(
            reference_biome(&[Biome::WindsweptSavanna]),
            reference_if(reference_surface_noise_above(system, 1.75), reference_state(BlockId::Stone)),
        ),
        reference_if(
            reference_biome(&[Biome::WindsweptGravellyHills]),
            reference_sequence(vec![
                reference_if(reference_surface_noise_above(system, 2.0), reference_gravel_or_stone()),
                reference_if(reference_surface_noise_above(system, 1.0), reference_state(BlockId::Stone)),
                reference_if(reference_surface_noise_above(system, -1.0), reference_state(BlockId::Dirt)),
                reference_gravel_or_stone(),
            ]),
        ),
        reference_if(reference_biome(&[Biome::MangroveSwamp]), reference_state(BlockId::Mud)),
        reference_state(BlockId::Dirt),
    ])
}

fn reference_biome_surface_rule(system: &Arc<SurfaceSystemRef>) -> Box<dyn SurfaceRuleEx> {
    reference_sequence(vec![
        reference_if(
            reference_biome(&[Biome::FrozenPeaks]),
            reference_sequence(vec![
                reference_if(Box::new(SteepConditionEx), reference_state(BlockId::PackedIce)),
                reference_if(reference_noise(system, NoiseKey::PackedIce, 0.0, 0.2), reference_state(BlockId::PackedIce)),
                reference_if(reference_noise(system, NoiseKey::Ice, 0.0, 0.025), reference_state(BlockId::Ice)),
                reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::SnowySlopes]),
            reference_sequence(vec![
                reference_if(Box::new(SteepConditionEx), reference_state(BlockId::Stone)),
                reference_powder_snow(system, 0.35, 0.6),
                reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::JaggedPeaks]),
            reference_sequence(vec![
                reference_if(Box::new(SteepConditionEx), reference_state(BlockId::Stone)),
                reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::Grove]),
            reference_sequence(vec![
                reference_powder_snow(system, 0.35, 0.6),
                reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
            ]),
        ),
        reference_common_surface_rules(system),
        reference_if(
            reference_biome(&[Biome::WindsweptSavanna]),
            reference_sequence(vec![
                reference_if(reference_surface_noise_above(system, 1.75), reference_state(BlockId::Stone)),
                reference_if(reference_surface_noise_above(system, -0.5), reference_state(BlockId::CoarseDirt)),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::WindsweptGravellyHills]),
            reference_sequence(vec![
                reference_if(reference_surface_noise_above(system, 2.0), reference_gravel_or_stone()),
                reference_if(reference_surface_noise_above(system, 1.0), reference_state(BlockId::Stone)),
                reference_if(reference_surface_noise_above(system, -1.0), reference_grass_or_dirt()),
                reference_gravel_or_stone(),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::OldGrowthPineTaiga, Biome::OldGrowthSpruceTaiga]),
            reference_sequence(vec![
                reference_if(reference_surface_noise_above(system, 1.75), reference_state(BlockId::CoarseDirt)),
                reference_if(reference_surface_noise_above(system, -0.95), reference_state(BlockId::Podzol)),
            ]),
        ),
        reference_if(
            reference_biome(&[Biome::IceSpikes]),
            reference_if(reference_water(0, 0, false), reference_state(BlockId::SnowBlock)),
        ),
        reference_if(reference_biome(&[Biome::MangroveSwamp]), reference_state(BlockId::Mud)),
        reference_if(reference_biome(&[Biome::MushroomFields]), reference_state(BlockId::Mycelium)),
        reference_grass_or_dirt(),
    ])
}

/// Minecraft 26.2 `SurfaceRuleData.overworld()` with unavailable outputs
/// represented by explicit preserve rules rather than compatibility fallbacks.
pub fn minecraft26_reference_overworld_rules(system: Arc<SurfaceSystemRef>) -> Box<dyn SurfaceRuleEx> {
    let badlands = [Biome::Badlands, Biome::ErodedBadlands, Biome::WoodedBadlands];
    let frozen_ocean = [Biome::FrozenOcean, Biome::DeepFrozenOcean];
    let sand_biomes = [Biome::WarmOcean, Biome::Beach, Biome::SnowyBeach];
    let clay_band = |min, max| reference_noise(&system, NoiseKey::Surface, min, max);
    let sulfur_cave_bands = reference_sulfur_cave_bands(&system);

    let wooded_badlands_and_swamps = reference_if(
        reference_on_floor(),
        reference_sequence(vec![
            reference_if(
                reference_biome(&[Biome::WoodedBadlands]),
                reference_if(
                    reference_y(97, 2, false),
                    reference_sequence(vec![
                        reference_if(clay_band(-0.909, -0.5454), reference_state(BlockId::CoarseDirt)),
                        reference_if(clay_band(-0.1818, 0.1818), reference_state(BlockId::CoarseDirt)),
                        reference_if(clay_band(0.5454, 0.909), reference_state(BlockId::CoarseDirt)),
                        reference_grass_or_dirt(),
                    ]),
                ),
            ),
            reference_if(
                reference_biome(&[Biome::Swamp]),
                reference_if(
                    reference_y(62, 0, false),
                    reference_if(
                        reference_not(reference_y(63, 0, false)),
                        reference_if(reference_noise(&system, NoiseKey::Swamp, 0.0, f64::MAX), reference_state(BlockId::Water)),
                    ),
                ),
            ),
            reference_if(
                reference_biome(&[Biome::MangroveSwamp]),
                reference_if(
                    reference_y(60, 0, false),
                    reference_if(
                        reference_not(reference_y(63, 0, false)),
                        reference_if(reference_noise(&system, NoiseKey::Swamp, 0.0, f64::MAX), reference_state(BlockId::Water)),
                    ),
                ),
            ),
        ]),
    );

    let badlands_rule = reference_if(
        reference_biome(&badlands),
        reference_sequence(vec![
            reference_if(
                reference_on_floor(),
                reference_sequence(vec![
                    reference_if(reference_y(256, 0, false), reference_state(BlockId::OrangeTerracotta)),
                    reference_if(
                        reference_y(74, 1, true),
                        reference_sequence(vec![
                            reference_if(clay_band(-0.909, -0.5454), reference_state(BlockId::Terracotta)),
                            reference_if(clay_band(-0.1818, 0.1818), reference_state(BlockId::Terracotta)),
                            reference_if(clay_band(0.5454, 0.909), reference_state(BlockId::Terracotta)),
                            Box::new(BandlandsRuleEx::new(system.clone())),
                        ]),
                    ),
                    reference_if(
                        reference_water(-1, 0, false),
                        reference_sequence(vec![reference_if(reference_on_ceiling(), reference_preserve()), reference_state(BlockId::RedSand)]),
                    ),
                    reference_if(reference_not(Box::new(HoleConditionEx)), reference_state(BlockId::OrangeTerracotta)),
                    reference_if(reference_water(-6, -1, true), reference_state(BlockId::WhiteTerracotta)),
                    reference_gravel_or_stone(),
                ]),
            ),
            reference_if(
                reference_y(63, -1, true),
                reference_sequence(vec![
                    reference_if(
                        reference_y(63, 0, false),
                        reference_if(reference_not(reference_y(74, 1, true)), reference_state(BlockId::OrangeTerracotta)),
                    ),
                    Box::new(BandlandsRuleEx::new(system.clone())),
                ]),
            ),
            reference_if(reference_under_floor(), reference_if(reference_water(-6, -1, true), reference_state(BlockId::WhiteTerracotta))),
        ]),
    );

    let close_to_surface = reference_sequence(vec![
        wooded_badlands_and_swamps,
        badlands_rule,
        reference_if(
            reference_on_floor(),
            reference_if(
                reference_water(-1, 0, false),
                reference_sequence(vec![
                    reference_if(
                        reference_biome(&frozen_ocean),
                        reference_if(
                            Box::new(HoleConditionEx),
                            reference_sequence(vec![
                                reference_if(reference_water(0, 0, false), reference_state(BlockId::Air)),
                                reference_if(Box::new(TemperatureConditionEx), reference_state(BlockId::Ice)),
                                reference_state(BlockId::Water),
                            ]),
                        ),
                    ),
                    reference_biome_surface_rule(&system),
                ]),
            ),
        ),
        reference_if(
            reference_water(-6, -1, true),
            reference_sequence(vec![
                reference_if(
                    reference_on_floor(),
                    reference_if(reference_biome(&frozen_ocean), reference_if(Box::new(HoleConditionEx), reference_state(BlockId::Water))),
                ),
                reference_if(reference_under_floor(), reference_biome_under_surface_rule(&system)),
                reference_if(
                    reference_biome(&sand_biomes),
                    reference_if(reference_stone_depth(0, true, 6, CaveSurface::Floor), reference_state(BlockId::Sandstone)),
                ),
                reference_if(
                    reference_biome(&[Biome::Desert]),
                    reference_if(reference_stone_depth(0, true, 30, CaveSurface::Floor), reference_state(BlockId::Sandstone)),
                ),
            ]),
        ),
        reference_if(
            reference_on_floor(),
            reference_sequence(vec![
                reference_if(reference_biome(&[Biome::FrozenPeaks, Biome::JaggedPeaks]), reference_state(BlockId::Stone)),
                reference_if(
                    reference_biome(&[Biome::WarmOcean, Biome::LukewarmOcean, Biome::DeepLukewarmOcean]),
                    reference_sand_or_sandstone(),
                ),
                reference_gravel_or_stone(),
            ]),
        ),
    ]);

    reference_sequence(vec![
        reference_if(
            Box::new(VerticalGradientConditionEx::new(
                system.noise_random_seed,
                "minecraft:bedrock_floor",
                VerticalAnchor::AboveBottom(0),
                VerticalAnchor::AboveBottom(5),
            )),
            reference_state(BlockId::Bedrock),
        ),
        reference_if(Box::new(AbovePreliminarySurfaceConditionEx), close_to_surface),
        reference_if(reference_biome(&[Biome::SulfurCaves]), sulfur_cave_bands),
        reference_if(
            Box::new(VerticalGradientConditionEx::new(
                system.noise_random_seed,
                "minecraft:deepslate",
                VerticalAnchor::Absolute(0),
                VerticalAnchor::Absolute(8),
            )),
            reference_state(BlockId::Deepslate),
        ),
    ])
}

pub fn default_overworld_rules(system: Arc<SurfaceSystemRef>) -> Box<dyn SurfaceRuleEx> {
    let mut rules: Vec<Box<dyn SurfaceRuleEx>> = Vec::new();

    // Badlands
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![
            Biome::Badlands,
            Biome::WoodedBadlands,
            Biome::ErodedBadlands,
        ])),
        Box::new(BandlandsRuleEx::new(system)),
    )));

    // Ocean biomes — use sand floor above stone
    let ocean_biomes = vec![
        Biome::Ocean,
        Biome::DeepOcean,
        Biome::WarmOcean,
        Biome::LukewarmOcean,
        Biome::ColdOcean,
        Biome::FrozenOcean,
        Biome::DeepLukewarmOcean,
        Biome::DeepColdOcean,
        Biome::DeepFrozenOcean,
    ];

    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(ocean_biomes)),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Sand)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Beach
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::Beach])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Sand)),
            )),
            Box::new(StateRuleEx::new(BlockId::Sandstone)),
        ])),
    )));

    // River
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::River])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::GrassBlock)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Swamp
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::Swamp])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::GrassBlock)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Snowy / frozen peaks
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![
            Biome::SnowyPlains,
            Biome::SnowySlopes,
            Biome::FrozenPeaks,
        ])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::SnowBlock)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Desert
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::Desert])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Sand)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Sandstone)),
            )),
            Box::new(StateRuleEx::new(BlockId::Sandstone)),
        ])),
    )));

    // Taiga
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![
            Biome::Taiga,
            Biome::OldGrowthPineTaiga,
            Biome::OldGrowthSpruceTaiga,
        ])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::GrassBlock)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Windswept Gravelly Hills
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::WindsweptGravellyHills])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Gravel)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Jungle
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::Jungle, Biome::BambooJungle])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::GrassBlock)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Mushroom Fields
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::MushroomFields])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Mycelium)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Dark Forest
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::DarkForest])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Podzol)),
            )),
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Dirt)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Stony / Mountain peaks
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![
            Biome::WindsweptHills,
            Biome::StonyPeaks,
            Biome::JaggedPeaks,
        ])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Stone)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Cave biomes — apply under the terrain surface (hole/cave conditions)
    // These run before the default grass/dirt/stone rule so they win for cave biomes.

    // Dripstone Caves: stone surface in cave openings
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::DripstoneCaves])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Stone)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Lush Caves: moss block on cave floors
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::LushCaves])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::MossBlock)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Deep Dark: stone surface (Sculk when block exists)
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::DeepDark])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Stone)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Sulfur Caves: stone surface
    rules.push(Box::new(TestRuleEx::new(
        Box::new(BiomeConditionEx::new(vec![Biome::SulfurCaves])),
        Box::new(SequenceRuleEx::new(vec![
            Box::new(TestRuleEx::new(
                Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
                Box::new(StateRuleEx::new(BlockId::Stone)),
            )),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ])),
    )));

    // Default: grass > dirt > stone
    rules.push(Box::new(SequenceRuleEx::new(vec![
        Box::new(TestRuleEx::new(
            Box::new(StoneDepthConditionEx::new(0, false, 0, CaveSurface::Floor)),
            Box::new(StateRuleEx::new(BlockId::GrassBlock)),
        )),
        Box::new(TestRuleEx::new(
            Box::new(StoneDepthConditionEx::new(0, true, 0, CaveSurface::Floor)),
            Box::new(StateRuleEx::new(BlockId::Dirt)),
        )),
        Box::new(StateRuleEx::new(BlockId::Stone)),
    ])));

    Box::new(SequenceRuleEx::new(rules))
}

// ============================================================================
// generate_surface — integration entry point
// ============================================================================

pub fn generate_surface(
    chunk: &mut Chunk,
    _router: &NoiseRouter,
    surface_rule: &dyn RuleSourceEx,
    sea_level: i32,
) {
    // This wrapper has no seed parameter for compatibility with existing
    // callers. Seed-aware callers should use generate_surface_with_biome_provider.
    generate_surface_with_biome_provider(
        chunk,
        _router,
        surface_rule,
        sea_level,
        0,
        |_, _, _| Biome::Plains,
    );
}

pub fn generate_surface_with_biome_provider(
    chunk: &mut Chunk,
    _router: &NoiseRouter,
    surface_rule: &dyn RuleSourceEx,
    sea_level: i32,
    world_seed: u64,
    biome_provider: impl FnMut(i32, i32, i32) -> Biome + Send + 'static,
) {
    let system_ref = SurfaceSystem::create_ref_from_seed(BlockId::Stone, sea_level, world_seed);
    SurfaceSystem::build_surface_with_biome_provider(
        system_ref,
        chunk,
        surface_rule,
        sea_level,
        0,
        biome_provider,
    );
}

/// Simple biome-to-surface-block mapping without the full rule system.
pub fn surface_blocks_for_biome(biome: Biome) -> (BlockId, BlockId, BlockId) {
    match biome {
        Biome::Badlands | Biome::ErodedBadlands => (BlockId::RedSand, BlockId::RedSand, BlockId::Terracotta),
        Biome::WoodedBadlands => (BlockId::CoarseDirt, BlockId::RedSand, BlockId::Terracotta),
        Biome::Beach => (BlockId::Sand, BlockId::Sand, BlockId::Sandstone),
        Biome::Desert => (BlockId::Sand, BlockId::Sandstone, BlockId::Sandstone),
        Biome::Ocean | Biome::DeepOcean
        | Biome::WarmOcean | Biome::LukewarmOcean
        | Biome::ColdOcean | Biome::FrozenOcean
        | Biome::DeepLukewarmOcean
        | Biome::DeepColdOcean | Biome::DeepFrozenOcean => {
            (BlockId::Water, BlockId::Sand, BlockId::Stone)
        }
        Biome::SnowyPlains | Biome::SnowySlopes | Biome::FrozenPeaks => {
            (BlockId::SnowBlock, BlockId::Dirt, BlockId::Stone)
        }
        Biome::WindsweptHills | Biome::StonyPeaks | Biome::JaggedPeaks => {
            (BlockId::Stone, BlockId::Stone, BlockId::Stone)
        }
        Biome::WindsweptGravellyHills => (BlockId::Gravel, BlockId::Stone, BlockId::Stone),
        Biome::MushroomFields => (BlockId::Mycelium, BlockId::Dirt, BlockId::Stone),
        Biome::DarkForest => (BlockId::Podzol, BlockId::Dirt, BlockId::Stone),
        Biome::Swamp => (BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
        _ => (BlockId::GrassBlock, BlockId::Dirt, BlockId::Stone),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::world_gen::density_fn::{constant, SinglePointContext};
    use std::sync::Mutex;

    #[test]
    fn test_generate_bands_length() {
        let mut rng = NoiseSeed::new(42);
        let bands = generate_bands(&mut rng);
        assert_eq!(bands.len(), 192);
        assert!(bands.iter().any(|&b| b == BlockId::Terracotta), "bands must contain terracotta");
        assert!(bands.iter().any(|&b| b == BlockId::WhiteTerracotta), "bands must contain white terracotta");
    }

    #[test]
    fn test_ore_veinifier_out_of_bounds() {
        let constant_neg = constant(-0.5);
        let constant_pos = constant(0.5);
        let constant_gap = constant(0.0);

        let pos = SinglePointContext {
            block_x: 0,
            block_y: 100,
            block_z: 0,
        };

        let base_random = NoiseSeed::new(42).fork_positional();
        let ore_random = base_random.from_hash_of("minecraft:ore").fork_positional();
        let result = OreVeinifier::apply(
            &constant_neg,
            &constant_pos,
            &constant_gap,
            &ore_random,
            &pos,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_ore_veinifier_uses_positional_random_factory() {
        let toggle = constant(0.5);
        let ridged = constant(-0.5);
        let gap = constant(0.0);
        let base_random = NoiseSeed::new(42).fork_positional();
        let ore_random = base_random.from_hash_of("minecraft:ore").fork_positional();

        let mut produced = false;
        for x in -8..=8 {
            for y in -60..=50 {
                for z in -8..=8 {
                    let pos = SinglePointContext {
                        block_x: x,
                        block_y: y,
                        block_z: z,
                    };
                    produced |= OreVeinifier::apply(
                        &toggle,
                        &ridged,
                        &gap,
                        &ore_random,
                        &pos,
                    )
                    .is_some();
                }
            }
        }
        assert!(produced);
    }

    #[test]
    fn test_surface_blocks_for_biome() {
        let (surface, subsurface, deep) = surface_blocks_for_biome(Biome::Plains);
        assert_eq!(surface, BlockId::GrassBlock);
        assert_eq!(subsurface, BlockId::Dirt);
        assert_eq!(deep, BlockId::Stone);

        let (surface, _, _) = surface_blocks_for_biome(Biome::Desert);
        assert_eq!(surface, BlockId::Sand);
    }

    fn build_continuous_stone_surface(
        chunk_x: i32,
        chunk_z: i32,
        biome: Biome,
        cave_y: Option<usize>,
    ) -> (Chunk, Arc<SurfaceSystemRef>) {
        let mut chunk = Chunk::new(chunk_x, chunk_z);
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 1..=64 {
                    chunk.set_block(x, y, z, Block::new(BlockId::Stone));
                }
            }
        }
        if let Some(y) = cave_y {
            chunk.set_block(0, y, 0, Block::new(BlockId::Air));
        }

        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x5EED);
        let source = StaticRuleSource::new(default_overworld_rules(system.clone()));
        SurfaceSystem::build_surface_with_biome_provider(
            system.clone(),
            &mut chunk,
            &source,
            63,
            0,
            move |_, _, _| biome,
        );
        (chunk, system)
    }

    #[test]
    fn simplified_biome_fallbacks_do_not_replace_deep_continuous_stone() {
        for (biome, expected_surface) in [
            (Biome::Desert, BlockId::Sand),
            (Biome::Beach, BlockId::Sand),
        ] {
            let (chunk, _) = build_continuous_stone_surface(0, 0, biome, None);
            assert_eq!(chunk.get_block(0, 64, 0).id, expected_surface, "{biome:?} surface");
            assert_eq!(chunk.get_block(0, 1, 0).id, BlockId::Stone, "{biome:?} deep stone");
        }

        let (chunk, system) = build_continuous_stone_surface(0, 0, Biome::Badlands, None);
        assert_eq!(chunk.get_block(0, 64, 0).id, system.get_band(0, 64, 0));
        assert_eq!(chunk.get_block(0, 1, 0).id, BlockId::Stone);
    }

    #[test]
    fn ordinary_surface_materials_do_not_reach_deep_plains_caves() {
        let (chunk, _) = build_continuous_stone_surface(0, 0, Biome::Plains, Some(40));
        assert_eq!(chunk.get_block(0, 39, 0).id, BlockId::Stone);
    }

    #[test]
    fn ordinary_surface_materials_reach_shallow_plains_caves() {
        let (chunk, _) = build_continuous_stone_surface(0, 0, Biome::Plains, Some(60));
        assert_eq!(chunk.get_block(0, 59, 0).id, BlockId::GrassBlock);
    }

    #[test]
    fn deep_cave_surface_scope_bypasses_the_ordinary_surface_gate() {
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x5EED);
        let mut ctx = SurfaceContext::new(
            system,
            63,
            Box::new(|_, _, _| Biome::Plains),
            Box::new(|_, _| 64),
        );
        ctx.update_xz(0, 0);
        ctx.update_y(1, 1, i32::MIN, 40);

        assert!(!ctx.permits_surface_rule(SurfaceRuleScope::Ordinary));
        assert!(ctx.permits_surface_rule(SurfaceRuleScope::DeepCaveException));
    }

    #[test]
    fn plains_top_surface_retains_grass_and_dirt_layers() {
        let (chunk, _) = build_continuous_stone_surface(0, 0, Biome::Plains, None);
        assert_eq!(chunk.get_block(0, 64, 0).id, BlockId::GrassBlock);
        assert_eq!(chunk.get_block(0, 63, 0).id, BlockId::Dirt);
    }

    #[test]
    fn surface_biome_provider_uses_y_within_one_column() {
        let mut chunk = Chunk::new(0, 0);
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 1..=64 {
                    chunk.set_block(x, y, z, Block::new(BlockId::Stone));
                }
            }
        }

        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x5EED);
        let source = StaticRuleSource::new(default_overworld_rules(system.clone()));
        let queried_y = Arc::new(Mutex::new(Vec::new()));
        let provider_queries = queried_y.clone();
        let mut _calls = 0;
        SurfaceSystem::build_surface_with_biome_provider(
            system,
            &mut chunk,
            &source,
            63,
            0,
            move |x, y, z| {
                _calls += 1;
                if (x, z) == (0, 0) {
                    provider_queries
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .push(y);
                }
                if y == 64 { Biome::Desert } else { Biome::Plains }
            },
        );

        assert_eq!(chunk.get_block(0, 64, 0).id, BlockId::Sand);
        assert_eq!(chunk.get_block(0, 63, 0).id, BlockId::Dirt);
        let queried_y = queried_y
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(queried_y.contains(&64));
        assert!(queried_y.contains(&63));
    }

    #[test]
    fn preliminary_surface_gate_is_stable_at_negative_chunk_coordinates() {
        let (first, _) = build_continuous_stone_surface(-3, -2, Biome::Plains, Some(40));
        let (second, _) = build_continuous_stone_surface(-3, -2, Biome::Plains, Some(40));

        assert_eq!(first.blocks, second.blocks);
        assert_eq!(first.get_block(0, 39, 0).id, BlockId::Stone);
        assert_eq!(first.get_block(0, 64, 0).id, BlockId::GrassBlock);
    }

    #[test]
    fn test_clamped_map() {
        let result = clamped_map(0.5, 0.0, 1.0, 0.0, 10.0);
        assert!((result - 5.0).abs() < 1e-9);

        let result = clamped_map(-1.0, 0.0, 1.0, 0.0, 10.0);
        assert!((result - 0.0).abs() < 1e-9);

        let result = clamped_map(2.0, 0.0, 1.0, 0.0, 10.0);
        assert!((result - 10.0).abs() < 1e-9);
    }

    #[test]
    fn seeded_surface_noise_uses_world_positional_factory() {
        let seed = 0x5eed_u64;
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, seed);

        let mut world_random = NoiseSeed::new(seed);
        let positional = world_random.fork_positional();
        let mut random = positional.from_hash_of("minecraft:surface");
        let parameters = crate::world::world_gen::noise_router::create_noise_parameters(&NoiseKey::Surface);
        let expected = NormalNoise::create(&mut random, &parameters).get_value(12.5, 0.0, -8.25);

        assert_eq!(system.surface_noise.sample(12.5, 0.0, -8.25), expected);
        assert_ne!(
            system.surface_noise.sample(12.5, 0.0, -8.25),
            SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, seed.wrapping_add(1))
                .surface_noise
                .sample(12.5, 0.0, -8.25)
        );
    }

    #[test]
    fn surface_depth_uses_deterministic_positional_coordinate_random() {
        let seed = 0x1234_5678_u64;
        let block_x = 12345;
        let block_z = -54321;
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, seed);

        let mut world_random = NoiseSeed::new(seed);
        let positional = world_random.fork_positional();
        let mut coordinate_random = positional.at(block_x, 0, block_z);
        let expected = (system.surface_noise.sample(block_x as f64, 0.0, block_z as f64) * 2.75
            + 3.0
            + coordinate_random.next_double() * 0.25) as i32;

        assert_eq!(system.get_surface_depth(block_x, block_z), expected);
        assert_eq!(system.get_surface_depth(block_x, block_z), expected);
    }

    #[test]
    fn reference_water_condition_matches_java_no_water_and_offset_semantics() {
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x26_02);
        let mut ctx = SurfaceContext::new_with_height(
            system,
            63,
            -64,
            384,
            Box::new(|_, _, _| Biome::Plains),
            Box::new(|_, _| 64),
        );
        ctx.update_xz(0, 0);
        ctx.surface_depth = 3;
        ctx.update_y(2, 10, i32::MIN, 60);
        assert!(WaterConditionEx::new(-1, 0, false).test(&mut ctx));

        ctx.update_y(2, 10, 64, 60);
        assert!(!WaterConditionEx::new(-1, 0, false).test(&mut ctx));
        assert!(WaterConditionEx::new(-6, -1, true).test(&mut ctx));
        assert!(60 + 2 >= 64 - 6 + 3 * -1);
    }

    #[test]
    fn reference_steep_uses_world_surface_wg_column_deltas() {
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 7);
        let mut ctx = SurfaceContext::new(system, 63, Box::new(|_, _, _| Biome::FrozenPeaks), Box::new(|_, _| 64));
        let mut heightmap = vec![60; CHUNK_SIZE * CHUNK_SIZE];
        heightmap[3 * CHUNK_SIZE + 5] = 64;
        ctx.world_surface_heightmap = Some(heightmap);
        ctx.update_xz(-13, -12); // local (3, 4), south is local z=5
        assert!(SteepConditionEx.test(&mut ctx));

        ctx.world_surface_heightmap = Some(vec![60; CHUNK_SIZE * CHUNK_SIZE]);
        assert!(!SteepConditionEx.test(&mut ctx));
    }

    #[test]
    fn reference_preliminary_surface_bilinear_interpolation_is_negative_safe() {
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 9);
        let mut ctx = SurfaceContext::new_with_height(
            system,
            63,
            -64,
            384,
            Box::new(|_, _, _| Biome::Plains),
            Box::new(|x, z| 80 + x.div_euclid(16) * 16 + z.div_euclid(16) * 32),
        );
        ctx.reference_preliminary_interpolation = true;
        ctx.update_xz(-1, -1);
        ctx.surface_depth = 3;
        assert_eq!(ctx.get_min_surface_level(), 72);
        ctx.update_y(1, 1, i32::MIN, 71);
        assert!(!AbovePreliminarySurfaceConditionEx.test(&mut ctx));
        ctx.update_y(1, 1, i32::MIN, 72);
        assert!(AbovePreliminarySurfaceConditionEx.test(&mut ctx));
    }

    #[test]
    fn reference_vertical_gradient_uses_named_positional_random_factory() {
        let seed = 0x26_02;
        let mut ctx = SurfaceContext::new_with_height(
            SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, seed),
            63,
            -64,
            384,
            Box::new(|_, _, _| Biome::Plains),
            Box::new(|_, _| 64),
        );
        let bedrock = VerticalGradientConditionEx::new(
            seed,
            "minecraft:bedrock_floor",
            VerticalAnchor::AboveBottom(0),
            VerticalAnchor::AboveBottom(5),
        );
        ctx.update_xz(-37, 91);
        ctx.update_y(1, 1, i32::MIN, -62);

        let mut root = NoiseSeed::new(seed);
        let positional = root.fork_positional();
        let mut named = positional.from_hash_of("minecraft:bedrock_floor");
        let expected = named.fork_positional().at(-37, -62, 91).next_float() < 0.6;
        assert_eq!(bedrock.test(&mut ctx), expected);

        let deepslate = VerticalGradientConditionEx::new(
            seed,
            "minecraft:deepslate",
            VerticalAnchor::AboveBottom(0),
            VerticalAnchor::AboveBottom(5),
        );
        assert!((-128..=128).any(|x| {
            ctx.update_xz(x, -19);
            ctx.update_y(1, 1, i32::MIN, -62);
            bedrock.test(&mut ctx) != deepslate.test(&mut ctx)
        }));
    }

    #[test]
    fn reference_surface_noise_above_scales_thresholds_but_clay_bands_do_not() {
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x26_02);
        let mut ctx = SurfaceContext::new_with_height(
            system.clone(),
            63,
            -64,
            384,
            Box::new(|_, _, _| Biome::WindsweptHills),
            Box::new(|_, _| 64),
        );
        let (scaled_x, scaled_z) = (-512..=512)
            .flat_map(|x| (-512..=512).map(move |z| (x, z)))
            .find(|&(x, z)| {
                let value = system.surface_noise.sample(x as f64, 0.0, z as f64);
                (1.0 / 8.25..1.0).contains(&value)
            })
            .unwrap();
        ctx.update_xz(scaled_x, scaled_z);
        ctx.update_y(1, 1, i32::MIN, 64);
        assert!(reference_surface_noise_above(&system, 1.0).test(&mut ctx));
        assert!(!reference_noise(&system, NoiseKey::Surface, 1.0, f64::MAX).test(&mut ctx));

        let (clay_x, clay_z) = (-512..=512)
            .flat_map(|x| (-512..=512).map(move |z| (x, z)))
            .find(|&(x, z)| {
                let value = system.surface_noise.sample(x as f64, 0.0, z as f64);
                (-0.1818..=0.1818).contains(&value)
            })
            .unwrap();
        ctx.update_xz(clay_x, clay_z);
        assert!(reference_noise(&system, NoiseKey::Surface, -0.1818, 0.1818).test(&mut ctx));
    }

    #[test]
    fn unsupported_sulfur_bands_do_not_bypass_deepslate_elsewhere() {
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x26_02);
        let mut root = NoiseSeed::new(system.noise_random_seed);
        let positional = root.fork_positional();
        let sulfur = seeded_surface_noise(&positional, NoiseKey::SulfurCaveGradient);
        let mut samples = (-512..=512).flat_map(|x| (-512..=512).map(move |z| (x, z)));
        let matching = samples
            .clone()
            .find(|&(x, z)| {
                let value = sulfur.sample(x as f64, -1.0, z as f64);
                (-0.4..=-0.1).contains(&value) || value >= 0.0
            })
            .unwrap();
        let ordinary = samples
            .find(|&(x, z)| {
                let value = sulfur.sample(x as f64, -1.0, z as f64);
                value < -0.4 || (-0.1..0.0).contains(&value)
            })
            .unwrap();
        let mut ctx = SurfaceContext::new_with_height(
            system.clone(),
            63,
            -64,
            384,
            Box::new(|_, _, _| Biome::SulfurCaves),
            Box::new(|_, _| 64),
        );
        let rule = minecraft26_reference_overworld_rules(system);

        ctx.update_xz(matching.0, matching.1);
        ctx.update_y_with_block(1, 1, i32::MIN, -1, BlockId::Stone);
        assert_eq!(rule.try_apply(&mut ctx, matching.0, -1, matching.1), Some(BlockId::Stone));

        ctx.update_xz(ordinary.0, ordinary.1);
        ctx.update_y_with_block(1, 1, i32::MIN, -1, BlockId::Stone);
        assert_eq!(rule.try_apply(&mut ctx, ordinary.0, -1, ordinary.1), Some(BlockId::Deepslate));
    }

    #[test]
    fn frozen_ocean_temperature_modifier_controls_ice_and_iceberg_melting() {
        assert_eq!(frozen_ocean_temperature(0, 63, 0, 63), 0.2);
        assert!(frozen_ocean_iceberg_melts_slightly(0, 0, 63));
        let non_melting = (-512..=512)
            .flat_map(|x| (-512..=512).map(move |z| (x, z)))
            .find(|&(x, z)| !frozen_ocean_iceberg_melts_slightly(x, z, 63))
            .unwrap();
        assert_eq!(non_melting, (-512, -512));
        assert_eq!(frozen_ocean_temperature(-512, 63, -512, 63), 0.0);

        let mut ctx = SurfaceContext::new_with_height(
            SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 1),
            63,
            -64,
            384,
            Box::new(|_, _, _| Biome::FrozenOcean),
            Box::new(|_, _| 63),
        );
        ctx.update_xz(0, 0);
        ctx.update_y(1, 1, i32::MIN, 63);
        assert!(!TemperatureConditionEx.test(&mut ctx));
        ctx.update_xz(-512, -512);
        ctx.update_y(1, 1, i32::MIN, 63);
        assert!(TemperatureConditionEx.test(&mut ctx));

        let cold = frozen_ocean_extension_bounds(2.0, 10.0, 63, false).unwrap();
        let melting = frozen_ocean_extension_bounds(2.0, 10.0, 63, true).unwrap();
        assert_eq!(cold, (67.8, 51.2));
        assert_eq!(melting, (65.8, 53.2));
    }

    fn build_reference_fixture(
        chunk_x: i32,
        chunk_z: i32,
        biome: Biome,
        top_world_y: i32,
        water_top: Option<i32>,
        cave_air_y: Option<i32>,
    ) -> Chunk {
        let world_min_y = -64;
        let mut chunk = Chunk::new(chunk_x, chunk_z);
        let top_local = (top_world_y - world_min_y) as usize;
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..=top_local {
                    chunk.set_block(x, y, z, Block::new(BlockId::Stone));
                }
                if let Some(water_top) = water_top {
                    for world_y in top_world_y + 1..=water_top {
                        chunk.set_block(x, (world_y - world_min_y) as usize, z, Block::new(BlockId::Water));
                    }
                }
            }
        }
        if let Some(cave_air_y) = cave_air_y {
            chunk.set_block(0, (cave_air_y - world_min_y) as usize, 0, Block::new(BlockId::Air));
        }
        let system = SurfaceSystem::create_ref_from_seed(BlockId::Stone, 63, 0x5EED);
        SurfaceSystem::build_reference_overworld_surface(
            system,
            &mut chunk,
            63,
            world_min_y,
            move |_, _, _| biome,
            move |_, _| top_world_y,
        );
        chunk
    }

    #[test]
    fn reference_representative_plains_desert_badlands_and_mountain_columns() {
        let plains = build_reference_fixture(-2, 3, Biome::Plains, 64, None, None);
        let desert = build_reference_fixture(-2, 3, Biome::Desert, 64, None, None);
        let badlands = build_reference_fixture(-2, 3, Biome::Badlands, 64, None, None);
        let mountain = build_reference_fixture(-2, 3, Biome::StonyPeaks, 96, None, None);
        assert_eq!(plains.get_block(0, 128, 0).id, BlockId::GrassBlock);
        assert_eq!(desert.get_block(0, 128, 0).id, BlockId::Sand);
        assert_eq!(badlands.get_block(0, 128, 0).id, BlockId::RedSand);
        assert!(matches!(mountain.get_block(0, 160, 0).id, BlockId::Stone | BlockId::Calcite));
    }

    #[test]
    fn reference_representative_frozen_ocean_and_cave_columns() {
        let frozen = build_reference_fixture(-3, -4, Biome::FrozenOcean, 50, Some(63), None);
        assert_eq!(frozen.get_block(0, 127, 0).id, BlockId::Water);
        assert!(matches!(frozen.get_block(0, 114, 0).id, BlockId::Stone | BlockId::Gravel | BlockId::Water));

        let cave = build_reference_fixture(-3, -4, Biome::DripstoneCaves, 64, None, Some(62));
        assert_eq!(cave.get_block(0, 125, 0).id, BlockId::Stone);
    }

    #[test]
    fn reference_world_surface_wg_height_includes_fluids() {
        let chunk = build_reference_fixture(-1, -1, Biome::FrozenOcean, 50, Some(63), None);
        let heightmap = build_world_surface_wg_heightmap(&chunk, -64);
        assert_eq!(heightmap[0], 63);
    }

    #[test]
    fn test_biome_condition() {
        let system = Arc::new(SurfaceSystemRef {
            default_block: BlockId::Stone,
            sea_level: 63,
            clay_bands: vec![BlockId::Terracotta; 192],
            clay_bands_offset_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::ClayBandsOffset,
                ),
            )),
            surface_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::Surface,
                ),
            )),
            surface_secondary_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::SurfaceSecondary,
                ),
            )),
            badlands_pillar_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::BadlandsPillar,
                ),
            )),
            badlands_pillar_roof_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::BadlandsPillarRoof,
                ),
            )),
            badlands_surface_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::BadlandsSurface,
                ),
            )),
            iceberg_pillar_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::IcebergPillar,
                ),
            )),
            iceberg_pillar_roof_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::IcebergPillarRoof,
                ),
            )),
            iceberg_surface_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::IcebergSurface,
                ),
            )),
            noise_random_seed: 0,
        });

        let mut ctx = SurfaceContext::new(
            system,
            63,
            Box::new(|_, _, _| Biome::Plains),
            Box::new(|_, _| 63),
        );
        ctx.update_xz(0, 0);
        ctx.update_y(1, 100, i32::MIN, 62);

        let condition = BiomeConditionEx::new(vec![Biome::Plains]);
        assert!(condition.test(&mut ctx));

        let condition = BiomeConditionEx::new(vec![Biome::Desert]);
        assert!(!condition.test(&mut ctx));
    }

    #[test]
    fn test_sequence_rule() {
        let system = Arc::new(SurfaceSystemRef {
            default_block: BlockId::Stone,
            sea_level: 63,
            clay_bands: vec![BlockId::Terracotta; 192],
            clay_bands_offset_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::ClayBandsOffset,
                ),
            )),
            surface_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::Surface,
                ),
            )),
            surface_secondary_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::SurfaceSecondary,
                ),
            )),
            badlands_pillar_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::BadlandsPillar,
                ),
            )),
            badlands_pillar_roof_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::BadlandsPillarRoof,
                ),
            )),
            badlands_surface_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::BadlandsSurface,
                ),
            )),
            iceberg_pillar_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::IcebergPillar,
                ),
            )),
            iceberg_pillar_roof_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::IcebergPillarRoof,
                ),
            )),
            iceberg_surface_noise: NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::new(0),
                &crate::world::world_gen::noise_router::create_noise_parameters(
                    &crate::world::world_gen::noise::NoiseKey::IcebergSurface,
                ),
            )),
            noise_random_seed: 0,
        });

        let rule = SequenceRuleEx::new(vec![
            Box::new(StateRuleEx::new(BlockId::Dirt)),
            Box::new(StateRuleEx::new(BlockId::Stone)),
        ]);

        let mut ctx = SurfaceContext::new(system, 63, Box::new(|_, _, _| Biome::Plains), Box::new(|_, _| 63));
        ctx.update_xz(0, 0);
        ctx.update_y(1, 100, i32::MIN, 62);

        assert_eq!(rule.try_apply(&mut ctx, 0, 62, 0), Some(BlockId::Dirt));
    }
}
