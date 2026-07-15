//! Port of Minecraft's cave density functions
//!
//! Corresponding Java source:
//! - `net.minecraft.world.level.levelgen.NoiseRouterData` (cave-related methods)
//! - `net.minecraft.world.level.levelgen.NoiseChunk`
//!
//! Each public function in this module constructs a `DenseFn` (density function
//! tree) that replicates the corresponding Minecraft cave type:
//!
//! - `spaghetti_roughness` — wall roughness for spaghetti caves
//! - `spaghetti_2d` — layer-aligned tunnel networks
//! - `spaghetti_3d` for `entrances` — random 3D tunnel networks
//! - `cheese_caves` — large open caverns ("Swiss cheese")
//! - `entrance_caves` — big cave entrances near the surface
//! - `noodle_caves` — thin, winding noodle caves
//! - `pillar_caves` — stone pillar columns
//! - `cave_density` — main combination (the `underground` function)

#![allow(dead_code)]

use crate::world::world_gen::density_fn::*;

// ============================================================================
// MappedNoise — noise with linear output mapping
// ============================================================================
//
// Corresponds to `DensityFunctions.mappedNoise(...)` in the Java source.
// The raw noise sample is transformed by:  output = sample * flatness + offset
// after being sampled at (x * xz_scale, y * y_scale, z * xz_scale).

#[derive(Clone)]
pub struct MappedNoise {
    pub noise: NoiseHandle,
    pub xz_scale: f64,
    pub y_scale: f64,
    pub flatness: f64,
    pub offset: f64,
}

impl DensityFunction for MappedNoise {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        self.noise.sample(
            ctx.block_x() as f64 * self.xz_scale,
            ctx.block_y() as f64 * self.y_scale,
            ctx.block_z() as f64 * self.xz_scale,
        ) * self.flatness + self.offset
    }
    fn min_value(&self) -> f64 {
        -self.noise.max_value() * self.flatness.abs() + self.offset
    }
    fn max_value(&self) -> f64 {
        self.noise.max_value() * self.flatness.abs() + self.offset
    }
    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        DenseFn(Box::new(MappedNoise {
            noise: visitor.visit_noise(self.noise.clone()),
            xz_scale: self.xz_scale,
            y_scale: self.y_scale,
            flatness: self.flatness,
            offset: self.offset,
        }))
    }
    fn clone_dyn(&self) -> DenseFn {
        DenseFn(Box::new(self.clone()))
    }
}

/// Mapped noise: `output = noise(x * xz_scale, y * y_scale, z * xz_scale) * flatness + offset`
pub fn mapped_noise(noise: NoiseHandle, xz_scale: f64, y_scale: f64, flatness: f64, offset: f64) -> DenseFn {
    DenseFn(Box::new(MappedNoise { noise, xz_scale, y_scale, flatness, offset }))
}

/// Mapped noise with zero offset: `output = noise * flatness`
pub fn mapped_noise_3(noise: NoiseHandle, xz_scale: f64, y_scale: f64, flatness: f64) -> DenseFn {
    mapped_noise(noise, xz_scale, y_scale, flatness, 0.0)
}

/// Mapped noise with default scales (xz_scale = y_scale = 1).
pub fn mapped_noise_2(noise: NoiseHandle, flatness: f64, offset: f64) -> DenseFn {
    mapped_noise(noise, 1.0, 1.0, flatness, offset)
}

// ============================================================================
// QuantizedSpaghettiRarity
// ============================================================================
//
// Port of `NoiseRouterData.QuantizedSpaghettiRarity`.
// Quantizes a rarity-modulator input into discrete rarity bins, each
// selecting a differently-scaled noise variant.

fn noise_fn_for_rarity(noise_h: NoiseHandle, rarity: f64) -> DenseFn {
    mul(constant(rarity), noise(noise_h, 1.0 / rarity, 1.0 / rarity))
}

