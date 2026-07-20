//! Minecraft 26.2's Overworld multi-noise biome source.
//!
//! The `overworld.json` input is a preset reference, not a materialized list.
//! This module therefore ports the ordered calls in `OverworldBiomeBuilder`.
//! Climate values use the six `NoiseRouter` channels in the same order as
//! `Climate.Sampler`; the seventh parameter is the parameter-point offset.

use super::density_fn::{DensityFunction, SinglePointContext};
use super::noise_router::NoiseRouter;
use super::Biome;

const QUANTIZATION_FACTOR: f32 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClimateParameter {
    min: i64,
    max: i64,
}

impl ClimateParameter {
    const fn new(min: i64, max: i64) -> Self {
        Self { min, max }
    }

    fn point(value: f64) -> Self {
        let value = quantize(value);
        Self::new(value, value)
    }

    fn span(min: f64, max: f64) -> Self {
        Self::new(quantize(min), quantize(max))
    }

    fn join(first: Self, second: Self) -> Self {
        Self::new(first.min.min(second.min), first.max.max(second.max))
    }

    /// Port of `Climate.Parameter.distance(long)`.
    fn distance(self, target: i64) -> i64 {
        let above = target - self.max;
        let below = self.min - target;
        if above > 0 { above } else { below.max(0) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParameterPoint {
    parameters: [ClimateParameter; 7],
    biome: BiomeId,
}

impl ParameterPoint {
    fn fitness(self, target: [i64; 7]) -> i64 {
        self.parameters
            .iter()
            .zip(target)
            .map(|(parameter, value)| parameter.distance(value).pow(2))
            .sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BiomeId {
    SnowyPlains,
    SnowyTaiga,
    IceSpikes,
    Plains,
    Forest,
    BirchForest,
    OldGrowthBirchForest,
    DarkForest,
    OldGrowthSpruceTaiga,
    OldGrowthPineTaiga,
    FlowerForest,
    SunflowerPlains,
    Savanna,
    SavannaPlateau,
    Jungle,
    SparseJungle,
    BambooJungle,
    Desert,
    Meadow,
    PaleGarden,
    CherryGrove,
    Badlands,
    WoodedBadlands,
    ErodedBadlands,
    WindsweptGravellyHills,
    WindsweptHills,
    WindsweptForest,
    WindsweptSavanna,
    StonyPeaks,
    JaggedPeaks,
    FrozenPeaks,
    SnowySlopes,
    Grove,
    Taiga,
    MushroomFields,
    DeepFrozenOcean,
    DeepColdOcean,
    DeepOcean,
    DeepLukewarmOcean,
    FrozenOcean,
    ColdOcean,
    Ocean,
    LukewarmOcean,
    WarmOcean,
    StonyShore,
    SnowyBeach,
    Beach,
    FrozenRiver,
    River,
    Swamp,
    MangroveSwamp,
    DripstoneCaves,
    LushCaves,
    SulfurCaves,
    DeepDark,
}

impl BiomeId {
    fn supported(self) -> Result<Biome, UnsupportedBiome> {
        match self {
            Self::SnowyPlains => Ok(Biome::SnowyPlains),
            Self::IceSpikes => Ok(Biome::IceSpikes),
            Self::SnowyTaiga => Ok(Biome::SnowyTaiga),
            Self::Plains => Ok(Biome::Plains),
            Self::Forest => Ok(Biome::Forest),
            Self::BirchForest => Ok(Biome::BirchForest),
            Self::OldGrowthBirchForest => Ok(Biome::OldGrowthBirchForest),
            Self::PaleGarden => Ok(Biome::PaleGarden),
            Self::DarkForest => Ok(Biome::DarkForest),
            Self::OldGrowthSpruceTaiga => Ok(Biome::OldGrowthSpruceTaiga),
            Self::OldGrowthPineTaiga => Ok(Biome::OldGrowthPineTaiga),
            Self::FlowerForest => Ok(Biome::FlowerForest),
            Self::SunflowerPlains => Ok(Biome::SunflowerPlains),
            Self::Savanna => Ok(Biome::Savanna),
            Self::SavannaPlateau => Ok(Biome::SavannaPlateau),
            Self::SparseJungle => Ok(Biome::SparseJungle),
            Self::Jungle => Ok(Biome::Jungle),
            Self::BambooJungle => Ok(Biome::BambooJungle),
            Self::Desert => Ok(Biome::Desert),
            Self::Meadow => Ok(Biome::Meadow),
            Self::CherryGrove => Ok(Biome::CherryGrove),
            Self::Badlands => Ok(Biome::Badlands),
            Self::WoodedBadlands => Ok(Biome::WoodedBadlands),
            Self::ErodedBadlands => Ok(Biome::ErodedBadlands),
            Self::WindsweptGravellyHills => Ok(Biome::WindsweptGravellyHills),
            Self::WindsweptHills => Ok(Biome::WindsweptHills),
            Self::WindsweptForest => Ok(Biome::WindsweptForest),
            Self::WindsweptSavanna => Ok(Biome::WindsweptSavanna),
            Self::StonyPeaks => Ok(Biome::StonyPeaks),
            Self::JaggedPeaks => Ok(Biome::JaggedPeaks),
            Self::FrozenPeaks => Ok(Biome::FrozenPeaks),
            Self::SnowySlopes => Ok(Biome::SnowySlopes),
            Self::Grove => Ok(Biome::Grove),
            Self::Taiga => Ok(Biome::Taiga),
            Self::MushroomFields => Ok(Biome::MushroomFields),
            Self::DeepFrozenOcean => Ok(Biome::DeepFrozenOcean),
            Self::DeepColdOcean => Ok(Biome::DeepColdOcean),
            Self::DeepOcean => Ok(Biome::DeepOcean),
            Self::DeepLukewarmOcean => Ok(Biome::DeepLukewarmOcean),
            Self::FrozenOcean => Ok(Biome::FrozenOcean),
            Self::ColdOcean => Ok(Biome::ColdOcean),
            Self::Ocean => Ok(Biome::Ocean),
            Self::LukewarmOcean => Ok(Biome::LukewarmOcean),
            Self::WarmOcean => Ok(Biome::WarmOcean),
            Self::Beach => Ok(Biome::Beach),
            Self::River => Ok(Biome::River),
            Self::Swamp => Ok(Biome::Swamp),
            Self::StonyShore => Ok(Biome::StonyShore),
            Self::SnowyBeach => Ok(Biome::SnowyBeach),
            Self::FrozenRiver => Ok(Biome::FrozenRiver),
            Self::MangroveSwamp => Ok(Biome::MangroveSwamp),
            Self::DripstoneCaves => Ok(Biome::DripstoneCaves),
            Self::LushCaves => Ok(Biome::LushCaves),
            Self::SulfurCaves => Ok(Biome::SulfurCaves),
            Self::DeepDark => Ok(Biome::DeepDark),
        }
    }
}

/// A 26.2 biome selected by the parameter list but not representable by the
/// repository's current `Biome` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedBiome {
    name: &'static str,
}

impl UnsupportedBiome {
    const fn new(name: &'static str) -> Self {
        Self { name }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }
}

impl std::fmt::Display for UnsupportedBiome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Overworld biome is not supported: {}", self.name)
    }
}

impl std::error::Error for UnsupportedBiome {}

/// Port of Minecraft 26.2 `Climate.RTree`. The Overworld contains thousands
/// of overlapping parameter boxes; a linear nearest-neighbor scan at every
/// quart sample dominates chunk generation at normal render distances.
#[derive(Clone)]
struct ClimateRTree {
    root: ClimateNode,
}

#[derive(Clone)]
struct ClimateNode {
    bounds: [ClimateParameter; 7],
    min_order: usize,
    kind: ClimateNodeKind,
}

#[derive(Clone)]
enum ClimateNodeKind {
    Leaf(ClimateLeaf),
    Branch(Vec<ClimateNode>),
}

#[derive(Clone, Copy)]
struct ClimateLeaf {
    point: ParameterPoint,
    order: usize,
}

impl ClimateNode {
    fn leaf(point: ParameterPoint, order: usize) -> Self {
        Self {
            bounds: point.parameters,
            min_order: order,
            kind: ClimateNodeKind::Leaf(ClimateLeaf { point, order }),
        }
    }

    fn branch(children: Vec<Self>) -> Self {
        let mut bounds = children[0].bounds;
        for child in children.iter().skip(1) {
            for (bound, child_bound) in bounds.iter_mut().zip(child.bounds) {
                *bound = ClimateParameter::join(*bound, child_bound);
            }
        }
        Self {
            bounds,
            min_order: children.iter().map(|child| child.min_order).min().unwrap(),
            kind: ClimateNodeKind::Branch(children),
        }
    }

    fn distance(&self, target: [i64; 7]) -> i64 {
        self.bounds
            .iter()
            .zip(target)
            .map(|(parameter, value)| parameter.distance(value).pow(2))
            .sum()
    }

    fn search(&self, target: [i64; 7], candidate: Option<ClimateLeaf>) -> ClimateLeaf {
        match &self.kind {
            ClimateNodeKind::Leaf(leaf) => *leaf,
            ClimateNodeKind::Branch(children) => {
                let mut closest = candidate;
                let mut min_distance = closest.map_or(i64::MAX, |leaf| leaf.point.fitness(target));
                for child in children {
                    let child_distance = child.distance(target);
                    let can_improve_order = closest
                        .is_some_and(|leaf| child_distance == min_distance && child.min_order < leaf.order);
                    if min_distance > child_distance || can_improve_order {
                        let leaf = child.search(target, closest);
                        let leaf_distance = if matches!(child.kind, ClimateNodeKind::Leaf(_)) {
                            child_distance
                        } else {
                            leaf.point.fitness(target)
                        };
                        if min_distance > leaf_distance
                            || (min_distance == leaf_distance
                                && closest.is_none_or(|closest| leaf.order < closest.order))
                        {
                            min_distance = leaf_distance;
                            closest = Some(leaf);
                        }
                    }
                }
                closest.expect("climate branch must contain a leaf")
            }
        }
    }
}

impl ClimateRTree {
    fn new(points: &[ParameterPoint]) -> Self {
        assert!(!points.is_empty(), "climate index requires at least one point");
        let leaves = points
            .iter()
            .copied()
            .enumerate()
            .map(|(order, point)| ClimateNode::leaf(point, order))
            .collect();
        Self { root: Self::build(leaves) }
    }

    fn build(mut nodes: Vec<ClimateNode>) -> ClimateNode {
        if nodes.len() == 1 {
            return nodes.pop().unwrap();
        }
        if nodes.len() <= 6 {
            nodes.sort_by_key(|node| {
                node.bounds
                    .iter()
                    .map(|parameter| ((parameter.min + parameter.max) / 2).abs())
                    .sum::<i64>()
            });
            return ClimateNode::branch(nodes);
        }

        let mut best_cost = i64::MAX;
        let mut best_dimension = 0;
        let mut best_buckets = Vec::new();
        for dimension in 0..7 {
            Self::sort_nodes(&mut nodes, dimension, false);
            let buckets = Self::bucketize(&nodes);
            let cost = buckets.iter().map(Self::node_cost).sum();
            if best_cost > cost {
                best_cost = cost;
                best_dimension = dimension;
                best_buckets = buckets;
            }
        }
        Self::sort_nodes(&mut best_buckets, best_dimension, true);
        ClimateNode::branch(
            best_buckets
                .into_iter()
                .map(|bucket| match bucket.kind {
                    ClimateNodeKind::Branch(children) => Self::build(children),
                    ClimateNodeKind::Leaf(_) => unreachable!("bucket is always a branch"),
                })
                .collect(),
        )
    }

    fn sort_nodes(nodes: &mut [ClimateNode], dimension: usize, absolute: bool) {
        nodes.sort_by(|a, b| {
            for offset in 0..7 {
                let d = (dimension + offset) % 7;
                let center = |node: &ClimateNode| {
                    let value = (node.bounds[d].min + node.bounds[d].max) / 2;
                    if absolute { value.abs() } else { value }
                };
                let ordering = center(a).cmp(&center(b));
                if !ordering.is_eq() {
                    return ordering;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    fn bucketize(nodes: &[ClimateNode]) -> Vec<ClimateNode> {
        let mut children_per_bucket = 1usize;
        while children_per_bucket.saturating_mul(6) < nodes.len() {
            children_per_bucket *= 6;
        }
        nodes
            .chunks(children_per_bucket)
            .map(|children| ClimateNode::branch(children.to_vec()))
            .collect()
    }

    fn node_cost(node: &ClimateNode) -> i64 {
        node.bounds
            .iter()
            .map(|parameter| (parameter.max - parameter.min).abs())
            .sum()
    }

    fn search(&self, target: [i64; 7]) -> BiomeId {
        self.root.search(target, None).point.biome
    }
}

/// Deterministic implementation of `MultiNoiseBiomeSource` using the
/// 26.2 Overworld parameter list.
#[derive(Clone)]
pub struct OverworldBiomeSource {
    router: NoiseRouter,
    index: ClimateRTree,
}

impl OverworldBiomeSource {
    pub fn from_router(router: NoiseRouter) -> Self {
        let parameters = build_parameter_points();
        Self {
            router,
            index: ClimateRTree::new(&parameters),
        }
    }

    pub(crate) fn from_router_compatibility(router: NoiseRouter) -> Self {
        // Persisted pre-Geometry profiles keep their original labels and axis
        // mistake so streaming cannot introduce profile-local biome seams.
        let parameters = build_compatibility_parameter_points();
        Self {
            router,
            index: ClimateRTree::new(&parameters),
        }
    }

    /// Returns the biome at block coordinates. Java's biome-source callers
    /// normally pass quart coordinates; use `get_biome_quart` for that API.
    pub fn get_biome(&self, x: i32, y: i32, z: i32) -> Result<Biome, UnsupportedBiome> {
        self.get_biome_for_target(self.sample_target(x, y, z))
    }

    /// Java-compatible entry point: quart coordinates are converted to block
    /// coordinates before evaluating the six NoiseRouter climate channels.
    pub fn get_biome_quart(
        &self,
        quart_x: i32,
        quart_y: i32,
        quart_z: i32,
    ) -> Result<Biome, UnsupportedBiome> {
        self.get_biome(
            quart_x.wrapping_mul(4),
            quart_y.wrapping_mul(4),
            quart_z.wrapping_mul(4),
        )
    }

    fn sample_target(&self, x: i32, y: i32, z: i32) -> [i64; 7] {
        let context = SinglePointContext {
            block_x: x,
            block_y: y,
            block_z: z,
        };
        [
            quantize(self.router.temperature.compute(&context)),
            quantize(self.router.vegetation.compute(&context)),
            quantize(self.router.continents.compute(&context)),
            quantize(self.router.erosion.compute(&context)),
            quantize(self.router.depth.compute(&context)),
            quantize(self.router.ridges.compute(&context)),
            0,
        ]
    }

    fn get_biome_for_target(&self, target: [i64; 7]) -> Result<Biome, UnsupportedBiome> {
        self.index.search(target).supported()
    }

    /// Port of `Climate.Sampler.findSpawnPosition` for the Overworld's two
    /// `spawn_target` points. This finds a climate-suitable *suggestion*;
    /// callers must still find a collision-free terrain position nearby.
    pub fn find_spawn_position(&self) -> (i32, i32) {
        let targets = spawn_target_points();
        let mut best = self.spawn_candidate_fitness(&targets, 0, 0);
        best = self.refine_spawn_position(&targets, best, 2048.0, 512.0);
        best = self.refine_spawn_position(&targets, best, 512.0, 32.0);
        (best.0, best.1)
    }

    fn refine_spawn_position(
        &self,
        targets: &[ParameterPoint],
        mut best: (i32, i32, i128),
        max_radius: f32,
        radius_increment: f32,
    ) -> (i32, i32, i128) {
        let (origin_x, origin_z) = (best.0, best.1);
        let mut angle = 0.0f32;
        let mut radius = radius_increment;
        while radius <= max_radius {
            let x = origin_x + (angle.sin() * radius) as i32;
            let z = origin_z + (angle.cos() * radius) as i32;
            let candidate = self.spawn_candidate_fitness(targets, x, z);
            if candidate.2 < best.2 {
                best = candidate;
            }
            angle += radius_increment / radius;
            if angle > std::f32::consts::TAU {
                angle = 0.0;
                radius += radius_increment;
            }
        }
        best
    }

    fn spawn_candidate_fitness(
        &self,
        targets: &[ParameterPoint],
        x: i32,
        z: i32,
    ) -> (i32, i32, i128) {
        let mut target = self.sample_target(x, 0, z);
        target[4] = 0; // `Climate.SpawnFinder` ignores sampled depth.
        let climate_fitness = targets
            .iter()
            .map(|point| point.fitness(target))
            .min()
            .unwrap_or(i64::MAX) as i128;
        let distance_bias = i128::from(x).pow(2) + i128::from(z).pow(2);
        (x, z, climate_fitness * i128::from(2048_i32.pow(2)) + distance_bias)
    }
}

fn spawn_target_points() -> [ParameterPoint; 2] {
    let full = ClimateParameter::span(-1.0, 1.0);
    let continentalness = ClimateParameter::span(-0.11, 1.0);
    let depth = ClimateParameter::point(0.0);
    let offset = ClimateParameter::point(0.0);
    [
        ParameterPoint {
            parameters: [
                full,
                full,
                continentalness,
                full,
                depth,
                ClimateParameter::span(-1.0, -0.16),
                offset,
            ],
            biome: BiomeId::Plains,
        },
        ParameterPoint {
            parameters: [
                full,
                full,
                continentalness,
                full,
                depth,
                ClimateParameter::span(0.16, 1.0),
                offset,
            ],
            biome: BiomeId::Plains,
        },
    ]
}

fn quantize(value: f64) -> i64 {
    ((value as f32) * QUANTIZATION_FACTOR) as i64
}

fn point(
    temperature: ClimateParameter,
    humidity: ClimateParameter,
    continentalness: ClimateParameter,
    erosion: ClimateParameter,
    depth: ClimateParameter,
    weirdness: ClimateParameter,
    offset: f64,
    biome: BiomeId,
) -> ParameterPoint {
    ParameterPoint {
        parameters: [
            temperature,
            humidity,
            continentalness,
            erosion,
            depth,
            weirdness,
            ClimateParameter::point(offset),
        ],
        biome,
    }
}

fn add_surface(
    output: &mut Vec<ParameterPoint>,
    temperature: ClimateParameter,
    humidity: ClimateParameter,
    continentalness: ClimateParameter,
    erosion: ClimateParameter,
    weirdness: ClimateParameter,
    offset: f64,
    biome: BiomeId,
) {
    output.push(point(
        temperature,
        humidity,
        continentalness,
        erosion,
        ClimateParameter::point(0.0),
        weirdness,
        offset,
        biome,
    ));
    output.push(point(
        temperature,
        humidity,
        continentalness,
        erosion,
        ClimateParameter::point(1.0),
        weirdness,
        offset,
        biome,
    ));
}

fn add_underground(
    output: &mut Vec<ParameterPoint>,
    temperature: ClimateParameter,
    humidity: ClimateParameter,
    continentalness: ClimateParameter,
    erosion: ClimateParameter,
    weirdness: ClimateParameter,
    offset: f64,
    biome: BiomeId,
) {
    output.push(point(
        temperature,
        humidity,
        continentalness,
        erosion,
        ClimateParameter::span(0.2, 0.9),
        weirdness,
        offset,
        biome,
    ));
}

fn add_bottom(
    output: &mut Vec<ParameterPoint>,
    temperature: ClimateParameter,
    humidity: ClimateParameter,
    continentalness: ClimateParameter,
    erosion: ClimateParameter,
    weirdness: ClimateParameter,
    offset: f64,
    biome: BiomeId,
) {
    output.push(point(
        temperature,
        humidity,
        continentalness,
        erosion,
        ClimateParameter::point(1.1),
        weirdness,
        offset,
        biome,
    ));
}

fn build_parameter_points() -> Vec<ParameterPoint> {
    let full = ClimateParameter::span(-1.0, 1.0);
    let temperatures = [
        ClimateParameter::span(-1.0, -0.45),
        ClimateParameter::span(-0.45, -0.15),
        ClimateParameter::span(-0.15, 0.2),
        ClimateParameter::span(0.2, 0.55),
        ClimateParameter::span(0.55, 1.0),
    ];
    let humidities = [
        ClimateParameter::span(-1.0, -0.35),
        ClimateParameter::span(-0.35, -0.1),
        ClimateParameter::span(-0.1, 0.1),
        ClimateParameter::span(0.1, 0.3),
        ClimateParameter::span(0.3, 1.0),
    ];
    let erosions = [
        ClimateParameter::span(-1.0, -0.78),
        ClimateParameter::span(-0.78, -0.375),
        ClimateParameter::span(-0.375, -0.2225),
        ClimateParameter::span(-0.2225, 0.05),
        ClimateParameter::span(0.05, 0.45),
        ClimateParameter::span(0.45, 0.55),
        ClimateParameter::span(0.55, 1.0),
    ];
    let mushroom = ClimateParameter::span(-1.2, -1.05);
    let deep_ocean = ClimateParameter::span(-1.05, -0.455);
    let ocean = ClimateParameter::span(-0.455, -0.19);
    let mut output = Vec::new();

    add_surface(&mut output, full, full, mushroom, full, full, 0.0, BiomeId::MushroomFields);
    for (index, temperature) in temperatures.iter().copied().enumerate() {
        let deep_biomes = [
            BiomeId::DeepFrozenOcean,
            BiomeId::DeepColdOcean,
            BiomeId::DeepOcean,
            BiomeId::DeepLukewarmOcean,
            BiomeId::WarmOcean,
        ];
        let ocean_biomes = [
            BiomeId::FrozenOcean,
            BiomeId::ColdOcean,
            BiomeId::Ocean,
            BiomeId::LukewarmOcean,
            BiomeId::WarmOcean,
        ];
        add_surface(&mut output, temperature, full, deep_ocean, full, full, 0.0, deep_biomes[index]);
        add_surface(&mut output, temperature, full, ocean, full, full, 0.0, ocean_biomes[index]);
    }

    add_mid_slice(&mut output, &temperatures, &humidities, &erosions, -1.0, -0.93333334);
    add_high_slice(&mut output, &temperatures, &humidities, &erosions, -0.93333334, -0.7666667);
    add_peaks(&mut output, &temperatures, &humidities, &erosions, -0.7666667, -0.56666666);
    add_high_slice(&mut output, &temperatures, &humidities, &erosions, -0.56666666, -0.4);
    add_mid_slice(&mut output, &temperatures, &humidities, &erosions, -0.4, -0.26666668);
    add_low_slice(&mut output, &temperatures, &humidities, &erosions, -0.26666668, -0.05);
    add_valleys(&mut output, &temperatures, &humidities, &erosions, -0.05, 0.05);
    add_low_slice(&mut output, &temperatures, &humidities, &erosions, 0.05, 0.26666668);
    add_mid_slice(&mut output, &temperatures, &humidities, &erosions, 0.26666668, 0.4);
    add_high_slice(&mut output, &temperatures, &humidities, &erosions, 0.4, 0.56666666);
    add_peaks(&mut output, &temperatures, &humidities, &erosions, 0.56666666, 0.7666667);
    add_high_slice(&mut output, &temperatures, &humidities, &erosions, 0.7666667, 0.93333334);
    add_mid_slice(&mut output, &temperatures, &humidities, &erosions, 0.93333334, 1.0);

    add_underground(&mut output, full, full, ClimateParameter::span(0.8, 1.0), full, full, 0.0, BiomeId::DripstoneCaves);
    add_underground(&mut output, full, ClimateParameter::span(0.7, 1.0), full, full, full, 0.0, BiomeId::LushCaves);
    add_underground(&mut output, full, full, ClimateParameter::span(-0.19, 0.55), ClimateParameter::span(0.45, 1.0), ClimateParameter::span(-1.1, -0.85), 0.0, BiomeId::SulfurCaves);
    add_bottom(&mut output, full, full, full, ClimateParameter::span(-1.0, -0.375), full, 0.0, BiomeId::DeepDark);

    output
}

fn build_compatibility_parameter_points() -> Vec<ParameterPoint> {
    let full = ClimateParameter::span(-1.0, 1.0);
    let dripstone_erosion = ClimateParameter::span(0.8, 1.0);
    let mut output = build_parameter_points();
    for point in &mut output {
        if point.biome == BiomeId::OldGrowthBirchForest {
            point.biome = BiomeId::BirchForest;
        } else if point.biome == BiomeId::DripstoneCaves {
            point.parameters[2] = full;
            point.parameters[3] = dripstone_erosion;
        }
    }
    output
}

fn middle_biome(temperature: usize, humidity: usize, positive_weirdness: bool) -> BiomeId {
    const MIDDLE: [[BiomeId; 5]; 5] = [
        [BiomeId::SnowyPlains, BiomeId::SnowyPlains, BiomeId::SnowyPlains, BiomeId::SnowyTaiga, BiomeId::Taiga],
        [BiomeId::Plains, BiomeId::Plains, BiomeId::Forest, BiomeId::Taiga, BiomeId::OldGrowthSpruceTaiga],
        [BiomeId::FlowerForest, BiomeId::Plains, BiomeId::Forest, BiomeId::BirchForest, BiomeId::DarkForest],
        [BiomeId::Savanna, BiomeId::Savanna, BiomeId::Forest, BiomeId::Jungle, BiomeId::Jungle],
        [BiomeId::Desert, BiomeId::Desert, BiomeId::Desert, BiomeId::Desert, BiomeId::Desert],
    ];
    const VARIANT: [[Option<BiomeId>; 5]; 5] = [
        [Some(BiomeId::IceSpikes), None, Some(BiomeId::SnowyTaiga), None, None],
        [None, None, None, None, Some(BiomeId::OldGrowthPineTaiga)],
        [Some(BiomeId::SunflowerPlains), None, None, Some(BiomeId::OldGrowthBirchForest), None],
        [None, None, Some(BiomeId::Plains), Some(BiomeId::SparseJungle), Some(BiomeId::BambooJungle)],
        [None, None, None, None, None],
    ];
    if positive_weirdness { VARIANT[temperature][humidity].unwrap_or(MIDDLE[temperature][humidity]) } else { MIDDLE[temperature][humidity] }
}

fn badlands_biome(humidity: usize, positive_weirdness: bool) -> BiomeId {
    if humidity < 2 { if positive_weirdness { BiomeId::ErodedBadlands } else { BiomeId::Badlands } }
    else if humidity < 3 { BiomeId::Badlands }
    else { BiomeId::WoodedBadlands }
}

fn plateau_biome(temperature: usize, humidity: usize, positive_weirdness: bool) -> BiomeId {
    if positive_weirdness {
        const VARIANT: [[Option<BiomeId>; 5]; 5] = [
            [Some(BiomeId::IceSpikes), None, None, None, None],
            [Some(BiomeId::CherryGrove), None, Some(BiomeId::Meadow), Some(BiomeId::Meadow), Some(BiomeId::OldGrowthPineTaiga)],
            [Some(BiomeId::CherryGrove), Some(BiomeId::CherryGrove), Some(BiomeId::Forest), Some(BiomeId::BirchForest), None],
            [None, None, None, None, None],
            [Some(BiomeId::ErodedBadlands), Some(BiomeId::ErodedBadlands), None, None, None],
        ];
        if let Some(variant) = VARIANT[temperature][humidity] { return variant; }
    }
    const PLATEAU: [[BiomeId; 5]; 5] = [
        [BiomeId::SnowyPlains, BiomeId::SnowyPlains, BiomeId::SnowyPlains, BiomeId::SnowyTaiga, BiomeId::SnowyTaiga],
        [BiomeId::Meadow, BiomeId::Meadow, BiomeId::Forest, BiomeId::Taiga, BiomeId::OldGrowthSpruceTaiga],
        [BiomeId::Meadow, BiomeId::Meadow, BiomeId::Meadow, BiomeId::Meadow, BiomeId::PaleGarden],
        [BiomeId::SavannaPlateau, BiomeId::SavannaPlateau, BiomeId::Forest, BiomeId::Forest, BiomeId::Jungle],
        [BiomeId::Badlands, BiomeId::Badlands, BiomeId::Badlands, BiomeId::WoodedBadlands, BiomeId::WoodedBadlands],
    ];
    PLATEAU[temperature][humidity]
}

fn slope_biome(temperature: usize, humidity: usize, positive_weirdness: bool) -> BiomeId {
    if temperature >= 3 { plateau_biome(temperature, humidity, positive_weirdness) }
    else if humidity <= 1 { BiomeId::SnowySlopes }
    else { BiomeId::Grove }
}

fn peak_biome(temperature: usize, humidity: usize, positive_weirdness: bool) -> BiomeId {
    if temperature <= 2 {
        if positive_weirdness { BiomeId::FrozenPeaks } else { BiomeId::JaggedPeaks }
    } else if temperature == 3 { BiomeId::StonyPeaks }
    else { badlands_biome(humidity, positive_weirdness) }
}

fn shattered_biome(temperature: usize, humidity: usize, positive_weirdness: bool) -> BiomeId {
    const SHATTERED: [[Option<BiomeId>; 5]; 5] = [
        [Some(BiomeId::WindsweptGravellyHills), Some(BiomeId::WindsweptGravellyHills), Some(BiomeId::WindsweptHills), Some(BiomeId::WindsweptForest), Some(BiomeId::WindsweptForest)],
        [Some(BiomeId::WindsweptGravellyHills), Some(BiomeId::WindsweptGravellyHills), Some(BiomeId::WindsweptHills), Some(BiomeId::WindsweptForest), Some(BiomeId::WindsweptForest)],
        [Some(BiomeId::WindsweptHills), Some(BiomeId::WindsweptHills), Some(BiomeId::WindsweptHills), Some(BiomeId::WindsweptForest), Some(BiomeId::WindsweptForest)],
        [None, None, None, None, None],
        [None, None, None, None, None],
    ];
    SHATTERED[temperature][humidity].unwrap_or_else(|| middle_biome(temperature, humidity, positive_weirdness))
}

fn windswept_savanna_if_needed(temperature: usize, humidity: usize, positive_weirdness: bool, underlying: BiomeId) -> BiomeId {
    if temperature > 1 && humidity < 4 && positive_weirdness { BiomeId::WindsweptSavanna } else { underlying }
}

fn beach_biome(temperature: usize) -> BiomeId {
    if temperature == 0 { BiomeId::SnowyBeach }
    else if temperature == 4 { BiomeId::Desert }
    else { BiomeId::Beach }
}

fn shattered_coast_biome(temperature: usize, humidity: usize, positive_weirdness: bool) -> BiomeId {
    let beach_or_middle = if positive_weirdness { middle_biome(temperature, humidity, true) } else { beach_biome(temperature) };
    windswept_savanna_if_needed(temperature, humidity, positive_weirdness, beach_or_middle)
}

fn add_peaks(output: &mut Vec<ParameterPoint>, temperatures: &[ClimateParameter; 5], humidities: &[ClimateParameter; 5], erosions: &[ClimateParameter; 7], weird_min: f64, weird_max: f64) {
    let weirdness = ClimateParameter::span(weird_min, weird_max);
    let positive = weird_max >= 0.0;
    for (temperature_index, temperature) in temperatures.iter().copied().enumerate() {
        for (humidity_index, humidity) in humidities.iter().copied().enumerate() {
            let middle = middle_biome(temperature_index, humidity_index, positive);
            let middle_or_badlands = if temperature_index == 4 { badlands_biome(humidity_index, positive) } else { middle };
            let middle_or_badlands_or_slope = if temperature_index == 0 { slope_biome(temperature_index, humidity_index, positive) } else { middle_or_badlands };
            let plateau = plateau_biome(temperature_index, humidity_index, positive);
            let shattered = shattered_biome(temperature_index, humidity_index, positive);
            let shattered_or_savanna = windswept_savanna_if_needed(temperature_index, humidity_index, positive, shattered);
            let peak = peak_biome(temperature_index, humidity_index, positive);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, 1.0), erosions[0], weirdness, 0.0, peak);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), erosions[1], weirdness, 0.0, middle_or_badlands_or_slope);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[1], weirdness, 0.0, peak);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[2], erosions[3]), weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[2], weirdness, 0.0, plateau);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.03), erosions[3], weirdness, 0.0, middle_or_badlands);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.3), erosions[3], weirdness, 0.0, plateau);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, 1.0), erosions[4], weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), erosions[5], weirdness, 0.0, shattered_or_savanna);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[5], weirdness, 0.0, shattered);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, 1.0), erosions[6], weirdness, 0.0, middle);
        }
    }
}

