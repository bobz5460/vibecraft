//! Minecraft 26.2's `Aquifer.NoiseBasedAquifer`.
//!
//! This module deliberately owns only the per-chunk aquifer state. The caller
//! supplies the density value already sampled by the noise interpolation loop
//! and uses `compute_substance` to decide whether that position is fluid,
//! air, or solid.
//!
//! The native block representation has `BlockId`, but no fluid-state level.
//! Consequently, returned water and lava are source block IDs rather than
//! Java's `BlockState` values. The Java fluid-update decision is retained and
//! is available through `should_schedule_fluid_update` for the integration
//! caller.

use std::sync::Arc;

use crate::world::block::BlockId;
use crate::world::world_gen::density_fn::{FunctionContext, SinglePointContext};
use crate::world::world_gen::noise::{NoiseSeed, PositionalRandomFactory};
use crate::world::world_gen::noise_router::NoiseRouter;

const X_SPACING: i32 = 16;
const Y_SPACING: i32 = 12;
const Z_SPACING: i32 = 16;
const SAMPLE_OFFSET_X: i32 = -5;
const SAMPLE_OFFSET_Y: i32 = 1;
const SAMPLE_OFFSET_Z: i32 = -5;
const MAX_CENTER_X_OFFSET: i32 = 10;
const MAX_CENTER_Y_OFFSET: i32 = 9;
const MAX_CENTER_Z_OFFSET: i32 = 10;
const FLOWING_UPDATE_SIMILARITY: f64 = -0.76;
const WAY_BELOW_MIN_Y: i32 = -32_512;

const SURFACE_SAMPLING_OFFSETS_IN_CHUNKS: [(i32, i32); 13] = [
    (0, 0),
    (-2, -1),
    (-1, -1),
    (0, -1),
    (1, -1),
    (-3, 0),
    (-2, 0),
    (-1, 0),
    (1, 0),
    (-2, 1),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// A Java `Aquifer.FluidStatus` represented with native block IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidStatus {
    pub fluid_level: i32,
    pub fluid_type: BlockId,
}

impl FluidStatus {
    fn at(self, block_y: i32) -> BlockId {
        if block_y < self.fluid_level {
            self.fluid_type
        } else {
            BlockId::Air
        }
    }
}

/// Global Overworld fluid levels used by `NoiseBasedAquifer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalFluidPicker {
    pub sea_level: i32,
    pub lava_level: i32,
    pub default_fluid: BlockId,
    pub lava_fluid: BlockId,
}

impl GlobalFluidPicker {
    pub const fn new(
        sea_level: i32,
        lava_level: i32,
        default_fluid: BlockId,
        lava_fluid: BlockId,
    ) -> Self {
        Self {
            sea_level,
            lava_level,
            default_fluid,
            lava_fluid,
        }
    }

    pub const fn overworld() -> Self {
        Self::new(63, -54, BlockId::Water, BlockId::Lava)
    }

