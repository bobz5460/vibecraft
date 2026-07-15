#![allow(dead_code)]

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use crate::world::gen::aquifer::{Aquifer, AquiferResult};
use crate::world::gen::caves::{CaveSystem, OreVeinSystem, VeinResult};
use crate::world::gen::density::Context;
use crate::world::gen::noise::SimpleRandom;
use crate::world::gen::router::{
    NoiseRouter, NoiseRouterData, SimpleTerrainSplines,
    OVERWORLD_MIN_Y, OVERWORLD_SEA_LEVEL,
};
use crate::world::gen::surface::{SurfaceSystem, SurfaceContext, SurfaceRule, default_overworld_surface};

/// Holds all terrain generation subsystems for a world seed.
pub struct TerrainPipeline {
    pub seed: u64,
    pub router: NoiseRouter,
    pub splines: SimpleTerrainSplines,
    pub cave_system: CaveSystem,
    pub ore_veins: OreVeinSystem,
    pub aquifer: Aquifer,
    pub surface_system: SurfaceSystem,
    pub surface_rule: Box<dyn SurfaceRule>,
    pub sea_level: i32,
}

impl TerrainPipeline {
    pub fn new(seed: u64) -> Self {
        let router = NoiseRouterData::create_overworld_router(seed);
        let splines = SimpleTerrainSplines::new(seed);
        TerrainPipeline {
            seed,
            router,
            splines,
            cave_system: CaveSystem::new(seed),
            ore_veins: OreVeinSystem::new(seed),
            aquifer: Aquifer::new(seed, OVERWORLD_SEA_LEVEL),
            surface_system: SurfaceSystem::new(seed),
            surface_rule: default_overworld_surface(),
            sea_level: OVERWORLD_SEA_LEVEL,
        }
    }

    /// Evaluate the master density at a world position.
    /// Returns a negative value for air/cave, positive for solid.
    pub fn density_at(&self, x: f64, y: f64, z: f64) -> f64 {
        let ctx = Context { block_x: x, block_y: y, block_z: z, min_y: OVERWORLD_MIN_Y };
        self.router.final_density.compute(&ctx)
    }
}

/// Generate a chunk using the new density-based terrain pipeline.
/// This is designed to work alongside or replace the existing `WorldGenerator::generate_chunk`.
pub fn generate_chunk_terrain(chunk: &mut Chunk, pipe: &TerrainPipeline) {
    let base_x = chunk.cx as i64 * CHUNK_SIZE as i64;
    let base_z = chunk.cz as i64 * CHUNK_SIZE as i64;

    // Pre-compute climate/biome info per column
    let mut column_data: Vec<(i32, /* biome */ crate::world::world_gen::Biome)> = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE);

    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let wx = (base_x + x as i64) as f64;
            let wz = (base_z + z as i64) as f64;

            // Use the density to find surface height (first Y where density flips from negative to positive)
            let surface_y = find_surface_y(wx, wz, pipe);
            let biome = sample_biome(wx, wz, pipe);

            column_data.push((surface_y, biome));
        }
    }

    // Generate terrain: iterate blocks and evaluate density
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let wx = (base_x + x as i64) as f64;
            let wz = (base_z + z as i64) as f64;
            let idx = x * CHUNK_SIZE + z;
            let (surface_y, biome) = column_data[idx];

            // Bedrock at bottom
            chunk.set_block(x, 0, z, Block::new(BlockId::Bedrock));

            for y in 1..CHUNK_HEIGHT {
                let wy = y as f64;
                let density = pipe.density_at(wx, wy, wz);

                if density > 0.0 {
                    // Solid block
                    let block = determine_solid_block(wx, wy, wz, y, surface_y, biome, pipe);
                    chunk.set_block(x, y, z, block);
                } else {
                    // Non-solid: check aquifer
                    let aquifer_result = pipe.aquifer.compute_substance(wx, wy, wz, density);
                    match aquifer_result {
                        AquiferResult::Water => {
                            chunk.set_block(x, y, z, Block::new(BlockId::Water));
                        }
                        AquiferResult::Lava => {
                            chunk.set_block(x, y, z, Block::new(BlockId::Lava));
                        }
                        AquiferResult::Air => {
                            // Leave as air
                        }
                        AquiferResult::Solid => {
                            // Aquifer barrier creates stone
                            chunk.set_block(x, y, z, Block::new(BlockId::Stone));
                        }
                    }
                }
            }

            // Apply surface rules
            apply_surface_to_column(chunk, x, z, surface_y, biome, pipe);

            // Fill water below sea level
            fill_water(chunk, x, z, surface_y, pipe.sea_level);
        }
    }

    // Apply ore veins
    apply_ore_veins(chunk, pipe, base_x, base_z);

    chunk.recount_fluids();
    chunk.is_dirty = true;
}

/// Find the surface height (highest Y where density > 0).
fn find_surface_y(wx: f64, wz: f64, pipe: &TerrainPipeline) -> i32 {
    for y in (1..CHUNK_HEIGHT).rev() {
        let d = pipe.density_at(wx, y as f64, wz);
        if d > 0.0 {
            return y as i32;
        }
    }
    1
}