/// 2-D spaghetti rarity: 5 bins over [-∞, -0.75, -0.5, 0.5, 0.75, ∞).
pub fn quantized_spaghetti_rarity_2d(input: DenseFn, noise_h: NoiseHandle) -> DenseFn {
    let thresholds = vec![-0.75, -0.5, 0.5, 0.75];
    let functions = vec![
        noise_fn_for_rarity(noise_h.clone(), 0.5),
        noise_fn_for_rarity(noise_h.clone(), 0.75),
        noise_fn_for_rarity(noise_h.clone(), 1.0),
        noise_fn_for_rarity(noise_h.clone(), 2.0),
        noise_fn_for_rarity(noise_h, 3.0),
    ];
    abs(interval_select(input, thresholds, functions))
}

/// 3-D spaghetti rarity: 4 bins over [-∞, -0.5, 0.0, 0.5, ∞).
pub fn quantized_spaghetti_rarity_3d(input: DenseFn, noise_h: NoiseHandle) -> DenseFn {
    let thresholds = vec![-0.5, 0.0, 0.5];
    let functions = vec![
        noise_fn_for_rarity(noise_h.clone(), 0.75),
        noise_fn_for_rarity(noise_h.clone(), 1.0),
        noise_fn_for_rarity(noise_h.clone(), 1.5),
        noise_fn_for_rarity(noise_h, 2.0),
    ];
    abs(interval_select(input, thresholds, functions))
}

// ============================================================================
// yLimitedInterpolatable
// ============================================================================
//
/// Equivalent to `NoiseRouterData.yLimitedInterpolatable`.
/// Creates a Y-bounded function: within [min_y, max_y] use `when_in_range`,
/// outside use `when_out_of_range`. Wrapped in Interpolated marker.

fn y_limited_interpolatable(
    y: DenseFn,
    when_in_range: DenseFn,
    min_y: i32,
    max_y: i32,
    when_out_of_range: f64,
) -> DenseFn {
    interpolated(range_choice(
        y,
        min_y as f64,
        (max_y + 1) as f64,
        when_in_range,
        constant(when_out_of_range),
    ))
}

// ============================================================================
// spaghetti_roughness_function
// ============================================================================
//
/// Port of `NoiseRouterData.spaghettiRoughnessFunction`.
///
/// Combines a roughness noise (abs + -0.4 clamp) with a Y-varying modulator,
/// producing rough cave-wall surfaces.

pub fn spaghetti_roughness_function(
    spaghetti_roughness: NoiseHandle,
    spaghetti_roughness_modulator: NoiseHandle,
) -> DenseFn {
    let rough_noise = noise(spaghetti_roughness, 1.0, 1.0);
    let rough_mod = mapped_noise(spaghetti_roughness_modulator, 0.0, -0.1, 1.0, 0.0);

    cache_once(mul(
        rough_mod,
        add(abs(rough_noise), constant(-0.4)),
    ))
}

// ============================================================================
// spaghetti_2d_thickness_modulator
// ============================================================================
//
/// Port of `NoiseRouterData.SPAGHETTI_2D_THICKNESS_MODULATOR`.
/// Maps the 2D thickness noise into [-1.3, -0.6] range.

pub fn spaghetti_2d_thickness_modulator(spaghetti_2d_thickness: NoiseHandle) -> DenseFn {
    cache_once(mapped_noise(spaghetti_2d_thickness, 2.0, 1.0, -0.35, -0.95))
}

// ============================================================================
// spaghetti_2d
// ============================================================================
//
/// Port of `NoiseRouterData.spaghetti2D`.
///
/// Creates layer-aligned spaghetti caves: combines a Y-elevation-ridged
/// layer function with a rarity-quantized 2D cave noise.  Output clamped
/// to [-1, 1].

pub fn spaghetti_2d(
    spaghetti_2d: NoiseHandle,
    spaghetti_2d_modulator: NoiseHandle,
    spaghetti_2d_elevation: NoiseHandle,
    spaghetti_2d_thickness: DenseFn,
) -> DenseFn {
    let rarity_mod = noise(spaghetti_2d_modulator, 2.0, 1.0);
    let spaghetti_2d_cave = quantized_spaghetti_rarity_2d(rarity_mod, spaghetti_2d);

    let elevation_mod = mapped_noise_3(spaghetti_2d_elevation, 0.0, -8.0, 8.0);

    let sloped_spaghetti = abs(add(
        flat_cache(elevation_mod),
        y_clamped_gradient(-64, 320, 8.0, -40.0),
    ));

    let layer_ridged = cube(add(sloped_spaghetti, spaghetti_2d_thickness.clone()));

    let ridge_offset = 0.083;
    let cave_noise = add(
        spaghetti_2d_cave,
        mul(constant(ridge_offset), spaghetti_2d_thickness),
    );

    clamp(max(cave_noise, layer_ridged), -1.0, 1.0)
}

