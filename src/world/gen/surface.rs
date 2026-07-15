#![allow(dead_code)]

use crate::world::block::BlockId;
use crate::world::gen::noise::NormalNoise;
use crate::world::world_gen::Biome;

// --- Context ---

pub struct SurfaceContext {
    pub block_x: i32,
    pub block_z: i32,
    pub block_y: i32,
    pub surface_depth: i32,
    pub surface_secondary: f64,
    pub biome: Biome,
    pub water_height: i32,
    pub stone_depth_above: i32,
    pub stone_depth_below: i32,
    pub min_surface_level: i32,
}

// --- Surface System ---

pub struct SurfaceSystem {
    pub surface_noise: NormalNoise,
    pub surface_secondary_noise: NormalNoise,
}

impl SurfaceSystem {
    pub fn new(seed: u64) -> Self {
        let s = seed as i64;
        SurfaceSystem {
            surface_noise: NormalNoise::new(s.wrapping_add(1), -7, &[1.0, 0.5, 0.25]),
            surface_secondary_noise: NormalNoise::new(s.wrapping_add(2), -7, &[1.0, 0.5, 0.25]),
        }
    }

    pub fn get_surface_depth(&self, block_x: i32, block_z: i32) -> i32 {
        let noise = self
            .surface_noise
            .get_value(block_x as f64 * 0.01, 0.0, block_z as f64 * 0.01);
        (noise * 2.75 + 3.0) as i32
    }

    pub fn get_surface_secondary(&self, block_x: i32, block_z: i32) -> f64 {
        self.surface_secondary_noise
            .get_value(block_x as f64 * 0.02, 0.0, block_z as f64 * 0.02)
    }
}

// --- Conditions ---

pub trait SurfaceCondition: Send + Sync {
    fn test(&self, ctx: &SurfaceContext) -> bool;
}

pub struct BiomeCondition {
    pub biomes: Vec<Biome>,
}

impl SurfaceCondition for BiomeCondition {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        self.biomes.contains(&ctx.biome)
    }
}

pub enum SurfaceType {
    Floor,
    Ceiling,
}

pub struct StoneDepthCondition {
    pub offset: i32,
    pub add_surface_depth: bool,
    pub surface_type: SurfaceType,
}

impl SurfaceCondition for StoneDepthCondition {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        let depth = ctx.stone_depth_above
            + self.offset
            + if self.add_surface_depth {
                ctx.surface_depth
            } else {
                0
            };
        match self.surface_type {
            SurfaceType::Floor => depth <= 1 + ctx.surface_depth,
            SurfaceType::Ceiling => ctx.stone_depth_below <= 1,
        }
    }
}

pub struct YCondition {
    pub min_y: i32,
    pub max_y: i32,
}

impl SurfaceCondition for YCondition {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        ctx.block_y >= self.min_y && ctx.block_y <= self.max_y
    }
}

pub struct WaterCondition {
    pub offset: i32,
}

impl SurfaceCondition for WaterCondition {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        ctx.block_y + ctx.stone_depth_above <= ctx.water_height + self.offset
    }
}

pub struct AbovePreliminarySurface;

impl SurfaceCondition for AbovePreliminarySurface {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        ctx.block_y >= ctx.min_surface_level
    }
}

pub struct HoleCondition;

impl SurfaceCondition for HoleCondition {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        ctx.surface_depth <= 0
    }
}

pub struct SteepCondition;

impl SurfaceCondition for SteepCondition {
    fn test(&self, ctx: &SurfaceContext) -> bool {
        ctx.stone_depth_above >= 4
    }
}

// --- Rules ---

pub trait SurfaceRule: Send + Sync {
    fn try_apply(&self, ctx: &SurfaceContext) -> Option<BlockId>;
}

pub struct StateRule(pub BlockId);

impl SurfaceRule for StateRule {
    fn try_apply(&self, _ctx: &SurfaceContext) -> Option<BlockId> {
        Some(self.0)
    }
}