fn add_high_slice(output: &mut Vec<ParameterPoint>, temperatures: &[ClimateParameter; 5], humidities: &[ClimateParameter; 5], erosions: &[ClimateParameter; 7], weird_min: f64, weird_max: f64) {
    let weirdness = ClimateParameter::span(weird_min, weird_max);
    let positive = weird_max >= 0.0;
    for (temperature_index, temperature) in temperatures.iter().copied().enumerate() {
        for (humidity_index, humidity) in humidities.iter().copied().enumerate() {
            let middle = middle_biome(temperature_index, humidity_index, positive);
            let middle_or_badlands = if temperature_index == 4 { badlands_biome(humidity_index, positive) } else { middle };
            let middle_or_badlands_or_slope = if temperature_index == 0 { slope_biome(temperature_index, humidity_index, positive) } else { middle_or_badlands };
            let plateau = plateau_biome(temperature_index, humidity_index, positive);
            let shattered = shattered_biome(temperature_index, humidity_index, positive);
            let middle_or_savanna = windswept_savanna_if_needed(temperature_index, humidity_index, positive, middle);
            let slope = slope_biome(temperature_index, humidity_index, positive);
            let peak = peak_biome(temperature_index, humidity_index, positive);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), erosions[0], weirdness, 0.0, slope);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[0], weirdness, 0.0, peak);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), erosions[1], weirdness, 0.0, middle_or_badlands_or_slope);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[1], weirdness, 0.0, slope);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[2], erosions[3]), weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[2], weirdness, 0.0, plateau);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.03), erosions[3], weirdness, 0.0, middle_or_badlands);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.3), erosions[3], weirdness, 0.0, plateau);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, 1.0), erosions[4], weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), erosions[5], weirdness, 0.0, middle_or_savanna);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[5], weirdness, 0.0, shattered);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, 1.0), erosions[6], weirdness, 0.0, middle);
        }
    }
}