// ============================================================================
// spaghetti_3d (helper for entrances)
// ============================================================================
//
/// Port of the 3D spaghetti cave sub-function used inside `entrances`.
/// Combines two rarity-quantized 3D noises with a thickness modulator.

fn spaghetti_3d(
    spaghetti_3d_rarity: NoiseHandle,
    spaghetti_3d_thickness: NoiseHandle,
    spaghetti_3d_1: NoiseHandle,
    spaghetti_3d_2: NoiseHandle,
) -> DenseFn {
    let rarity_mod = cache_once(noise(spaghetti_3d_rarity, 2.0, 1.0));
    let thickness_mod = mapped_noise(spaghetti_3d_thickness, 1.0, 1.0, -0.088, -0.065);

    let cave_1 = quantized_spaghetti_rarity_3d(rarity_mod.clone(), spaghetti_3d_1);
    let cave_2 = quantized_spaghetti_rarity_3d(rarity_mod, spaghetti_3d_2);

    clamp(
        add(max(cave_1, cave_2), thickness_mod),
        -1.0,
        1.0,
    )
}

// ============================================================================
// entrance_caves
// ============================================================================
//
/// Port of `NoiseRouterData.entrances`.
///
/// Combines spaghetti roughness, 3D spaghetti, and big entrance noise
/// to create cave entrance openings near the surface.

pub fn entrance_caves(
    spaghetti_roughness: NoiseHandle,
    spaghetti_roughness_modulator: NoiseHandle,
    spaghetti_3d_rarity: NoiseHandle,
    spaghetti_3d_thickness: NoiseHandle,
    spaghetti_3d_1: NoiseHandle,
    spaghetti_3d_2: NoiseHandle,
    cave_entrance: NoiseHandle,
) -> DenseFn {
    let rough_fn = spaghetti_roughness_function(
        spaghetti_roughness,
        spaghetti_roughness_modulator,
    );

    let spaghetti_3d_fn = spaghetti_3d(
        spaghetti_3d_rarity,
        spaghetti_3d_thickness,
        spaghetti_3d_1,
        spaghetti_3d_2,
    );

    let big_entrance = noise(cave_entrance, 0.75, 0.5);

    let big_entrances = add(
        add(big_entrance, constant(0.37)),
        y_clamped_gradient(-10, 30, 0.3, 0.0),
    );

    cache_once(min(big_entrances, add(rough_fn, spaghetti_3d_fn)))
}

// ============================================================================
// cheese_caves
// ============================================================================
//
/// Port of the cheese cavern sub-function from `NoiseRouterData.underground`.
///
/// Creates large open caverns ("Swiss cheese") using:
/// - `cave_layer` noise squared and multiplied by 4 (layered caverns)
/// - `cave_cheese` noise clamped to [-1, 1] and combined with sloped_cheese
///   top slide (solidified cheese)

pub fn cheese_caves(
    cave_layer: NoiseHandle,
    cave_cheese: NoiseHandle,
    sloped_cheese: DenseFn,
) -> DenseFn {
    let layer_noise = noise(cave_layer, 8.0, 8.0);
    let layered_caverns = mul(constant(4.0), square(layer_noise));

    let cheese = noise(cave_cheese, 0.6666666666666666, 0.6666666666666666);

    let solidified_cheese = add(
        clamp(add(constant(0.27), cheese), -1.0, 1.0),
        clamp(
            add(constant(1.5), mul(constant(-0.64), sloped_cheese)),
            0.0,
            0.5,
        ),
    );

    add(layered_caverns, solidified_cheese)
}