pub struct TestRule {
    pub condition: Box<dyn SurfaceCondition>,
    pub then_run: Box<dyn SurfaceRule>,
}

impl SurfaceRule for TestRule {
    fn try_apply(&self, ctx: &SurfaceContext) -> Option<BlockId> {
        if self.condition.test(ctx) {
            self.then_run.try_apply(ctx)
        } else {
            None
        }
    }
}

pub struct SequenceRule {
    pub sequence: Vec<Box<dyn SurfaceRule>>,
}

impl SurfaceRule for SequenceRule {
    fn try_apply(&self, ctx: &SurfaceContext) -> Option<BlockId> {
        for rule in &self.sequence {
            if let Some(result) = rule.try_apply(ctx) {
                return Some(result);
            }
        }
        None
    }
}

// --- Default Overworld Surface Rule ---

pub fn default_overworld_surface() -> Box<dyn SurfaceRule> {
    Box::new(SequenceRule {
        sequence: vec![
            Box::new(TestRule {
                condition: Box::new(BiomeCondition {
                    biomes: vec![Biome::Badlands],
                }),
                then_run: Box::new(SequenceRule {
                    sequence: vec![
                        Box::new(TestRule {
                            condition: Box::new(StoneDepthCondition {
                                offset: 0,
                                add_surface_depth: false,
                                surface_type: SurfaceType::Floor,
                            }),
                            then_run: Box::new(StateRule(BlockId::RedSand)),
                        }),
                        Box::new(StateRule(BlockId::Terracotta)),
                    ],
                }),
            }),
            Box::new(TestRule {
                condition: Box::new(BiomeCondition {
                    biomes: vec![Biome::Beach],
                }),
                then_run: Box::new(StateRule(BlockId::Sand)),
            }),
            Box::new(TestRule {
                condition: Box::new(BiomeCondition {
                    biomes: vec![Biome::Desert],
                }),
                then_run: Box::new(SequenceRule {
                    sequence: vec![
                        Box::new(TestRule {
                            condition: Box::new(StoneDepthCondition {
                                offset: 0,
                                add_surface_depth: false,
                                surface_type: SurfaceType::Floor,
                            }),
                            then_run: Box::new(StateRule(BlockId::Sand)),
                        }),
                        Box::new(StateRule(BlockId::Sandstone)),
                    ],
                }),
            }),
            Box::new(TestRule {
                condition: Box::new(StoneDepthCondition {
                    offset: 0,
                    add_surface_depth: false,
                    surface_type: SurfaceType::Floor,
                }),
                then_run: Box::new(StateRule(BlockId::GrassBlock)),
            }),
            Box::new(TestRule {
                condition: Box::new(StoneDepthCondition {
                    offset: 0,
                    add_surface_depth: true,
                    surface_type: SurfaceType::Floor,
                }),
                then_run: Box::new(StateRule(BlockId::Dirt)),
            }),
            Box::new(StateRule(BlockId::Stone)),
        ],
    })
}

// --- Badlands Terracotta Bands ---

pub fn badlands_terracotta_band(y: i32, x: i32, z: i32, clay_noise: &NormalNoise) -> BlockId {
    let bands = [
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::OrangeTerracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::YellowTerracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::BrownTerracotta,
        BlockId::BrownTerracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::RedTerracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::Terracotta,
        BlockId::WhiteTerracotta,
        BlockId::LightGrayTerracotta,
        BlockId::WhiteTerracotta,
        BlockId::LightGrayTerracotta,
    ];

    let mut full_bands = Vec::with_capacity(192);
    for i in 0..192 {
        full_bands.push(bands[i % bands.len()]);
    }

    let offset = (clay_noise.get_value(x as f64 * 0.01, 0.0, z as f64 * 0.01) * 4.0).round() as i32;
    let idx = ((y + offset + 192).unsigned_abs() as usize) % 192;
    full_bands[idx]
}