fn add_mid_slice(output: &mut Vec<ParameterPoint>, temperatures: &[ClimateParameter; 5], humidities: &[ClimateParameter; 5], erosions: &[ClimateParameter; 7], weird_min: f64, weird_max: f64) {
    let weirdness = ClimateParameter::span(weird_min, weird_max);
    let positive = weird_max >= 0.0;
    add_surface(output, ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[0], erosions[2]), weirdness, 0.0, BiomeId::StonyShore);
    add_surface(output, ClimateParameter::join(temperatures[1], temperatures[2]), ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-0.11, 1.0), erosions[6], weirdness, 0.0, BiomeId::Swamp);
    add_surface(output, ClimateParameter::join(temperatures[3], temperatures[4]), ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-0.11, 1.0), erosions[6], weirdness, 0.0, BiomeId::MangroveSwamp);
    for (temperature_index, temperature) in temperatures.iter().copied().enumerate() {
        for (humidity_index, humidity) in humidities.iter().copied().enumerate() {
            let middle = middle_biome(temperature_index, humidity_index, positive);
            let middle_or_badlands = if temperature_index == 4 { badlands_biome(humidity_index, positive) } else { middle };
            let middle_or_badlands_or_slope = if temperature_index == 0 { slope_biome(temperature_index, humidity_index, positive) } else { middle_or_badlands };
            let shattered = shattered_biome(temperature_index, humidity_index, positive);
            let plateau = plateau_biome(temperature_index, humidity_index, positive);
            let beach = beach_biome(temperature_index);
            let middle_or_savanna = windswept_savanna_if_needed(temperature_index, humidity_index, positive, middle);
            let shattered_coast = shattered_coast_biome(temperature_index, humidity_index, positive);
            let slope = slope_biome(temperature_index, humidity_index, positive);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.11, 1.0), erosions[0], weirdness, 0.0, slope);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.11, 0.03), erosions[1], weirdness, 0.0, middle_or_badlands_or_slope);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.3), erosions[1], weirdness, 0.0, if temperature_index == 0 { slope } else { plateau });
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), erosions[2], weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.03), erosions[2], weirdness, 0.0, middle_or_badlands);
            add_surface(output, temperature, humidity, ClimateParameter::point(0.3), erosions[2], weirdness, 0.0, plateau);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), erosions[3], weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[3], weirdness, 0.0, middle_or_badlands);
            if weird_max < 0.0 { add_surface(output, temperature, humidity, ClimateParameter::point(-0.19), erosions[4], weirdness, 0.0, beach); add_surface(output, temperature, humidity, ClimateParameter::span(-0.11, 1.0), erosions[4], weirdness, 0.0, middle); }
            else { add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, 1.0), erosions[4], weirdness, 0.0, middle); }
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.19), erosions[5], weirdness, 0.0, shattered_coast);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), erosions[5], weirdness, 0.0, middle_or_savanna);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[5], weirdness, 0.0, shattered);
            if weird_max < 0.0 { add_surface(output, temperature, humidity, ClimateParameter::point(-0.19), erosions[6], weirdness, 0.0, beach); }
            else { add_surface(output, temperature, humidity, ClimateParameter::point(-0.19), erosions[6], weirdness, 0.0, middle); }
            if temperature_index == 0 { add_surface(output, temperature, humidity, ClimateParameter::span(-0.11, 1.0), erosions[6], weirdness, 0.0, middle); }
        }
    }
}