    /// Equivalent to `NoiseBasedChunkGenerator.createFluidPicker`.
    pub fn compute_fluid(self, _x: i32, y: i32, _z: i32) -> FluidStatus {
        if y < self.lava_level.min(self.sea_level) {
            FluidStatus {
                fluid_level: self.lava_level,
                fluid_type: self.lava_fluid,
            }
        } else {
            FluidStatus {
                fluid_level: self.sea_level,
                fluid_type: self.default_fluid,
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AquiferLocation {
    x: i32,
    y: i32,
    z: i32,
}

/// Per-chunk implementation of Minecraft 26.2's noise-based aquifer.
pub struct NoiseBasedAquifer {
    router: Arc<NoiseRouter>,
    positional_random_factory: PositionalRandomFactory,
    global_fluid_picker: GlobalFluidPicker,
    aquifer_cache: Vec<Option<FluidStatus>>,
    aquifer_location_cache: Vec<Option<AquiferLocation>>,
    min_grid_x: i32,
    min_grid_y: i32,
    min_grid_z: i32,
    grid_size_x: i32,
    grid_size_y: i32,
    grid_size_z: i32,
    skip_sampling_above_y: i32,
    should_schedule_fluid_update: bool,
}

impl NoiseBasedAquifer {
    /// Construct an aquifer for one 16x16 chunk column.
    ///
    /// `min_block_y` and `y_block_size` must describe the same vertical range
    /// used by the density interpolation loop. `positional_random_factory`
    /// must be RandomState's named `minecraft:aquifer` factory, not the root
    /// levelgen factory.
    pub fn with_positional_random_factory(
        chunk_x: i32,
        chunk_z: i32,
        router: Arc<NoiseRouter>,
        positional_random_factory: PositionalRandomFactory,
        min_block_y: i32,
        y_block_size: i32,
        global_fluid_picker: GlobalFluidPicker,
    ) -> Self {
        assert!(y_block_size > 0, "aquifer y block size must be positive");

        let chunk_min_x = chunk_x.wrapping_mul(16);
        let chunk_max_x = chunk_min_x.wrapping_add(15);
        let chunk_min_z = chunk_z.wrapping_mul(16);
        let chunk_max_z = chunk_min_z.wrapping_add(15);

        let min_grid_x = grid_x(chunk_min_x + SAMPLE_OFFSET_X);
        let max_grid_x = grid_x(chunk_max_x + SAMPLE_OFFSET_X) + 1;
        let min_grid_y = grid_y(min_block_y + SAMPLE_OFFSET_Y) - 1;
        let max_grid_y = grid_y(min_block_y + y_block_size + SAMPLE_OFFSET_Y) + 1;
        let min_grid_z = grid_z(chunk_min_z + SAMPLE_OFFSET_Z);
        let max_grid_z = grid_z(chunk_max_z + SAMPLE_OFFSET_Z) + 1;

        let grid_size_x = max_grid_x - min_grid_x + 1;
        let grid_size_y = max_grid_y - min_grid_y + 1;
        let grid_size_z = max_grid_z - min_grid_z + 1;
        let cache_size = (grid_size_x * grid_size_y * grid_size_z) as usize;

        let max_surface = max_preliminary_surface_level(
            &router,
            from_grid_x(min_grid_x, 0),
            from_grid_z(min_grid_z, 0),
            from_grid_x(max_grid_x, 9),
            from_grid_z(max_grid_z, 9),
        );
        let skip_sampling_above_grid_y = grid_y(adjust_surface_level(max_surface) + 12) + 1;
        let skip_sampling_above_y = from_grid_y(skip_sampling_above_grid_y, 11) - 1;

        Self {
            router,
            positional_random_factory,
            global_fluid_picker,
            aquifer_cache: vec![None; cache_size],
            aquifer_location_cache: vec![None; cache_size],
            min_grid_x,
            min_grid_y,
            min_grid_z,
            grid_size_x,
            grid_size_y,
            grid_size_z,
            skip_sampling_above_y,
            should_schedule_fluid_update: false,
        }
    }

    /// Construct the standard Overworld aquifer for a chunk.
    pub fn overworld(
        chunk_x: i32,
        chunk_z: i32,
        router: Arc<NoiseRouter>,
        seed: u64,
    ) -> Self {
        let mut root_seed = NoiseSeed::new(seed);
        let levelgen_random = root_seed.fork_positional();
        let mut aquifer_random = levelgen_random.from_hash_of("minecraft:aquifer");
        Self::with_positional_random_factory(
            chunk_x,
            chunk_z,
            router,
            aquifer_random.fork_positional(),
            -64,
            384,
            GlobalFluidPicker::overworld(),
        )
    }

    /// Compute the block to use for a density sample.
    ///
    /// `None` means the density remains solid and the caller should place its
    /// default stone block. `Some(BlockId::Air)` is an explicit empty result.
    pub fn compute_substance(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        density: f64,
    ) -> Option<BlockId> {
        if density > 0.0 {
            self.should_schedule_fluid_update = false;
            return None;
        }

        let context = SinglePointContext {
            block_x: x,
            block_y: y,
            block_z: z,
        };
        let global_fluid = self.global_fluid_picker.compute_fluid(x, y, z);

        if y > self.skip_sampling_above_y {
            self.should_schedule_fluid_update = false;
            return Some(global_fluid.at(y));
        }

        if global_fluid.at(y) == BlockId::Lava {
            self.should_schedule_fluid_update = false;
            return Some(BlockId::Lava);
        }

        let x_anchor = grid_x(x + SAMPLE_OFFSET_X);
        let y_anchor = grid_y(y + SAMPLE_OFFSET_Y);
        let z_anchor = grid_z(z + SAMPLE_OFFSET_Z);

        // When the block position falls outside the aquifer's sampled grid
        // range (including the +-1 Y offset used for center sampling, which
        // can occur for blocks at the world bottom or from carver edges),
        // fall back to the global fluid picker rather than panicking.
        if !self.contains_block_with_offsets(x, y, z) {
            self.should_schedule_fluid_update = false;
            return Some(global_fluid.at(y));
        }

        let mut nearest = [(i64::MAX, 0usize); 4];
        for x_offset in 0..=1 {
            for y_offset in -1..=1 {
                for z_offset in 0..=1 {
                    let index = self.get_index(
                        x_anchor + x_offset,
                        y_anchor + y_offset,
                        z_anchor + z_offset,
                    );
                    let location = self.get_aquifer_location(index, x_anchor + x_offset, y_anchor + y_offset, z_anchor + z_offset);
                    let dx = location.x as i64 - x as i64;
                    let dy = location.y as i64 - y as i64;
                    let dz = location.z as i64 - z as i64;
                    let distance = dx * dx + dy * dy + dz * dz;
                    insert_nearest(&mut nearest, distance, index);
                }
            }
        }

        let (distance1, closest_index1) = nearest[0];
        let (distance2, closest_index2) = nearest[1];
        let (distance3, closest_index3) = nearest[2];
        let (distance4, closest_index4) = nearest[3];
        let closest_status1 = self.get_aquifer_status(closest_index1);
        let similarity12 = similarity(distance1, distance2);
        let actual_fluid = closest_status1.at(y);

        if similarity12 <= 0.0 {
            if similarity12 >= FLOWING_UPDATE_SIMILARITY {
                self.should_schedule_fluid_update =
                    closest_status1 != self.get_aquifer_status(closest_index2);
            } else {
                self.should_schedule_fluid_update = false;
            }
            return Some(actual_fluid);
        }

        if actual_fluid == BlockId::Water
            && self
                .global_fluid_picker
                .compute_fluid(x, y - 1, z)
                .at(y - 1)
                == BlockId::Lava
        {
            self.should_schedule_fluid_update = true;
            return Some(actual_fluid);
        }

        let closest_status2 = self.get_aquifer_status(closest_index2);
        let mut barrier_noise_value = None;
        let barrier12 = similarity12
            * self.calculate_pressure(
                &context,
                closest_status1,
                closest_status2,
                &mut barrier_noise_value,
            );
        if density + barrier12 > 0.0 {
            self.should_schedule_fluid_update = false;
            return None;
        }

        let closest_status3 = self.get_aquifer_status(closest_index3);
        let similarity13 = similarity(distance1, distance3);
        if similarity13 > 0.0 {
            let barrier13 = similarity12
                * similarity13
                * self.calculate_pressure(
                    &context,
                    closest_status1,
                    closest_status3,
                    &mut barrier_noise_value,
                );
            if density + barrier13 > 0.0 {
                self.should_schedule_fluid_update = false;
                return None;
            }
        }

        let similarity23 = similarity(distance2, distance3);
        if similarity23 > 0.0 {
            let barrier23 = similarity12
                * similarity23
                * self.calculate_pressure(
                    &context,
                    closest_status2,
                    closest_status3,
                    &mut barrier_noise_value,
                );
            if density + barrier23 > 0.0 {
                self.should_schedule_fluid_update = false;
                return None;
            }
        }

        let may_flow12 = closest_status1 != closest_status2;
        let may_flow23 = similarity23 >= FLOWING_UPDATE_SIMILARITY
            && closest_status2 != closest_status3;
        let may_flow13 = similarity13 >= FLOWING_UPDATE_SIMILARITY
            && closest_status1 != closest_status3;
        self.should_schedule_fluid_update = if may_flow12 || may_flow23 || may_flow13 {
            true
        } else {
            similarity13 >= FLOWING_UPDATE_SIMILARITY
                && similarity(distance1, distance4) >= FLOWING_UPDATE_SIMILARITY
                && closest_status1 != self.get_aquifer_status(closest_index4)
        };

        Some(actual_fluid)
    }

    /// Whether the last substance computation requested a fluid update.
    pub fn should_schedule_fluid_update(&self) -> bool {
        self.should_schedule_fluid_update
    }

    /// Number of center locations populated so far. Useful for diagnostics and
    /// for verifying that repeated samples use the per-chunk cache.
    pub fn cached_aquifer_centers(&self) -> usize {
        self.aquifer_location_cache
            .iter()
            .filter(|location| location.is_some())
            .count()
    }

    fn get_index(&self, grid_x: i32, grid_y: i32, grid_z: i32) -> usize {
        let x = grid_x - self.min_grid_x;
        let y = grid_y - self.min_grid_y;
        let z = grid_z - self.min_grid_z;
        debug_assert!(x >= 0 && x < self.grid_size_x);
        debug_assert!(y >= 0 && y < self.grid_size_y);
        debug_assert!(z >= 0 && z < self.grid_size_z);
        (y * self.grid_size_z * self.grid_size_x + z * self.grid_size_x + x) as usize
    }

    /// Check whether a block coordinate falls within the aquifer's sampled
    /// grid range, including the +-1 offset in Y that `compute_substance`
    /// applies when sampling nearest aquifer centers.
    fn contains_block_with_offsets(&self, block_x: i32, block_y: i32, block_z: i32) -> bool {
        let gx = grid_x(block_x + SAMPLE_OFFSET_X);
        let gy = grid_y(block_y + SAMPLE_OFFSET_Y);
        let gz = grid_z(block_z + SAMPLE_OFFSET_Z);
        let x = gx - self.min_grid_x;
        let y = gy - self.min_grid_y;
        let z = gz - self.min_grid_z;
        // X/Z iterate 0..=1, Y iterates -1..=1
        x >= 0 && x + 1 < self.grid_size_x
            && y - 1 >= 0 && y + 1 < self.grid_size_y
            && z >= 0 && z + 1 < self.grid_size_z
    }

    fn get_aquifer_location(
        &mut self,
        index: usize,
        grid_x: i32,
        grid_y: i32,
        grid_z: i32,
    ) -> AquiferLocation {
        if let Some(location) = self.aquifer_location_cache[index] {
            return location;
        }

        let mut random = self.positional_random_factory.at(grid_x, grid_y, grid_z);
        let location = AquiferLocation {
            x: from_grid_x(grid_x, random.next_int(MAX_CENTER_X_OFFSET)),
            y: from_grid_y(grid_y, random.next_int(MAX_CENTER_Y_OFFSET)),
            z: from_grid_z(grid_z, random.next_int(MAX_CENTER_Z_OFFSET)),
        };
        self.aquifer_location_cache[index] = Some(location);
        location
    }

    fn get_aquifer_status(&mut self, index: usize) -> FluidStatus {
        if let Some(status) = self.aquifer_cache[index] {
            return status;
        }

        let location = self.aquifer_location_cache[index]
            .expect("aquifer status requested before its center was sampled");
        let status = self.compute_fluid(location.x, location.y, location.z);
        self.aquifer_cache[index] = Some(status);
        status
    }

    fn compute_fluid(&self, x: i32, y: i32, z: i32) -> FluidStatus {
        let global_fluid = self.global_fluid_picker.compute_fluid(x, y, z);
        let top_of_aquifer_cell = y + Y_SPACING;
        let bottom_of_aquifer_cell = y - Y_SPACING;
        let mut lowest_preliminary_surface = i32::MAX;
        let mut surface_at_center_is_under_global_fluid_level = false;

        for (offset_x, offset_z) in SURFACE_SAMPLING_OFFSETS_IN_CHUNKS {
            let sample_x = x + offset_x * 16;
            let sample_z = z + offset_z * 16;
            let preliminary_surface_level = preliminary_surface_level(&self.router, sample_x, sample_z);
            let adjusted_surface_level = adjust_surface_level(preliminary_surface_level);
            let is_center = offset_x == 0 && offset_z == 0;

            if is_center && bottom_of_aquifer_cell > adjusted_surface_level {
                return global_fluid;
            }

            let top_pokes_above_surface = top_of_aquifer_cell > adjusted_surface_level;
            if top_pokes_above_surface || is_center {
                let global_fluid_at_surface = self
                    .global_fluid_picker
                    .compute_fluid(sample_x, adjusted_surface_level, sample_z);
                if global_fluid_at_surface.at(adjusted_surface_level) != BlockId::Air {
                    if is_center {
                        surface_at_center_is_under_global_fluid_level = true;
                    }
                    if top_pokes_above_surface {
                        return global_fluid_at_surface;
                    }
                }
            }

            lowest_preliminary_surface = lowest_preliminary_surface.min(preliminary_surface_level);
        }

        let fluid_surface_level = self.compute_surface_level(
            x,
            y,
            z,
            global_fluid,
            lowest_preliminary_surface,
            surface_at_center_is_under_global_fluid_level,
        );
        FluidStatus {
            fluid_level: fluid_surface_level,
            fluid_type: self.compute_fluid_type(x, y, z, global_fluid, fluid_surface_level),
        }
    }

    fn compute_surface_level(
        &self,
        x: i32,
        y: i32,
        z: i32,
        global_fluid: FluidStatus,
        lowest_preliminary_surface: i32,
        surface_at_center_is_under_global_fluid_level: bool,
    ) -> i32 {
        let context = SinglePointContext {
            block_x: x,
            block_y: y,
            block_z: z,
        };
        let (partially_floodedness, fully_floodedness) = if is_deep_dark_region(&self.router, &context) {
            (-1.0, -1.0)
        } else {
            let distance_below_surface = lowest_preliminary_surface + 8 - y;
            let floodedness_factor = if surface_at_center_is_under_global_fluid_level {
                clamped_map(distance_below_surface as f64, 0.0, 64.0, 1.0, 0.0)
            } else {
                0.0
            };
            let floodedness_noise_value = self
                .router
                .fluid_level_floodedness_noise
                .compute(&context)
                .clamp(-1.0, 1.0);
            let fully_flooded_threshold = map(floodedness_factor, 1.0, 0.0, -0.3, 0.8);
            let partially_flooded_threshold = map(floodedness_factor, 1.0, 0.0, -0.8, 0.4);
            (
                floodedness_noise_value - partially_flooded_threshold,
                floodedness_noise_value - fully_flooded_threshold,
            )
        };

        if fully_floodedness > 0.0 {
            global_fluid.fluid_level
        } else if partially_floodedness > 0.0 {
            self.compute_randomized_fluid_surface_level(x, y, z, lowest_preliminary_surface)
        } else {
            WAY_BELOW_MIN_Y
        }
    }

    fn compute_randomized_fluid_surface_level(
        &self,
        x: i32,
        y: i32,
        z: i32,
        lowest_preliminary_surface: i32,
    ) -> i32 {
        let fluid_level_cell_x = x.div_euclid(16);
        let fluid_level_cell_y = y.div_euclid(40);
        let fluid_level_cell_z = z.div_euclid(16);
        let fluid_cell_middle_y = fluid_level_cell_y * 40 + 20;
        let context = SinglePointContext {
            block_x: fluid_level_cell_x,
            block_y: fluid_level_cell_y,
            block_z: fluid_level_cell_z,
        };
        let fluid_level_spread = self.router.fluid_level_spread_noise.compute(&context) * 10.0;
        let fluid_level_spread_quantized = floor_div_f64(fluid_level_spread, 3) * 3;
        (lowest_preliminary_surface).min(fluid_cell_middle_y + fluid_level_spread_quantized)
    }

    fn compute_fluid_type(
        &self,
        x: i32,
        y: i32,
        z: i32,
        global_fluid: FluidStatus,
        fluid_surface_level: i32,
    ) -> BlockId {
        let mut fluid_type = global_fluid.fluid_type;
        if fluid_surface_level <= -10
            && fluid_surface_level != WAY_BELOW_MIN_Y
            && global_fluid.fluid_type != BlockId::Lava
        {
            let context = SinglePointContext {
                block_x: x.div_euclid(64),
                block_y: y.div_euclid(40),
                block_z: z.div_euclid(64),
            };
            if self.router.lava_noise.compute(&context).abs() > 0.3 {
                fluid_type = BlockId::Lava;
            }
        }
        fluid_type
    }

    fn calculate_pressure(
        &self,
        context: &dyn FunctionContext,
        status_closest1: FluidStatus,
        status_closest2: FluidStatus,
        barrier_noise_value: &mut Option<f64>,
    ) -> f64 {
        let type1 = status_closest1.at(context.block_y());
        let type2 = status_closest2.at(context.block_y());
        if (type1 == BlockId::Lava && type2 == BlockId::Water)
            || (type1 == BlockId::Water && type2 == BlockId::Lava)
        {
            return 2.0;
        }

        let fluid_y_diff = (status_closest1.fluid_level - status_closest2.fluid_level).abs();
        if fluid_y_diff == 0 {
            return 0.0;
        }

        let average_fluid_y = 0.5 * (status_closest1.fluid_level + status_closest2.fluid_level) as f64;
        let how_far_above_average_fluid_point = context.block_y() as f64 + 0.5 - average_fluid_y;
        let base_value = fluid_y_diff as f64 / 2.0;
        let distance_from_barrier_edge_towards_middle =
            base_value - how_far_above_average_fluid_point.abs();

        let gradient = if how_far_above_average_fluid_point > 0.0 {
            let center_point = distance_from_barrier_edge_towards_middle;
            if center_point > 0.0 {
                center_point / 1.5
            } else {
                center_point / 2.5
            }
        } else {
            let center_point = 3.0 + distance_from_barrier_edge_towards_middle;
            if center_point > 0.0 {
                center_point / 3.0
            } else {
                center_point / 10.0
            }
        };

        let noise_value = if !(-2.0..=2.0).contains(&gradient) {
            0.0
        } else {
            *barrier_noise_value
                .get_or_insert_with(|| self.router.barrier_noise.compute(context))
        };
        2.0 * (noise_value + gradient)
    }
}

fn insert_nearest(nearest: &mut [(i64, usize); 4], distance: i64, index: usize) {
    if distance <= nearest[0].0 {
        nearest[3] = nearest[2];
        nearest[2] = nearest[1];
        nearest[1] = nearest[0];
        nearest[0] = (distance, index);
    } else if distance <= nearest[1].0 {
        nearest[3] = nearest[2];
        nearest[2] = nearest[1];
        nearest[1] = (distance, index);
    } else if distance <= nearest[2].0 {
        nearest[3] = nearest[2];
        nearest[2] = (distance, index);
    } else if distance <= nearest[3].0 {
        nearest[3] = (distance, index);
    }
}

fn similarity(distance1: i64, distance2: i64) -> f64 {
    1.0 - (distance2 - distance1) as f64 / 25.0
}

fn grid_x(block_coord: i32) -> i32 {
    block_coord.div_euclid(X_SPACING)
}

fn from_grid_x(grid_coord: i32, block_offset: i32) -> i32 {
    grid_coord * X_SPACING + block_offset
}

fn grid_y(block_coord: i32) -> i32 {
    block_coord.div_euclid(Y_SPACING)
}

fn from_grid_y(grid_coord: i32, block_offset: i32) -> i32 {
    grid_coord * Y_SPACING + block_offset
}

fn grid_z(block_coord: i32) -> i32 {
    block_coord.div_euclid(Z_SPACING)
}

fn from_grid_z(grid_coord: i32, block_offset: i32) -> i32 {
    grid_coord * Z_SPACING + block_offset
}

fn preliminary_surface_level(router: &NoiseRouter, x: i32, z: i32) -> i32 {
    let context = SinglePointContext {
        block_x: x.div_euclid(4) * 4,
        block_y: 0,
        block_z: z.div_euclid(4) * 4,
    };
    router.preliminary_surface_level.compute(&context).floor() as i32
}

fn max_preliminary_surface_level(
    router: &NoiseRouter,
    min_x: i32,
    min_z: i32,
    max_x: i32,
    max_z: i32,
) -> i32 {
    let mut max_surface = i32::MIN;
    let mut z = min_z;
    while z <= max_z {
        let mut x = min_x;
        while x <= max_x {
            max_surface = max_surface.max(preliminary_surface_level(router, x, z));
            x += 4;
        }
        z += 4;
    }
    max_surface
}

fn adjust_surface_level(preliminary_surface_level: i32) -> i32 {
    preliminary_surface_level + 8
}

fn is_deep_dark_region(router: &NoiseRouter, context: &dyn FunctionContext) -> bool {
    router.erosion.compute(context) < -0.22499999403953552
        && router.depth.compute(context) > 0.8999999761581421
}

fn clamped_map(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    map(value.clamp(from_min, from_max), from_min, from_max, to_min, to_max)
}

fn map(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let t = (value - from_min) / (from_max - from_min);
    to_min + (to_max - to_min) * t
}

fn floor_div_f64(value: f64, divisor: i32) -> i32 {
    (value / divisor as f64).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::world_gen::noise_router::NoiseRouterData;

    fn test_aquifer() -> NoiseBasedAquifer {
        NoiseBasedAquifer::overworld(
            0,
            0,
            Arc::new(NoiseRouterData::create_overworld_router(42, false, false)),
            42,
        )
    }

    #[test]
    fn center_locations_are_deterministic_and_cached() {
        let mut first = test_aquifer();
        let mut second = test_aquifer();
        let first_result = first.compute_substance(7, 0, 9, -1.0);
        let first_cache_size = first.cached_aquifer_centers();
        let second_result = second.compute_substance(7, 0, 9, -1.0);
        assert_eq!(first_result, second_result);
        assert_eq!(first_cache_size, first.cached_aquifer_centers());
        assert!(first_cache_size > 0);

        let before = first.cached_aquifer_centers();
        let _ = first.compute_substance(7, 0, 9, -1.0);
        assert_eq!(before, first.cached_aquifer_centers());
    }

    #[test]
    fn global_fluid_levels_match_overworld_picker() {
        let picker = GlobalFluidPicker::overworld();
        assert_eq!(picker.compute_fluid(0, -55, 0), FluidStatus { fluid_level: -54, fluid_type: BlockId::Lava });
        assert_eq!(picker.compute_fluid(0, -54, 0).at(-54), BlockId::Water);
        assert_eq!(picker.compute_fluid(0, 62, 0).at(62), BlockId::Water);
        assert_eq!(picker.compute_fluid(0, 63, 0).at(63), BlockId::Air);
    }

    #[test]
    fn positive_density_keeps_solid_and_clears_update_flag() {
        let mut aquifer = test_aquifer();
        assert_eq!(aquifer.compute_substance(0, 20, 0, 1.0), None);
        assert!(!aquifer.should_schedule_fluid_update());
    }

    #[test]
    fn overworld_uses_random_state_aquifer_fork() {
        let router = Arc::new(NoiseRouterData::create_overworld_router(42, false, false));
        let aquifer = NoiseBasedAquifer::overworld(0, 0, router, 42);

        let mut root = NoiseSeed::new(42);
        let levelgen = root.fork_positional();
        let mut expected_seed = levelgen.from_hash_of("minecraft:aquifer");
        assert_eq!(
            aquifer.positional_random_factory,
            expected_seed.fork_positional()
        );
    }
}