/// Sample biome from climate noise.
fn sample_biome(wx: f64, wz: f64, pipe: &TerrainPipeline) -> crate::world::world_gen::Biome {
    // Simplified - just use the existing biome sampling as a fallback.
    // In a full implementation this would use the climate router.
    use crate::world::world_gen::WorldGenerator;
    // Create a temporary generator just for biome queries
    let gen = WorldGenerator::new(pipe.seed);
    gen.get_biome(wx, wz)
}

/// Determine what solid block to place at a position.
fn determine_solid_block(
    wx: f64, wy: f64, wz: f64,
    y: usize, _surface_y: i32, _biome: crate::world::world_gen::Biome,
    pipe: &TerrainPipeline,
) -> Block {
    // Deep underground: deepslate/stone with variants
    if y < 16 {
        Block::new(BlockId::Deepslate)
    } else if y < 64 {
        // Gradual transition from deepslate to stone
        let deep_noise = pipe.splines.base_3d_noise.get_value(wx * 0.05, wy * 0.05, wz * 0.05);
        let threshold = (y as f64 - 16.0) / 48.0;
        if deep_noise > threshold * 2.0 - 1.0 {
            Block::new(BlockId::Deepslate)
        } else {
            // Stone variants
            let variant = pipe.splines.base_3d_noise.get_value(wx * 0.03, 0.0, wz * 0.03);
            if variant > 0.4 {
                Block::new(BlockId::Granite)
            } else if variant < -0.4 {
                Block::new(BlockId::Andesite)
            } else {
                Block::new(BlockId::Stone)
            }
        }
    } else {
        // Stone variants
        let variant = pipe.splines.base_3d_noise.get_value(wx * 0.03, 0.0, wz * 0.03);
        if variant > 0.4 {
            Block::new(BlockId::Granite)
        } else if variant < -0.4 {
            Block::new(BlockId::Diorite)
        } else {
            Block::new(BlockId::Stone)
        }
    }
}

/// Apply surface rules to a column (replace top blocks with biome-appropriate materials).
fn apply_surface_to_column(
    chunk: &mut Chunk, x: usize, z: usize,
    surface_y: i32, biome: crate::world::world_gen::Biome,
    pipe: &TerrainPipeline,
) {
    let mut ctx = SurfaceContext {
        block_x: x as i32,
        block_z: z as i32,
        block_y: 0,
        surface_depth: pipe.surface_system.get_surface_depth(x as i32, z as i32),
        surface_secondary: pipe.surface_system.get_surface_secondary(x as i32, z as i32),
        biome,
        water_height: pipe.sea_level,
        stone_depth_above: 0,
        stone_depth_below: 0,
        min_surface_level: surface_y - 8,
    };

    for y in (1..=surface_y as usize).rev() {
        let block = chunk.get_block(x, y, z);
        if block.id == BlockId::Stone || block.id == BlockId::Deepslate || block.id == BlockId::Granite
            || block.id == BlockId::Diorite || block.id == BlockId::Andesite
        {
            ctx.block_y = y as i32;
            ctx.stone_depth_above += 1;

            if let Some(new_id) = pipe.surface_rule.try_apply(&ctx) {
                chunk.set_block(x, y, z, Block::new(new_id));
            }
        } else {
            ctx.stone_depth_above = 0;
        }
    }
}

/// Fill water below sea level where there's air.
fn fill_water(chunk: &mut Chunk, x: usize, z: usize, surface_y: i32, sea_level: i32) {
    if surface_y < sea_level {
        for y in surface_y + 1..sea_level {
            if y > 0 && y < CHUNK_HEIGHT as i32 {
                let block = chunk.get_block(x, y as usize, z);
                if block.is_air() {
                    chunk.set_block(x, y as usize, z, Block::new(BlockId::Water));
                }
            }
        }
    }
}

/// Apply ore veins throughout the chunk.
fn apply_ore_veins(chunk: &mut Chunk, pipe: &TerrainPipeline, base_x: i64, base_z: i64) {
    let mut rng = SimpleRandom::new(pipe.seed ^ (base_x as u64).wrapping_mul(0x9e3779b97f4a7c15));

    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let wx = (base_x + x as i64) as f64;
            let wz = (base_z + z as i64) as f64;

            for y in 1..CHUNK_HEIGHT.min(128) {
                let block_id = chunk.get_block(x, y, z).id;
                let is_stone = matches!(block_id, BlockId::Stone | BlockId::Deepslate | BlockId::Granite | BlockId::Diorite | BlockId::Andesite | BlockId::Tuff);
                if !is_stone {
                    continue;
                }

                let wy = y as f64;
                match pipe.ore_veins.calculate(wx, wy, wz, &mut rng) {
                    VeinResult::CopperVein(ore_id) => {
                        chunk.set_block(x, y, z, Block::new(ore_id));
                    }
                    VeinResult::IronVein(ore_id) => {
                        chunk.set_block(x, y, z, Block::new(ore_id));
                    }
                    VeinResult::None => {}
                }
            }
        }
    }
}