fn add_low_slice(output: &mut Vec<ParameterPoint>, temperatures: &[ClimateParameter; 5], humidities: &[ClimateParameter; 5], erosions: &[ClimateParameter; 7], weird_min: f64, weird_max: f64) {
    let weirdness = ClimateParameter::span(weird_min, weird_max);
    let positive = weird_max >= 0.0;
    add_surface(output, ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[0], erosions[2]), weirdness, 0.0, BiomeId::StonyShore);
    add_surface(output, ClimateParameter::join(temperatures[1], temperatures[2]), ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-0.11, 1.0), erosions[6], weirdness, 0.0, BiomeId::Swamp);
    add_surface(output, ClimateParameter::join(temperatures[3], temperatures[4]), ClimateParameter::span(-1.0, 1.0), ClimateParameter::span(-0.11, 1.0), erosions[6], weirdness, 0.0, BiomeId::MangroveSwamp);
    for (temperature_index, temperature) in temperatures.iter().copied().enumerate() {
        for (humidity_index, humidity) in humidities.iter().copied().enumerate() {
            let middle = middle_biome(temperature_index, humidity_index, positive);
            let middle_or_badlands = if temperature_index == 4 { badlands_biome(humidity_index, positive) } else { middle };
            let middle_or_badlands_or_slope = if temperature_index == 0 { slope_biome(temperature_index, humidity_index, positive) } else { middle_or_badlands };
            let beach = beach_biome(temperature_index);
            let middle_or_savanna = windswept_savanna_if_needed(temperature_index, humidity_index, positive, middle);
            let shattered_coast = shattered_coast_biome(temperature_index, humidity_index, positive);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, middle_or_badlands);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, middle_or_badlands_or_slope);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), ClimateParameter::join(erosions[2], erosions[3]), weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), ClimateParameter::join(erosions[2], erosions[3]), weirdness, 0.0, middle_or_badlands);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[3], erosions[4]), weirdness, 0.0, beach);
            add_surface(output, temperature, humidity, ClimateParameter::span(-0.11, 1.0), erosions[4], weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.19), erosions[5], weirdness, 0.0, shattered_coast);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.11), erosions[5], weirdness, 0.0, middle_or_savanna);
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), erosions[5], weirdness, 0.0, middle);
            add_surface(output, temperature, humidity, ClimateParameter::point(-0.19), erosions[6], weirdness, 0.0, beach);
            if temperature_index == 0 { add_surface(output, temperature, humidity, ClimateParameter::span(-0.11, 1.0), erosions[6], weirdness, 0.0, middle); }
        }
    }
}

