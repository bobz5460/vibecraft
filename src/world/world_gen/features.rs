//! Bounded Minecraft 26.2-oriented Overworld placed-feature geometry.
//!
//! Feature candidates are owned by the chunk containing their origin. Every
//! target chunk independently plans the fixed owner halo and projects only
//! writes whose world coordinates belong to that target. No loaded neighbor is
//! read or mutated, so chunk generation order cannot affect the result.

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_HEIGHT, CHUNK_SIZE};
use crate::world::world_gen::decoration::{decoration_seed, feature_seed};
use crate::world::world_gen::generator::VanillaWorldGenerator;
use crate::world::world_gen::noise::NoiseSeed;
use crate::world::world_gen::{Biome, MINECRAFT26_OVERWORLD_BIOMES};
use std::collections::HashMap;

/// One chunk is sufficient for every currently supported feature footprint.
pub const FEATURE_OWNER_HALO: i32 = 1;
/// Hard cap on projected work for one target chunk.
pub const MAX_PROJECTED_WRITES: usize = 32_768;
/// Upper bound on attempts emitted by one owner across all families.
pub const MAX_CANDIDATES_PER_OWNER: usize = 1_024;

/// Geometry families with an explicit native implementation.
pub const SUPPORTED_FEATURE_FAMILIES: &[&str] = &[
    "overworld ores: coal, iron, copper, gold, redstone, diamond, lapis, emerald",
    "underground blobs: dirt, gravel, granite, diorite, andesite",
    "trees: oak, spruce, birch, jungle, acacia, dark oak, cherry, mangrove",
    "surface vegetation: grass, fern, flowers, mushrooms, dead bush, cactus, sugar cane, pumpkin, melon, bamboo, vines",
    "aquatic vegetation: kelp, seagrass, coral blocks/coral/fans, sea pickles",
    "underwater disks: sand and gravel",
    "water/lava springs and bounded water/lava lake cavities",
    "snow and ice top-layer freezing",
    "dripstone-block and lush-cave native decorations",
    "amethyst geode shells and clusters",
    "desert-well geometry",
    "monster-room geometry",
];

/// Reference outputs deliberately skipped because no faithful native state or
/// runtime behavior exists. None of these are replaced with a lookalike block.
pub const UNSUPPORTED_FEATURE_OUTPUTS: &[&str] = &[
    "clay disks (no native clay BlockId)",
    "pointed dripstone states (no native pointed-dripstone BlockId/state)",
    "small/medium/large amethyst buds and cluster facing/waterlogged states",
    "smooth-basalt geode outer shells (no native smooth-basalt BlockId)",
    "tree log-axis, leaf-distance/persistence, bee nest, cocoa, propagule, and hanging-leaf states",
    "double-plant half states and multi-pickle count states",
    "sandstone slabs and suspicious-sand archaeology in desert wells",
    "monster-spawner mob configuration, chest loot tables, and feature block entities",
    "Java biome feature-index compatibility and exact configured-feature processors",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeatureCoverage {
    pub reference: &'static str,
    pub owner_halo_chunks: i32,
    pub max_candidates_per_owner: usize,
    pub max_projected_writes: usize,
    pub supported_families: &'static [&'static str],
    pub unsupported_outputs: &'static [&'static str],
}

