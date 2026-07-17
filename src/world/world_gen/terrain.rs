//! Port of Minecraft's TerrainProvider cubic spline terrain shaping
//!
//! Defines the cubic splines for the overworld terrain offset, factor,
//! and jaggedness based on continents, erosion, ridges, and weirdness.
//!
//! Reference: net.minecraft.data.worldgen.TerrainProvider

use crate::world::world_gen::density_fn::{
    DenseFn, DensityFunction, FunctionContext, SinglePointContext,
    SplineCoordinate, SplinePoint, Visitor,
};

// ---------------------------------------------------------------------------
// Constants matching TerrainProvider.java
// ---------------------------------------------------------------------------

const DEEP_OCEAN_CONTINENTALNESS: f32 = -0.51;
#[allow(dead_code)]
const OCEAN_CONTINENTALNESS: f32 = -0.4;
#[allow(dead_code)]
const PLAINS_CONTINENTALNESS: f32 = 0.1;
#[allow(dead_code)]
const BEACH_CONTINENTALNESS: f32 = -0.15;

// ---------------------------------------------------------------------------
// Transformer functions for amplified mode
// ---------------------------------------------------------------------------

type TransformFn = fn(f32) -> f32;

fn no_transform(x: f32) -> f32 {
    x
}

fn amplified_offset(x: f32) -> f32 {
    if x < 0.0 { x } else { x * 2.0 }
}

fn amplified_factor(x: f32) -> f32 {
    1.25 - 6.25 / (x + 5.0)
}

fn amplified_jaggedness(x: f32) -> f32 {
    x * 2.0
}

// ---------------------------------------------------------------------------
// Spline value: constant or nested spline
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum SplineNode {
    Constant(f32),
    Spline(DenseFn),
}

impl From<f32> for SplineNode {
    fn from(v: f32) -> Self {
        SplineNode::Constant(v)
    }
}

impl From<TerrainSpline> for SplineNode {
    fn from(s: TerrainSpline) -> Self {
        SplineNode::Spline(DenseFn(Box::new(s)))
    }
}

// ---------------------------------------------------------------------------
// Control point for TerrainSpline
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TerrainControlPoint {
    location: f32,
    value: SplineNode,
    derivative: f32,
}

// ---------------------------------------------------------------------------
// Cubic spline that supports nested spline values
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TerrainSpline {
    coordinate: SplineCoordinate,
    points: Vec<TerrainControlPoint>,
    min_value: f64,
    max_value: f64,
    evaluation: SplineEvaluation,
}

#[derive(Clone, Copy)]
enum SplineEvaluation {
    Legacy,
    Java,
}

impl TerrainSpline {
    fn new(
        coordinate: SplineCoordinate,
        points: Vec<TerrainControlPoint>,
        evaluation: SplineEvaluation,
    ) -> Self {
        let (min_value, max_value) = compute_bounds(&points);
        Self {
            coordinate,
            points,
            min_value,
            max_value,
            evaluation,
        }
    }

    fn eval_node(&self, node: &SplineNode, ctx: &dyn FunctionContext) -> f32 {
        match node {
            SplineNode::Constant(v) => *v,
            SplineNode::Spline(f) => f.compute(ctx) as f32,
        }
    }

