//! Port of Minecraft's NoiseRouter and NoiseRouterData
//!
//! Corresponding Java classes:
//! - `net.minecraft.world.level.levelgen.NoiseRouter`
//! - `net.minecraft.world.level.levelgen.NoiseRouterData`
//! - `net.minecraft.world.level.levelgen.Noises`
//! - `net.minecraft.world.level.levelgen.synth.BlendedNoise`

use crate::world::world_gen::density_fn::*;
use crate::world::world_gen::noise::*;

// ---------------------------------------------------------------------------
// Bridge: implement NoiseSampler for NormalNoise so it can be used with
// NoiseHandle (our Arc<dyn NoiseSampler> wrapper).
// ---------------------------------------------------------------------------

impl NoiseSampler for NormalNoise {
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.get_value(x, y, z)
    }
    fn max_value(&self) -> f64 {
        self.max_value()
    }
}

impl NoiseSampler for PerlinNoise {
    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.get_value(x, y, z)
    }
    fn max_value(&self) -> f64 {
        self.max_value()
    }
}

// ============================================================================
// Constants from NoiseRouterData.java
// ============================================================================

pub const GLOBAL_OFFSET: f64 = -0.5037500262260437;
const ORE_THICKNESS: f64 = 0.08;
const VEININESS_FREQUENCY: f64 = 1.5;
const NOODLE_SPACING_AND_STRAIGHTNESS: f64 = 1.5;
pub const SURFACE_DENSITY_THRESHOLD: f64 = 1.5625;
pub const CHEESE_NOISE_TARGET: f64 = -0.703125;
const DENSITY_Y_ANCHOR_BOTTOM: i32 = -64;
const DENSITY_Y_ANCHOR_TOP: i32 = 320;
const DENSITY_Y_BOTTOM: f64 = 1.5;
const DENSITY_Y_TOP: f64 = -1.5;
const OVERWORLD_BOTTOM_SLIDE_HEIGHT: i32 = 24;
const BASE_DENSITY_MULTIPLIER: f64 = 4.0;
pub const NOISE_ZERO: f64 = 0.390625;

const BLENDING_FACTOR_VALUE: f64 = 10.0;

// Vein Y bounds from OreVeinifier.VeinType
const VEIN_MIN_Y: i32 = -60;
const VEIN_MAX_Y: i32 = 56;

// Noodle Y bounds
const NOODLE_MIN_Y: i32 = -60;
const NOODLE_MAX_Y: i32 = 320;

// ============================================================================
// NoiseRouter
// ============================================================================

/// Port of `net.minecraft.world.level.levelgen.NoiseRouter`.
///
/// Holds all 15 density functions that drive world generation.
#[derive(Clone)]
pub struct NoiseRouter {
    pub barrier_noise: DenseFn,
    pub fluid_level_floodedness_noise: DenseFn,
    pub fluid_level_spread_noise: DenseFn,
    pub lava_noise: DenseFn,
    pub temperature: DenseFn,
    pub vegetation: DenseFn,
    pub continents: DenseFn,
    pub erosion: DenseFn,
    pub depth: DenseFn,
    pub ridges: DenseFn,
    pub preliminary_surface_level: DenseFn,
    pub final_density: DenseFn,
    pub vein_toggle: DenseFn,
    pub vein_ridged: DenseFn,
    pub vein_gap: DenseFn,
}

impl NoiseRouter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        barrier_noise: DenseFn,
        fluid_level_floodedness_noise: DenseFn,
        fluid_level_spread_noise: DenseFn,
        lava_noise: DenseFn,
        temperature: DenseFn,
        vegetation: DenseFn,
        continents: DenseFn,
        erosion: DenseFn,
        depth: DenseFn,
        ridges: DenseFn,
        preliminary_surface_level: DenseFn,
        final_density: DenseFn,
        vein_toggle: DenseFn,
        vein_ridged: DenseFn,
        vein_gap: DenseFn,
    ) -> Self {
        NoiseRouter {
            barrier_noise,
            fluid_level_floodedness_noise,
            fluid_level_spread_noise,
            lava_noise,
            temperature,
            vegetation,
            continents,
            erosion,
            depth,
            ridges,
            preliminary_surface_level,
            final_density,
            vein_toggle,
            vein_ridged,
            vein_gap,
        }
    }
}

// ============================================================================
// NoiseRouterData
// ============================================================================

/// Port of `net.minecraft.world.level.levelgen.NoiseRouterData`.
///
/// Builder/creator for `NoiseRouter` instances with all wired-up density
/// functions. Each method corresponds directly to a method in the Java class.
pub struct NoiseRouterData;

impl NoiseRouterData {
    // ------------------------------------------------------------------
    // peaksAndValleys — the positive-negative ridge formula
    // ------------------------------------------------------------------

    /// Scalar version: `peaksAndValleys(float weirdness)` from the Java source.
    /// Computes `-| -|weirdness| - 2/3| - 1/3| * 3`.
    pub fn peaks_and_valleys(weirdness: f64) -> f64 {
        -((weirdness.abs() - 2.0 / 3.0).abs() - 1.0 / 3.0) * 3.0
    }

    /// Density-function version: builds a DenseFn that applies the
    /// peaks-and-valleys formula to its input.
    pub fn peaks_and_valleys_fn(weirdness: DenseFn) -> DenseFn {
        let step1 = abs(weirdness);
        let step2 = add(step1, constant(-2.0 / 3.0));
        let step3 = abs(step2);
        let step4 = add(step3, constant(-1.0 / 3.0));
        mul(step4, constant(-3.0))
    }

    // ------------------------------------------------------------------
    // slide — top/bottom density smoothing
    // ------------------------------------------------------------------

