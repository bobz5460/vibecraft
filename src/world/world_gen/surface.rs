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
use crate::world::world_gen::Biome;
use std::sync::Arc;

// ============================================================================
// CaveSurface
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaveSurface {
    Floor,
    Ceiling,
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
    pub(crate) biome: Option<Biome>,
    pub(crate) last_update_y: u64,

    // Sea level
    pub(crate) sea_level: i32,
    pub(crate) world_min_y: i32,
    pub(crate) world_height: i32,
    // Height getter (wx, wy, wz) -> biome
    pub(crate) get_height: Box<dyn Fn(i32, i32, i32) -> Biome + Send + Sync>,
    // Preliminary surface level getter (x, z) -> y
    pub(crate) get_preliminary_surface: Box<dyn Fn(i32, i32) -> i32 + Send + Sync>,
}

impl SurfaceContext {
    pub fn new(
        system: Arc<SurfaceSystemRef>,
        sea_level: i32,
        get_height: Box<dyn Fn(i32, i32, i32) -> Biome + Send + Sync>,
        get_preliminary_surface: Box<dyn Fn(i32, i32) -> i32 + Send + Sync>,
    ) -> Self {
        Self::new_with_height(system, sea_level, 0, CHUNK_HEIGHT as i32, get_height, get_preliminary_surface)
    }

    pub fn new_with_height(
        system: Arc<SurfaceSystemRef>,
        sea_level: i32,
        world_min_y: i32,
        world_height: i32,
        get_height: Box<dyn Fn(i32, i32, i32) -> Biome + Send + Sync>,
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
            biome: None,
            last_update_y: 0,
            sea_level,
            world_min_y,
            world_height,
            get_height,
            get_preliminary_surface,
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
            let prelim = (self.get_preliminary_surface)(self.block_x, self.block_z);
            self.min_surface_level = prelim + self.surface_depth - 8;
        }
        self.min_surface_level
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
            return false;
        }
        let y = ctx.block_y + if self.add_stone_depth { ctx.stone_depth_above } else { 0 };
        y + self.offset >= ctx.water_height + ctx.surface_depth * self.surface_depth_multiplier
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
    random_noise: NoiseHandle,
}

impl VerticalGradientConditionEx {
    pub fn new(
        seed: u64,
        _random_name: &str,
        true_at_and_below: VerticalAnchor,
        false_at_and_above: VerticalAnchor,
    ) -> Self {
        let mut rng = NoiseSeed::new(seed);
        let params = crate::world::world_gen::noise_router::create_noise_parameters(
            &crate::world::world_gen::noise::NoiseKey::Temperature,
        );
        let noise = NormalNoise::create(&mut rng, &params);
        VerticalGradientConditionEx {
            true_at_and_below,
            false_at_and_above,
            random_noise: NoiseHandle::new(noise),
        }
    }
}

impl ConditionEx for VerticalGradientConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let true_y = self.true_at_and_below.resolve(0, 384);
        let false_y = self.false_at_and_above.resolve(0, 384);
        let noise = self.random_noise.sample(ctx.block_x as f64, 0.0, ctx.block_z as f64);
        let gradient = (ctx.block_y as f64 - true_y as f64) / (false_y - true_y) as f64;
        let gradient = gradient.clamp(0.0, 1.0);
        let threshold = gradient + noise * 0.2;
        threshold <= 0.5
    }

    fn clone_box(&self) -> Box<dyn ConditionEx> {
        Box::new(self.clone())
    }
}

// --- TemperatureCondition ---

#[derive(Clone)]
pub struct TemperatureConditionEx;

impl ConditionEx for TemperatureConditionEx {
    fn test(&self, ctx: &mut SurfaceContext) -> bool {
        let biome = ctx.get_biome(ctx.block_y);
        matches!(
            biome,
            Biome::SnowyTundra
                | Biome::Taiga
                | Biome::FrozenOcean
                | Biome::DeepFrozenOcean
                | Biome::FrozenPeaks
                | Biome::JaggedPeaks
                | Biome::SnowySlopes
                | Biome::Grove
                | Biome::Mountains
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
        let chunk_x = ctx.block_x & 0xF;
        let chunk_z = ctx.block_z & 0xF;

        let z_north = chunk_z.saturating_sub(1).max(0);
        let z_south = (chunk_z + 1).min(15);

        let height_north = 0;
        let height_south = 0;
        let _ = (z_north, z_south, height_north, height_south);

        let x_west = chunk_x.saturating_sub(1).max(0);
        let x_east = (chunk_x + 1).min(15);

        let height_west = 0;
        let height_east = 0;
        let _ = (x_west, x_east, height_west, height_east);

        false
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
        biome_provider: impl Fn(i32, i32, i32) -> Biome + Send + Sync + 'static,
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

                        if blk.id == system.default_block {
                            if let Some(new_state) = rule.try_apply(&mut ctx, block_x, world_y, block_z) {
                                chunk.set_block(x, y as usize, z, Block::new(new_state));
                            }
                        }
                    }
                }
            }
        }
    }
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
        Biome::DeepWarmOcean,
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
            Biome::SnowyTundra,
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
            Biome::Mountains,
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
    biome_provider: impl Fn(i32, i32, i32) -> Biome + Send + Sync + 'static,
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
        | Biome::DeepWarmOcean | Biome::DeepLukewarmOcean
        | Biome::DeepColdOcean | Biome::DeepFrozenOcean => {
            (BlockId::Water, BlockId::Sand, BlockId::Stone)
        }
        Biome::SnowyTundra | Biome::SnowySlopes | Biome::FrozenPeaks => {
            (BlockId::SnowBlock, BlockId::Dirt, BlockId::Stone)
        }
        Biome::Mountains | Biome::WindsweptHills | Biome::StonyPeaks | Biome::JaggedPeaks => {
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