    fn evaluate_at(&self, x: f32, ctx: &dyn FunctionContext) -> f32 {
        let pts = &self.points;
        if pts.is_empty() {
            return 0.0;
        }

        // Java's CubicSpline finds the first location strictly greater than
        // the input, then uses the preceding point as the interval start.
        let mut lo = 0usize;
        let mut hi = pts.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if x < pts[mid].location {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let start = lo as isize - 1;

        if start < 0 {
            let p = &pts[0];
            return self.eval_node(&p.value, ctx) + p.derivative * (x - p.location);
        }

        let start = start as usize;
        if start == pts.len() - 1 {
            let p = &pts[start];
            return self.eval_node(&p.value, ctx) + p.derivative * (x - p.location);
        }

        let p1 = &pts[start];
        let p2 = &pts[start + 1];
        let y1 = self.eval_node(&p1.value, ctx);
        let y2 = self.eval_node(&p2.value, ctx);
        let dx = p2.location - p1.location;
        if dx == 0.0 {
            return y1;
        }

        let t = (x - p1.location) / dx;
        let a = match self.evaluation {
            SplineEvaluation::Legacy => p1.derivative * dx - y2 - y1,
            SplineEvaluation::Java => p1.derivative * dx - (y2 - y1),
        };
        let b = -p2.derivative * dx + y2 - y1;
        y1 + t * (y2 - y1) + t * (1.0 - t) * (a + t * (b - a))
    }
}

fn compute_bounds(points: &[TerrainControlPoint]) -> (f64, f64) {
    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;
    for p in points {
        match &p.value {
            SplineNode::Constant(v) => {
                let v = *v as f64;
                if v < min_val {
                    min_val = v;
                }
                if v > max_val {
                    max_val = v;
                }
            }
            SplineNode::Spline(f) => {
                let a = f.min_value();
                let b = f.max_value();
                if a < min_val {
                    min_val = a;
                }
                if b > max_val {
                    max_val = b;
                }
            }
        }
    }
    if min_val == f64::INFINITY {
        min_val = 0.0;
    }
    if max_val == f64::NEG_INFINITY {
        max_val = 0.0;
    }
    (min_val, max_val)
}

impl DensityFunction for TerrainSpline {
    fn compute(&self, ctx: &dyn FunctionContext) -> f64 {
        let point = SplinePoint::new(SinglePointContext {
            block_x: ctx.block_x(),
            block_y: ctx.block_y(),
            block_z: ctx.block_z(),
        });
        let coord = self.coordinate.evaluate(&point);
        self.evaluate_at(coord, ctx) as f64
    }

    fn min_value(&self) -> f64 {
        self.min_value
    }

    fn max_value(&self) -> f64 {
        self.max_value
    }

    fn map_children(&self, visitor: &dyn Visitor) -> DenseFn {
        let new_coord = SplineCoordinate(visitor.apply(self.coordinate.0.clone()));
        let new_points: Vec<TerrainControlPoint> = self
            .points
            .iter()
            .map(|p| TerrainControlPoint {
                location: p.location,
                value: match &p.value {
                    SplineNode::Constant(v) => SplineNode::Constant(*v),
                    SplineNode::Spline(f) => SplineNode::Spline(visitor.apply(f.clone())),
                },
                derivative: p.derivative,
            })
            .collect();
        DenseFn(Box::new(TerrainSpline::new(
            new_coord,
            new_points,
            self.evaluation,
        )))
    }