    /// Port of `slide(DensityFunction, int, int, int, int, double, int, int, double)`.
    ///
    /// Applies top and bottom smoothing to the density value by lerping
    /// toward `topTarget` above `(minY + height - topStartY)` and toward
    /// `bottomTarget` below `(minY + bottomStartY)`.
    pub fn slide(
        caves: DenseFn,
        min_y: i32,
        height: i32,
        top_start_y: i32,
        top_end_y: i32,
        top_target: f64,
        bottom_start_y: i32,
        bottom_end_y: i32,
        bottom_target: f64,
    ) -> DenseFn {
        let noise_value = caves;

        let top_from = min_y + height - top_start_y;
        let top_to = min_y + height - top_end_y;
        let top_factor = y_clamped_gradient(top_from, top_to, 1.0, 0.0);
        let noise_value = lerp(top_factor, constant(top_target), noise_value);

        let bottom_from = min_y + bottom_start_y;
        let bottom_to = min_y + bottom_end_y;
        let bottom_factor = y_clamped_gradient(bottom_from, bottom_to, 0.0, 1.0);
        lerp(bottom_factor, constant(bottom_target), noise_value)
    }

    // ------------------------------------------------------------------
    // slideOverworld
    // ------------------------------------------------------------------

    /// Port of `slideOverworld(boolean, DensityFunction)`.
    fn slide_overworld(amplified: bool, caves: DenseFn) -> DenseFn {
        if amplified {
            Self::slide(caves, -64, 384, 16, 0, -0.078125, 0, 24, 0.4)
        } else {
            Self::slide(caves, -64, 384, 80, 64, -0.078125, 0, 24, 0.1171875)
        }
    }

    // ------------------------------------------------------------------
    // postProcess — final density processing
    // ------------------------------------------------------------------

    /// Port of `postProcess(DensityFunction)`.
    ///
    /// Applies blend density, interpolates, multiplies by 0.64, and squeezes.
    pub fn post_process(slide: DenseFn) -> DenseFn {
        let blended = blend_density(slide);
        let scaled = mul(blended, constant(0.64));
        let interpolated = interpolated(scaled);
        squeeze(interpolated)
    }

    // ------------------------------------------------------------------
    // noiseGradientDensity
    // ------------------------------------------------------------------

    /// Port of `noiseGradientDensity(DensityFunction, DensityFunction)`.
    ///
    /// Computes `depth_with_jaggedness * factor * 4.0 * quarter_negative`.
    pub fn noise_gradient_density(factor: DenseFn, depth_with_jaggedness: DenseFn) -> DenseFn {
        let unscaled = mul(depth_with_jaggedness, factor);
        mul(constant(4.0), quarter_negative(unscaled))
    }

    // ------------------------------------------------------------------
    // offsetToDepth
    // ------------------------------------------------------------------

    /// Port of `offsetToDepth(DensityFunction)`.
    ///
    /// Computes `y_clamped_gradient(-64, 320, 1.5, -1.5) + offset`.
    pub fn offset_to_depth(offset: DenseFn) -> DenseFn {
        let gradient = y_clamped_gradient(-64, 320, 1.5, -1.5);
        add(gradient, offset)
    }

    // ------------------------------------------------------------------
    // splineWithBlending
    // ------------------------------------------------------------------

    /// Port of `splineWithBlending(DensityFunction, DensityFunction)`.
    fn spline_with_blending(spline: DenseFn, blend_target: DenseFn) -> DenseFn {
        let blended = lerp(blend_alpha(), blend_target, spline);
        flat_cache(cache2d(blended))
    }

    // ------------------------------------------------------------------
    // remap
    // ------------------------------------------------------------------

