#![allow(dead_code)]

use std::sync::Arc;
use crate::world::gen::density::{Context, DensityFunction, YClampedGradient, slide_overworld, quarter_negative};
use crate::world::gen::noise::NormalNoise;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const GLOBAL_OFFSET: f64 = -0.50375;
pub const SURFACE_DENSITY_THRESHOLD: f64 = 1.5625;
pub const CHEESE_NOISE_TARGET: f64 = -0.703125;
pub const NOISE_ZERO: f64 = 0.390625;
pub const BASE_DENSITY_MULTIPLIER: f64 = 4.0;
pub const OVERWORLD_MIN_Y: i32 = -64;
pub const OVERWORLD_HEIGHT: i32 = 384;
pub const OVERWORLD_SEA_LEVEL: i32 = 63;
pub const CELL_WIDTH: i32 = 4;
pub const CELL_HEIGHT: i32 = 8;

// ---------------------------------------------------------------------------
// NoiseRouter — 15 density-function slots
// ---------------------------------------------------------------------------

pub struct NoiseRouter {
    pub barrier_noise: Arc<dyn DensityFunction>,
    pub fluid_level_floodedness_noise: Arc<dyn DensityFunction>,
    pub fluid_level_spread_noise: Arc<dyn DensityFunction>,
    pub lava_noise: Arc<dyn DensityFunction>,
    pub temperature: Arc<dyn DensityFunction>,
    pub vegetation: Arc<dyn DensityFunction>,
    pub continents: Arc<dyn DensityFunction>,
    pub erosion: Arc<dyn DensityFunction>,
    pub depth: Arc<dyn DensityFunction>,
    pub ridges: Arc<dyn DensityFunction>,
    pub preliminary_surface_level: Arc<dyn DensityFunction>,
    pub final_density: Arc<dyn DensityFunction>,
    pub vein_toggle: Arc<dyn DensityFunction>,
    pub vein_ridged: Arc<dyn DensityFunction>,
    pub vein_gap: Arc<dyn DensityFunction>,
}

// ---------------------------------------------------------------------------
// Simple density wrappers that use the density.rs DensityFunction trait
// ---------------------------------------------------------------------------

/// A density function that simply samples a NormalNoise.
pub struct NoiseDensity {
    noise: NormalNoise,
    xz_scale: f64,
    y_scale: f64,
}

impl NoiseDensity {
    pub fn new(noise: NormalNoise) -> Self {
        NoiseDensity { noise, xz_scale: 1.0, y_scale: 1.0 }
    }
    pub fn with_scale(noise: NormalNoise, xz_scale: f64, y_scale: f64) -> Self {
        NoiseDensity { noise, xz_scale, y_scale }
    }
}

impl DensityFunction for NoiseDensity {
    fn compute(&self, ctx: &Context) -> f64 {
        self.noise.get_value(
            ctx.block_x * self.xz_scale,
            ctx.block_y * self.y_scale,
            ctx.block_z * self.xz_scale,
        )
    }
    fn min_value(&self) -> f64 { -f64::MAX }
    fn max_value(&self) -> f64 { f64::MAX }
}

/// 2D noise density — ignores Y.
pub struct NoiseDensity2D {
    noise: NormalNoise,
    scale: f64,
}

impl NoiseDensity2D {
    pub fn new(noise: NormalNoise) -> Self {
        NoiseDensity2D { noise, scale: 1.0 }
    }
    pub fn with_scale(noise: NormalNoise, scale: f64) -> Self {
        NoiseDensity2D { noise, scale }
    }
}

impl DensityFunction for NoiseDensity2D {
    fn compute(&self, ctx: &Context) -> f64 {
        self.noise.get_value(ctx.block_x * self.scale, 0.0, ctx.block_z * self.scale)
    }
    fn min_value(&self) -> f64 { -f64::MAX }
    fn max_value(&self) -> f64 { f64::MAX }
}

/// Domain-warped 2D noise.
pub struct ShiftedNoise2D {
    noise: NormalNoise,
    shift_x: Arc<dyn DensityFunction>,
    shift_z: Arc<dyn DensityFunction>,
}

impl ShiftedNoise2D {
    pub fn new(noise: NormalNoise, shift_x: Arc<dyn DensityFunction>, shift_z: Arc<dyn DensityFunction>) -> Self {
        ShiftedNoise2D { noise, shift_x, shift_z }
    }
}