    fn clone_dyn(&self) -> DenseFn {
        DenseFn(Box::new(self.clone()))
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TerrainSplineBuilder {
    coordinate: SplineCoordinate,
    points: Vec<TerrainControlPoint>,
    transformer: TransformFn,
    evaluation: SplineEvaluation,
}

impl TerrainSplineBuilder {
    pub fn new(coordinate: SplineCoordinate, transformer: TransformFn) -> Self {
        Self {
            coordinate,
            points: Vec::new(),
            transformer,
            evaluation: SplineEvaluation::Legacy,
        }
    }

    fn new_reference(coordinate: SplineCoordinate, transformer: TransformFn) -> Self {
        Self {
            coordinate,
            points: Vec::new(),
            transformer,
            evaluation: SplineEvaluation::Java,
        }
    }

    fn with_evaluation(
        coordinate: SplineCoordinate,
        transformer: TransformFn,
        evaluation: SplineEvaluation,
    ) -> Self {
        match evaluation {
            SplineEvaluation::Legacy => Self::new(coordinate, transformer),
            SplineEvaluation::Java => Self::new_reference(coordinate, transformer),
        }
    }

    pub fn point(mut self, location: f32, value: impl Into<SplineNode>) -> Self {
        assert!(
            self.points
                .last()
                .map_or(true, |previous| location > previous.location),
            "spline control points must be in ascending order"
        );
        let value = transform_node(value.into(), self.transformer);
        self.points.push(TerrainControlPoint {
            location,
            value,
            derivative: 0.0,
        });
        self
    }

    pub fn point_with_derivative(
        mut self,
        location: f32,
        value: impl Into<SplineNode>,
        derivative: f32,
    ) -> Self {
        assert!(
            self.points
                .last()
                .map_or(true, |previous| location > previous.location),
            "spline control points must be in ascending order"
        );
        let value = transform_node(value.into(), self.transformer);
        self.points.push(TerrainControlPoint {
            location,
            value,
            derivative,
        });
        self
    }

    pub fn build(self) -> TerrainSpline {
        assert!(!self.points.is_empty(), "spline must contain control points");
        TerrainSpline::new(self.coordinate, self.points, self.evaluation)
    }
}

fn transform_node(node: SplineNode, transformer: TransformFn) -> SplineNode {
    match node {
        SplineNode::Constant(value) => SplineNode::Constant(transformer(value)),
        SplineNode::Spline(spline) => SplineNode::Spline(spline),
    }
}

// ---------------------------------------------------------------------------
// Helper functions — faithful port of TerrainProvider.java
// ---------------------------------------------------------------------------

fn lerp(delta: f32, start: f32, end: f32) -> f32 {
    start + delta * (end - start)
}

fn calculate_slope(y1: f32, y2: f32, x1: f32, x2: f32) -> f32 {
    (y2 - y1) / (x2 - x1)
}

fn mountain_continentalness(ridge: f32, modulation: f32, allow_rivers_below: f32) -> f32 {
    let ridge_offset = 1.17;
    let ridge_amplitude = 0.46082947;
    let ridge_slope = 1.0 - (1.0 - modulation) * 0.5;
    let ridge_intersect = 0.5 * (1.0 - modulation);

    let adjusted_ridge_height = (ridge + ridge_offset) * ridge_amplitude;
    let continentalness = adjusted_ridge_height * ridge_slope - ridge_intersect;

    if ridge < allow_rivers_below {
        continentalness.max(-0.2222)
    } else {
        continentalness.max(0.0)
    }
}

fn calculate_mountain_ridge_zero_continentalness_point(modulation: f32) -> f32 {
    let ridge_offset = 1.17;
    let ridge_amplitude = 0.46082947;
    let ridge_slope = 1.0 - (1.0 - modulation) * 0.5;
    let ridge_intersect = 0.5 * (1.0 - modulation);

    ridge_intersect / ridge_amplitude * ridge_slope - ridge_offset
}

// ---------------------------------------------------------------------------
// Ridge spline
// ---------------------------------------------------------------------------

fn ridge_spline(
    ridges: SplineCoordinate,
    valley: f32,
    low: f32,
    mid: f32,
    high: f32,
    peaks: f32,
    min_valley_steepness: f32,
    transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let d1 = (0.5 * (low - valley)).max(min_valley_steepness);
    let d2 = 5.0 * (mid - low);
    TerrainSplineBuilder::with_evaluation(ridges, transformer, evaluation)
        .point_with_derivative(-1.0, valley, d1)
        .point_with_derivative(-0.4, low, d1.min(d2))
        .point_with_derivative(0.0, mid, d2)
        .point_with_derivative(0.4, high, 2.0 * (high - mid))
        .point_with_derivative(1.0, peaks, 0.7 * (peaks - high))
        .build()
}

// ---------------------------------------------------------------------------
// Mountain ridge spline (with optional river terrace)
// ---------------------------------------------------------------------------

fn build_mountain_ridge_spline_with_points(
    ridges: SplineCoordinate,
    modulation: f32,
    saddle: bool,
    transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let allow_rivers_below = -0.7;
    let min_point_continentalness = mountain_continentalness(-1.0, modulation, allow_rivers_below);
    let max_point_continentalness = mountain_continentalness(1.0, modulation, allow_rivers_below);

    let ridge_zero_point = calculate_mountain_ridge_zero_continentalness_point(modulation);

    let after_river_point = -0.65;

    let mut builder = TerrainSplineBuilder::with_evaluation(ridges, transformer, evaluation);

    if after_river_point < ridge_zero_point && ridge_zero_point < 1.0
    {
        let after_river_threshold_continentalness =
            mountain_continentalness(-0.65, modulation, allow_rivers_below);
        let before_river_point = -0.75;
        let before_river_threshold_continentalness =
            mountain_continentalness(-0.75, modulation, allow_rivers_below);

        let min_point_derivative = calculate_slope(
            min_point_continentalness,
            before_river_threshold_continentalness,
            -1.0,
            before_river_point,
        );
        builder =
            builder.point_with_derivative(-1.0, min_point_continentalness, min_point_derivative);

        builder = builder.point(before_river_point, before_river_threshold_continentalness);
        builder = builder.point(after_river_point, after_river_threshold_continentalness);

        let ridge_zero_point_continentalness =
            mountain_continentalness(ridge_zero_point, modulation, allow_rivers_below);
        let max_point_derivative = calculate_slope(
            ridge_zero_point_continentalness,
            max_point_continentalness,
            ridge_zero_point,
            1.0,
        );
        let small_offset = 0.01;
        builder =
            builder.point(ridge_zero_point - small_offset, ridge_zero_point_continentalness);
        builder = builder.point_with_derivative(
            ridge_zero_point,
            ridge_zero_point_continentalness,
            max_point_derivative,
        );
        builder =
            builder.point_with_derivative(1.0, max_point_continentalness, max_point_derivative);
    } else {
        let simple_derivative =
            calculate_slope(min_point_continentalness, max_point_continentalness, -1.0, 1.0);

        if saddle {
            builder = builder.point(-1.0, min_point_continentalness.max(0.2));
            builder = builder.point_with_derivative(
                0.0,
                lerp(0.5, min_point_continentalness, max_point_continentalness),
                simple_derivative,
            );
        } else {
            builder = builder.point_with_derivative(-1.0, min_point_continentalness, simple_derivative);
        }
        builder =
            builder.point_with_derivative(1.0, max_point_continentalness, simple_derivative);
    }

    builder.build()
}

// ---------------------------------------------------------------------------
// Erosion offset spline
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_erosion_offset_spline(
    erosion: SplineCoordinate,
    ridges: SplineCoordinate,
    low_valley: f32,
    hill: f32,
    tall_hill: f32,
    mountain_factor: f32,
    plain: f32,
    swamp: f32,
    include_extreme_hills: bool,
    saddle: bool,
    offset_transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let very_low_erosion_mountains = build_mountain_ridge_spline_with_points(
        ridges.clone(),
        lerp(mountain_factor, 0.6, 1.5),
        saddle,
        offset_transformer,
        evaluation,
    );
    let low_erosion_mountains = build_mountain_ridge_spline_with_points(
        ridges.clone(),
        lerp(mountain_factor, 0.6, 1.0),
        saddle,
        offset_transformer,
        evaluation,
    );
    let mountains = build_mountain_ridge_spline_with_points(
        ridges.clone(),
        mountain_factor,
        saddle,
        offset_transformer,
        evaluation,
    );

    let mid_wide = lerp(0.5, 0.5, 0.5) * mountain_factor;

    let wide_plateau = ridge_spline(
        ridges.clone(),
        low_valley - 0.15,
        0.5 * mountain_factor,
        mid_wide,
        0.5 * mountain_factor,
        0.6 * mountain_factor,
        0.5,
        offset_transformer,
        evaluation,
    );

    let narrow_plateau = ridge_spline(
        ridges.clone(),
        low_valley,
        plain * mountain_factor,
        hill * mountain_factor,
        0.5 * mountain_factor,
        0.6 * mountain_factor,
        0.5,
        offset_transformer,
        evaluation,
    );

    let plains = ridge_spline(
        ridges.clone(),
        low_valley,
        plain,
        plain,
        hill,
        tall_hill,
        0.5,
        offset_transformer,
        evaluation,
    );

    let plains_far_inland = ridge_spline(
        ridges.clone(),
        low_valley,
        plain,
        plain,
        hill,
        tall_hill,
        0.5,
        offset_transformer,
        evaluation,
    );

    let extreme_hills = TerrainSplineBuilder::with_evaluation(
        ridges.clone(),
        offset_transformer,
        evaluation,
    )
        .point(-1.0, low_valley)
        .point(-0.4, plains.clone())
        .point(0.0, tall_hill + 0.07)
        .build();

    let swamps = ridge_spline(
        ridges,
        -0.02,
        swamp,
        swamp,
        hill,
        tall_hill,
        0.0,
        offset_transformer,
        evaluation,
    );

    let mut builder = TerrainSplineBuilder::with_evaluation(
        erosion,
        offset_transformer,
        evaluation,
    )
        .point(-0.85, very_low_erosion_mountains)
        .point(-0.7, low_erosion_mountains)
        .point(-0.4, mountains)
        .point(-0.35, wide_plateau)
        .point(-0.1, narrow_plateau)
        .point(0.2, plains);

    if include_extreme_hills {
        builder = builder
            .point(0.4, plains_far_inland.clone())
            .point(0.45, extreme_hills.clone())
            .point(0.55, extreme_hills)
            .point(0.58, plains_far_inland);
    }
    builder = builder.point(0.7, swamps);

    builder.build()
}

// ---------------------------------------------------------------------------
// Erosion factor spline (used in overworldFactor)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn get_erosion_factor(
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    base_value: f32,
    shattered_terrain: bool,
    factor_transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let base_spline = TerrainSplineBuilder::with_evaluation(
        weirdness.clone(),
        factor_transformer,
        evaluation,
    )
        .point(-0.2, 6.3)
        .point(0.2, base_value)
        .build();

    let weirdness_spline_neg =
        TerrainSplineBuilder::with_evaluation(weirdness.clone(), factor_transformer, evaluation)
            .point(-0.05, 6.3)
            .point(0.05, 2.67)
            .build();

    let weirdness_spline_pos =
        TerrainSplineBuilder::with_evaluation(weirdness.clone(), factor_transformer, evaluation)
            .point(-0.05, 2.67)
            .point(0.05, 6.3)
            .build();

    let mut erosion_points = TerrainSplineBuilder::with_evaluation(
        erosion,
        factor_transformer,
        evaluation,
    )
        .point(-0.6, base_spline.clone())
        .point(-0.5, weirdness_spline_neg)
        .point(-0.35, base_spline.clone())
        .point(-0.25, base_spline.clone())
        .point(-0.1, weirdness_spline_pos)
        .point(0.03, base_spline.clone());

    if shattered_terrain {
        let weirdness_shattered =
            TerrainSplineBuilder::with_evaluation(weirdness, factor_transformer, evaluation)
                .point(0.0, base_value)
                .point(0.1, 0.625)
                .build();

        let ridges_shattered =
            TerrainSplineBuilder::with_evaluation(ridges, factor_transformer, evaluation)
            .point(-0.9, base_value)
            .point(-0.69, weirdness_shattered)
            .build();

        erosion_points = erosion_points
            .point(0.35, base_value)
            .point(0.45, ridges_shattered.clone())
            .point(0.55, ridges_shattered)
            .point(0.62, base_value);
    } else {
        let extreme_hills_terrain_from_mid_slice_and_up =
            TerrainSplineBuilder::with_evaluation(ridges.clone(), factor_transformer, evaluation)
                .point(-0.7, base_spline.clone())
                .point(-0.15, 1.37)
                .build();

        let extra_3d_noise_on_peaks_only =
            TerrainSplineBuilder::with_evaluation(ridges, factor_transformer, evaluation)
                .point(0.45, base_spline.clone())
                .point(0.7, 1.56)
                .build();

        erosion_points = erosion_points
            .point(0.05, extra_3d_noise_on_peaks_only.clone())
            .point(0.4, extra_3d_noise_on_peaks_only)
            .point(0.45, extreme_hills_terrain_from_mid_slice_and_up.clone())
            .point(0.55, extreme_hills_terrain_from_mid_slice_and_up)
            .point(0.58, base_value);
    }

    erosion_points.build()
}

// ---------------------------------------------------------------------------
// Jaggedness spline helper functions
// ---------------------------------------------------------------------------

fn build_weirdness_jaggedness_spline(
    weirdness: SplineCoordinate,
    jaggedness_factor: f32,
    jaggedness_transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let max_jaggedness_at_negative_weirdness = 0.63 * jaggedness_factor;
    let max_jaggedness_at_positive_weirdness = 0.3 * jaggedness_factor;

    TerrainSplineBuilder::with_evaluation(weirdness, jaggedness_transformer, evaluation)
        .point(-0.01, max_jaggedness_at_negative_weirdness)
        .point(0.01, max_jaggedness_at_positive_weirdness)
        .build()
}

fn build_ridge_jaggedness_spline(
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    jaggedness_factor_at_peak_ridge: f32,
    jaggedness_factor_at_high_ridge: f32,
    jaggedness_transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let high_slice_start = peaks_and_valleys(0.4);
    let high_slice_end = peaks_and_valleys(0.56666666);
    let high_slice_middle = (high_slice_start + high_slice_end) / 2.0;

    let mut ridge_spline_builder =
        TerrainSplineBuilder::with_evaluation(ridges, jaggedness_transformer, evaluation);
    ridge_spline_builder = ridge_spline_builder.point(high_slice_start, 0.0);

    if jaggedness_factor_at_high_ridge > 0.0 {
        let weirdness_jaggedness_spline = build_weirdness_jaggedness_spline(
            weirdness.clone(),
            jaggedness_factor_at_high_ridge,
            jaggedness_transformer,
            evaluation,
        );
        ridge_spline_builder =
            ridge_spline_builder.point(high_slice_middle, weirdness_jaggedness_spline);
    } else {
        ridge_spline_builder = ridge_spline_builder.point(high_slice_middle, 0.0);
    }

    if jaggedness_factor_at_peak_ridge > 0.0 {
        let weirdness_jaggedness_spline = build_weirdness_jaggedness_spline(
            weirdness,
            jaggedness_factor_at_peak_ridge,
            jaggedness_transformer,
            evaluation,
        );
        ridge_spline_builder =
            ridge_spline_builder.point(1.0, weirdness_jaggedness_spline);
    } else {
        ridge_spline_builder = ridge_spline_builder.point(1.0, 0.0);
    }

    ridge_spline_builder.build()
}

fn build_erosion_jaggedness_spline(
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    jaggedness_factor_at_peak_ridge_and_erosion_index_0: f32,
    jaggedness_factor_at_peak_ridge_and_erosion_index_1: f32,
    jaggedness_factor_at_high_ridge_and_erosion_index_0: f32,
    jaggedness_factor_at_high_ridge_and_erosion_index_1: f32,
    jaggedness_transformer: TransformFn,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let ridge_jaggedness_spline_at_erosion_0 = build_ridge_jaggedness_spline(
        weirdness.clone(),
        ridges.clone(),
        jaggedness_factor_at_peak_ridge_and_erosion_index_0,
        jaggedness_factor_at_high_ridge_and_erosion_index_0,
        jaggedness_transformer,
        evaluation,
    );
    let ridge_jaggedness_spline_at_erosion_1 = build_ridge_jaggedness_spline(
        weirdness,
        ridges,
        jaggedness_factor_at_peak_ridge_and_erosion_index_1,
        jaggedness_factor_at_high_ridge_and_erosion_index_1,
        jaggedness_transformer,
        evaluation,
    );

    TerrainSplineBuilder::with_evaluation(erosion, jaggedness_transformer, evaluation)
        .point(-1.0, ridge_jaggedness_spline_at_erosion_0)
        .point(-0.78, ridge_jaggedness_spline_at_erosion_1.clone())
        .point(-0.5775, ridge_jaggedness_spline_at_erosion_1)
        .point(-0.375, 0.0)
        .build()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute the peaks-and-valleys folded noise from raw weirdness.
///
/// Maps the weirdness value into a [-1, 1] range where positive values
/// correspond to ridge peaks and negative values to valleys.
pub fn peaks_and_valleys(weirdness: f32) -> f32 {
    -((weirdness.abs() - 0.6666667).abs() - 0.33333334) * 3.0
}

/// Build the overworld terrain offset spline.
///
/// The offset controls the base height of the terrain, driven by
/// continents (land/ocean), erosion (ruggedness), and ridges (mountain spines).
pub fn overworld_offset(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
) -> TerrainSpline {
    overworld_offset_with_evaluation(
        continents,
        erosion,
        ridges,
        amplified,
        SplineEvaluation::Legacy,
    )
}

pub fn overworld_offset_reference(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
) -> TerrainSpline {
    overworld_offset_with_evaluation(
        continents,
        erosion,
        ridges,
        amplified,
        SplineEvaluation::Java,
    )
}

fn overworld_offset_with_evaluation(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let offset_transformer: TransformFn = if amplified {
        amplified_offset
    } else {
        no_transform
    };

    let beach_spline = build_erosion_offset_spline(
        erosion.clone(),
        ridges.clone(),
        -0.15, 0.0, 0.0, 0.1, 0.0, -0.03,
        false, false,
        offset_transformer,
        evaluation,
    );
    let low_spline = build_erosion_offset_spline(
        erosion.clone(),
        ridges.clone(),
        -0.1, 0.03, 0.1, 0.1, 0.01, -0.03,
        false, false,
        offset_transformer,
        evaluation,
    );
    let mid_spline = build_erosion_offset_spline(
        erosion.clone(),
        ridges.clone(),
        -0.1, 0.03, 0.1, 0.7, 0.01, -0.03,
        true, true,
        offset_transformer,
        evaluation,
    );
    let high_spline = build_erosion_offset_spline(
        erosion,
        ridges,
        -0.05, 0.03, 0.1, 1.0, 0.01, 0.01,
        true, true,
        offset_transformer,
        evaluation,
    );

    TerrainSplineBuilder::with_evaluation(continents, offset_transformer, evaluation)
        .point(-1.1, 0.044)
        .point(-1.02, -0.2222)
        .point(-0.51, -0.2222)
        .point(-0.44, -0.12)
        .point(-0.18, -0.12)
        .point(-0.16, beach_spline.clone())
        .point(-0.15, beach_spline)
        .point(-0.1, low_spline)
        .point(0.25, mid_spline)
        .point(1.0, high_spline)
        .build()
}

/// Build the overworld terrain factor spline.
///
/// The factor controls the steepness of the density gradient, driven by
/// continents, erosion, weirdness, and ridges.
pub fn overworld_factor(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
) -> TerrainSpline {
    overworld_factor_with_evaluation(
        continents,
        erosion,
        weirdness,
        ridges,
        amplified,
        SplineEvaluation::Legacy,
    )
}

pub fn overworld_factor_reference(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
) -> TerrainSpline {
    overworld_factor_with_evaluation(
        continents,
        erosion,
        weirdness,
        ridges,
        amplified,
        SplineEvaluation::Java,
    )
}

fn overworld_factor_with_evaluation(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let factor_transformer: TransformFn = if amplified {
        amplified_factor
    } else {
        no_transform
    };

    TerrainSplineBuilder::with_evaluation(continents, no_transform, evaluation)
        .point(-0.19, 3.95)
        .point(
            -0.15,
            get_erosion_factor(
                erosion.clone(),
                weirdness.clone(),
                ridges.clone(),
                6.25,
                true,
                no_transform,
                evaluation,
            ),
        )
        .point(
            -0.1,
            get_erosion_factor(
                erosion.clone(),
                weirdness.clone(),
                ridges.clone(),
                5.47,
                true,
                factor_transformer,
                evaluation,
            ),
        )
        .point(
            0.03,
            get_erosion_factor(
                erosion.clone(),
                weirdness.clone(),
                ridges.clone(),
                5.08,
                true,
                factor_transformer,
                evaluation,
            ),
        )
        .point(
            0.06,
            get_erosion_factor(
                erosion,
                weirdness,
                ridges,
                4.69,
                false,
                factor_transformer,
                evaluation,
            ),
        )
        .build()
}

/// Build the overworld jaggedness spline.
///
/// The jaggedness controls the sharpness of mountain peaks, driven by
/// continents, erosion, weirdness, and ridges.
pub fn overworld_jaggedness(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
) -> TerrainSpline {
    overworld_jaggedness_with_evaluation(
        continents,
        erosion,
        weirdness,
        ridges,
        amplified,
        SplineEvaluation::Legacy,
    )
}

pub fn overworld_jaggedness_reference(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
) -> TerrainSpline {
    overworld_jaggedness_with_evaluation(
        continents,
        erosion,
        weirdness,
        ridges,
        amplified,
        SplineEvaluation::Java,
    )
}

fn overworld_jaggedness_with_evaluation(
    continents: SplineCoordinate,
    erosion: SplineCoordinate,
    weirdness: SplineCoordinate,
    ridges: SplineCoordinate,
    amplified: bool,
    evaluation: SplineEvaluation,
) -> TerrainSpline {
    let jaggedness_transformer: TransformFn = if amplified {
        amplified_jaggedness
    } else {
        no_transform
    };

    TerrainSplineBuilder::with_evaluation(continents, jaggedness_transformer, evaluation)
        .point(-0.11, 0.0)
        .point(
            0.03,
            build_erosion_jaggedness_spline(
                erosion.clone(),
                weirdness.clone(),
                ridges.clone(),
                1.0, 0.5, 0.0, 0.0,
                jaggedness_transformer,
                evaluation,
            ),
        )
        .point(
            0.65,
            build_erosion_jaggedness_spline(
                erosion, weirdness, ridges,
                1.0, 1.0, 1.0, 0.0,
                jaggedness_transformer,
                evaluation,
            ),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::world_gen::density_fn::{constant, SinglePointContext};

    fn context() -> SinglePointContext {
        SinglePointContext {
            block_x: 0,
            block_y: 0,
            block_z: 0,
        }
    }

    #[test]
    fn legacy_cubic_spline_uses_zero_default_derivatives() {
        let spline = TerrainSplineBuilder::new(
            SplineCoordinate(constant(0.25)),
            no_transform,
        )
        .point(0.0, 0.0)
        .point(1.0, 10.0)
        .build();

        // CubicSpline.Multipoint.sample at t=.25 with d1=d2=0.
        let expected = 1.5625;
        assert!((spline.compute(&context()) - expected).abs() < 1e-6);
    }

    #[test]
    fn java_cubic_spline_uses_delta_for_first_hermite_term() {
        let spline = TerrainSplineBuilder::new_reference(
            SplineCoordinate(constant(0.5)),
            no_transform,
        )
        .point_with_derivative(0.0, 2.0, 1.0)
        .point_with_derivative(2.0, 5.0, -2.0)
        .build();

        // Java float arithmetic: t=.25, a=1*2-(5-2)=-1, b=-(-2)*2+(5-2)=7.
        assert_eq!(spline.compute(&context()) as f32, 2.9375);
    }

    #[test]
    fn java_cubic_spline_linearly_extrapolates_with_endpoint_derivatives() {
        let below = TerrainSplineBuilder::new(
            SplineCoordinate(constant(-1.0)),
            no_transform,
        )
        .point_with_derivative(0.0, 2.0, 3.0)
        .point_with_derivative(1.0, 4.0, 5.0)
        .build();
        let above = TerrainSplineBuilder::new(
            SplineCoordinate(constant(2.0)),
            no_transform,
        )
        .point_with_derivative(0.0, 2.0, 3.0)
        .point_with_derivative(1.0, 4.0, 5.0)
        .build();

        assert!((below.compute(&context()) + 1.0).abs() < 1e-6);
        assert!((above.compute(&context()) - 9.0).abs() < 1e-6);
    }

    #[test]
    fn amplified_transform_applies_to_control_values_not_coordinate() {
        let spline = TerrainSplineBuilder::new(
            SplineCoordinate(constant(0.25)),
            amplified_offset,
        )
        .point(0.0, 1.0)
        .point(1.0, 3.0)
        .build();

        // The Java builder stores values 2 and 6, while the coordinate stays .25.
        assert!((spline.compute(&context()) - 2.0625).abs() < 1e-6);
    }

    #[test]
    fn ridge_spline_control_points_match_terrain_provider() {
        let spline = ridge_spline(
            SplineCoordinate(constant(0.0)),
            -0.15,
            0.05,
            0.1,
            0.2,
            0.3,
            0.5,
            no_transform,
            SplineEvaluation::Legacy,
        );

        let locations: Vec<f32> = spline.points.iter().map(|point| point.location).collect();
        let derivatives: Vec<f32> = spline.points.iter().map(|point| point.derivative).collect();
        assert_eq!(locations, vec![-1.0, -0.4, 0.0, 0.4, 1.0]);
        let expected_derivatives = [0.5, 0.25, 0.25, 0.2, 0.07];
        for (actual, expected) in derivatives.iter().zip(expected_derivatives) {
            assert!((actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn overworld_control_locations_match_terrain_provider() {
        let coordinate = SplineCoordinate(constant(0.0));
        let spline = overworld_offset(coordinate.clone(), coordinate.clone(), coordinate, false);
        let locations: Vec<f32> = spline.points.iter().map(|point| point.location).collect();

        assert_eq!(
            locations,
            vec![-1.1, -1.02, -0.51, -0.44, -0.18, -0.16, -0.15, -0.1, 0.25, 1.0]
        );
    }

    #[test]
    fn factor_and_jaggedness_control_locations_match_terrain_provider() {
        let coordinate = SplineCoordinate(constant(0.0));
        let factor = overworld_factor(
            coordinate.clone(),
            coordinate.clone(),
            coordinate.clone(),
            coordinate.clone(),
            false,
        );
        let jaggedness = overworld_jaggedness(
            coordinate.clone(),
            coordinate.clone(),
            coordinate.clone(),
            coordinate,
            false,
        );

        let factor_locations: Vec<f32> =
            factor.points.iter().map(|point| point.location).collect();
        let jaggedness_locations: Vec<f32> = jaggedness
            .points
            .iter()
            .map(|point| point.location)
            .collect();
        assert_eq!(factor_locations, vec![-0.19, -0.15, -0.1, 0.03, 0.06]);
        assert_eq!(jaggedness_locations, vec![-0.11, 0.03, 0.65]);
    }
}