    /// Port of `remap(DensityFunction, double, double, double, double)`.
    fn remap(input: DenseFn, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> DenseFn {
        let factor = (to_max - to_min) / (from_max - from_min);
        let offset = to_min - from_min * factor;
        add(mul(input, constant(factor)), constant(offset))
    }

    // ------------------------------------------------------------------
    // yLimitedInterpolatable
    // ------------------------------------------------------------------

    /// Port of `yLimitedInterpolatable(DensityFunction, DensityFunction, int, int, int)`.
    fn y_limited_interpolatable(
        y: DenseFn,
        when_in_range: DenseFn,
        min_y_inclusive: i32,
        max_y_inclusive: i32,
        when_out_of_range: f64,
    ) -> DenseFn {
        interpolated(range_choice(
            y,
            min_y_inclusive as f64,
            (max_y_inclusive + 1) as f64,
            when_in_range,
            constant(when_out_of_range),
        ))
    }

    // ------------------------------------------------------------------
    // Cave sub-functions
    // ------------------------------------------------------------------

    /// Port of `spaghettiRoughnessFunction(HolderGetter)`.
    fn spaghetti_roughness_function(noises: &NoiseMap) -> DenseFn {
        let rough_noise = noise(noises.spaghetti_roughness.clone(), 1.0, 1.0);
        let rough_mod = mapped_noise(noises.spaghetti_roughness_modulator.clone(), 1.0, 1.0, 0.0, -0.1);

        cache_once(mul(
            rough_mod,
            add(abs(rough_noise), constant(-0.4)),
        ))
    }

    /// Port of `entrances(HolderGetter, HolderGetter)`.
    pub fn entrances(functions: &DensityFnMap, noises: &NoiseMap) -> DenseFn {
        let rarity_mod = cache_once(noise(noises.spaghetti_3d_rarity.clone(), 2.0, 1.0));
        let thickness_mod = mapped_noise(noises.spaghetti_3d_thickness.clone(), 1.0, 1.0, -0.065, -0.088);

        let cave_3d_1 = quantized_spaghetti_rarity_3d(&rarity_mod, noises.spaghetti_3d_1.clone());
        let cave_3d_2 = quantized_spaghetti_rarity_3d(&rarity_mod, noises.spaghetti_3d_2.clone());

        let spaghetti_3d = clamp(
            add(max(cave_3d_1, cave_3d_2), thickness_mod),
            -1.0,
            1.0,
        );

        let rough_fn = functions.spaghetti_roughness_function.clone();
        let big_entrance = noise(noises.cave_entrance.clone(), 0.75, 0.5);

        let big_entrances = add(
            add(big_entrance, constant(0.37)),
            y_clamped_gradient(-10, 30, 0.3, 0.0),
        );

        cache_once(min(big_entrances, add(rough_fn, spaghetti_3d)))
    }

    /// Port of `noodle(HolderGetter, HolderGetter)`.
    pub fn noodle(functions: &DensityFnMap, noises: &NoiseMap) -> DenseFn {
        let y = functions.y.clone();
        let noodle_toggle = Self::y_limited_interpolatable(
            y.clone(),
            noise(noises.noodle.clone(), 1.0, 1.0),
            NOODLE_MIN_Y,
            NOODLE_MAX_Y,
            -1.0,
        );

        let noodle_thickness = Self::y_limited_interpolatable(
            y.clone(),
            mapped_noise(noises.noodle_thickness.clone(), 1.0, 1.0, -0.05, -0.1),
            NOODLE_MIN_Y,
            NOODLE_MAX_Y,
            0.0,
        );

        let ridge_freq = 2.6666666666666665;
        let ridge_a = Self::y_limited_interpolatable(
            y.clone(),
            noise(noises.noodle_ridge_a.clone(), ridge_freq, ridge_freq),
            NOODLE_MIN_Y,
            NOODLE_MAX_Y,
            0.0,
        );
        let ridge_b = Self::y_limited_interpolatable(
            y,
            noise(noises.noodle_ridge_b.clone(), ridge_freq, ridge_freq),
            NOODLE_MIN_Y,
            NOODLE_MAX_Y,
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
            add(noodle_thickness, noodle_ridged),
        )
    }

    /// Port of `pillars(HolderGetter)`.
    pub fn pillars(noises: &NoiseMap) -> DenseFn {
        let pillar_noise = noise(noises.pillar.clone(), 25.0, 0.3);
        let pillar_rareness = mapped_noise_2(noises.pillar_rareness.clone(), 0.0, -2.0);
        let pillar_thickness = mapped_noise_2(noises.pillar_thickness.clone(), 0.0, 1.1);

        let pillars_with_rareness = add(
            mul(pillar_noise, constant(2.0)),
            pillar_rareness,
        );

        cache_once(mul(pillars_with_rareness, cube(pillar_thickness)))
    }

    /// Port of `spaghetti2D(HolderGetter, HolderGetter)`.
    pub fn spaghetti_2d(functions: &DensityFnMap, noises: &NoiseMap) -> DenseFn {
        let rarity_mod = noise(noises.spaghetti_2d_modulator.clone(), 2.0, 1.0);
        let spaghetti_2d_cave =
            quantized_spaghetti_rarity_2d(&rarity_mod, noises.spaghetti_2d.clone());

        let elevation_mod = mapped_noise_3(
            noises.spaghetti_2d_elevation.clone(),
            0.0,
            (-64.0_f64 / 8.0).floor(),
            8.0,
        );

        let thickness_mod = functions.spaghetti_2d_thickness_modulator.clone();

        let sloped_spaghetti = abs(add(
            flat_cache(elevation_mod),
            y_clamped_gradient(-64, 320, 8.0, -40.0),
        ));

        let layer_ridged = cube(add(sloped_spaghetti, thickness_mod.clone()));

        let ridge_offset = 0.083;
        let cave_noise = add(
            spaghetti_2d_cave,
            mul(constant(ridge_offset), thickness_mod),
        );

        clamp(max(cave_noise, layer_ridged), -1.0, 1.0)
    }

    /// Port of `underground(HolderGetter, HolderGetter, DensityFunction)`.
    pub fn underground(functions: &DensityFnMap, noises: &NoiseMap, sloped_cheese: DenseFn) -> DenseFn {
        let spaghetti_2d_fn = functions.spaghetti_2d.clone();
        let roughness_fn = functions.spaghetti_roughness_function.clone();

        let layer_noise = noise(noises.cave_layer.clone(), 8.0, 8.0);
        let layered_caverns = mul(constant(4.0), square(layer_noise));

        let cheese = noise(noises.cave_cheese.clone(), 0.6666666666666666, 0.6666666666666666);

        let solidified_cheese = add(
            clamp(add(constant(0.27), cheese), -1.0, 1.0),
            clamp(
                add(constant(1.5), mul(constant(-0.64), sloped_cheese)),
                0.0,
                0.5,
            ),
        );

        let base_cave_density = add(layered_caverns, solidified_cheese);

        let underground_subtractions = min(
            min(base_cave_density, functions.entrances.clone()),
            add(spaghetti_2d_fn, roughness_fn),
        );

        let pillars_no_cutoff = functions.pillars.clone();
        let pillars = range_choice(
            pillars_no_cutoff.clone(),
            -1000000.0,
            0.03,
            constant(-1000000.0),
            pillars_no_cutoff,
        );

        max(underground_subtractions, pillars)
    }

    // ------------------------------------------------------------------
    // preliminarySurfaceLevel
    // ------------------------------------------------------------------

    /// Port of `preliminarySurfaceLevel(DensityFunction, DensityFunction, boolean)`.
    pub fn preliminary_surface_level(
        offset: DenseFn,
        factor: DenseFn,
        amplified: bool,
    ) -> DenseFn {
        let cached_factor = cache2d(factor);
        let cached_offset = cache2d(offset);

        let upper_bound = Self::remap(
            add(
                mul(constant(0.2734375), invert(cached_factor.clone())),
                mul(constant(-1.0), cached_offset.clone()),
            ),
            1.5,
            -1.5,
            -64.0,
            320.0,
        );
        let upper_bound = clamp(upper_bound, -40.0, 320.0);

        let density = add(
            Self::slide_overworld(
                amplified,
                clamp(
                    add(
                        Self::noise_gradient_density(
                            cached_factor,
                            Self::offset_to_depth(cached_offset),
                        ),
                        constant(CHEESE_NOISE_TARGET),
                    ),
                    -64.0,
                    64.0,
                ),
            ),
            constant(-NOISE_ZERO),
        );

        // cell_height = 8 for overworld (size_vertical = 2, quart = 4*2)
        find_top_surface(density, upper_bound, -64, 8)
    }

    // ------------------------------------------------------------------
    // Terrain spline providers (approximations of TerrainProvider)
    // ------------------------------------------------------------------

    /// Approximates `TerrainProvider.overworldOffset(continents, erosion, ridges, amplified)`.
    ///
    /// Returns a DenseFn that combines the three inputs into a terrain
    /// height offset value using approximated spline-like behavior.
    fn terrain_offset(
        _continents: DenseFn,
        _erosion: DenseFn,
        _ridges: DenseFn,
        amplified: bool,
    ) -> DenseFn {
        // Approximated: continents dominates the offset, erosion modulates,
        // and ridges adds mountain variation.
        let extra = if amplified { constant(0.3) } else { constant(0.0) };
        add(add(_continents, mul(_erosion, constant(-0.4))), add(mul(_ridges, constant(0.2)), extra))
    }

    /// Approximates `TerrainProvider.overworldFactor(continents, erosion, weirdness, ridges, amplified)`.
    fn terrain_factor(
        _continents: DenseFn,
        _erosion: DenseFn,
        _weirdness: DenseFn,
        _ridges: DenseFn,
        amplified: bool,
    ) -> DenseFn {
        // Approximated: factor combines continentalness (land gets more
        // height variation) and weirdness (mountain regions get more), with
        // amplified boosting everything.
        let base = add(
            add(mul(_continents, constant(2.0)), mul(_erosion, constant(0.8))),
            add(mul(_weirdness, constant(1.5)), mul(_ridges, constant(0.5))),
        );
        let clamped = clamp(base, 0.0, 8.0);
        if amplified {
            mul(clamped, constant(2.0))
        } else {
            mul(clamped, constant(1.0))
        }
    }

    /// Approximates `TerrainProvider.overworldJaggedness(continents, erosion, weirdness, ridges, amplified)`.
    fn terrain_jaggedness(
        _continents: DenseFn,
        _erosion: DenseFn,
        _weirdness: DenseFn,
        _ridges: DenseFn,
        amplified: bool,
    ) -> DenseFn {
        // Approximated: jaggedness is highest in areas with high weirdness
        // (mountain ridges) and low erosion (not worn down).
        let base = add(
            mul(_weirdness, constant(0.6)),
            mul(_ridges, constant(0.3)),
        );
        let clamped = clamp(base, 0.0, 1.0);
        if amplified {
            mul(clamped, constant(1.5))
        } else {
            clamped
        }
    }

    // ------------------------------------------------------------------
    // registerTerrainNoises — builds offset, factor, depth, jaggedness,
    //                         and sloped_cheese
    // ------------------------------------------------------------------

    fn register_terrain_noises(
        functions: &mut DensityFnMap,
        noises: &NoiseMap,
        jagged_noise: DenseFn,
        continents_function: DenseFn,
        erosion_function: DenseFn,
        amplified: bool,
    ) -> DenseFn {
        let ridge_fn = Self::peaks_and_valleys_fn(noise(noises.ridge.clone(), 1.0, 0.0));
        let weirdness_fn = Self::peaks_and_valleys_fn(noise(noises.ridge.clone(), 1.0, 0.0));

        let offset_spline = Self::terrain_offset(
            continents_function.clone(),
            erosion_function.clone(),
            ridge_fn.clone(),
            amplified,
        );
        let offset = Self::spline_with_blending(
            add(constant(GLOBAL_OFFSET), offset_spline),
            blend_offset(),
        );
        functions.offset = offset.clone();

        let factor_spline = Self::terrain_factor(
            continents_function.clone(),
            erosion_function.clone(),
            weirdness_fn.clone(),
            ridge_fn.clone(),
            amplified,
        );
        let factor = Self::spline_with_blending(factor_spline, constant(BLENDING_FACTOR_VALUE));
        functions.factor = factor.clone();

        let depth = Self::offset_to_depth(offset.clone());
        functions.depth = depth.clone();

        let unscaled_jaggedness_spline = Self::terrain_jaggedness(
            continents_function,
            erosion_function,
            weirdness_fn,
            ridge_fn,
            amplified,
        );
        let unscaled_jaggedness = Self::spline_with_blending(
            unscaled_jaggedness_spline,
            zero(),
        );

        let jaggedness = flat_cache(mul(unscaled_jaggedness, half_negative(jagged_noise)));
        functions.jaggedness = jaggedness.clone();

        let initial_density = Self::noise_gradient_density(factor, add(depth, jaggedness));
        let sloped_cheese = add(initial_density, functions.base_3d_noise.clone());
        functions.sloped_cheese = sloped_cheese.clone();

        sloped_cheese
    }

    // ------------------------------------------------------------------
    // create_overworld_router — the main entry point
    // ------------------------------------------------------------------

    /// Port of `NoiseRouterData.overworld(HolderGetter, HolderGetter, boolean, boolean)`.
    ///
    /// Creates a fully-wired `NoiseRouter` for the overworld dimension.
    pub fn create_overworld_router(seed: u64, large_biomes: bool, amplified: bool) -> NoiseRouter {
        let mut functions = DensityFnMap::new();
        let noises = NoiseMap::from_seed(seed, large_biomes);
        let mut seed_gen = NoiseSeed::new(seed);

        // --- Y function ---
        // below_bottom = -128, above_top = 640 (DimensionType.MIN_Y=-64, MAX_Y=320, *2)
        functions.y = y_clamped_gradient(-128, 640, -128.0, 640.0);

        // --- Shift X/Z ---
        let shift_noise = NoiseHandle::new(NormalNoise::create(
            &mut NoiseSeed::from_hash_of(seed, "offset"),
            &create_noise_parameters(&NoiseKey::Shift),
        ));
        let shift_x = flat_cache(cache2d(shift_a(shift_noise.clone())));
        let shift_z = flat_cache(cache2d(shift_b(shift_noise)));

        // --- Base 3D noise (BlendedNoise) ---
        functions.base_3d_noise = create_blended_noise(&mut seed_gen, 0.25, 0.125, 80.0, 160.0, 8.0);

        // --- Continentalness, erosion, ridge ---
        let continents_key = if large_biomes {
            NoiseKey::ContinentalnessLarge
        } else {
            NoiseKey::Continentalness
        };
        let erosion_key = if large_biomes {
            NoiseKey::ErosionLarge
        } else {
            NoiseKey::Erosion
        };
        let temp_key = if large_biomes {
            NoiseKey::TemperatureLarge
        } else {
            NoiseKey::Temperature
        };
        let veg_key = if large_biomes {
            NoiseKey::VegetationLarge
        } else {
            NoiseKey::Vegetation
        };

        let continents = flat_cache(shifted_noise_2d(
            shift_x.clone(),
            shift_z.clone(),
            0.25,
            NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::from_hash_of(seed, continents_key.name()),
                &create_noise_parameters(&continents_key),
            )),
        ));