pub const MINECRAFT26_GEOMETRY_COVERAGE: FeatureCoverage = FeatureCoverage {
    reference: "Minecraft Java Edition 26.2 data pack 107.1 geometry-oriented subset",
    owner_halo_chunks: FEATURE_OWNER_HALO,
    max_candidates_per_owner: MAX_CANDIDATES_PER_OWNER,
    max_projected_writes: MAX_PROJECTED_WRITES,
    supported_families: SUPPORTED_FEATURE_FAMILIES,
    unsupported_outputs: UNSUPPORTED_FEATURE_OUTPUTS,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootCoverageDisposition {
    Implemented,
    GeometryApproximatedDueStates(&'static str),
    Blocked(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeatureRootCoverage {
    pub key: &'static str,
    pub step: u8,
    pub disposition: RootCoverageDisposition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RootDefinition {
    key: &'static str,
    step: u8,
    biomes: [u32; 2],
}

impl RootDefinition {
    fn applies_to(self, biome: Biome) -> bool {
        let Some(index) = MINECRAFT26_OVERWORLD_BIOMES
            .iter()
            .position(|candidate| *candidate == biome)
        else {
            return false;
        };
        self.biomes[index / 32] & (1 << (index % 32)) != 0
    }
}

// Generated from the 55 supplied biome JSON lists. Mask bit positions are the
// indices in MINECRAFT26_OVERWORLD_BIOMES; the order is their common
// topological order within each of Java's 11 generation steps.
const MINECRAFT26_FEATURE_ROOTS: [RootDefinition; 168] = include!("feature_roots.in");
const MINECRAFT26_FEATURE_ROOT_ORDER: [&str; 168] = include!("feature_root_order.in");

pub fn minecraft26_feature_root_coverage(
) -> impl ExactSizeIterator<Item = FeatureRootCoverage> {
    MINECRAFT26_FEATURE_ROOTS.iter().map(|root| FeatureRootCoverage {
        key: root.key,
        step: root.step,
        disposition: root_coverage_disposition(root.key),
    })
}

fn root_coverage_disposition(key: &str) -> RootCoverageDisposition {
    use RootCoverageDisposition::{Blocked, GeometryApproximatedDueStates as Approx, Implemented};

    match key {
        "disk_clay" | "ore_clay" | "lush_caves_clay" => {
            Blocked("minecraft:clay is not present in the native BlockId registry")
        }
        "pointed_dripstone" => Blocked(
            "pointed-dripstone thickness, direction, and waterlogged states are not representable",
        ),
        "sculk_vein" | "sculk_patch_deep_dark" => {
            Blocked("native sculk blocks, states, spread behavior, and catalysts are unavailable")
        }
        "pale_garden_vegetation" | "pale_moss_patch" | "pale_garden_flowers"
        | "flower_pale_garden" => Blocked(
            "pale oak, pale moss, eyeblossom, creaking-heart, and required states are unavailable",
        ),
        "rooted_sulfur_spring" | "sulfur_pool" | "sulfur_spike_cluster"
        | "sulfur_spike" => Blocked(
            "sulfur, cinnabar, sulfur-spike states, and sulfur fluid behavior are unavailable",
        ),
        "ore_infested" => Blocked("infested stone variants are unavailable"),
        "patch_bush" | "patch_sunflower" | "patch_dry_grass_desert" | "patch_dry_grass_badlands"
        | "patch_firefly_bush_near_water" | "patch_firefly_bush_swamp"
        | "patch_firefly_bush_near_water_swamp" | "patch_leaf_litter"
        | "wildflowers_birch_forest" | "wildflowers_meadow" | "flower_cherry"
        | "flower_forest_flowers" | "forest_flowers" => {
            Blocked("the configured output block or required state is unavailable")
        }
        "glow_lichen" => Approx(
            "native GlowLichen is emitted in its default state; attachment-face state is unavailable",
        ),
        "dripstone_cluster" | "large_dripstone" => Approx(
            "dripstone-block bodies are represented; pointed-dripstone state outputs are omitted",
        ),
        "amethyst_geode" => Approx(
            "bud facing/waterlogged states and smooth-basalt outer shells are unavailable",
        ),
        "desert_well" => Approx(
            "sandstone slab states, suspicious sand, archaeology, and loot are unavailable",
        ),
        "monster_room" | "monster_room_deep" => Approx(
            "room geometry is represented without spawner mob data or chest loot tables",
        ),
        key if key.starts_with("trees_") || key == "birch_tall" => Approx(
            "native tree geometry uses default log axes and default leaf states",
        ),
        "dark_forest_vegetation" | "mushroom_island_vegetation" => Approx(
            "huge mushrooms and fallen logs use default block states",
        ),
        "bamboo" | "bamboo_light" | "bamboo_vegetation" => Approx(
            "bamboo age/leaves/stage and tree leaf/log states use native defaults",
        ),
        "vines" | "classic_vines_cave_feature" => {
            Approx("vine attachment-face states use the native default state")
        }
        "rooted_azalea_tree" => Approx(
            "rooted dirt, hanging roots, and azalea geometry omit waterlogged and tree states",
        ),
        "cave_vines" | "lush_caves_ceiling_vegetation" | "lush_caves_vegetation"
        | "spore_blossom" => Approx(
            "available lush-cave blocks are emitted with default native states",
        ),
        key if key.starts_with("patch_") || key.starts_with("flower_")
            || key.starts_with("brown_mushroom_") || key.starts_with("red_mushroom_") =>
        {
            Approx("configured patch geometry uses available default-state native blocks")
        }
        "warm_ocean_vegetation" | "sea_pickle" | "kelp_warm" | "kelp_cold"
        | "seagrass_warm" | "seagrass_normal" | "seagrass_cold"
        | "seagrass_deep_warm" | "seagrass_deep" | "seagrass_deep_cold"
        | "seagrass_river" | "seagrass_swamp" => Approx(
            "aquatic geometry omits age, half, waterlogged, and pickle-count states",
        ),
        _ => Implemented,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WorldPos {
    x: i32,
    y: i32,
    z: i32,
}

impl WorldPos {
    const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy)]
enum Replacement {
    Air,
    Water,
    LakeAir,
    LakeFluid,
    BaseStone,
    Ore {
        stone: BlockId,
        deepslate: BlockId,
        discard_if_exposed: bool,
    },
    Disk,
    UnderwaterDisk,
    UnderwaterMagma,
    PlantOnGrass,
    PlantOnDirt,
    Mushroom,
    Cactus,
    SugarCane,
    Spring { frozen: bool },
    Natural,
    CaveFloor,
    CaveCeiling,
}

#[derive(Clone, Copy)]
struct ProjectedWrite {
    pos: WorldPos,
    block: BlockId,
    replacement: Replacement,
    order: u32,
}

struct FeatureStage<'a> {
    generator: &'a VanillaWorldGenerator,
    world_seed: u64,
    min_y: i32,
    max_y: i32,
    target_x: i32,
    target_z: i32,
    writes: Vec<ProjectedWrite>,
    next_order: u32,
    surface_cache: HashMap<(i32, i32), i32>,
    target_ocean_floor: [i32; CHUNK_SIZE * CHUNK_SIZE],
}

impl<'a> FeatureStage<'a> {
    fn new(
        generator: &'a VanillaWorldGenerator,
        world_seed: u64,
        min_y: i32,
        height: i32,
        chunk: &Chunk,
    ) -> Self {
        let mut target_ocean_floor = [min_y; CHUNK_SIZE * CHUNK_SIZE];
        for local_x in 0..CHUNK_SIZE {
            for local_z in 0..CHUNK_SIZE {
                if let Some(local_y) = (0..crate::world::chunk::CHUNK_HEIGHT).rev().find(|&local_y| {
                    let block = chunk.get_block(local_x, local_y, local_z);
                    !block.is_air() && !matches!(block.id, BlockId::Water | BlockId::Lava)
                }) {
                    target_ocean_floor[local_x * CHUNK_SIZE + local_z] =
                        min_y + local_y as i32;
                }
            }
        }
        Self {
            generator,
            world_seed,
            min_y,
            max_y: min_y + height,
            target_x: chunk.cx,
            target_z: chunk.cz,
            writes: Vec::new(),
            next_order: 0,
            surface_cache: HashMap::new(),
            target_ocean_floor,
        }
    }

    fn push(&mut self, pos: WorldPos, block: BlockId, replacement: Replacement) {
        if self.writes.len() >= MAX_PROJECTED_WRITES
            || pos.y < self.min_y
            || pos.y >= self.max_y
            || pos.x.div_euclid(CHUNK_SIZE as i32) != self.target_x
            || pos.z.div_euclid(CHUNK_SIZE as i32) != self.target_z
        {
            return;
        }
        let order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        self.writes.push(ProjectedWrite {
            pos,
            block,
            replacement,
            order,
        });
    }

    fn intersects_target(&self, x: i32, z: i32, radius: i32) -> bool {
        let min_x = self.target_x * CHUNK_SIZE as i32;
        let min_z = self.target_z * CHUNK_SIZE as i32;
        x + radius >= min_x
            && x - radius < min_x + CHUNK_SIZE as i32
            && z + radius >= min_z
            && z - radius < min_z + CHUNK_SIZE as i32
    }

    fn feature_rng(&self, owner_x: i32, owner_z: i32, step: i32, index: i32) -> NoiseSeed {
        let decoration = decoration_seed(
            self.world_seed,
            owner_x.wrapping_mul(CHUNK_SIZE as i32),
            owner_z.wrapping_mul(CHUNK_SIZE as i32),
        );
        feature_seed(decoration, step, index)
    }

    fn surface_y(&mut self, x: i32, z: i32) -> i32 {
        if x.div_euclid(CHUNK_SIZE as i32) == self.target_x
            && z.div_euclid(CHUNK_SIZE as i32) == self.target_z
        {
            let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
            let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
            return self.target_ocean_floor[local_x * CHUNK_SIZE + local_z];
        }
        if let Some(height) = self.surface_cache.get(&(x, z)) {
            return *height;
        }
        let height = self
            .generator
            .get_height(x, z)
            .clamp(self.min_y + 1, self.max_y - 2);
        self.surface_cache.insert((x, z), height);
        height
    }

    fn biome(&self, x: i32, y: i32, z: i32) -> Biome {
        self.generator.get_biome_at(x, y, z)
    }

    fn apply(mut self, chunk: &mut Chunk) {
        self.writes.sort_by_key(|write| (write.order, write.pos));
        for write in self.writes {
            let local_x = write.pos.x.rem_euclid(CHUNK_SIZE as i32) as usize;
            let local_y = (write.pos.y - self.min_y) as usize;
            let local_z = write.pos.z.rem_euclid(CHUNK_SIZE as i32) as usize;
            let current = chunk.get_block(local_x, local_y, local_z).id;
            if replacement_matches(write.replacement, current, chunk, local_x, local_y, local_z) {
                let block = replacement_block(write.replacement, current, write.block);
                chunk.set_block(local_x, local_y, local_z, Block::new(block));
            }
        }
        chunk.recount_fluids();
    }
}

/// Apply the new-world-only feature stage to an already completed base chunk.
pub(crate) fn apply_minecraft26_geometry(
    generator: &VanillaWorldGenerator,
    world_seed: u64,
    min_y: i32,
    height: i32,
    chunk: &mut Chunk,
) {
    let mut stage = FeatureStage::new(generator, world_seed, min_y, height, chunk);
    for owner_z in chunk.cz - FEATURE_OWNER_HALO..=chunk.cz + FEATURE_OWNER_HALO {
        for owner_x in chunk.cx - FEATURE_OWNER_HALO..=chunk.cx + FEATURE_OWNER_HALO {
            plan_owner(&mut stage, owner_x, owner_z);
        }
    }
    stage.apply(chunk);
}

fn plan_owner(stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32) {
    let base_x = owner_x.wrapping_mul(CHUNK_SIZE as i32);
    let base_z = owner_z.wrapping_mul(CHUNK_SIZE as i32);
    let mut sampled_biomes = [false; 55];
    for dx in [2, 6, 10, 14] {
        for dz in [2, 6, 10, 14] {
            for y in [-32, 32, 96, 224] {
                let biome = stage.biome(base_x + dx, y, base_z + dz);
                if let Some(index) = MINECRAFT26_OVERWORLD_BIOMES
                    .iter()
                    .position(|candidate| *candidate == biome)
                {
                    sampled_biomes[index] = true;
                }
            }
        }
    }
    let mut candidates = 0usize;

    for step in 0..11 {
        for (index, key) in MINECRAFT26_FEATURE_ROOT_ORDER.iter().copied().enumerate() {
            let root = MINECRAFT26_FEATURE_ROOTS
                .iter()
                .copied()
                .find(|root| root.key == key)
                .expect("checked Minecraft 26.2 feature root order");
            if root.step != step
                || matches!(root_coverage_disposition(root.key), RootCoverageDisposition::Blocked(_))
                || !MINECRAFT26_OVERWORLD_BIOMES
                    .iter()
                    .copied()
                    .enumerate()
                    .any(|(index, biome)| sampled_biomes[index] && root.applies_to(biome))
                || candidates >= MAX_CANDIDATES_PER_OWNER
            {
                continue;
            }
            plan_root(stage, owner_x, owner_z, root, index as i32, &mut candidates);
        }
    }
}

fn plan_root(
    stage: &mut FeatureStage<'_>,
    owner_x: i32,
    owner_z: i32,
    root: RootDefinition,
    index: i32,
    candidates: &mut usize,
) {
    match root.key {
        "lake_lava_underground" | "lake_lava_surface" => {
            plan_lake(stage, owner_x, owner_z, root, index, candidates)
        }
        "amethyst_geode" => plan_geode_root(stage, owner_x, owner_z, root, index, candidates),
        "iceberg_packed" | "iceberg_blue" => {
            plan_iceberg(stage, owner_x, owner_z, root, index, candidates)
        }
        "forest_rock" => plan_forest_rock(stage, owner_x, owner_z, root, index, candidates),
        "large_dripstone" => {
            plan_large_dripstone(stage, owner_x, owner_z, root, index, candidates)
        }
        "fossil_upper" | "fossil_lower" => {
            plan_fossil(stage, owner_x, owner_z, root, index, candidates)
        }
        "monster_room" | "monster_room_deep" => {
            plan_monster_room(stage, owner_x, owner_z, root, index, candidates)
        }
        "ice_spike" | "ice_patch" | "blue_ice" => {
            plan_ice_feature(stage, owner_x, owner_z, root, index, candidates)
        }
        "desert_well" => plan_desert_well_root(stage, owner_x, owner_z, root, index, candidates),
        key if key.starts_with("ore_") => {
            plan_ore_or_blob(stage, owner_x, owner_z, root, index, candidates)
        }
        "underwater_magma" => {
            plan_underwater_magma(stage, owner_x, owner_z, root, index, candidates)
        }
        "disk_sand" | "disk_gravel" | "disk_grass" => {
            plan_disk(stage, owner_x, owner_z, root, index, candidates)
        }
        "dripstone_cluster" | "glow_lichen" | "lush_caves_ceiling_vegetation"
        | "cave_vines" | "lush_caves_vegetation" | "rooted_azalea_tree"
        | "spore_blossom" | "classic_vines_cave_feature" => {
            plan_cave_root(stage, owner_x, owner_z, root, index, candidates)
        }
        "spring_water" | "spring_lava" | "spring_lava_frozen" => {
            plan_spring(stage, owner_x, owner_z, root, index, candidates)
        }
        key if key.starts_with("trees_") || key == "birch_tall" => {
            plan_tree_root(stage, owner_x, owner_z, root, index, candidates)
        }
        "dark_forest_vegetation" | "mushroom_island_vegetation" => {
            plan_large_vegetation(stage, owner_x, owner_z, root, index, candidates)
        }
        "warm_ocean_vegetation" | "sea_pickle" | "kelp_warm" | "kelp_cold"
        | "seagrass_warm" | "seagrass_normal" | "seagrass_cold"
        | "seagrass_deep_warm" | "seagrass_deep" | "seagrass_deep_cold"
        | "seagrass_river" | "seagrass_swamp" => {
            plan_aquatic_root(stage, owner_x, owner_z, root, index, candidates)
        }
        "bamboo" | "bamboo_light" | "bamboo_vegetation" | "vines"
        | "patch_pumpkin" | "patch_melon" | "patch_melon_sparse"
        | "patch_cactus_desert" | "patch_cactus_decorated"
        | "patch_sugar_cane" | "patch_sugar_cane_desert" | "patch_sugar_cane_swamp"
        | "patch_dead_bush" | "patch_dead_bush_2" | "patch_dead_bush_badlands"
        | "patch_large_fern" | "patch_berry_common" | "patch_berry_rare"
        | "patch_waterlily" | "patch_sunflower" | "patch_tall_grass"
        | "patch_tall_grass_2" | "patch_grass_badlands" | "patch_grass_forest"
        | "patch_grass_jungle" | "patch_grass_meadow" | "patch_grass_normal"
        | "patch_grass_plain" | "patch_grass_savanna" | "patch_grass_taiga"
        | "patch_grass_taiga_2" | "flower_cherry" | "flower_default"
        | "flower_flower_forest" | "flower_forest_flowers" | "flower_meadow"
        | "flower_plains" | "flower_swamp" | "flower_warm" | "forest_flowers"
        | "brown_mushroom_normal" | "brown_mushroom_old_growth"
        | "brown_mushroom_swamp" | "brown_mushroom_taiga" | "red_mushroom_normal"
        | "red_mushroom_old_growth" | "red_mushroom_swamp" | "red_mushroom_taiga" => {
            plan_patch(stage, owner_x, owner_z, root, index, candidates)
        }
        "freeze_top_layer" => plan_freeze_root(stage, owner_x, owner_z, root),
        _ => {}
    }
}

fn root_applies_at(stage: &FeatureStage<'_>, root: RootDefinition, pos: WorldPos) -> bool {
    root.applies_to(stage.biome(pos.x, pos.y, pos.z))
}

fn claim_candidate(candidates: &mut usize) -> bool {
    if *candidates >= MAX_CANDIDATES_PER_OWNER {
        false
    } else {
        *candidates += 1;
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeightDistribution {
    Uniform(i32, i32),
    Triangle(i32, i32),
}

impl HeightDistribution {
    fn sample(self, random: &mut NoiseSeed, min_y: i32, max_y: i32) -> i32 {
        self.sample_unbounded(random).clamp(min_y, max_y - 1)
    }

    fn sample_valid(self, random: &mut NoiseSeed, min_y: i32, max_y: i32) -> Option<i32> {
        let y = self.sample_unbounded(random);
        (min_y..max_y).contains(&y).then_some(y)
    }

    fn sample_unbounded(self, random: &mut NoiseSeed) -> i32 {
        let (low, high, triangular) = match self {
            Self::Uniform(low, high) => (low, high, false),
            Self::Triangle(low, high) => (low, high, true),
        };
        let span = high - low + 1;
        if triangular {
            low + (random.next_int(span) + random.next_int(span)) / 2
        } else {
            low + random.next_int(span)
        }
    }
}

#[derive(Clone, Copy)]
struct OreConfig {
    key: &'static str,
    count: i32,
    rarity: i32,
    size: i32,
    height: HeightDistribution,
    stone: BlockId,
    deepslate: BlockId,
    discard_chance_on_air_exposure: f32,
    biome_gate: OreBiomeGate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OreBiomeGate {
    Any,
    Mountains,
    Dripstone,
    Badlands,
}

const ORES: &[OreConfig] = &[
    // Counts and ranges mirror the 26.2 placed-feature JSON where available.
    OreConfig { key: "ore_coal_lower", count: 20, rarity: 1, size: 17, height: HeightDistribution::Triangle(0, 192), stone: BlockId::CoalOre, deepslate: BlockId::DeepslateCoalOre, discard_chance_on_air_exposure: 0.5, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_coal_upper", count: 30, rarity: 1, size: 17, height: HeightDistribution::Uniform(136, 319), stone: BlockId::CoalOre, deepslate: BlockId::DeepslateCoalOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_iron_middle", count: 10, rarity: 1, size: 9, height: HeightDistribution::Triangle(-24, 56), stone: BlockId::IronOre, deepslate: BlockId::DeepslateIronOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_iron_upper", count: 90, rarity: 1, size: 9, height: HeightDistribution::Triangle(80, 384), stone: BlockId::IronOre, deepslate: BlockId::DeepslateIronOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_iron_small", count: 10, rarity: 1, size: 4, height: HeightDistribution::Uniform(-64, 72), stone: BlockId::IronOre, deepslate: BlockId::DeepslateIronOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_copper", count: 16, rarity: 1, size: 10, height: HeightDistribution::Triangle(-16, 112), stone: BlockId::CopperOre, deepslate: BlockId::DeepslateCopperOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_copper_large", count: 16, rarity: 1, size: 20, height: HeightDistribution::Triangle(-16, 112), stone: BlockId::CopperOre, deepslate: BlockId::DeepslateCopperOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Dripstone },
    OreConfig { key: "ore_gold", count: 4, rarity: 1, size: 9, height: HeightDistribution::Triangle(-64, 32), stone: BlockId::GoldOre, deepslate: BlockId::DeepslateGoldOre, discard_chance_on_air_exposure: 0.5, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_gold_extra", count: 50, rarity: 1, size: 9, height: HeightDistribution::Uniform(32, 256), stone: BlockId::GoldOre, deepslate: BlockId::DeepslateGoldOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Badlands },
    OreConfig { key: "ore_gold_lower", count: 1, rarity: 2, size: 9, height: HeightDistribution::Uniform(-64, -48), stone: BlockId::GoldOre, deepslate: BlockId::DeepslateGoldOre, discard_chance_on_air_exposure: 0.5, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_redstone", count: 4, rarity: 1, size: 8, height: HeightDistribution::Uniform(-64, 15), stone: BlockId::RedstoneOre, deepslate: BlockId::DeepslateRedstoneOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_redstone_lower", count: 8, rarity: 1, size: 8, height: HeightDistribution::Triangle(-96, -32), stone: BlockId::RedstoneOre, deepslate: BlockId::DeepslateRedstoneOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_diamond", count: 7, rarity: 1, size: 4, height: HeightDistribution::Triangle(-144, 16), stone: BlockId::DiamondOre, deepslate: BlockId::DeepslateDiamondOre, discard_chance_on_air_exposure: 0.5, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_diamond_medium", count: 2, rarity: 1, size: 8, height: HeightDistribution::Uniform(-64, -4), stone: BlockId::DiamondOre, deepslate: BlockId::DeepslateDiamondOre, discard_chance_on_air_exposure: 0.5, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_diamond_large", count: 1, rarity: 9, size: 12, height: HeightDistribution::Triangle(-144, 16), stone: BlockId::DiamondOre, deepslate: BlockId::DeepslateDiamondOre, discard_chance_on_air_exposure: 0.7, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_diamond_buried", count: 4, rarity: 1, size: 8, height: HeightDistribution::Triangle(-144, 16), stone: BlockId::DiamondOre, deepslate: BlockId::DeepslateDiamondOre, discard_chance_on_air_exposure: 1.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_lapis", count: 2, rarity: 1, size: 7, height: HeightDistribution::Triangle(-32, 32), stone: BlockId::LapisOre, deepslate: BlockId::DeepslateLapisOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_lapis_buried", count: 4, rarity: 1, size: 7, height: HeightDistribution::Uniform(-64, 64), stone: BlockId::LapisOre, deepslate: BlockId::DeepslateLapisOre, discard_chance_on_air_exposure: 1.0, biome_gate: OreBiomeGate::Any },
    OreConfig { key: "ore_emerald", count: 100, rarity: 1, size: 3, height: HeightDistribution::Triangle(-16, 480), stone: BlockId::EmeraldOre, deepslate: BlockId::DeepslateEmeraldOre, discard_chance_on_air_exposure: 0.0, biome_gate: OreBiomeGate::Mountains },
];

fn plan_ore_or_blob(
    stage: &mut FeatureStage<'_>,
    owner_x: i32,
    owner_z: i32,
    root: RootDefinition,
    index: i32,
    candidates: &mut usize,
) {
    let base_x = owner_x.wrapping_mul(CHUNK_SIZE as i32);
    let base_z = owner_z.wrapping_mul(CHUNK_SIZE as i32);
    if let Some(config) = ORES.iter().copied().find(|config| config.key == root.key) {
        let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
        if config.rarity > 1 && random.next_int(config.rarity) != 0 {
            return;
        }
        for _ in 0..config.count {
            if !claim_candidate(candidates) {
                return;
            }
            let x = base_x + random.next_int(CHUNK_SIZE as i32);
            let z = base_z + random.next_int(CHUNK_SIZE as i32);
            let Some(y) = config.height.sample_valid(&mut random, stage.min_y, stage.max_y) else {
                continue;
            };
            let biome = stage.biome(x, y, z);
            let allowed = match config.biome_gate {
                OreBiomeGate::Any => true,
                OreBiomeGate::Mountains => is_mountain(biome),
                OreBiomeGate::Dripstone => biome == Biome::DripstoneCaves,
                OreBiomeGate::Badlands => matches!(biome, Biome::Badlands | Biome::WoodedBadlands | Biome::ErodedBadlands),
            };
            if !allowed || !root.applies_to(biome) {
                continue;
            }
            emit_ore_vein(stage, WorldPos::new(x, y, z), config, &mut random);
        }
        return;
    }

    let blob = match root.key {
        "ore_dirt" => Some((BlockId::Dirt, 7, 33, 1, HeightDistribution::Uniform(0, 160))),
        "ore_gravel" => Some((BlockId::Gravel, 14, 33, 1, HeightDistribution::Uniform(-64, 319))),
        "ore_granite_upper" => Some((BlockId::Granite, 1, 64, 6, HeightDistribution::Uniform(64, 128))),
        "ore_granite_lower" => Some((BlockId::Granite, 2, 64, 1, HeightDistribution::Uniform(0, 60))),
        "ore_diorite_upper" => Some((BlockId::Diorite, 1, 64, 6, HeightDistribution::Uniform(64, 128))),
        "ore_diorite_lower" => Some((BlockId::Diorite, 2, 64, 1, HeightDistribution::Uniform(0, 60))),
        "ore_andesite_upper" => Some((BlockId::Andesite, 1, 64, 6, HeightDistribution::Uniform(64, 128))),
        "ore_andesite_lower" => Some((BlockId::Andesite, 2, 64, 1, HeightDistribution::Uniform(0, 60))),
        "ore_tuff" => Some((BlockId::Tuff, 2, 64, 1, HeightDistribution::Uniform(-64, 0))),
        _ => None,
    };
    let Some((block, count, size, rarity, height)) = blob else {
        return;
    };
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    if rarity > 1 && random.next_int(rarity) != 0 {
        return;
    }
    for _ in 0..count {
        if !claim_candidate(candidates) {
            return;
        }
        let Some(y) = height.sample_valid(&mut random, stage.min_y, stage.max_y) else {
            continue;
        };
        let origin = WorldPos::new(
            base_x + random.next_int(CHUNK_SIZE as i32),
            y,
            base_z + random.next_int(CHUNK_SIZE as i32),
        );
        if root_applies_at(stage, root, origin) {
            emit_blob(stage, origin, block, size, &mut random);
        }
    }
}

fn emit_ore_vein(stage: &mut FeatureStage<'_>, origin: WorldPos, config: OreConfig, random: &mut NoiseSeed) {
    let radius = (config.size / 4 + 2).min(8);
    if !stage.intersects_target(origin.x, origin.z, radius) {
        return;
    }
    let angle = random.next_double() * std::f64::consts::PI;
    let span = config.size as f64 / 8.0;
    let x0 = origin.x as f64 + angle.sin() * span;
    let x1 = origin.x as f64 - angle.sin() * span;
    let z0 = origin.z as f64 + angle.cos() * span;
    let z1 = origin.z as f64 - angle.cos() * span;
    let y0 = origin.y as f64 + random.next_int(3) as f64 - 1.0;
    let y1 = origin.y as f64 + random.next_int(3) as f64 - 1.0;
    for step in 0..config.size {
        let t = step as f64 / config.size.max(1) as f64;
        let cx = x0 + (x1 - x0) * t;
        let cy = y0 + (y1 - y0) * t;
        let cz = z0 + (z1 - z0) * t;
        let r = ((std::f64::consts::PI * t).sin() + 1.0)
            * (random.next_double() * config.size as f64 / 16.0 + 1.0)
            / 2.0;
        emit_ellipsoid(stage, cx, cy, cz, r, config);
    }
}

fn emit_ellipsoid(stage: &mut FeatureStage<'_>, cx: f64, cy: f64, cz: f64, radius: f64, config: OreConfig) {
    let min_x = (cx - radius).floor() as i32;
    let max_x = (cx + radius).ceil() as i32;
    let min_y = (cy - radius).floor() as i32;
    let max_y = (cy + radius).ceil() as i32;
    let min_z = (cz - radius).floor() as i32;
    let max_z = (cz + radius).ceil() as i32;
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                let dx = (x as f64 + 0.5 - cx) / radius.max(0.5);
                let dy = (y as f64 + 0.5 - cy) / radius.max(0.5);
                let dz = (z as f64 + 0.5 - cz) / radius.max(0.5);
                if dx * dx + dy * dy + dz * dz <= 1.0 {
                    stage.push(
                        WorldPos::new(x, y, z),
                        config.stone,
                        Replacement::Ore {
                            stone: config.stone,
                            deepslate: config.deepslate,
                            discard_if_exposed: coordinate_chance(
                                stage.world_seed,
                                WorldPos::new(x, y, z),
                                config.discard_chance_on_air_exposure,
                            ),
                        },
                    );
                }
            }
        }
    }
}

fn emit_blob(stage: &mut FeatureStage<'_>, origin: WorldPos, block: BlockId, size: i32, random: &mut NoiseSeed) {
    let radius = (size as f64).cbrt().ceil() as i32 + 1;
    if !stage.intersects_target(origin.x, origin.z, radius) {
        return;
    }
    let rx = radius - random.next_int(2);
    let ry = (radius - 1).max(1);
    let rz = radius - random.next_int(2);
    for dx in -rx..=rx {
        for dy in -ry..=ry {
            for dz in -rz..=rz {
                let distance = (dx * dx) as f64 / (rx * rx).max(1) as f64
                    + (dy * dy) as f64 / (ry * ry).max(1) as f64
                    + (dz * dz) as f64 / (rz * rz).max(1) as f64;
                if distance <= 1.0 {
                    stage.push(
                        WorldPos::new(origin.x + dx, origin.y + dy, origin.z + dz),
                        block,
                        Replacement::BaseStone,
                    );
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum TreeKind {
    Oak,
    Spruce,
    Birch,
    Jungle,
    Acacia,
    DarkOak,
    Cherry,
    Mangrove,
}

fn emit_tree(stage: &mut FeatureStage<'_>, origin: WorldPos, kind: TreeKind, random: &mut NoiseSeed) {
    if !stage.intersects_target(origin.x, origin.z, 5) {
        return;
    }
    let (log, leaves, height, radius): (BlockId, BlockId, i32, i32) = match kind {
        TreeKind::Oak => (BlockId::OakLog, BlockId::OakLeaves, 4 + random.next_int(3), 2),
        TreeKind::Spruce => (BlockId::SpruceLog, BlockId::SpruceLeaves, 6 + random.next_int(4), 3),
        TreeKind::Birch => (BlockId::BirchLog, BlockId::BirchLeaves, 5 + random.next_int(3), 2),
        TreeKind::Jungle => (BlockId::JungleLog, BlockId::JungleLeaves, 7 + random.next_int(5), 3),
        TreeKind::Acacia => (BlockId::AcaciaLog, BlockId::AcaciaLeaves, 5 + random.next_int(3), 3),
        TreeKind::DarkOak => (BlockId::DarkOakLog, BlockId::DarkOakLeaves, 6 + random.next_int(3), 4),
        TreeKind::Cherry => (BlockId::CherryLog, BlockId::CherryLeaves, 5 + random.next_int(3), 4),
        TreeKind::Mangrove => (BlockId::MangroveLog, BlockId::MangroveLeaves, 6 + random.next_int(3), 3),
    };
    let root_y = origin.y + 1;
    let trunk_width = if matches!(kind, TreeKind::DarkOak) { 2 } else { 1 };
    for dx in 0..trunk_width {
        for dz in 0..trunk_width {
            for dy in 0..height {
                stage.push(WorldPos::new(origin.x + dx, root_y + dy, origin.z + dz), log, Replacement::Air);
            }
        }
    }
    let canopy_y = root_y + height - 2;
    let layers = if matches!(kind, TreeKind::Spruce) { 5 } else { 4 };
    for layer in 0..layers {
        let layer_radius: i32 = if matches!(kind, TreeKind::Spruce) {
            (radius - layer / 2).max(0)
        } else if layer == layers - 1 {
            (radius - 1).max(1)
        } else {
            radius
        };
        for dx in -layer_radius..=layer_radius {
            for dz in -layer_radius..=layer_radius {
                if (dx == -layer_radius || dx == layer_radius)
                    && (dz == -layer_radius || dz == layer_radius)
                    && random.next_boolean()
                {
                    continue;
                }
                stage.push(
                    WorldPos::new(origin.x + dx, canopy_y + layer, origin.z + dz),
                    leaves,
                    Replacement::Air,
                );
            }
        }
    }
    if matches!(kind, TreeKind::Jungle | TreeKind::Mangrove) {
        for dy in 1..height {
            if random.next_int(3) == 0 {
                stage.push(WorldPos::new(origin.x + 1, root_y + dy, origin.z), BlockId::Vine, Replacement::Air);
            }
        }
    }
}

fn emit_coral(stage: &mut FeatureStage<'_>, origin: WorldPos, random: &mut NoiseSeed) {
    let blocks = [BlockId::TubeCoralBlock, BlockId::BrainCoralBlock, BlockId::BubbleCoralBlock, BlockId::FireCoralBlock, BlockId::HornCoralBlock];
    let corals = [BlockId::TubeCoral, BlockId::BrainCoral, BlockId::BubbleCoral, BlockId::FireCoral, BlockId::HornCoral];
    let fans = [BlockId::TubeCoralFan, BlockId::BrainCoralFan, BlockId::BubbleCoralFan, BlockId::FireCoralFan, BlockId::HornCoralFan];
    let block = blocks[random.next_int(blocks.len() as i32) as usize];
    let coral_index = random.next_int(corals.len() as i32) as usize;
    let coral = if random.next_boolean() { corals[coral_index] } else { fans[coral_index] };
    for dx in -2_i32..=2 {
        for dz in -2_i32..=2 {
            if dx * dx + dz * dz <= 4 && random.next_int(4) != 0 {
                stage.push(WorldPos::new(origin.x + dx, origin.y, origin.z + dz), block, Replacement::Disk);
                stage.push(WorldPos::new(origin.x + dx, origin.y + 1, origin.z + dz), coral, Replacement::Water);
            }
        }
    }
}

fn emit_geode(stage: &mut FeatureStage<'_>, origin: WorldPos, random: &mut NoiseSeed) {
    let outer = 4 + random.next_int(3);
    if !stage.intersects_target(origin.x, origin.z, outer) {
        return;
    }
    for dx in -outer..=outer {
        for dy in -outer..=outer {
            for dz in -outer..=outer {
                let d2 = dx * dx + dy * dy + dz * dz;
                let block = if d2 > outer * outer {
                    continue;
                } else if d2 >= (outer - 1) * (outer - 1) {
                    // Java places smooth basalt here. Leaving the base block
                    // intact is the explicit unsupported-output behavior.
                    continue;
                } else if d2 >= (outer - 2) * (outer - 2) {
                    BlockId::Calcite
                } else if d2 >= (outer - 3).max(1) * (outer - 3).max(1) {
                    if random.next_int(12) == 0 { BlockId::BuddingAmethyst } else { BlockId::AmethystBlock }
                } else {
                    BlockId::Air
                };
                stage.push(WorldPos::new(origin.x + dx, origin.y + dy, origin.z + dz), block, Replacement::Natural);
            }
        }
    }
    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
        stage.push(WorldPos::new(origin.x + dx, origin.y + dy, origin.z + dz), BlockId::AmethystCluster, Replacement::Air);
    }
}

fn plan_lake(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let chance = if root.key == "lake_lava_underground" { 9 } else { 200 };
    if random.next_int(chance) != 0 || !claim_candidate(candidates) {
        return;
    }
    let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let y = if root.key == "lake_lava_surface" {
        stage.surface_y(x, z)
    } else {
        let Some(y) = HeightDistribution::Uniform(0, stage.max_y - 1)
            .sample_valid(&mut random, stage.min_y, stage.max_y)
        else { return };
        if y > stage.surface_y(x, z) - 5 { return; }
        y
    };
    let origin = WorldPos::new(x, y, z);
    if !root_applies_at(stage, root, origin) || !stage.intersects_target(x, z, 5) {
        return;
    }
    let rx = 3 + random.next_int(3);
    let rz = 3 + random.next_int(3);
    for dx in -rx..=rx {
        for dy in -2..=2 {
            for dz in -rz..=rz {
                let distance = (dx * dx) as f64 / (rx * rx) as f64
                    + (dy * dy) as f64 / 4.0
                    + (dz * dz) as f64 / (rz * rz) as f64;
                if distance <= 1.0 {
                    stage.push(
                        WorldPos::new(x + dx, y + dy, z + dz),
                        if dy <= 0 { BlockId::Lava } else { BlockId::Air },
                        if dy <= 0 { Replacement::LakeFluid } else { Replacement::LakeAir },
                    );
                }
            }
        }
    }
}

fn plan_geode_root(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    if random.next_int(24) != 0 || !claim_candidate(candidates) { return; }
    let origin = WorldPos::new(
        owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
        HeightDistribution::Uniform(stage.min_y + 6, 30).sample(&mut random, stage.min_y, stage.max_y),
        owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
    );
    if root_applies_at(stage, root, origin) { emit_geode(stage, origin, &mut random); }
}

fn plan_iceberg(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let chance = if root.key == "iceberg_blue" { 200 } else { 16 };
    if random.next_int(chance) != 0 || !claim_candidate(candidates) { return; }
    let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let origin = WorldPos::new(x, 63, z);
    if !root_applies_at(stage, root, origin) || !stage.intersects_target(x, z, 9) { return; }
    let block = if root.key == "iceberg_blue" { BlockId::BlueIce } else { BlockId::PackedIce };
    let height = 7 + random.next_int(9);
    let radius = 4 + random.next_int(5);
    for dy in -3..height {
        let layer_radius = if dy < 0 { radius - 1 } else { (radius * (height - dy) / height).max(1) };
        for dx in -layer_radius..=layer_radius {
            for dz in -layer_radius..=layer_radius {
                if dx * dx + dz * dz <= layer_radius * layer_radius
                    && coordinate_chance(stage.world_seed ^ index as u64, WorldPos::new(x + dx, 63 + dy, z + dz), 0.82)
                {
                    stage.push(WorldPos::new(x + dx, 63 + dy, z + dz), block, Replacement::Natural);
                }
            }
        }
    }
}

fn plan_forest_rock(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    for _ in 0..2 {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let y = stage.surface_y(x, z) + 1;
        let origin = WorldPos::new(x, y, z);
        if !root_applies_at(stage, root, origin) { continue; }
        let radius = 1 + random.next_int(2);
        for dx in -radius..=radius {
            for dy in 0..=radius {
                for dz in -radius..=radius {
                    if dx * dx + dz * dz + dy * dy <= radius * radius + 1 {
                        stage.push(WorldPos::new(x + dx, y + dy, z + dz), BlockId::MossyCobblestone, Replacement::Air);
                    }
                }
            }
        }
    }
}

fn plan_large_dripstone(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let count = 10 + random.next_int(39);
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let origin = WorldPos::new(
            owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
            HeightDistribution::Uniform(stage.min_y, 256).sample(&mut random, stage.min_y, stage.max_y),
            owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
        );
        if !root_applies_at(stage, root, origin) { continue; }
        let height = 3 + random.next_int(9);
        let radius = 1 + random.next_int(3);
        for dy in 0..height {
            let r = (radius * (height - dy) / height).max(0);
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx * dx + dz * dz <= r * r {
                        stage.push(WorldPos::new(origin.x + dx, origin.y + dy, origin.z + dz), BlockId::DripstoneBlock, if dy == 0 { Replacement::CaveFloor } else { Replacement::Air });
                    }
                }
            }
        }
    }
}

fn plan_fossil(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    if random.next_int(64) != 0 || !claim_candidate(candidates) { return; }
    let height = if root.key == "fossil_lower" {
        HeightDistribution::Uniform(stage.min_y, -8)
    } else {
        HeightDistribution::Uniform(0, stage.max_y - 1)
    };
    let Some(y) = height.sample_valid(&mut random, stage.min_y, stage.max_y) else { return; };
    let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let origin = WorldPos::new(x, y, z);
    if !root_applies_at(stage, root, origin) || y >= stage.surface_y(x, z) { return; }
    let overlay = if root.key == "fossil_lower" { BlockId::DiamondOre } else { BlockId::CoalOre };
    for dx in -4..=4 {
        stage.push(WorldPos::new(x + dx, y, z), BlockId::BoneBlock, Replacement::Natural);
        if dx.abs() <= 3 {
            stage.push(WorldPos::new(x + dx, y + 2, z), BlockId::BoneBlock, Replacement::Natural);
        }
        if dx % 2 == 0 {
            stage.push(WorldPos::new(x + dx, y + 1, z + 1), overlay, Replacement::Natural);
            stage.push(WorldPos::new(x + dx, y + 1, z - 1), BlockId::BoneBlock, Replacement::Natural);
        }
    }
}

fn plan_monster_room(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let (count, height) = if root.key == "monster_room_deep" {
        (4, HeightDistribution::Uniform(stage.min_y + 6, -1))
    } else {
        (10, HeightDistribution::Uniform(0, stage.max_y - 1))
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let Some(y) = height.sample_valid(&mut random, stage.min_y, stage.max_y) else { continue; };
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let origin = WorldPos::new(x, y, z);
        if random.next_int(8) != 0 || !root_applies_at(stage, root, origin)
            || y >= stage.surface_y(x, z) - 5 || !stage.intersects_target(x, z, 4) { continue; }
        let rx = 2 + random.next_int(2);
        let rz = 2 + random.next_int(2);
        for dx in -rx..=rx {
            for dy in -1..=3 {
                for dz in -rz..=rz {
                    let wall = dx.abs() == rx || dz.abs() == rz || dy == -1 || dy == 3;
                    let block = if wall {
                        if dy == -1 && random.next_int(4) == 0 { BlockId::MossyCobblestone } else { BlockId::Cobblestone }
                    } else { BlockId::Air };
                    stage.push(WorldPos::new(x + dx, y + dy, z + dz), block, Replacement::Natural);
                }
            }
        }
        stage.push(origin, BlockId::Spawner, Replacement::Natural);
        if random.next_boolean() { stage.push(WorldPos::new(x + rx - 1, y, z), BlockId::Chest, Replacement::Air); }
    }
}

fn plan_ice_feature(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let count = match root.key {
        "ice_spike" => 3,
        "ice_patch" => 2,
        _ => random.next_int(20),
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let y = if root.key == "blue_ice" { 30 + random.next_int(32) } else { stage.surface_y(x, z) };
        let origin = WorldPos::new(x, y, z);
        if !root_applies_at(stage, root, origin) { continue; }
        match root.key {
            "ice_spike" => {
                let height = 7 + random.next_int(5);
                for dy in 0..height {
                    let radius = ((height - dy + 3) / 4).max(1);
                    for dx in -radius..=radius { for dz in -radius..=radius {
                        if dx * dx + dz * dz <= radius * radius {
                            stage.push(WorldPos::new(x + dx, y + dy, z + dz), BlockId::PackedIce, Replacement::Natural);
                        }
                    }}
                }
            }
            "ice_patch" => {
                let radius = 2 + random.next_int(2);
                for dx in -radius..=radius { for dz in -radius..=radius {
                    if dx * dx + dz * dz <= radius * radius {
                        stage.push(WorldPos::new(x + dx, y - 1, z + dz), BlockId::PackedIce, Replacement::Disk);
                    }
                }}
            }
            _ => {
                for dx in -1_i32..=1 { for dy in -1_i32..=1 { for dz in -1_i32..=1 {
                    if dx * dx + dy * dy + dz * dz <= 2 {
                        stage.push(WorldPos::new(x + dx, y + dy, z + dz), BlockId::BlueIce, Replacement::Natural);
                    }
                }}}
            }
        }
    }
}

fn plan_desert_well_root(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    if random.next_int(1_000) != 0 || !claim_candidate(candidates) { return; }
    let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
    let y = stage.surface_y(x, z);
    if !root_applies_at(stage, root, WorldPos::new(x, y, z)) || !stage.intersects_target(x, z, 2) { return; }
    for dy in -2..=0 { for dx in -2..=2 { for dz in -2..=2 {
        stage.push(WorldPos::new(x + dx, y + dy, z + dz), BlockId::Sandstone, Replacement::Natural);
    }}}
    for (dx, dz) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
        stage.push(WorldPos::new(x + dx, y, z + dz), BlockId::Water, Replacement::Natural);
    }
    for dy in 1..=3 { for (dx, dz) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
        stage.push(WorldPos::new(x + dx, y + dy, z + dz), BlockId::Sandstone, Replacement::Air);
    }}
    stage.push(WorldPos::new(x, y + 4, z), BlockId::Sandstone, Replacement::Air);
}

fn plan_underwater_magma(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let count = 44 + random.next_int(9);
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        // 26.2 placement order is in_square followed by a uniform height from
        // the world bottom through absolute Y=256. Only attempts whose scan
        // origin lies within five blocks below the ocean floor can succeed.
        let attempt_y = stage.min_y + random.next_int(256 - stage.min_y + 1);
        if !stage.intersects_target(x, z, 1) { continue; }
        let floor = stage.surface_y(x, z);
        let pos = WorldPos::new(x, attempt_y, z);
        if !root_applies_at(stage, root, pos)
            || attempt_y > floor - 2
            || attempt_y < floor - 5
        {
            continue;
        }
        for dx in -1..=1 { for dy in -1..=1 { for dz in -1..=1 {
            if random.next_float() < 0.5 {
                stage.push(
                    WorldPos::new(x + dx, floor + dy, z + dz),
                    BlockId::MagmaBlock,
                    Replacement::UnderwaterMagma,
                );
            }
        }}}
    }
}

fn plan_disk(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let (block, count, max_radius) = match root.key {
        "disk_sand" => (BlockId::Sand, 3, 6),
        "disk_grass" => (BlockId::GrassBlock, 1, 3),
        _ => (BlockId::Gravel, 1, 5),
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let y = stage.surface_y(x, z);
        let origin = WorldPos::new(x, y, z);
        if !root_applies_at(stage, root, origin) { continue; }
        let radius = 2 + random.next_int(max_radius - 1);
        for dx in -radius..=radius { for dz in -radius..=radius {
            if dx * dx + dz * dz <= radius * radius {
                for dy in -2..=0 {
                    stage.push(WorldPos::new(x + dx, y + dy, z + dz), block, Replacement::UnderwaterDisk);
                }
            }
        }}
    }
}

fn plan_cave_root(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let count = match root.key {
        "glow_lichen" => 104 + random.next_int(54),
        "dripstone_cluster" => 48 + random.next_int(49),
        "lush_caves_ceiling_vegetation" | "lush_caves_vegetation" => 125,
        "cave_vines" => 188,
        "rooted_azalea_tree" => 1 + random.next_int(2),
        "classic_vines_cave_feature" => 256,
        _ => 25,
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let pos = WorldPos::new(
            owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
            HeightDistribution::Uniform(stage.min_y, 256).sample(&mut random, stage.min_y, stage.max_y),
            owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
        );
        let radius = if root.key == "dripstone_cluster" { 8 } else { 3 };
        if !stage.intersects_target(pos.x, pos.z, radius) { continue; }
        if !root_applies_at(stage, root, pos) { continue; }
        match root.key {
            "glow_lichen" => stage.push(pos, BlockId::GlowLichen, if random.next_boolean() { Replacement::CaveFloor } else { Replacement::CaveCeiling }),
            "dripstone_cluster" => {
                let radius = 2 + random.next_int(7);
                for dx in -radius..=radius { for dz in -radius..=radius {
                    if dx * dx + dz * dz <= radius * radius {
                        stage.push(WorldPos::new(pos.x + dx, pos.y, pos.z + dz), BlockId::DripstoneBlock, Replacement::CaveFloor);
                    }
                }}
            }
            "lush_caves_ceiling_vegetation" => {
                stage.push(pos, BlockId::MossBlock, Replacement::CaveCeiling);
                if random.next_int(12) == 0 { stage.push(WorldPos::new(pos.x, pos.y - 1, pos.z), BlockId::HangingRoots, Replacement::Air); }
            }
            "lush_caves_vegetation" => {
                stage.push(pos, BlockId::MossBlock, Replacement::CaveFloor);
                let plant = match random.next_int(5) { 0 => BlockId::Azalea, 1 => BlockId::FloweringAzalea, 2 => BlockId::MossCarpet, _ => BlockId::Grass };
                stage.push(WorldPos::new(pos.x, pos.y + 1, pos.z), plant, Replacement::Air);
            }
            "rooted_azalea_tree" => emit_root_system(stage, pos, &mut random),
            "spore_blossom" => stage.push(pos, BlockId::SporeBlossom, Replacement::CaveCeiling),
            "cave_vines" => stage.push(pos, BlockId::Vine, Replacement::CaveCeiling),
            _ => stage.push(pos, BlockId::Vine, Replacement::CaveCeiling),
        }
    }
}

fn emit_root_system(stage: &mut FeatureStage<'_>, origin: WorldPos, random: &mut NoiseSeed) {
    for dy in 0..6 {
        stage.push(WorldPos::new(origin.x, origin.y + dy, origin.z), BlockId::RootedDirt, Replacement::Natural);
    }
    for _ in 0..20 {
        let dx = random.next_int(7) - 3;
        let dz = random.next_int(7) - 3;
        stage.push(WorldPos::new(origin.x + dx, origin.y - random.next_int(3), origin.z + dz), BlockId::HangingRoots, Replacement::Air);
    }
    stage.push(WorldPos::new(origin.x, origin.y + 6, origin.z), BlockId::Azalea, Replacement::Air);
}

fn plan_spring(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let (block, count, frozen) = match root.key {
        "spring_water" => (BlockId::Water, 25, false),
        "spring_lava_frozen" => (BlockId::Lava, 20, true),
        _ => (BlockId::Lava, 20, false),
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let y = if root.key == "spring_water" {
            HeightDistribution::Uniform(stage.min_y, 192).sample(&mut random, stage.min_y, stage.max_y)
        } else {
            let inner = random.next_int((stage.max_y - stage.min_y - 8).max(1)) + 8;
            stage.min_y + random.next_int(inner.max(1))
        };
        let pos = WorldPos::new(
            owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32), y,
            owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32),
        );
        if root_applies_at(stage, root, pos) {
            stage.push(pos, block, Replacement::Spring { frozen });
        }
    }
}

fn plan_tree_root(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let (count, primary) = match root.key {
        "trees_birch" | "birch_tall" => (count_extra(&mut random, 10), TreeKind::Birch),
        "trees_taiga" | "trees_grove" | "trees_old_growth_pine_taiga"
        | "trees_old_growth_spruce_taiga" => (count_extra(&mut random, 10), TreeKind::Spruce),
        "trees_snowy" => (count_extra(&mut random, 0), TreeKind::Spruce),
        "trees_jungle" => (count_extra(&mut random, 50), TreeKind::Jungle),
        "trees_sparse_jungle" => (count_extra(&mut random, 2), TreeKind::Jungle),
        "trees_savanna" => (count_extra(&mut random, 1), TreeKind::Acacia),
        "trees_windswept_savanna" => (count_extra(&mut random, 2), TreeKind::Acacia),
        "trees_cherry" => (count_extra(&mut random, 10), TreeKind::Cherry),
        "trees_mangrove" => (25, TreeKind::Mangrove),
        "trees_badlands" => (count_extra(&mut random, 5), TreeKind::Oak),
        "trees_flower_forest" => (count_extra(&mut random, 6), TreeKind::Birch),
        "trees_birch_and_oak_leaf_litter" => (count_extra(&mut random, 10), TreeKind::Oak),
        "trees_windswept_forest" => (count_extra(&mut random, 3), TreeKind::Birch),
        "trees_windswept_hills" | "trees_water" => (count_extra(&mut random, 0), TreeKind::Oak),
        "trees_plains" => ((random.next_int(20) == 0) as i32, TreeKind::Oak),
        "trees_meadow" => ((random.next_int(100) == 0) as i32, TreeKind::Birch),
        "trees_swamp" => (count_extra(&mut random, 2), TreeKind::Oak),
        _ => (1, TreeKind::Oak),
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        if !stage.intersects_target(x, z, 5) { continue; }
        let y = stage.surface_y(x, z);
        let origin = WorldPos::new(x, y, z);
        if !root_applies_at(stage, root, origin) { continue; }
        let (kind, fallen) = if root.key == "trees_birch_and_oak_leaf_litter" {
            // Configured random_selector order is significant: each failed
            // entry consumes its own float before the default oak is chosen.
            if random.next_float() < 0.0025 {
                (TreeKind::Birch, true)
            } else if random.next_float() < 0.2 {
                (TreeKind::Birch, false)
            } else if random.next_float() < 0.1 {
                (TreeKind::Oak, false)
            } else if random.next_float() < 0.0125 {
                (TreeKind::Oak, true)
            } else {
                (TreeKind::Oak, false)
            }
        } else {
            (primary, false)
        };
        if fallen {
            emit_fallen_tree(stage, origin, kind, &mut random);
        } else {
            emit_tree(stage, origin, kind, &mut random);
        }
    }
}

fn count_extra(random: &mut NoiseSeed, base: i32) -> i32 {
    base + (random.next_int(10) == 0) as i32
}

fn emit_fallen_tree(stage: &mut FeatureStage<'_>, origin: WorldPos, kind: TreeKind, random: &mut NoiseSeed) {
    let (block, min, span) = match kind {
        TreeKind::Oak => (BlockId::OakLog, 4, 4),
        TreeKind::Birch => (BlockId::BirchLog, 5, 4),
        TreeKind::Spruce => (BlockId::SpruceLog, 6, 5),
        _ => (BlockId::JungleLog, 4, 8),
    };
    let length = min + random.next_int(span);
    let along_x = random.next_boolean();
    stage.push(WorldPos::new(origin.x, origin.y + 1, origin.z), block, Replacement::Air);
    for offset in 0..length {
        let pos = WorldPos::new(origin.x + if along_x { offset + 1 } else { 0 }, origin.y + 1, origin.z + if along_x { 0 } else { offset + 1 });
        stage.push(pos, block, Replacement::Air);
        if random.next_int(10) == 0 { stage.push(WorldPos::new(pos.x, pos.y + 1, pos.z), if random.next_boolean() { BlockId::RedMushroom } else { BlockId::BrownMushroom }, Replacement::Air); }
    }
}

fn plan_large_vegetation(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let count = if root.key == "dark_forest_vegetation" { 16 } else { 1 };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        if !stage.intersects_target(x, z, 4) { continue; }
        let origin = WorldPos::new(x, stage.surface_y(x, z), z);
        if !root_applies_at(stage, root, origin) { continue; }
        if root.key == "mushroom_island_vegetation" {
            emit_huge_mushroom(stage, origin, random.next_boolean(), &mut random);
            continue;
        }
        let roll = random.next_float();
        if roll < 0.025 {
            emit_huge_mushroom(stage, origin, false, &mut random);
        } else if roll < 0.075 {
            emit_huge_mushroom(stage, origin, true, &mut random);
        } else if roll < 0.7416667 {
            emit_tree(stage, origin, TreeKind::DarkOak, &mut random);
        } else if roll < 0.7441667 {
            emit_fallen_tree(stage, origin, TreeKind::Birch, &mut random);
        } else if roll < 0.9441667 {
            emit_tree(stage, origin, TreeKind::Birch, &mut random);
        } else if roll < 0.9566667 {
            emit_fallen_tree(stage, origin, TreeKind::Oak, &mut random);
        } else {
            emit_tree(stage, origin, TreeKind::Oak, &mut random);
        }
    }
}

fn emit_huge_mushroom(stage: &mut FeatureStage<'_>, origin: WorldPos, red: bool, random: &mut NoiseSeed) {
    let height = 4 + random.next_int(3);
    for dy in 1..=height { stage.push(WorldPos::new(origin.x, origin.y + dy, origin.z), BlockId::MushroomStem, Replacement::Air); }
    let cap = if red { BlockId::RedMushroomBlock } else { BlockId::BrownMushroomBlock };
    let radius: i32 = if red { 2 } else { 3 };
    for dx in -radius..=radius { for dz in -radius..=radius {
        if !red || dx.abs() + dz.abs() <= radius + 1 {
            stage.push(WorldPos::new(origin.x + dx, origin.y + height, origin.z + dz), cap, Replacement::Air);
        }
    }}
}

fn plan_aquatic_root(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    let count = match root.key {
        "warm_ocean_vegetation" => 2,
        "sea_pickle" => if random.next_int(16) == 0 { 4 } else { 0 },
        key if key.starts_with("kelp_") => 20 + random.next_int(30),
        "seagrass_deep" | "seagrass_deep_cold" | "seagrass_deep_warm" => 80,
        "seagrass_swamp" => 64,
        _ => 48,
    };
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        if !stage.intersects_target(x, z, 2) { continue; }
        let floor = stage.surface_y(x, z);
        let origin = WorldPos::new(x, floor, z);
        if !root_applies_at(stage, root, origin) { continue; }
        match root.key {
            "warm_ocean_vegetation" => emit_coral(stage, origin, &mut random),
            "sea_pickle" => stage.push(WorldPos::new(x, floor + 1, z), BlockId::SeaPickle, Replacement::Water),
            key if key.starts_with("kelp_") => {
                let height = 2 + random.next_int(8);
                for dy in 1..=height { stage.push(WorldPos::new(x, floor + dy, z), if dy == height { BlockId::Kelp } else { BlockId::KelpPlant }, Replacement::Water); }
            }
            _ => stage.push(WorldPos::new(x, floor + 1, z), BlockId::Seagrass, Replacement::Water),
        }
    }
}