impl DensityFunction for ShiftedNoise2D {
    fn compute(&self, ctx: &Context) -> f64 {
        let sx = self.shift_x.compute(ctx);
        let sz = self.shift_z.compute(ctx);
        self.noise.get_value(ctx.block_x + sx, 0.0, ctx.block_z + sz)
    }
    fn min_value(&self) -> f64 { -f64::MAX }
    fn max_value(&self) -> f64 { f64::MAX }
}

// ---------------------------------------------------------------------------
// ShiftedNoiseSet — climate/terrain 2D noises
// ---------------------------------------------------------------------------

pub struct ShiftedNoiseSet {
    pub shift_x: Arc<dyn DensityFunction>,
    pub shift_z: Arc<dyn DensityFunction>,
    pub continents: Arc<dyn DensityFunction>,
    pub erosion: Arc<dyn DensityFunction>,
    pub ridges: Arc<dyn DensityFunction>,
    pub temperature: Arc<dyn DensityFunction>,
    pub vegetation: Arc<dyn DensityFunction>,
}

// ---------------------------------------------------------------------------
// SimpleTerrainSplines — noise-backed terrain shaping
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SimpleTerrainSplines {
    pub offset_noise: NormalNoise,
    pub factor_noise: NormalNoise,
    pub jagged_noise: NormalNoise,
    pub base_3d_noise: NormalNoise,
}

impl SimpleTerrainSplines {
    pub fn new(seed: u64) -> Self {
        SimpleTerrainSplines {
            offset_noise: NormalNoise::with_frequency(seed.wrapping_add(7), 0.001),
            factor_noise: NormalNoise::with_frequency(seed.wrapping_add(8), 0.001),
            jagged_noise: NormalNoise::with_frequency(seed.wrapping_add(9), 0.001),
            base_3d_noise: NormalNoise::with_frequency(seed.wrapping_add(10), 0.025),
        }
    }

    pub fn compute_offset(&self, continental: f64, erosion: f64, weirdness: f64) -> f64 {
        continental * 0.5 + erosion * (-0.3) + weirdness * 0.1
    }

    pub fn compute_factor(&self, _continental: f64, erosion: f64, weirdness: f64, _ridges: f64) -> f64 {
        (1.0 - erosion.max(-0.5).min(0.5).abs()) * 0.8 + 0.2 + weirdness * 0.1
    }

    pub fn compute_jaggedness(&self, _continental: f64, erosion: f64, _weirdness: f64, ridges: f64) -> f64 {
        ridges * 0.3 + (1.0 - erosion.abs()) * 0.2
    }

    pub fn compute_base_3d(&self, x: f64, y: f64, z: f64) -> f64 {
        self.base_3d_noise.get_value(x, y, z)
    }
}

// ---------------------------------------------------------------------------
// FinalDensity — the master density function
// ---------------------------------------------------------------------------

pub struct FinalDensity {
    pub splines: SimpleTerrainSplines,
    pub cave_noise: NormalNoise,
    pub continents: Arc<dyn DensityFunction>,
    pub erosion: Arc<dyn DensityFunction>,
    pub ridges: Arc<dyn DensityFunction>,
    pub temperature: Arc<dyn DensityFunction>,
    pub vegetation: Arc<dyn DensityFunction>,
}

impl DensityFunction for FinalDensity {
    fn compute(&self, ctx: &Context) -> f64 {
        let continental = self.continents.compute(ctx);
        let erosion_val = self.erosion.compute(ctx);
        let ridges_val = self.ridges.compute(ctx);
        let _temp = self.temperature.compute(ctx);
        let _veg = self.vegetation.compute(ctx);

        let weirdness = ridges_val;
        let ridges_folded = crate::world::gen::density::peaks_and_valleys(ridges_val);

        let offset = self.splines.compute_offset(continental, erosion_val, weirdness) + GLOBAL_OFFSET;
        let depth = y_clamped_gradient_y(ctx.block_y, -64.0, 320.0, 1.5, -1.5) + offset;
        let factor = self.splines.compute_factor(continental, erosion_val, weirdness, ridges_folded);
        let jaggedness = self.splines.compute_jaggedness(continental, erosion_val, weirdness, ridges_folded);
        let base_3d = self.splines.compute_base_3d(ctx.block_x, ctx.block_y, ctx.block_z);

        let sloped_cheese = BASE_DENSITY_MULTIPLIER * quarter_negative((depth + jaggedness) * factor) + base_3d;
        let cave_density = compute_cave_density(ctx, sloped_cheese, &self.cave_noise);
        let slid = slide_overworld(cave_density, ctx.block_y, OVERWORLD_MIN_Y, OVERWORLD_HEIGHT);
        (slid * 0.64).max(-1.0).min(1.0)
    }
    fn min_value(&self) -> f64 { -1.0 }
    fn max_value(&self) -> f64 { 1.0 }
}