        let erosion = flat_cache(shifted_noise_2d(
            shift_x.clone(),
            shift_z.clone(),
            0.25,
            NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::from_hash_of(seed, erosion_key.name()),
                &create_noise_parameters(&erosion_key),
            )),
        ));

        let ridge = flat_cache(shifted_noise_2d(
            shift_x.clone(),
            shift_z.clone(),
            0.25,
            NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::from_hash_of(seed, "ridge"),
                &create_noise_parameters(&NoiseKey::Ridge),
            )),
        ));
        functions.ridges = ridge.clone();
        functions.ridges_folded = Self::peaks_and_valleys_fn(ridge.clone());

        // --- Jagged noise ---
        let jagged_noise = half_negative(noise(
            NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::from_hash_of(seed, "jagged"),
                &create_noise_parameters(&NoiseKey::Jagged),
            )),
            1500.0,
            0.0,
        ));

        // --- Register terrain noises (offset, factor, depth, jaggedness, sloped_cheese) ---
        let sloped_cheese = Self::register_terrain_noises(
            &mut functions,
            &noises,
            jagged_noise,
            continents,
            erosion,
            amplified,
        );

        // --- Cache the sloped cheese ---
        let sloped_cheese_cached = cache_once(sloped_cheese);

        // --- Cave functions ---
        functions.spaghetti_roughness_function =
            Self::spaghetti_roughness_function(&noises);
        functions.spaghetti_2d_thickness_modulator = cache_once(mapped_noise(
            noises.spaghetti_2d_thickness.clone(),
            2.0,
            1.0,
            -0.6,
            -1.3,
        ));
        functions.spaghetti_2d = Self::spaghetti_2d(&functions, &noises);
        functions.entrances = Self::entrances(&functions, &noises);
        functions.noodle = Self::noodle(&functions, &noises);
        functions.pillars = Self::pillars(&noises);

        // --- Preliminary surface level ---
        let offset_fn = functions.offset.clone();
        let factor_fn = functions.factor.clone();
        let preliminary_surface = Self::preliminary_surface_level(offset_fn, factor_fn, amplified);

        // --- Build final density ---
        let surface_with_entrances = min(
            sloped_cheese_cached.clone(),
            mul(constant(5.0), functions.entrances.clone()),
        );

        let caves = range_choice(
            sloped_cheese_cached.clone(),
            -1000000.0,
            SURFACE_DENSITY_THRESHOLD,
            surface_with_entrances,
            Self::underground(&functions, &noises, sloped_cheese_cached.clone()),
        );

        let full_noise = min(
            Self::post_process(Self::slide_overworld(amplified, caves)),
            functions.noodle.clone(),
        );

        // --- Vein functions ---
        let y_fn = functions.y.clone();
        let vein_toggle = Self::y_limited_interpolatable(
            y_fn.clone(),
            noise(noises.ore_veininess.clone(), VEININESS_FREQUENCY, VEININESS_FREQUENCY),
            VEIN_MIN_Y,
            VEIN_MAX_Y,
            0.0,
        );

        let vein_a = Self::y_limited_interpolatable(
            y_fn.clone(),
            noise(noises.ore_vein_a.clone(), 4.0, 4.0),
            VEIN_MIN_Y,
            VEIN_MAX_Y,
            0.0,
        );
        let vein_b = Self::y_limited_interpolatable(
            y_fn,
            noise(noises.ore_vein_b.clone(), 4.0, 4.0),
            VEIN_MIN_Y,
            VEIN_MAX_Y,
            0.0,
        );

        let vein_ridged = add(
            constant(-0.07999999821186066),
            max(abs(vein_a), abs(vein_b)),
        );

        let vein_gap = noise(noises.ore_gap.clone(), 1.0, 1.0);

        // --- Temperature & vegetation ---
        let temperature = shifted_noise_2d(
            shift_x.clone(),
            shift_z.clone(),
            0.25,
            NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::from_hash_of(seed, temp_key.name()),
                &create_noise_parameters(&temp_key),
            )),
        );

        let vegetation = shifted_noise_2d(
            shift_x,
            shift_z,
            0.25,
            NoiseHandle::new(NormalNoise::create(
                &mut NoiseSeed::from_hash_of(seed, veg_key.name()),
                &create_noise_parameters(&veg_key),
            )),
        );

        // --- Aquifer noises ---
        let barrier_noise = noise(noises.aquifer_barrier.clone(), 0.5, 0.5);
        let fluid_floodedness = noise(noises.aquifer_fluid_floodedness.clone(), 0.67, 0.67);
        let fluid_spread = noise(noises.aquifer_fluid_spread.clone(), 0.7142857142857143, 0.7142857142857143);
        let lava_noise = noise(noises.aquifer_lava.clone(), 1.0, 1.0);

        NoiseRouter {
            barrier_noise,
            fluid_level_floodedness_noise: fluid_floodedness,
            fluid_level_spread_noise: fluid_spread,
            lava_noise,
            temperature,
            vegetation,
            continents: if large_biomes {
                noise(noises.continentalness_large.clone(), 1.0, 0.0)
            } else {
                noise(noises.continentalness.clone(), 1.0, 0.0)
            },
            erosion: if large_biomes {
                noise(noises.erosion_large.clone(), 1.0, 0.0)
            } else {
                noise(noises.erosion.clone(), 1.0, 0.0)
            },
            depth: functions.depth,
            ridges: ridge,
            preliminary_surface_level: preliminary_surface,
            final_density: full_noise,
            vein_toggle,
            vein_ridged,
            vein_gap,
        }
    }
}