fn add_valleys(output: &mut Vec<ParameterPoint>, temperatures: &[ClimateParameter; 5], humidities: &[ClimateParameter; 5], erosions: &[ClimateParameter; 7], weird_min: f64, weird_max: f64) {
    let weirdness = ClimateParameter::span(weird_min, weird_max);
    let frozen = temperatures[0];
    let unfrozen = ClimateParameter::join(temperatures[1], temperatures[4]);
    let full = ClimateParameter::span(-1.0, 1.0);
    add_surface(output, frozen, full, ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, if weird_max < 0.0 { BiomeId::StonyShore } else { BiomeId::FrozenRiver });
    add_surface(output, unfrozen, full, ClimateParameter::span(-0.19, -0.11), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, if weird_max < 0.0 { BiomeId::StonyShore } else { BiomeId::River });
    add_surface(output, frozen, full, ClimateParameter::span(-0.11, 0.03), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, BiomeId::FrozenRiver);
    add_surface(output, unfrozen, full, ClimateParameter::span(-0.11, 0.03), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, BiomeId::River);
    add_surface(output, frozen, full, ClimateParameter::span(-0.19, 1.0), ClimateParameter::join(erosions[2], erosions[5]), weirdness, 0.0, BiomeId::FrozenRiver);
    add_surface(output, unfrozen, full, ClimateParameter::span(-0.19, 1.0), ClimateParameter::join(erosions[2], erosions[5]), weirdness, 0.0, BiomeId::River);
    add_surface(output, frozen, full, ClimateParameter::span(-0.19, -0.11), erosions[6], weirdness, 0.0, BiomeId::FrozenRiver);
    add_surface(output, unfrozen, full, ClimateParameter::span(-0.19, -0.11), erosions[6], weirdness, 0.0, BiomeId::River);
    add_surface(output, ClimateParameter::join(temperatures[1], temperatures[2]), full, ClimateParameter::span(-0.11, 0.55), erosions[6], weirdness, 0.0, BiomeId::Swamp);
    add_surface(output, ClimateParameter::join(temperatures[3], temperatures[4]), full, ClimateParameter::span(-0.11, 0.55), erosions[6], weirdness, 0.0, BiomeId::MangroveSwamp);
    add_surface(output, frozen, full, ClimateParameter::span(-0.11, 0.55), erosions[6], weirdness, 0.0, BiomeId::FrozenRiver);
    for (temperature_index, temperature) in temperatures.iter().copied().enumerate() {
        for (humidity_index, humidity) in humidities.iter().copied().enumerate() {
            let middle_or_badlands = if temperature_index == 4 { badlands_biome(humidity_index, weird_max >= 0.0) } else { middle_biome(temperature_index, humidity_index, weird_max >= 0.0) };
            add_surface(output, temperature, humidity, ClimateParameter::span(0.03, 1.0), ClimateParameter::join(erosions[0], erosions[1]), weirdness, 0.0, middle_or_badlands);
        }
    }
}