// ---------------------------------------------------------------------------
// NoiseRouterData — factory
// ---------------------------------------------------------------------------

pub struct NoiseRouterData;

impl NoiseRouterData {
    pub fn create_overworld_router(seed: u64) -> NoiseRouter {
        let shifted = Self::create_shifted_noises(seed);
        let splines = SimpleTerrainSplines::new(seed);

        let barrier = NoiseDensity::with_scale(NormalNoise::simple(seed.wrapping_add(12)), 0.5, 0.5);
        let fluid_flood = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(13)), 0.67);
        let fluid_spread = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(14)), 0.714);
        let lava = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(15)), 0.5);

        let depth = YClampedGradient {
            from_y: -64,
            to_y: 320,
            from_value: 1.5,
            to_value: -1.5,
        };

        let prelim_surface = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(16)), 0.005);

        let cave_noise = NormalNoise::with_frequency(seed.wrapping_add(11), 0.1);

        let final_density = FinalDensity {
            splines,
            cave_noise,
            continents: shifted.continents.clone(),
            erosion: shifted.erosion.clone(),
            ridges: shifted.ridges.clone(),
            temperature: shifted.temperature.clone(),
            vegetation: shifted.vegetation.clone(),
        };

        let vein_toggle = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(17)), 0.01);
        let vein_ridged = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(18)), 0.01);
        let vein_gap = NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(19)), 0.01);

        NoiseRouter {
            barrier_noise: Arc::new(barrier),
            fluid_level_floodedness_noise: Arc::new(fluid_flood),
            fluid_level_spread_noise: Arc::new(fluid_spread),
            lava_noise: Arc::new(lava),
            temperature: shifted.temperature,
            vegetation: shifted.vegetation,
            continents: shifted.continents,
            erosion: shifted.erosion,
            depth: Arc::new(depth),
            ridges: shifted.ridges,
            preliminary_surface_level: Arc::new(prelim_surface),
            final_density: Arc::new(final_density),
            vein_toggle: Arc::new(vein_toggle),
            vein_ridged: Arc::new(vein_ridged),
            vein_gap: Arc::new(vein_gap),
        }
    }

    fn create_shifted_noises(seed: u64) -> ShiftedNoiseSet {
        let shift_x: Arc<dyn DensityFunction> = Arc::new(NoiseDensity2D::with_scale(NormalNoise::simple(seed), 0.001));
        let shift_z: Arc<dyn DensityFunction> = Arc::new(NoiseDensity2D::with_scale(NormalNoise::simple(seed.wrapping_add(1)), 0.001));

        let continents: Arc<dyn DensityFunction> = Arc::new(ShiftedNoise2D::new(
            NormalNoise::simple(seed.wrapping_add(2)),
            shift_x.clone(), shift_z.clone(),
        ));

        let erosion: Arc<dyn DensityFunction> = Arc::new(ShiftedNoise2D::new(
            NormalNoise::simple(seed.wrapping_add(3)),
            shift_x.clone(), shift_z.clone(),
        ));

        let ridges: Arc<dyn DensityFunction> = Arc::new(ShiftedNoise2D::new(
            NormalNoise::simple(seed.wrapping_add(4)),
            shift_x.clone(), shift_z.clone(),
        ));

        let temperature: Arc<dyn DensityFunction> = Arc::new(ShiftedNoise2D::new(
            NormalNoise::simple(seed.wrapping_add(5)),
            shift_x.clone(), shift_z.clone(),
        ));

        let vegetation: Arc<dyn DensityFunction> = Arc::new(ShiftedNoise2D::new(
            NormalNoise::simple(seed.wrapping_add(6)),
            shift_x.clone(), shift_z.clone(),
        ));

        ShiftedNoiseSet { shift_x, shift_z, continents, erosion, ridges, temperature, vegetation }
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

fn y_clamped_gradient_y(y: f64, from_y: f64, to_y: f64, from_value: f64, to_value: f64) -> f64 {
    if y <= from_y { from_value }
    else if y >= to_y { to_value }
    else {
        let t = (y - from_y) / (to_y - from_y);
        from_value + (to_value - from_value) * t
    }
}

pub fn compute_cave_density(ctx: &Context, sloped_cheese: f64, cave_noise: &NormalNoise) -> f64 {
    if sloped_cheese >= SURFACE_DENSITY_THRESHOLD {
        sloped_cheese
    } else {
        let cn = cave_noise.get_value(ctx.block_x, ctx.block_y, ctx.block_z);
        sloped_cheese.min(cn)
    }
}