// ============================================================================
// QuantizedSpaghettiRarity — inner class from NoiseRouterData
// ============================================================================

/// Port of `NoiseRouterData.QuantizedSpaghettiRarity`.
fn quantized_spaghetti_rarity_2d(input: &DenseFn, noise: NoiseHandle) -> DenseFn {
    let thresholds = vec![-0.75, -0.5, 0.5, 0.75];
    let functions = vec![
        noise_fn_for_rarity(noise.clone(), 0.5),
        noise_fn_for_rarity(noise.clone(), 0.75),
        noise_fn_for_rarity(noise.clone(), 1.0),
        noise_fn_for_rarity(noise.clone(), 2.0),
        noise_fn_for_rarity(noise, 3.0),
    ];
    abs(interval_select(input.clone(), thresholds, functions))
}

fn quantized_spaghetti_rarity_3d(input: &DenseFn, noise: NoiseHandle) -> DenseFn {
    let thresholds = vec![-0.5, 0.0, 0.5];
    let functions = vec![
        noise_fn_for_rarity(noise.clone(), 0.75),
        noise_fn_for_rarity(noise.clone(), 1.0),
        noise_fn_for_rarity(noise.clone(), 1.5),
        noise_fn_for_rarity(noise, 2.0),
    ];
    abs(interval_select(input.clone(), thresholds, functions))
}