#[cfg(test)]
fn closest_biome(parameters: &[ParameterPoint], target: [i64; 7]) -> BiomeId {
    let mut best = parameters[0];
    let mut best_fitness = best.fitness(target);
    for candidate in parameters.iter().copied().skip(1) {
        let fitness = candidate.fitness(target);
        // Java's ParameterList brute-force tie behavior keeps the earlier
        // entry. Keeping the ordered list here makes that rule explicit.
        if fitness < best_fitness {
            best = candidate;
            best_fitness = fitness;
        }
    }
    best.biome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::world_gen::MINECRAFT26_OVERWORLD_BIOMES;
    use std::collections::HashSet;

    #[test]
    fn parameter_distance_is_zero_inside_and_signed_outside() {
        let parameter = ClimateParameter::span(-0.25, 0.25);
        assert_eq!(parameter.distance(quantize(0.0)), 0);
        assert_eq!(parameter.distance(quantize(-0.5)), quantize(0.25));
        assert_eq!(parameter.distance(quantize(0.5)), quantize(0.25));
    }

    #[test]
    fn equal_fitness_keeps_the_first_parameter() {
        let target = [0; 7];
        let parameters = vec![
            point(ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), 0.0, BiomeId::Plains),
            point(ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), ClimateParameter::point(0.0), 0.0, BiomeId::Forest),
        ];
        assert_eq!(closest_biome(&parameters, target), BiomeId::Plains);
    }

    #[test]
    fn climate_rtree_matches_brute_force_distance_for_parameter_centers_and_samples() {
        let parameters = build_parameter_points();
        let index = ClimateRTree::new(&parameters);
        for point in parameters.iter().step_by(37) {
            let target = point.parameters.map(|parameter| (parameter.min + parameter.max) / 2);
            let indexed = index.root.search(target, None).point;
            let brute = parameters
                .iter()
                .copied()
                .min_by_key(|point| point.fitness(target))
                .unwrap();
            assert_eq!(indexed.fitness(target), brute.fitness(target));
        }

        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..1024 {
            let mut target = [0; 7];
            for value in &mut target {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *value = ((state >> 32) as i32 % 20_001 - 10_000) as i64;
            }
            target[6] = 0;
            let indexed = index.root.search(target, None).point;
            let brute = parameters
                .iter()
                .copied()
                .min_by_key(|point| point.fitness(target))
                .unwrap();
            assert_eq!(indexed.fitness(target), brute.fitness(target));
        }
    }

    #[test]
    fn seed_one_172_182_uses_reference_parameter_order_for_equal_fitness() {
        let source = OverworldBiomeSource::from_router(
            crate::world::world_gen::noise_router::NoiseRouterData::create_overworld_router_reference(1, false, false),
        );
        let target = source.sample_target(172, 96, 182);
        let parameters = build_parameter_points();
        let brute = parameters.iter().copied().min_by_key(|point| point.fitness(target)).unwrap();
        let indexed = source.index.root.search(target, None).point;
        assert_eq!(brute.biome, BiomeId::Forest);
        assert_eq!(indexed.biome, BiomeId::Forest);
        assert_eq!(indexed.fitness(target), brute.fitness(target));

        let target = source.sample_target(158, 88, 165);
        let indexed = source.index.root.search(target, None).point;
        let brute = parameters.iter().copied().min_by_key(|point| point.fitness(target)).unwrap();
        assert_eq!(indexed.biome, BiomeId::Forest);
        assert_eq!(indexed, brute);
    }

    #[test]
    fn builder_order_starts_with_mushroom_fields_and_ends_with_deep_dark() {
        let parameters = build_parameter_points();
        assert_eq!(parameters.len(), 7594);
        assert_eq!(parameters.first().unwrap().biome, BiomeId::MushroomFields);
        assert_eq!(parameters.last().unwrap().biome, BiomeId::DeepDark);
        let target = [0, 0, quantize(-1.125), 0, 0, 0, 0];
        assert_eq!(closest_biome(&parameters, target), BiomeId::MushroomFields);
    }

    #[test]
    fn reference_builder_has_exact_55_biome_identities_and_7594_point_labels() {
        let parameters = build_parameter_points();
        assert_eq!(parameters.len(), 7594);

        let internal: HashSet<_> = parameters.iter().map(|point| point.biome).collect();
        assert_eq!(internal.len(), 55);
        let public: HashSet<_> = MINECRAFT26_OVERWORLD_BIOMES.into_iter().collect();
        assert_eq!(public.len(), 55);
        assert_eq!(
            internal
                .iter()
                .copied()
                .map(|biome| biome.supported().unwrap())
                .collect::<HashSet<_>>(),
            public
        );

        let label_fingerprint = parameters.iter().fold(0xcbf29ce484222325_u64, |hash, point| {
            format!("{:?}", point.biome)
                .bytes()
                .chain(std::iter::once(0xff))
                .fold(hash, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
        });
        assert_eq!(label_fingerprint, 11_087_558_769_596_216_506);
    }

    #[test]
    fn reference_old_growth_birch_slot_is_not_collapsed_into_birch_forest() {
        assert_eq!(middle_biome(2, 3, false), BiomeId::BirchForest);
        assert_eq!(middle_biome(2, 3, true), BiomeId::OldGrowthBirchForest);
        assert!(build_parameter_points()
            .iter()
            .any(|point| point.biome == BiomeId::OldGrowthBirchForest));
        assert!(!build_compatibility_parameter_points()
            .iter()
            .any(|point| point.biome == BiomeId::OldGrowthBirchForest));
    }

    #[test]
    fn reference_dripstone_uses_continentalness_axis_only() {
        let full = ClimateParameter::span(-1.0, 1.0);
        let high = ClimateParameter::span(0.8, 1.0);
        let reference = build_parameter_points()
            .into_iter()
            .find(|point| point.biome == BiomeId::DripstoneCaves)
            .unwrap();
        assert_eq!(reference.parameters[2], high);
        assert_eq!(reference.parameters[3], full);

        let compatibility = build_compatibility_parameter_points()
            .into_iter()
            .find(|point| point.biome == BiomeId::DripstoneCaves)
            .unwrap();
        assert_eq!(compatibility.parameters[2], full);
        assert_eq!(compatibility.parameters[3], high);
    }

    #[test]
    fn router_sample_uses_climate_channels_and_maps_supported_biome() {
        use super::super::density_fn::constant;

        let router = NoiseRouter::new(
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(-1.125),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
        );
        let source = OverworldBiomeSource::from_router(router);
        assert_eq!(source.get_biome(0, 64, 0), Ok(Biome::MushroomFields));
    }

    #[test]
    fn seed_one_spawn_suggestion_matches_the_reference_search() {
        let source = OverworldBiomeSource::from_router(
            crate::world::world_gen::noise_router::NoiseRouterData::create_overworld_router_reference(1, false, false),
        );
        assert_eq!(source.find_spawn_position(), (126, 178));
    }

    #[test]
    fn router_sample_returns_supported_deep_dark() {
        use super::super::density_fn::constant;

        let router = NoiseRouter::new(
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(-0.5),
            constant(1.1),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
            constant(0.0),
        );
        let source = OverworldBiomeSource::from_router(router);
        assert_eq!(source.get_biome(0, 64, 0), Ok(Biome::DeepDark));
    }
}