// ============================================================================
// noodle_caves
// ============================================================================
//
/// Port of `NoiseRouterData.noodle`.
///
/// Thin, winding noodle caves created from ridge noise (abs of two
/// ridge-function noises) combined with thickness modulation.

pub fn noodle_caves(
    y_fn: DenseFn,
    noodle: NoiseHandle,
    noodle_thickness: NoiseHandle,
    noodle_ridge_a: NoiseHandle,
    noodle_ridge_b: NoiseHandle,
) -> DenseFn {
    let noodle_min_y = -60;
    let noodle_max_y = 320;

    let noodle_toggle = y_limited_interpolatable(
        y_fn.clone(),
        noise(noodle, 1.0, 1.0),
        noodle_min_y,
        noodle_max_y,
        -1.0,
    );

    let noodle_thickness_fn = y_limited_interpolatable(
        y_fn.clone(),
        mapped_noise(noodle_thickness, 1.0, 1.0, -0.05, -0.1),
        noodle_min_y,
        noodle_max_y,
        0.0,
    );

    let ridge_freq = 2.6666666666666665;
    let ridge_a = y_limited_interpolatable(
        y_fn.clone(),
        noise(noodle_ridge_a, ridge_freq, ridge_freq),
        noodle_min_y,
        noodle_max_y,
        0.0,
    );
    let ridge_b = y_limited_interpolatable(
        y_fn,
        noise(noodle_ridge_b, ridge_freq, ridge_freq),
        noodle_min_y,
        noodle_max_y,
        0.0,
    );

    let noodle_ridged = mul(
        constant(1.5),
        max(abs(ridge_a), abs(ridge_b)),
    );

    range_choice(
        noodle_toggle,
        -1000000.0,
        0.0,
        constant(64.0),
        add(noodle_thickness_fn, noodle_ridged),
    )
}

// ============================================================================
// pillar_caves
// ============================================================================
//
/// Port of `NoiseRouterData.pillars`.
///
/// Stone pillars: pillar noise multiplied by rareness and thickness
/// modulators, cubed for steep column shapes.

pub fn pillar_caves(
    pillar: NoiseHandle,
    pillar_rareness: NoiseHandle,
    pillar_thickness: NoiseHandle,
) -> DenseFn {
    let pillar_noise = noise(pillar, 25.0, 0.3);
    let pillar_rareness_fn = mapped_noise_2(pillar_rareness, -2.0, 0.0);
    let pillar_thickness_fn = mapped_noise_2(pillar_thickness, 1.1, 0.0);

    let pillars_with_rareness = add(
        mul(pillar_noise, constant(2.0)),
        pillar_rareness_fn,
    );

    cache_once(mul(pillars_with_rareness, cube(pillar_thickness_fn)))
}

// ============================================================================
// cave_density — main combination (the `underground` function)
// ============================================================================
//
/// Port of `NoiseRouterData.underground`.
///
/// The primary cave combination that feeds into the final density.
/// Combines:
///   - cheese caves (layered_caverns + solidified_cheese)
///   - spaghetti 2D + spaghetti roughness
///   - entrance caves
///   - filtered pillars
///
/// Result = max(min(cheese, entrances, spaghetti2d + roughness), pillars_filtered)

pub fn cave_density(
    sloped_cheese: DenseFn,
    cheese_layer: DenseFn,
    cheese_cavern: DenseFn,
    spaghetti_2d_fn: DenseFn,
    roughness_fn: DenseFn,
    entrances_fn: DenseFn,
    pillars_fn: DenseFn,
) -> DenseFn {
    let base_cave_density = add(
        mul(constant(4.0), square(cheese_layer)),
        add(
            clamp(add(constant(0.27), cheese_cavern), -1.0, 1.0),
            clamp(
                add(constant(1.5), mul(constant(-0.64), sloped_cheese)),
                0.0,
                0.5,
            ),
        ),
    );

    let underground_subtractions = min(
        min(base_cave_density, entrances_fn),
        add(spaghetti_2d_fn, roughness_fn),
    );

    let pillars_filtered = range_choice(
        pillars_fn.clone(),
        -1000000.0,
        0.03,
        constant(-1000000.0),
        pillars_fn,
    );

    max(underground_subtractions, pillars_filtered)
}