fn noise_fn_for_rarity(noise: NoiseHandle, rarity: f64) -> DenseFn {
    mul(constant(rarity), noise_fn(noise, 1.0 / rarity, 1.0 / rarity))
}

// ============================================================================
// MappedNoise — Noise with linear mapping: output = flatness * noise + offset
// ============================================================================

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

/// Mapped noise: maps the output of a noise via `output = sample * flatness + offset`.
fn mapped_noise(noise: NoiseHandle, xz_scale: f64, y_scale: f64, flatness: f64, offset: f64) -> DenseFn {
    DenseFn(Box::new(MappedNoise {
        noise,
        xz_scale,
        y_scale,
        flatness,
        offset,
    }))
}

/// Convenience: xz_scale, y_scale, flatness, offset=0.
fn mapped_noise_3(noise: NoiseHandle, xz_scale: f64, y_scale: f64, flatness: f64) -> DenseFn {
    mapped_noise(noise, xz_scale, y_scale, flatness, 0.0)
}

/// Convenience: flatness, offset, xz_scale=y_scale=1.
fn mapped_noise_2(noise: NoiseHandle, flatness: f64, offset: f64) -> DenseFn {
    mapped_noise(noise, 1.0, 1.0, flatness, offset)
}

// ============================================================================
// BlendedNoise — port of Minecraft's BlendedNoise density function
// ============================================================================

/// Port of `net.minecraft.world.level.levelgen.synth.BlendedNoise`.
///
/// Uses three PerlinNoise instances (minLimit, maxLimit, main) with a
/// special sampling algorithm that blends between them.
#[derive(Clone)]
pub struct BlendedNoiseDensity {
    min_limit_noise: PerlinNoise,
    max_limit_noise: PerlinNoise,
    main_noise: PerlinNoise,
    xz_multiplier: f64,
    y_multiplier: f64,
    xz_factor: f64,
    y_factor: f64,
    smear_scale_multiplier: f64,
    max_value: f64,
}

impl BlendedNoiseDensity {
    /// Create with random source and the 5 blended noise parameters.
    /// Equivalent to `BlendedNoise(RandomSource, double, double, double, double, double)`.
    pub fn new(random: &mut NoiseSeed, xz_scale: f64, y_scale: f64, xz_factor: f64, y_factor: f64, smear_scale_multiplier: f64) -> Self {
        // Match BlendedNoise constructor: octaves -15..0 for min/max, -7..0 for main
        let min_noise = PerlinNoise::create_with_amplitudes(
            random, -15, vec![1.0; 16],
        );
        let max_noise = PerlinNoise::create_with_amplitudes(
            random, -15, vec![1.0; 16],
        );
        let main = PerlinNoise::create_with_amplitudes(
            random, -7, vec![1.0; 8],
        );

        let xz_mult = 684.412 * xz_scale;
        let y_mult = 684.412 * y_scale;

        let max_val = min_noise.max_broken_value(y_mult);

        BlendedNoiseDensity {
            min_limit_noise: min_noise,
            max_limit_noise: max_noise,
            main_noise: main,
            xz_multiplier: xz_mult,
            y_multiplier: y_mult,
            xz_factor,
            y_factor,
            smear_scale_multiplier,
            max_value: max_val,
        }
    }
}

impl DensityFunction for BlendedNoiseDensity {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let bx = ctx.block_x() as f64;
        let by = ctx.block_y() as f64;
        let bz = ctx.block_z() as f64;

        let limit_x = bx * self.xz_multiplier;
        let limit_y = by * self.y_multiplier;
        let limit_z = bz * self.xz_multiplier;

        let main_x = limit_x / self.xz_factor;
        let main_y = limit_y / self.y_factor;
        let main_z = limit_z / self.xz_factor;

        let limit_smear = self.y_multiplier * self.smear_scale_multiplier;
        let main_smear = limit_smear / self.y_factor;

        let mut main_value = 0.0;
        let mut pow = 1.0;
        for i in 0..8 {
            if let Some(noise) = self.main_noise.get_octave_noise(i as i32) {
                let nx = PerlinNoise::wrap(main_x * pow);
                let ny = PerlinNoise::wrap(main_y * pow);
                let nz = PerlinNoise::wrap(main_z * pow);
                main_value += noise.noise_with_y_scale(
                    nx, ny, nz, main_smear * pow, main_y * pow,
                ) / pow;
            }
            pow /= 2.0;
        }

        let factor = (main_value / 10.0 + 1.0) / 2.0;
        let is_max = factor >= 1.0;
        let is_min = factor <= 0.0;

        let mut blend_min = 0.0;
        let mut blend_max = 0.0;
        pow = 1.0;
        for j in 0..16 {
            let wx = PerlinNoise::wrap(limit_x * pow);
            let wy = PerlinNoise::wrap(limit_y * pow);
            let wz = PerlinNoise::wrap(limit_z * pow);
            let y_scale_pow = limit_smear * pow;

            if !is_max {
                if let Some(noise) = self.min_limit_noise.get_octave_noise(j as i32) {
                    blend_min += noise.noise_with_y_scale(
                        wx, wy, wz, y_scale_pow, limit_y * pow,
                    ) / pow;
                }
            }
            if !is_min {
                if let Some(noise) = self.max_limit_noise.get_octave_noise(j as i32) {
                    blend_max += noise.noise_with_y_scale(
                        wx, wy, wz, y_scale_pow, limit_y * pow,
                    ) / pow;
                }
            }
            pow /= 2.0;
        }