fn plan_patch(
    stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition,
    index: i32, candidates: &mut usize,
) {
    let mut random = stage.feature_rng(owner_x, owner_z, root.step as i32, index);
    if root.key.starts_with("flower_") || root.key == "forest_flowers" {
        plan_flower_patch(stage, owner_x, owner_z, root, &mut random, candidates);
        return;
    }
    if is_clustered_ground_patch(root.key) {
        plan_clustered_ground_patch(stage, owner_x, owner_z, root, &mut random, candidates);
        return;
    }
    let (chance, count, block) = match root.key {
        "patch_pumpkin" => (300, 96, BlockId::Pumpkin),
        "patch_melon" => (6, 64, BlockId::Melon),
        "patch_melon_sparse" => (64, 64, BlockId::Melon),
        "patch_cactus_desert" => (6, 10, BlockId::Cactus),
        "patch_cactus_decorated" => (13, 10, BlockId::Cactus),
        "patch_sugar_cane" => (6, 20, BlockId::SugarCane),
        "patch_sugar_cane_desert" => (1, 20, BlockId::SugarCane),
        "patch_sugar_cane_swamp" => (3, 20, BlockId::SugarCane),
        "patch_sunflower" => (3, 64, BlockId::Dandelion),
        "patch_waterlily" => (1, 10, BlockId::LilyPad),
        "patch_large_fern" => (5, 64, BlockId::Fern),
        "patch_berry_common" | "patch_berry_rare" => (if root.key.ends_with("rare") { 24 } else { 12 }, 64, BlockId::SweetBerryBush),
        key if key.starts_with("brown_mushroom_") => (8, 64, BlockId::BrownMushroom),
        key if key.starts_with("red_mushroom_") => (8, 64, BlockId::RedMushroom),
        key if key.starts_with("patch_dead_bush") => (1, 32, BlockId::DeadBush),
        "bamboo" => (1, 80, BlockId::Bamboo),
        "bamboo_light" => (4, 32, BlockId::Bamboo),
        "bamboo_vegetation" => (1, 30, BlockId::Bamboo),
        "vines" => (1, 127, BlockId::Vine),
        _ => (1, 32, BlockId::Grass),
    };
    if chance > 1 && random.next_int(chance) != 0 { return; }
    for _ in 0..count {
        if !claim_candidate(candidates) { return; }
        let base_x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let base_z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let spread = if matches!(block, BlockId::Pumpkin | BlockId::Melon | BlockId::Cactus) { 7 } else if block == BlockId::SugarCane { 4 } else { 0 };
        let x = base_x + if spread == 0 { 0 } else { random.next_int(spread * 2 + 1) - spread };
        let z = base_z + if spread == 0 { 0 } else { random.next_int(spread * 2 + 1) - spread };
        if !stage.intersects_target(x, z, 2) { continue; }
        let y = if root.key == "vines" { 64 + random.next_int(37) } else { stage.surface_y(x, z) + 1 };
        let pos = WorldPos::new(x, y, z);
        if !root_applies_at(stage, root, pos) { continue; }
        let height = match block {
            BlockId::Cactus => 1 + random.next_int(3),
            BlockId::SugarCane => 2 + random.next_int(3),
            BlockId::Bamboo => 5 + random.next_int(8),
            _ => 1,
        };
        if root.key == "bamboo" && random.next_float() < 0.2 {
            for dx in -2..=2 { for dz in -2..=2 {
                if dx * dx + dz * dz <= 4 { stage.push(WorldPos::new(x + dx, y - 1, z + dz), BlockId::Podzol, Replacement::Disk); }
            }}
        }
        for dy in 0..height {
            let replacement = match block {
                BlockId::Pumpkin | BlockId::Melon => Replacement::PlantOnGrass,
                BlockId::Cactus => Replacement::Cactus,
                BlockId::SugarCane => Replacement::SugarCane,
                _ => Replacement::Air,
            };
            stage.push(WorldPos::new(x, y + dy, z), block, replacement);
        }
    }
}

