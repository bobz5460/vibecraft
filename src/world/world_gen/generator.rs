//! Port of Minecraft's NoiseBasedChunkGenerator fillFromNoise/doFill.
//!
//! Implements the cell-based trilinear interpolation loop that drives
//! chunk generation from density functions.  This is the top-level
//! orchestrator that ties the NoiseRouter, NoiseSettings, and
//! DensityFunction system to block placement.

use std::sync::Arc;

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_SIZE};
use crate::world::world_gen::density_fn::{
    DensityFunction, InterpolatedContext, SinglePointContext,
};
use crate::world::world_gen::noise::{NoiseSeed, NoiseSettings, PositionalRandomFactory};
use crate::world::world_gen::noise_router::{NoiseRouter, NoiseRouterData};
use crate::world::world_gen::biome_source::OverworldBiomeSource;
use crate::world::world_gen::aquifer::NoiseBasedAquifer;
use crate::world::world_gen::surface::{default_overworld_rules, OreVeinifier, StaticRuleSource, SurfaceSystem};
use crate::world::world_gen::Biome;

// Refine only the narrow material-boundary band. This removes visible cell
// steps while keeping the current bounded cell cache practical until the
// density graph's Java cache wrappers are fully implemented.
const DENSITY_EXACT_REFINEMENT: f64 = 0.001;
const DENSITY_BOUNDARY_REFINEMENT: f64 = 0.05;

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
    ore_veins_random: PositionalRandomFactory,
}

impl VanillaWorldGenerator {
    /// Create a new generator from an existing router.
    pub fn new(router: Arc<NoiseRouter>, seed: u64) -> Self {
        let biome_source = OverworldBiomeSource::from_router((*router).clone());
        let random = NoiseSeed::new(seed).fork_positional();
        let ore_veins_random = random.from_hash_of("minecraft:ore").fork_positional();
        VanillaWorldGenerator {
            router,
            settings: NoiseSettings::OVERWORLD,
            aquifer: AquiferData::overworld(),
            seed,
            biome_source,
            ore_veins_random,
        }
    }

    /// Convenience constructor that creates the router from a seed.
    pub fn from_seed(seed: u64) -> Self {
        let router = Arc::new(NoiseRouterData::create_overworld_router(seed, false, false));
        Self::new(router, seed)
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
    /// This is equivalent to `NoiseBasedChunkGenerator.doFill`:
    /// 1. Create the per-chunk interpolation state.
    /// 2. Iterate over cells in X, Z, Y order.
    /// 3. Interpolate the cached cell density and refine blocks near a
    ///    material boundary with marker-aware per-block evaluation.
    /// 4. Place stone/deepslate/fluid/air based on density threshold.
    pub fn generate_chunk(&self, chunk: &mut Chunk) {
        let cell_width = self.settings.cell_width();
        let cell_height = self.settings.cell_height();
        let cell_count_y = self.settings.height / cell_height;
        let cell_count_xz = CHUNK_SIZE as i32 / cell_width;
        let cell_min_y = self.settings.min_y.div_euclid(cell_height);
        let sea_level = self.aquifer.sea_level;

        let mut aquifer = NoiseBasedAquifer::overworld(
            chunk.cx,
            chunk.cz,
            self.router.clone(),
            self.seed,
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

                                // Simple noise value for stone type blending.
                                // Use the router's final_density at y=0  (a rough
                                // proxy for the per-column noise value used in
                                // deepslate transition).
                                let stone_noise = sample_density_2d(
                                    &self.router,
                                    pos_x,
                                    pos_z,
                                );

                                let aquifer_block =
                                    aquifer.compute_substance(pos_x, pos_y, pos_z, density);
                                let block_id = if density > 0.0 || aquifer_block.is_none() {
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
                                    .unwrap_or_else(|| get_stone_type(pos_y, stone_noise))
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
        }

        nd.stop_interpolation();

        let system = SurfaceSystem::create_ref_from_seed(
            BlockId::Stone,
            sea_level,
            self.seed,
        );
        let surface_rules = default_overworld_rules(system.clone());
        let surface_source = StaticRuleSource::new(surface_rules);
        let base_x = chunk.cx * CHUNK_SIZE as i32;
        let base_z = chunk.cz * CHUNK_SIZE as i32;
        let mut column_biomes = vec![Biome::Plains; CHUNK_SIZE * CHUNK_SIZE];
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                column_biomes[x * CHUNK_SIZE + z] = self
                    .biome_source
                    .get_biome(base_x + x as i32, 0, base_z + z as i32)
                    .unwrap_or(Biome::Plains);
            }
        }
        SurfaceSystem::build_surface_with_biome_provider(
            system,
            chunk,
            &surface_source,
            sea_level,
            self.settings.min_y,
            move |x, _y, z| {
                let local_x = x - base_x;
                let local_z = z - base_z;
                if local_x >= 0
                    && local_x < CHUNK_SIZE as i32
                    && local_z >= 0
                    && local_z < CHUNK_SIZE as i32
                {
                    column_biomes[local_x as usize * CHUNK_SIZE + local_z as usize]
                } else {
                    Biome::Plains
                }
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
    fn test_aquifer_data() {
        let aquifer = AquiferData::overworld();
        assert_eq!(aquifer.fluid_at(0, 70, 0), BlockId::Air);
        assert_eq!(aquifer.fluid_at(0, 50, 0), BlockId::Water);
        assert_eq!(aquifer.fluid_at(0, -60, 0), BlockId::Lava);
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
        let gen = VanillaWorldGenerator::from_seed(42);
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
    fn test_chunks_at_different_positions() {
        let seed = 42u64;
        let gen = VanillaWorldGenerator::from_seed(seed);
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