        let blended = (blend_min / 512.0) + (blend_max / 512.0 - blend_min / 512.0) * factor;
        blended / 128.0
    }

    fn min_value(&self) -> f64 {
        -self.max_value
    }

    fn max_value(&self) -> f64 {
        self.max_value
    }

    fn map_children(&self, _visitor: &dyn Visitor) -> DenseFn {
        self.clone_dyn()
    }

    fn clone_dyn(&self) -> DenseFn {
        DenseFn(Box::new(self.clone()))
    }
}

/// Create a BlendedNoise DenseFn directly, used as the base_3d_noise.
fn create_blended_noise(
    random: &mut NoiseSeed,
    xz_scale: f64,
    y_scale: f64,
    xz_factor: f64,
    y_factor: f64,
    smear_scale_multiplier: f64,
) -> DenseFn {
    DenseFn(Box::new(BlendedNoiseDensity::new(
        random,
        xz_scale,
        y_scale,
        xz_factor,
        y_factor,
        smear_scale_multiplier,
    )))
}

// ============================================================================
// NoiseMap — holds all NoiseHandle instances for the overworld
// ============================================================================

/// All noise handles needed by the overworld noise router, created from a seed.
pub struct NoiseMap {
    pub temperature: NoiseHandle,
    pub vegetation: NoiseHandle,
    pub continentalness: NoiseHandle,
    pub erosion: NoiseHandle,
    pub temperature_large: NoiseHandle,
    pub vegetation_large: NoiseHandle,
    pub continentalness_large: NoiseHandle,
    pub erosion_large: NoiseHandle,
    pub ridge: NoiseHandle,
    pub shift: NoiseHandle,
    pub aquifer_barrier: NoiseHandle,
    pub aquifer_fluid_floodedness: NoiseHandle,
    pub aquifer_lava: NoiseHandle,
    pub aquifer_fluid_spread: NoiseHandle,
    pub pillar: NoiseHandle,
    pub pillar_rareness: NoiseHandle,
    pub pillar_thickness: NoiseHandle,
    pub spaghetti_2d: NoiseHandle,
    pub spaghetti_2d_elevation: NoiseHandle,
    pub spaghetti_2d_modulator: NoiseHandle,
    pub spaghetti_2d_thickness: NoiseHandle,
    pub spaghetti_3d_1: NoiseHandle,
    pub spaghetti_3d_2: NoiseHandle,
    pub spaghetti_3d_rarity: NoiseHandle,
    pub spaghetti_3d_thickness: NoiseHandle,
    pub spaghetti_roughness: NoiseHandle,
    pub spaghetti_roughness_modulator: NoiseHandle,
    pub cave_entrance: NoiseHandle,
    pub cave_layer: NoiseHandle,
    pub cave_cheese: NoiseHandle,
    pub ore_veininess: NoiseHandle,
    pub ore_vein_a: NoiseHandle,
    pub ore_vein_b: NoiseHandle,
    pub ore_gap: NoiseHandle,
    pub noodle: NoiseHandle,
    pub noodle_thickness: NoiseHandle,
    pub noodle_ridge_a: NoiseHandle,
    pub noodle_ridge_b: NoiseHandle,
    pub jagged: NoiseHandle,
    pub surface: NoiseHandle,
    pub surface_secondary: NoiseHandle,
    pub clay_bands_offset: NoiseHandle,
}

impl NoiseMap {
    /// Create all overworld noise instances from a seed.
    pub fn from_seed(seed: u64, large_biomes: bool) -> Self {
        let temp_key = if large_biomes { NoiseKey::TemperatureLarge } else { NoiseKey::Temperature };
        let veg_key = if large_biomes { NoiseKey::VegetationLarge } else { NoiseKey::Vegetation };
        let cont_key = if large_biomes { NoiseKey::ContinentalnessLarge } else { NoiseKey::Continentalness };
        let eros_key = if large_biomes { NoiseKey::ErosionLarge } else { NoiseKey::Erosion };

        NoiseMap {
            temperature: Self::create_noise(seed, &temp_key),
            vegetation: Self::create_noise(seed, &veg_key),
            continentalness: Self::create_noise(seed, &cont_key),
            erosion: Self::create_noise(seed, &eros_key),
            temperature_large: Self::create_noise(seed, &NoiseKey::TemperatureLarge),
            vegetation_large: Self::create_noise(seed, &NoiseKey::VegetationLarge),
            continentalness_large: Self::create_noise(seed, &NoiseKey::ContinentalnessLarge),
            erosion_large: Self::create_noise(seed, &NoiseKey::ErosionLarge),
            ridge: Self::create_noise(seed, &NoiseKey::Ridge),
            shift: Self::create_noise(seed, &NoiseKey::Shift),
            aquifer_barrier: Self::create_noise(seed, &NoiseKey::AquiferBarrier),
            aquifer_fluid_floodedness: Self::create_noise(seed, &NoiseKey::AquiferFluidLevelFloodedness),
            aquifer_lava: Self::create_noise(seed, &NoiseKey::AquiferLava),
            aquifer_fluid_spread: Self::create_noise(seed, &NoiseKey::AquiferFluidLevelSpread),
            pillar: Self::create_noise(seed, &NoiseKey::Pillar),
            pillar_rareness: Self::create_noise(seed, &NoiseKey::PillarRareness),
            pillar_thickness: Self::create_noise(seed, &NoiseKey::PillarThickness),
            spaghetti_2d: Self::create_noise(seed, &NoiseKey::Spaghetti2d),
            spaghetti_2d_elevation: Self::create_noise(seed, &NoiseKey::Spaghetti2dElevation),
            spaghetti_2d_modulator: Self::create_noise(seed, &NoiseKey::Spaghetti2dModulator),
            spaghetti_2d_thickness: Self::create_noise(seed, &NoiseKey::Spaghetti2dThickness),
            spaghetti_3d_1: Self::create_noise(seed, &NoiseKey::Spaghetti3d1),
            spaghetti_3d_2: Self::create_noise(seed, &NoiseKey::Spaghetti3d2),
            spaghetti_3d_rarity: Self::create_noise(seed, &NoiseKey::Spaghetti3dRarity),
            spaghetti_3d_thickness: Self::create_noise(seed, &NoiseKey::Spaghetti3dThickness),
            spaghetti_roughness: Self::create_noise(seed, &NoiseKey::SpaghettiRoughness),
            spaghetti_roughness_modulator: Self::create_noise(seed, &NoiseKey::SpaghettiRoughnessModulator),
            cave_entrance: Self::create_noise(seed, &NoiseKey::CaveEntrance),
            cave_layer: Self::create_noise(seed, &NoiseKey::CaveLayer),
            cave_cheese: Self::create_noise(seed, &NoiseKey::CaveCheese),
            ore_veininess: Self::create_noise(seed, &NoiseKey::OreVeininess),
            ore_vein_a: Self::create_noise(seed, &NoiseKey::OreVeinA),
            ore_vein_b: Self::create_noise(seed, &NoiseKey::OreVeinB),
            ore_gap: Self::create_noise(seed, &NoiseKey::OreGap),
            noodle: Self::create_noise(seed, &NoiseKey::Noodle),
            noodle_thickness: Self::create_noise(seed, &NoiseKey::NoodleThickness),
            noodle_ridge_a: Self::create_noise(seed, &NoiseKey::NoodleRidgeA),
            noodle_ridge_b: Self::create_noise(seed, &NoiseKey::NoodleRidgeB),
            jagged: Self::create_noise(seed, &NoiseKey::Jagged),
            surface: Self::create_noise(seed, &NoiseKey::Surface),
            surface_secondary: Self::create_noise(seed, &NoiseKey::SurfaceSecondary),
            clay_bands_offset: Self::create_noise(seed, &NoiseKey::ClayBandsOffset),
        }
    }