fn is_clustered_ground_patch(key: &str) -> bool {
    matches!(
        key,
        "patch_grass_plain"
            | "patch_grass_meadow"
            | "patch_grass_forest"
            | "patch_grass_badlands"
            | "patch_grass_savanna"
            | "patch_grass_normal"
            | "patch_grass_taiga_2"
            | "patch_grass_taiga"
            | "patch_grass_jungle"
            | "patch_tall_grass_2"
            | "patch_tall_grass"
            | "patch_large_fern"
            | "brown_mushroom_normal"
            | "red_mushroom_normal"
            | "brown_mushroom_taiga"
            | "red_mushroom_taiga"
            | "brown_mushroom_old_growth"
            | "red_mushroom_old_growth"
            | "brown_mushroom_swamp"
            | "red_mushroom_swamp"
    )
}

fn noise_threshold_count(owner_x: i32, owner_z: i32, below: i32, above: i32) -> i32 {
    let x = owner_x.wrapping_mul(CHUNK_SIZE as i32);
    let z = owner_z.wrapping_mul(CHUNK_SIZE as i32);
    if crate::world::world_gen::surface::biome_info_noise(x, z) < -0.8 {
        below
    } else {
        above
    }
}

fn plan_clustered_ground_patch(
    stage: &mut FeatureStage<'_>,
    owner_x: i32,
    owner_z: i32,
    root: RootDefinition,
    random: &mut NoiseSeed,
    candidates: &mut usize,
) {
    // Keep Java's modifier pipeline intact: the first count/rarity modifiers
    // choose patch origins, then the configured patch makes clustered
    // triangular offsets around each origin. Sampling a fresh surface height
    // for every inner attempt turns almost every try into a success and was
    // the source of Vibecraft's uniformly carpeted terrain.
    let (outer_count, rarity, tries, block, replacement) = match root.key {
        "patch_grass_plain" => (
            noise_threshold_count(owner_x, owner_z, 5, 10), 1, 32,
            BlockId::Grass, Replacement::PlantOnDirt,
        ),
        "patch_grass_meadow" => (
            noise_threshold_count(owner_x, owner_z, 5, 10), 1, 16,
            BlockId::Grass, Replacement::PlantOnDirt,
        ),
        "patch_grass_forest" => (2, 1, 32, BlockId::Grass, Replacement::PlantOnDirt),
        "patch_grass_badlands" | "patch_grass_taiga_2" => {
            (1, 1, 32, BlockId::Grass, Replacement::PlantOnDirt)
        }
        "patch_grass_savanna" => (20, 1, 32, BlockId::Grass, Replacement::PlantOnDirt),
        "patch_grass_normal" => (5, 1, 32, BlockId::Grass, Replacement::PlantOnDirt),
        "patch_grass_taiga" => (7, 1, 32, BlockId::Fern, Replacement::PlantOnDirt),
        "patch_grass_jungle" => (25, 1, 32, BlockId::Grass, Replacement::PlantOnDirt),
        "patch_tall_grass_2" => (
            noise_threshold_count(owner_x, owner_z, 0, 7), 32, 96,
            BlockId::Grass, Replacement::PlantOnDirt,
        ),
        "patch_tall_grass" => (1, 5, 96, BlockId::Grass, Replacement::PlantOnDirt),
        "patch_large_fern" => (1, 5, 96, BlockId::Fern, Replacement::PlantOnDirt),
        "brown_mushroom_normal" => (1, 256, 96, BlockId::BrownMushroom, Replacement::Mushroom),
        "red_mushroom_normal" => (1, 512, 96, BlockId::RedMushroom, Replacement::Mushroom),
        "brown_mushroom_taiga" => (1, 4, 96, BlockId::BrownMushroom, Replacement::Mushroom),
        "red_mushroom_taiga" => (1, 256, 96, BlockId::RedMushroom, Replacement::Mushroom),
        "brown_mushroom_old_growth" => (3, 4, 96, BlockId::BrownMushroom, Replacement::Mushroom),
        "red_mushroom_old_growth" => (1, 171, 96, BlockId::RedMushroom, Replacement::Mushroom),
        "brown_mushroom_swamp" => (2, 1, 96, BlockId::BrownMushroom, Replacement::Mushroom),
        "red_mushroom_swamp" => (1, 64, 96, BlockId::RedMushroom, Replacement::Mushroom),
        _ => return,
    };

    for _ in 0..outer_count {
        // RarityFilter follows the initial CountPlacement in these 26.2
        // registrations, so each prospective origin receives its own roll.
        if rarity > 1 && random.next_int(rarity) != 0 {
            continue;
        }
        let origin_x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let origin_z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let origin_y = stage.surface_y(origin_x, origin_z) + 1;
        if !root_applies_at(stage, root, WorldPos::new(origin_x, origin_y, origin_z)) {
            continue;
        }
        for _ in 0..tries {
            if !claim_candidate(candidates) {
                return;
            }
            let x = origin_x + trapezoid_offset(random, 7);
            let y = origin_y + trapezoid_offset(random, 3);
            let z = origin_z + trapezoid_offset(random, 7);
            if stage.intersects_target(x, z, 0) {
                stage.push(WorldPos::new(x, y, z), block, replacement);
            }
        }
    }
}

fn plan_flower_patch(
    stage: &mut FeatureStage<'_>,
    owner_x: i32,
    owner_z: i32,
    root: RootDefinition,
    random: &mut NoiseSeed,
    candidates: &mut usize,
) {
    // These are the outer placed-feature modifiers followed by the configured
    // random-patch attempt count. Keeping the two levels separate matters:
    // ordinary flowers are rare clustered patches, not dozens of independent
    // guaranteed plants spread uniformly through every chunk.
    let (outer_count, rarity, tries, xz_spread, y_spread) = match root.key {
        "flower_default" | "flower_swamp" => (1, 32, 64, 7, 3),
        "flower_warm" => (1, 16, 64, 7, 3),
        "flower_plains" => (
            noise_threshold_count(owner_x, owner_z, 15, 4), 32, 64, 6, 2,
        ),
        "flower_flower_forest" => (3, 2, 96, 6, 2),
        "flower_meadow" | "flower_cherry" => (1, 1, 96, 6, 2),
        _ => (1, 32, 64, 7, 3),
    };
    for _ in 0..outer_count {
        if rarity > 1 && random.next_int(rarity) != 0 {
            continue;
        }
        let origin_x = owner_x * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let origin_z = owner_z * CHUNK_SIZE as i32 + random.next_int(CHUNK_SIZE as i32);
        let origin_y = stage.surface_y(origin_x, origin_z) + 1;
        if !root_applies_at(stage, root, WorldPos::new(origin_x, origin_y, origin_z)) {
            continue;
        }
        for _ in 0..tries {
            if !claim_candidate(candidates) {
                return;
            }
            let x = origin_x + trapezoid_offset(random, xz_spread);
            let y = origin_y + trapezoid_offset(random, y_spread);
            let z = origin_z + trapezoid_offset(random, xz_spread);
            if !stage.intersects_target(x, z, 0) {
                continue;
            }
            let pos = WorldPos::new(x, y, z);
            if root_applies_at(stage, root, pos) {
                stage.push(pos, flower_block(root.key, random), Replacement::PlantOnDirt);
            }
        }
    }
}