    fn create_noise(seed: u64, key: &NoiseKey) -> NoiseHandle {
        let params = create_noise_parameters(key);
        let mut rng = NoiseSeed::from_hash_of(seed, key.name());
        NoiseHandle::new(NormalNoise::create(&mut rng, &params))
    }
}

// ============================================================================
// DensityFnMap — holds cross-referenced density functions
// ============================================================================

/// Like the Java `HolderGetter<DensityFunction>` — stores named functions
/// that are built during router creation and referenced by other functions.
#[derive(Clone)]
pub struct DensityFnMap {
    pub y: DenseFn,
    pub base_3d_noise: DenseFn,
    pub ridges: DenseFn,
    pub ridges_folded: DenseFn,
    pub offset: DenseFn,
    pub factor: DenseFn,
    pub jaggedness: DenseFn,
    pub depth: DenseFn,
    pub sloped_cheese: DenseFn,
    pub spaghetti_roughness_function: DenseFn,
    pub entrances: DenseFn,
    pub noodle: DenseFn,
    pub pillars: DenseFn,
    pub spaghetti_2d_thickness_modulator: DenseFn,
    pub spaghetti_2d: DenseFn,
}

impl DensityFnMap {
    fn new() -> Self {
        DensityFnMap {
            y: zero(),
            base_3d_noise: zero(),
            ridges: zero(),
            ridges_folded: zero(),
            offset: zero(),
            factor: zero(),
            jaggedness: zero(),
            depth: zero(),
            sloped_cheese: zero(),
            spaghetti_roughness_function: zero(),
            entrances: zero(),
            noodle: constant(64.0),
            pillars: zero(),
            spaghetti_2d_thickness_modulator: zero(),
            spaghetti_2d: zero(),
        }
    }
}

// ============================================================================
// noise() convenience — wraps NoiseHandle into a DenseFn
// ============================================================================

fn noise_fn(handle: NoiseHandle, xz_scale: f64, y_scale: f64) -> DenseFn {
    noise(handle, xz_scale, y_scale)
}

// ============================================================================
// create_noise_parameters — maps NoiseKey to NoiseParameters
// ============================================================================

/// Port of the noise parameter constants from `Noises.java`.
///
/// Returns the `NoiseParameters` (first_octave + amplitudes) that match
/// Minecraft's noise definitions for each key.
pub fn create_noise_parameters(key: &NoiseKey) -> NoiseParameters {
    match key {
        NoiseKey::Temperature => NoiseParameters::from_slice(-9, &[1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Vegetation => NoiseParameters::from_slice(-8, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Continentalness => NoiseParameters::from_slice(-9, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Erosion => NoiseParameters::from_slice(-9, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::TemperatureLarge => NoiseParameters::from_slice(-10, &[1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::VegetationLarge => NoiseParameters::from_slice(-9, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::ContinentalnessLarge => NoiseParameters::from_slice(-10, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::ErosionLarge => NoiseParameters::from_slice(-10, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Ridge => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Shift => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Jagged => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),

        // Cave noises
        NoiseKey::Spaghetti2d => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti2dElevation => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti2dModulator => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti2dThickness => NoiseParameters::from_slice(-4, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti3d1 => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti3d2 => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti3dRarity => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Spaghetti3dThickness => NoiseParameters::from_slice(-4, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::SpaghettiRoughness => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::SpaghettiRoughnessModulator => NoiseParameters::from_slice(-4, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::CaveEntrance => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::CaveLayer => NoiseParameters::from_slice(-1, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::CaveCheese => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::OreVeininess => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::OreVeinA => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::OreVeinB => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::OreGap => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::Noodle => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::NoodleThickness => NoiseParameters::from_slice(-4, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::NoodleRidgeA => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::NoodleRidgeB => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),

        // Aquifer noises
        NoiseKey::AquiferBarrier => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::AquiferFluidLevelFloodedness => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::AquiferLava => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::AquiferFluidLevelSpread => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),

        // Surface noises
        NoiseKey::Surface => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::SurfaceSecondary => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::ClayBandsOffset => NoiseParameters::from_slice(-6, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),

        // Pillar noises
        NoiseKey::Pillar => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::PillarRareness => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        NoiseKey::PillarThickness => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),

        // Remaining keys (placeholder — same generic params)
        _ => NoiseParameters::from_slice(-7, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    }
}