fn trapezoid_offset(random: &mut NoiseSeed, spread: i32) -> i32 {
    if spread == 0 {
        0
    } else {
        random.next_int(spread + 1) + random.next_int(spread + 1) - spread
    }
}

fn flower_block(key: &str, random: &mut NoiseSeed) -> BlockId {
    // configured_feature/flower_default uses weights poppy=2, dandelion=1.
    const DEFAULT: [BlockId; 3] = [
        BlockId::Poppy,
        BlockId::Poppy,
        BlockId::Dandelion,
    ];
    const FOREST: [BlockId; 11] = [
        BlockId::Dandelion,
        BlockId::Poppy,
        BlockId::Allium,
        BlockId::AzureBluet,
        BlockId::RedTulip,
        BlockId::OrangeTulip,
        BlockId::WhiteTulip,
        BlockId::PinkTulip,
        BlockId::OxeyeDaisy,
        BlockId::Cornflower,
        BlockId::LilyOfTheValley,
    ];
    const SWAMP: [BlockId; 1] = [BlockId::BlueOrchid];
    // DualNoiseProvider changes spatial grouping, but its 26.2 state list is
    // exact and contains two grass entries plus these six flowers.
    const MEADOW: [BlockId; 8] = [
        BlockId::Grass,
        BlockId::Allium,
        BlockId::Poppy,
        BlockId::AzureBluet,
        BlockId::Dandelion,
        BlockId::Cornflower,
        BlockId::OxeyeDaisy,
        BlockId::Grass,
    ];
    let choices: &[BlockId] = if key == "flower_swamp" {
        &SWAMP
    } else if key == "flower_meadow" {
        &MEADOW
    } else if matches!(key, "flower_flower_forest" | "flower_forest_flowers" | "forest_flowers") {
        &FOREST
    } else {
        &DEFAULT
    };
    choices[random.next_int(choices.len() as i32) as usize]
}

fn plan_freeze_root(stage: &mut FeatureStage<'_>, owner_x: i32, owner_z: i32, root: RootDefinition) {
    if (owner_x, owner_z) != (stage.target_x, stage.target_z) { return; }
    let base_x = owner_x * CHUNK_SIZE as i32;
    let base_z = owner_z * CHUNK_SIZE as i32;
    for dx in 0..CHUNK_SIZE as i32 { for dz in 0..CHUNK_SIZE as i32 {
        let x = base_x + dx;
        let z = base_z + dz;
        let y = stage.surface_y(x, z);
        let biome = stage.biome(x, y, z);
        if root.applies_to(biome) && is_cold(biome) {
            stage.push(WorldPos::new(x, y, z), BlockId::Ice, Replacement::Water);
            stage.push(WorldPos::new(x, y + 1, z), BlockId::Snow, Replacement::Air);
        }
    }}
}

fn replacement_matches(
    replacement: Replacement,
    current: BlockId,
    chunk: &Chunk,
    x: usize,
    y: usize,
    z: usize,
) -> bool {
    match replacement {
        Replacement::Air => current == BlockId::Air,
        Replacement::Water => current == BlockId::Water,
        // Java LakeFeature rejects a candidate when the upper cavity boundary
        // intersects liquid and requires a solid lower boundary. Preserve
        // those safety properties even though this geometry path uses a
        // simpler ellipsoid than Java's full boolean mask.
        Replacement::LakeAir => current == BlockId::Air,
        Replacement::LakeFluid => {
            is_solid_natural(current)
                && (y + 1..=(y + 5).min(CHUNK_HEIGHT - 1))
                    .all(|above| chunk.get_block(x, above, z).id != BlockId::Water)
        }
        Replacement::BaseStone => is_base_stone(current),
        Replacement::Ore { discard_if_exposed, .. } => {
            is_ore_replaceable(current)
                && (!discard_if_exposed || !is_exposed_to_air(chunk, x, y, z))
        }
        Replacement::Disk => matches!(
            current,
            BlockId::Dirt | BlockId::GrassBlock | BlockId::Sand | BlockId::Gravel | BlockId::Stone
        ),
        Replacement::UnderwaterDisk => {
            matches!(current, BlockId::Dirt | BlockId::GrassBlock)
                && y + 1 < CHUNK_HEIGHT
                && chunk.get_block(x, y + 1, z).id == BlockId::Water
        }
        Replacement::UnderwaterMagma => {
            if x == 0 || x + 1 == CHUNK_SIZE || y == 0 || z == 0 || z + 1 == CHUNK_SIZE {
                return false;
            }
            is_solid_natural(current)
                && is_solid_natural(chunk.get_block(x, y - 1, z).id)
                // UnderwaterMagmaFeature first requires its scan origin to be
                // water and finds the floor of that water column. The emitted
                // cube reaches only one block below that floor, so every valid
                // result has water one or two blocks above it.
                && (chunk.get_block(x, y + 1, z).id == BlockId::Water
                    || (y + 2 < CHUNK_HEIGHT
                        && chunk.get_block(x, y + 2, z).id == BlockId::Water))
                && [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
                    .into_iter()
                    .all(|(x, z)| is_solid_natural(chunk.get_block(x, y, z).id))
        }
        Replacement::PlantOnGrass => {
            current == BlockId::Air
                && y > 0
                && chunk.get_block(x, y - 1, z).id == BlockId::GrassBlock
        }
        Replacement::PlantOnDirt => {
            current == BlockId::Air
                && y > 0
                && matches!(
                    chunk.get_block(x, y - 1, z).id,
                    BlockId::GrassBlock | BlockId::Dirt | BlockId::CoarseDirt | BlockId::Podzol
                )
        }
        Replacement::Mushroom => {
            current == BlockId::Air
                && y > 0
                && chunk.get_block(x, y - 1, z).id.is_solid()
                && (chunk.get_block(x, y - 1, z).id == BlockId::Podzol
                    || (y + 1..CHUNK_HEIGHT)
                        .any(|above| chunk.get_block(x, above, z).id.is_solid()))
        }
        Replacement::Cactus => {
            current == BlockId::Air
                && y > 0
                && matches!(chunk.get_block(x, y - 1, z).id, BlockId::Sand | BlockId::RedSand | BlockId::Cactus)
                && x > 0
                && x + 1 < CHUNK_SIZE
                && z > 0
                && z + 1 < CHUNK_SIZE
                && [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
                    .into_iter()
                    .all(|(x, z)| !chunk.get_block(x, y, z).id.is_solid())
        }
        Replacement::SugarCane => {
            current == BlockId::Air
                && y > 0
                && (chunk.get_block(x, y - 1, z).id == BlockId::SugarCane
                    || (matches!(chunk.get_block(x, y - 1, z).id, BlockId::GrassBlock | BlockId::Dirt | BlockId::Sand | BlockId::RedSand)
                        && x > 0
                        && x + 1 < CHUNK_SIZE
                        && z > 0
                        && z + 1 < CHUNK_SIZE
                        && [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
                            .into_iter()
                            .any(|(x, z)| chunk.get_block(x, y - 1, z).id == BlockId::Water)))
        }
        Replacement::Spring { frozen } => {
            if x == 0 || x + 1 == CHUNK_SIZE || y == 0 || y + 1 == CHUNK_HEIGHT || z == 0 || z + 1 == CHUNK_SIZE {
                return false;
            }
            let valid = |block: BlockId| {
                if frozen {
                    matches!(block, BlockId::SnowBlock | BlockId::PackedIce)
                } else {
                    is_base_stone(block) || matches!(block, BlockId::Calcite | BlockId::Dirt | BlockId::SnowBlock | BlockId::PackedIce)
                }
            };
            let neighbors = [
                chunk.get_block(x - 1, y, z).id,
                chunk.get_block(x + 1, y, z).id,
                chunk.get_block(x, y, z - 1).id,
                chunk.get_block(x, y, z + 1).id,
                chunk.get_block(x, y - 1, z).id,
            ];
            valid(chunk.get_block(x, y + 1, z).id)
                && neighbors.into_iter().filter(|block| valid(*block)).count() == 4
                && neighbors.into_iter().filter(|block| *block == BlockId::Air).count() == 1
                && (current == BlockId::Air || valid(current))
        }
        Replacement::Natural => current != BlockId::Bedrock && !is_feature_block(current),
        Replacement::CaveFloor => {
            current == BlockId::Air && y > 0 && is_solid_natural(chunk.get_block(x, y - 1, z).id)
        }
        Replacement::CaveCeiling => {
            current == BlockId::Air
                && y + 1 < CHUNK_HEIGHT
                && is_solid_natural(chunk.get_block(x, y + 1, z).id)
        }
    }
}

fn replacement_block(replacement: Replacement, current: BlockId, requested: BlockId) -> BlockId {
    match replacement {
        Replacement::Ore { stone, deepslate, .. } => {
            if current == BlockId::Deepslate { deepslate } else { stone }
        }
        _ => requested,
    }
}

fn coordinate_chance(seed: u64, pos: WorldPos, chance: f32) -> bool {
    if chance <= 0.0 {
        return false;
    }
    if chance >= 1.0 {
        return true;
    }
    let mut value = seed
        ^ (pos.x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (pos.y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (pos.z as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    ((value >> 40) as f32) * (1.0 / 16_777_216.0) < chance
}

fn is_base_stone(block: BlockId) -> bool {
    matches!(block, BlockId::Stone | BlockId::Deepslate | BlockId::Tuff | BlockId::Granite | BlockId::Diorite | BlockId::Andesite)
}

fn is_ore_replaceable(block: BlockId) -> bool {
    matches!(block, BlockId::Stone | BlockId::Deepslate | BlockId::Granite | BlockId::Diorite | BlockId::Andesite)
}

fn is_solid_natural(block: BlockId) -> bool {
    is_base_stone(block) || matches!(block, BlockId::Dirt | BlockId::GrassBlock | BlockId::Sand | BlockId::Gravel | BlockId::DripstoneBlock)
}

fn is_feature_block(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Spawner
            | BlockId::Chest
            | BlockId::OakLog
            | BlockId::SpruceLog
            | BlockId::BirchLog
            | BlockId::JungleLog
            | BlockId::AcaciaLog
            | BlockId::DarkOakLog
            | BlockId::CherryLog
            | BlockId::MangroveLog
    )
}

fn is_exposed_to_air(chunk: &Chunk, x: usize, y: usize, z: usize) -> bool {
    [
        (x.checked_sub(1), Some(y), Some(z)),
        (x.checked_add(1).filter(|&v| v < CHUNK_SIZE), Some(y), Some(z)),
        (Some(x), y.checked_sub(1), Some(z)),
        (Some(x), y.checked_add(1).filter(|&v| v < CHUNK_HEIGHT), Some(z)),
        (Some(x), Some(y), z.checked_sub(1)),
        (Some(x), Some(y), z.checked_add(1).filter(|&v| v < CHUNK_SIZE)),
    ]
    .into_iter()
    .any(|(x, y, z)| x.zip(y).zip(z).is_some_and(|((x, y), z)| chunk.get_block(x, y, z).id == BlockId::Air))
}

fn is_mountain(biome: Biome) -> bool {
    matches!(
        biome,
        Biome::WindsweptHills
            | Biome::WindsweptForest
            | Biome::WindsweptGravellyHills
            | Biome::Meadow
            | Biome::Grove
            | Biome::SnowySlopes
            | Biome::JaggedPeaks
            | Biome::FrozenPeaks
            | Biome::StonyPeaks
    )
}

fn is_cold(biome: Biome) -> bool {
    matches!(
        biome,
        Biome::SnowyTaiga
            | Biome::SnowyPlains
            | Biome::IceSpikes
            | Biome::FrozenOcean
            | Biome::DeepFrozenOcean
            | Biome::FrozenRiver
            | Biome::SnowyBeach
            | Biome::Grove
            | Biome::SnowySlopes
            | Biome::JaggedPeaks
            | Biome::FrozenPeaks
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generation::WorldGenerationProfile;

    fn generator(seed: u64) -> VanillaWorldGenerator {
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Geometry)
    }

    #[test]
    fn negative_world_positions_use_euclidean_target_projection() {
        let generator = generator(7);
        let chunk = Chunk::new(-1, -1);
        let mut stage = FeatureStage::new(&generator, 7, -64, 384, &chunk);
        stage.push(WorldPos::new(-1, 64, -1), BlockId::OakLog, Replacement::Air);
        stage.push(WorldPos::new(0, 64, 0), BlockId::OakLog, Replacement::Air);
        assert_eq!(stage.writes.len(), 1);
        assert_eq!(stage.writes[0].pos, WorldPos::new(-1, 64, -1));
    }

    #[test]
    fn tree_geometry_projects_across_a_chunk_border() {
        let generator = generator(11);
        let mut chunk = Chunk::new(1, 0);
        let mut stage = FeatureStage::new(&generator, 11, -64, 384, &chunk);
        let mut random = NoiseSeed::new(4);
        emit_tree(&mut stage, WorldPos::new(15, 70, 8), TreeKind::Oak, &mut random);
        assert!(stage.writes.iter().any(|write| write.pos.x >= 16));
        stage.apply(&mut chunk);
        assert!(chunk.blocks.iter().any(|block| block.id == BlockId::OakLeaves));
    }

    #[test]
    fn feature_surface_uses_completed_target_column_not_preliminary_height() {
        let generator = generator(1);
        let mut chunk = Chunk::new(8, 11);
        let local_y = (70 - (-64)) as usize;
        chunk.set_block(3, local_y, 4, Block::new(BlockId::GrassBlock));
        let mut stage = FeatureStage::new(&generator, 1, -64, 384, &chunk);
        assert_eq!(stage.surface_y(8 * 16 + 3, 11 * 16 + 4), 70);
    }

    #[test]
    fn default_and_swamp_flower_providers_use_reference_native_states() {
        let mut random = NoiseSeed::new(0x26_02);
        let mut saw_poppy = false;
        let mut saw_dandelion = false;
        for _ in 0..128 {
            match flower_block("flower_default", &mut random) {
                BlockId::Poppy => saw_poppy = true,
                BlockId::Dandelion => saw_dandelion = true,
                other => panic!("unexpected default flower {other:?}"),
            }
            assert_eq!(flower_block("flower_swamp", &mut random), BlockId::BlueOrchid);
        }
        assert!(saw_poppy && saw_dandelion);
        let meadow = (0..128)
            .map(|_| flower_block("flower_meadow", &mut random))
            .collect::<std::collections::HashSet<_>>();
        assert!(meadow.contains(&BlockId::Grass));
        assert!(meadow.contains(&BlockId::Allium));
        assert!(meadow.contains(&BlockId::Dandelion));
    }

    #[test]
    fn flower_survival_accepts_native_dirt_family_but_not_sand() {
        let mut chunk = Chunk::new(0, 0);
        let y = 70usize;
        for (x, support) in [
            (1, BlockId::GrassBlock),
            (2, BlockId::Dirt),
            (3, BlockId::CoarseDirt),
            (4, BlockId::Podzol),
            (5, BlockId::Sand),
        ] {
            chunk.set_block(x, y - 1, 1, Block::new(support));
        }
        for x in 1..=4 {
            assert!(replacement_matches(Replacement::PlantOnDirt, BlockId::Air, &chunk, x, y, 1));
        }
        assert!(!replacement_matches(Replacement::PlantOnDirt, BlockId::Air, &chunk, 5, y, 1));
    }

    #[test]
    fn seed_one_forest_chunk_places_canopy_above_completed_surface() {
        let generator = generator(1);
        // This coordinate is Forest under Minecraft 26.2's production
        // Climate.RTree tie ordering (the former brute-force selector used a
        // different equal-distance label at the old fixture coordinate).
        let mut chunk = Chunk::new(4, 9);
        generator.generate_chunk(&mut chunk);
        let canopy_blocks = chunk.blocks.iter().filter(|block| {
            matches!(block.id, BlockId::OakLeaves | BlockId::BirchLeaves)
        }).count();
        let flower_blocks = chunk.blocks.iter().filter(|block| matches!(
            block.id,
            BlockId::Dandelion | BlockId::Poppy | BlockId::BlueOrchid | BlockId::Allium
                | BlockId::AzureBluet | BlockId::RedTulip | BlockId::OrangeTulip
                | BlockId::WhiteTulip | BlockId::PinkTulip | BlockId::OxeyeDaisy
                | BlockId::Cornflower | BlockId::LilyOfTheValley
        )).count();
        assert!(canopy_blocks >= 100, "seed-one forest canopy was still missing: {canopy_blocks} leaf blocks");
        assert!(flower_blocks <= 16, "seed-one forest vegetation regressed to {flower_blocks} flowers in one chunk");
    }

    #[test]
    fn seed_one_near_inland_boundary_generates_forest_not_saved_beach() {
        let generator = generator(1);
        assert_eq!(generator.get_biome_at(158, 88, 165), Biome::Forest);
        let mut chunk = Chunk::new(9, 10);
        generator.generate_chunk(&mut chunk);
        let tree_blocks = chunk.blocks.iter().filter(|block| {
            matches!(
                block.id,
                BlockId::OakLog | BlockId::OakLeaves | BlockId::BirchLog | BlockId::BirchLeaves
            )
        }).count();
        let sand = chunk.blocks.iter().filter(|block| block.id == BlockId::Sand).count();
        let local_x = 158usize % CHUNK_SIZE;
        let local_z = 165usize % CHUNK_SIZE;
        let top = (0..CHUNK_HEIGHT).rev().find_map(|y| {
            let block = chunk.get_block(local_x, y, local_z).id;
            (!matches!(block, BlockId::Air | BlockId::Water)).then_some((y, block))
        });
        println!("tree_blocks={tree_blocks} sand={sand} top={top:?}");
        for world_y in 84..=100 {
            let local_y = (world_y - (-64)) as usize;
            println!("y={world_y} block={:?} biome={:?}", chunk.get_block(local_x, local_y, local_z).id, generator.get_biome_at(158, world_y, 165));
        }
        assert!(tree_blocks > 0, "seed-one chunk 9/10 lost its reference forest");
        assert!(matches!(top, Some((_, BlockId::GrassBlock | BlockId::Dirt))));
    }

    #[test]
    fn ore_replacement_selects_native_stone_variants() {
        let replacement = Replacement::Ore {
            stone: BlockId::IronOre,
            deepslate: BlockId::DeepslateIronOre,
            discard_if_exposed: false,
        };
        assert_eq!(replacement_block(replacement, BlockId::Stone, BlockId::IronOre), BlockId::IronOre);
        assert_eq!(replacement_block(replacement, BlockId::Deepslate, BlockId::IronOre), BlockId::DeepslateIronOre);
    }

    #[test]
    fn representative_ore_and_cave_writes_apply_to_native_blocks() {
        let generator = generator(17);
        let mut chunk = Chunk::new(0, 0);
        let stone_y = (0 - (-64)) as usize;
        chunk.set_block(7, stone_y, 7, Block::new(BlockId::Stone));
        chunk.set_block(8, stone_y, 7, Block::new(BlockId::Deepslate));
        chunk.set_block(4, 53, 4, Block::new(BlockId::Stone));
        chunk.set_block(5, 59, 5, Block::new(BlockId::Stone));
        let mut stage = FeatureStage::new(&generator, 17, -64, 384, &chunk);
        let ore = Replacement::Ore {
            stone: BlockId::IronOre,
            deepslate: BlockId::DeepslateIronOre,
            discard_if_exposed: false,
        };
        stage.push(WorldPos::new(7, 0, 7), BlockId::IronOre, ore);
        stage.push(WorldPos::new(8, 0, 7), BlockId::IronOre, ore);
        stage.push(WorldPos::new(4, -10, 4), BlockId::DripstoneBlock, Replacement::CaveFloor);
        stage.push(WorldPos::new(5, -6, 5), BlockId::HangingRoots, Replacement::CaveCeiling);
        stage.apply(&mut chunk);
        assert_eq!(chunk.get_block(7, stone_y, 7).id, BlockId::IronOre);
        assert_eq!(chunk.get_block(8, stone_y, 7).id, BlockId::DeepslateIronOre);
        assert_eq!(chunk.get_block(4, 54, 4).id, BlockId::DripstoneBlock);
        assert_eq!(chunk.get_block(5, 58, 5).id, BlockId::HangingRoots);
    }

    #[test]
    fn representative_tree_aquatic_and_cave_geometry_is_emitted() {
        let generator = generator(13);
        let chunk = Chunk::new(0, 0);
        let mut stage = FeatureStage::new(&generator, 13, -64, 384, &chunk);
        let mut random = NoiseSeed::new(9);
        emit_tree(&mut stage, WorldPos::new(8, 70, 8), TreeKind::Birch, &mut random);
        emit_coral(&mut stage, WorldPos::new(8, 50, 8), &mut random);
        emit_geode(&mut stage, WorldPos::new(8, 0, 8), &mut random);
        assert!(stage.writes.iter().any(|write| write.block == BlockId::BirchLog));
        assert!(stage.writes.iter().any(|write| matches!(write.block, BlockId::TubeCoralBlock | BlockId::BrainCoralBlock | BlockId::BubbleCoralBlock | BlockId::FireCoralBlock | BlockId::HornCoralBlock)));
        assert!(stage.writes.iter().any(|write| write.block == BlockId::AmethystBlock));
        assert!(stage.writes.iter().any(|write| write.block == BlockId::Calcite));
    }

    #[test]
    fn representative_aquatic_geometry_replaces_ocean_floor_and_water() {
        let generator = generator(19);
        let mut chunk = Chunk::new(0, 0);
        let floor_y = (50 - (-64)) as usize;
        for x in 5..=11 {
            for z in 5..=11 {
                chunk.set_block(x, floor_y, z, Block::new(BlockId::Stone));
                chunk.set_block(x, floor_y + 1, z, Block::new(BlockId::Water));
            }
        }
        let mut stage = FeatureStage::new(&generator, 19, -64, 384, &chunk);
        let mut random = NoiseSeed::new(3);
        emit_coral(&mut stage, WorldPos::new(8, 50, 8), &mut random);
        stage.apply(&mut chunk);
        assert!(chunk.blocks.iter().any(|block| {
            matches!(
                block.id,
                BlockId::TubeCoralBlock
                    | BlockId::BrainCoralBlock
                    | BlockId::BubbleCoralBlock
                    | BlockId::FireCoralBlock
                    | BlockId::HornCoralBlock
            )
        }));
        assert!(chunk.blocks.iter().any(|block| {
            matches!(
                block.id,
                BlockId::TubeCoral
                    | BlockId::BrainCoral
                    | BlockId::BubbleCoral
                    | BlockId::FireCoral
                    | BlockId::HornCoral
                    | BlockId::TubeCoralFan
                    | BlockId::BrainCoralFan
                    | BlockId::BubbleCoralFan
                    | BlockId::FireCoralFan
                    | BlockId::HornCoralFan
            )
        }));
    }

    #[test]
    fn lake_fluid_does_not_replace_an_ocean_floor_below_water() {
        let mut chunk = Chunk::new(0, 0);
        let floor = (50 - (-64)) as usize;
        chunk.set_block(8, floor, 8, Block::new(BlockId::Sand));
        for y in floor + 1..=floor + 5 {
            chunk.set_block(8, y, 8, Block::new(BlockId::Water));
        }
        assert!(!replacement_matches(
            Replacement::LakeFluid,
            BlockId::Sand,
            &chunk,
            8,
            floor,
            8,
        ));
        assert!(!replacement_matches(
            Replacement::LakeAir,
            BlockId::Water,
            &chunk,
            8,
            floor + 1,
            8,
        ));
    }

    #[test]
    fn underwater_magma_keeps_the_random_height_filter() {
        let generator = generator(1);
        let mut chunk = Chunk::new(0, 0);
        let floor = (50 - (-64)) as usize;
        for x in 0..CHUNK_SIZE { for z in 0..CHUNK_SIZE {
            chunk.set_block(x, floor, z, Block::new(BlockId::Sand));
            for y in floor + 1..=(63 - (-64)) as usize {
                chunk.set_block(x, y, z, Block::new(BlockId::Water));
            }
        }}
        let root = MINECRAFT26_FEATURE_ROOTS.iter().copied()
            .find(|root| root.key == "underwater_magma").unwrap();
        let mut stage = FeatureStage::new(&generator, 1, -64, 384, &chunk);
        let mut candidates = 0;
        plan_underwater_magma(&mut stage, 0, 0, root, 0, &mut candidates);
        assert!(stage.writes.len() <= 27, "random-height filtering was bypassed");
    }

    #[test]
    fn underwater_magma_requires_the_reference_water_column() {
        let floor = (50 - (-64)) as usize;
        let mut dry_shore = Chunk::new(0, 0);
        for x in 6..=10 { for z in 6..=10 {
            for y in 0..=floor {
                dry_shore.set_block(x, y, z, Block::new(BlockId::Stone));
            }
        }}
        assert!(!replacement_matches(
            Replacement::UnderwaterMagma,
            BlockId::Stone,
            &dry_shore,
            8,
            floor,
            8,
        ));

        dry_shore.set_block(8, floor + 1, 8, Block::new(BlockId::Water));
        assert!(replacement_matches(
            Replacement::UnderwaterMagma,
            BlockId::Stone,
            &dry_shore,
            8,
            floor,
            8,
        ));
    }

    #[test]
    fn mushrooms_require_podzol_or_sky_occlusion() {
        let y = 70usize;
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(8, y - 1, 8, Block::new(BlockId::GrassBlock));
        assert!(!replacement_matches(
            Replacement::Mushroom, BlockId::Air, &chunk, 8, y, 8,
        ));

        chunk.set_block(8, y + 4, 8, Block::new(BlockId::Stone));
        assert!(replacement_matches(
            Replacement::Mushroom, BlockId::Air, &chunk, 8, y, 8,
        ));

        chunk.set_block(8, y + 4, 8, Block::new(BlockId::Air));
        chunk.set_block(8, y - 1, 8, Block::new(BlockId::Podzol));
        assert!(replacement_matches(
            Replacement::Mushroom, BlockId::Air, &chunk, 8, y, 8,
        ));
    }

    #[test]
    fn geometry_generation_is_deterministic_at_negative_coordinates() {
        let generator = generator(0x5eed);
        let mut first = Chunk::new(-2, -3);
        let mut second = Chunk::new(-2, -3);
        generator.generate_chunk(&mut first);
        generator.generate_chunk(&mut second);
        assert_eq!(first.blocks, second.blocks);
    }

    #[test]
    fn geometry_profile_is_isolated_from_existing_profiles() {
        let seed = 0x1234;
        let mut base = Chunk::new(2, -1);
        let mut preview = Chunk::new(2, -1);
        let mut geometry = Chunk::new(2, -1);
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Base).generate_chunk(&mut base);
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26NativeDecorationPreview).generate_chunk(&mut preview);
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Geometry).generate_chunk(&mut geometry);
        assert_ne!(geometry.blocks, base.blocks);
        assert_ne!(geometry.blocks, preview.blocks);
    }

    #[test]
    fn target_result_is_independent_of_other_chunk_generation_order() {
        let generator = generator(0xcafe);
        let mut expected = Chunk::new(-1, 2);
        generator.generate_chunk(&mut expected);
        let mut unrelated = Chunk::new(4, -7);
        generator.generate_chunk(&mut unrelated);
        let mut repeated = Chunk::new(-1, 2);
        generator.generate_chunk(&mut repeated);
        assert_eq!(expected.blocks, repeated.blocks);
    }

    #[test]
    fn coverage_report_names_skipped_outputs() {
        assert!(MINECRAFT26_GEOMETRY_COVERAGE
            .unsupported_outputs
            .iter()
            .any(|entry| entry.contains("clay")));
        assert!(MINECRAFT26_GEOMETRY_COVERAGE
            .unsupported_outputs
            .iter()
            .any(|entry| entry.contains("pointed dripstone")));
    }

    fn root(key: &str) -> RootDefinition {
        MINECRAFT26_FEATURE_ROOTS
            .iter()
            .copied()
            .find(|root| root.key == key)
            .unwrap()
    }

    #[test]
    fn exact_biome_root_table_closes_all_55_biomes_and_168_roots() {
        use std::collections::HashSet;

        assert_eq!(MINECRAFT26_OVERWORLD_BIOMES.len(), 55);
        assert_eq!(MINECRAFT26_FEATURE_ROOTS.len(), 168);
        assert_eq!(MINECRAFT26_FEATURE_ROOT_ORDER.len(), 168);
        assert_eq!(
            MINECRAFT26_FEATURE_ROOTS.iter().map(|root| root.key).collect::<HashSet<_>>().len(),
            168
        );
        assert_eq!(
            MINECRAFT26_FEATURE_ROOT_ORDER.iter().copied().collect::<HashSet<_>>(),
            MINECRAFT26_FEATURE_ROOTS.iter().map(|root| root.key).collect::<HashSet<_>>(),
        );
        assert!(MINECRAFT26_FEATURE_ROOTS.iter().all(|root| root.step < 11));
        assert!(minecraft26_feature_root_coverage().all(|coverage| {
            !coverage.key.is_empty()
                && matches!(
                    coverage.disposition,
                    RootCoverageDisposition::Implemented
                        | RootCoverageDisposition::GeometryApproximatedDueStates(_)
                        | RootCoverageDisposition::Blocked(_)
                )
        }));

        let expected_counts = [
            48, 48, 46, 48, 50, 52, 45, 48, 47, 49, 48, 46, 49, 52, 52, 48, 48,
            47, 47, 49, 49, 49, 46, 49, 48, 49, 48, 48, 49, 45, 44, 43, 42, 41,
            41, 40, 48, 47, 45, 45, 45, 49, 48, 48, 48, 48, 48, 48, 49, 49, 43,
            48, 47, 43, 43,
        ];
        for (index, biome) in MINECRAFT26_OVERWORLD_BIOMES.iter().copied().enumerate() {
            let roots = MINECRAFT26_FEATURE_ROOTS
                .iter()
                .copied()
                .filter(|root| root.applies_to(biome))
                .count();
            assert_eq!(roots, expected_counts[index], "root count for {biome:?}");
            let ordered_steps = (0..11)
                .flat_map(|step| MINECRAFT26_FEATURE_ROOT_ORDER.iter().map(|key| root(key)).filter(move |root| root.step == step))
                .filter(|root| root.applies_to(biome))
                .map(|root| root.step)
                .collect::<Vec<_>>();
            assert!(ordered_steps.windows(2).all(|steps| steps[0] <= steps[1]));
        }
    }

    #[test]
    fn exact_root_membership_prevents_known_biome_leaks() {
        assert!(!root("lake_lava_surface").applies_to(Biome::DeepDark));
        assert!(!root("spring_water").applies_to(Biome::DeepDark));
        assert!(!root("spring_lava").applies_to(Biome::DeepDark));
        assert!(!root("ore_copper").applies_to(Biome::DripstoneCaves));
        assert!(root("ore_copper_large").applies_to(Biome::DripstoneCaves));
        assert!(!root("ore_copper_large").applies_to(Biome::Plains));
        assert!(root("ore_gold").applies_to(Biome::Plains));
        assert!(root("ore_gold_extra").applies_to(Biome::Badlands));
        assert!(!root("ore_gold_extra").applies_to(Biome::Plains));
        assert!(root("warm_ocean_vegetation").applies_to(Biome::WarmOcean));
        assert!(!root("warm_ocean_vegetation").applies_to(Biome::LukewarmOcean));
        assert!(root("seagrass_river").applies_to(Biome::River));
        assert!(!root("seagrass_river").applies_to(Biome::Ocean));
        assert!(!root("patch_pumpkin").applies_to(Biome::SulfurCaves));
    }

    #[test]
    fn corrected_ore_configs_keep_json_counts_ranges_and_failed_world_bounds() {
        let config = |key| ORES.iter().copied().find(|config| config.key == key).unwrap();
        assert_eq!(config("ore_gold").count, 4);
        assert!(matches!(config("ore_gold").biome_gate, OreBiomeGate::Any));
        assert_eq!(config("ore_gold_extra").count, 50);
        assert!(matches!(config("ore_gold_extra").biome_gate, OreBiomeGate::Badlands));
        assert_eq!(config("ore_copper").size, 10);
        assert_eq!(config("ore_copper_large").size, 20);
        assert_eq!(config("ore_redstone_lower").height, HeightDistribution::Triangle(-96, -32));
        assert_eq!(config("ore_diamond").height, HeightDistribution::Triangle(-144, 16));
        assert_eq!(config("ore_emerald").height, HeightDistribution::Triangle(-16, 480));

        let mut random = NoiseSeed::new(0x26_02);
        let failed = (0..512)
            .filter(|_| HeightDistribution::Triangle(-144, 16).sample_valid(&mut random, -64, 320).is_none())
            .count();
        assert!(failed > 0, "below-bottom diamond attempts must fail instead of clamping");
        let mut random = NoiseSeed::new(0x26_03);
        let failed = (0..512)
            .filter(|_| HeightDistribution::Triangle(80, 384).sample_valid(&mut random, -64, 320).is_none())
            .count();
        assert!(failed > 0, "above-top iron attempts must fail instead of clamping");
    }

    #[test]
    fn unavailable_feature_roots_are_blocked_by_exact_key() {
        for key in [
            "pale_garden_vegetation", "pale_moss_patch", "pale_garden_flowers",
            "flower_pale_garden", "rooted_sulfur_spring", "sulfur_pool",
            "sulfur_spike_cluster", "sulfur_spike", "pointed_dripstone", "disk_clay",
            "ore_clay", "lush_caves_clay", "sculk_vein", "sculk_patch_deep_dark",
            "flower_cherry", "flower_forest_flowers", "forest_flowers",
        ] {
            assert!(matches!(root_coverage_disposition(key), RootCoverageDisposition::Blocked(reason) if !reason.is_empty()), "{key}");
        }
        assert!(matches!(
            root_coverage_disposition("glow_lichen"),
            RootCoverageDisposition::GeometryApproximatedDueStates(reason) if reason.contains("default state")
        ));
    }

    #[test]
    fn representative_new_geometry_uses_available_native_blocks() {
        let generator = generator(0x2602);
        let chunk = Chunk::new(-1, 0);
        let mut stage = FeatureStage::new(&generator, 0x2602, -64, 384, &chunk);
        let mut random = NoiseSeed::new(77);
        emit_fallen_tree(&mut stage, WorldPos::new(-8, 70, 8), TreeKind::Spruce, &mut random);
        emit_huge_mushroom(&mut stage, WorldPos::new(-8, 70, 8), true, &mut random);
        emit_root_system(&mut stage, WorldPos::new(-8, 30, 8), &mut random);
        assert!(stage.writes.iter().any(|write| write.block == BlockId::SpruceLog));
        assert!(stage.writes.iter().any(|write| write.block == BlockId::MushroomStem));
        assert!(stage.writes.iter().any(|write| write.block == BlockId::RedMushroomBlock));
        assert!(stage.writes.iter().any(|write| write.block == BlockId::RootedDirt));
        assert!(stage.writes.iter().any(|write| write.block == BlockId::HangingRoots));
    }
}
