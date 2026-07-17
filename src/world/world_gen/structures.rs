//! Minecraft Java 26.2 Overworld structure-placement candidates.
//!
//! This module stops at candidate chunks and weighted structure attempt order.
//! Biome validation, structure starts, templates, processors, and block placement
//! belong to later generation stages.

const JAVA_RANDOM_MULTIPLIER: u64 = 25_214_903_917;
const JAVA_RANDOM_ADDEND: u64 = 11;
const JAVA_RANDOM_MASK: u64 = (1_u64 << 48) - 1;
const LARGE_FEATURE_X_MULTIPLIER: i64 = 341_873_128_712;
const LARGE_FEATURE_Z_MULTIPLIER: i64 = 132_897_987_541;
const LEGACY_ARBITRARY_SALT: i32 = 10_387_320;

/// Java's 48-bit `LegacyRandomSource`, including Java integer overflow rules.
#[derive(Clone, Debug)]
pub struct JavaLegacyRandom {
    seed: u64,
    calls: u64,
}

impl JavaLegacyRandom {
    pub fn new(seed: i64) -> Self {
        let mut random = Self { seed: 0, calls: 0 };
        random.set_seed(seed);
        random
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = ((seed as u64) ^ JAVA_RANDOM_MULTIPLIER) & JAVA_RANDOM_MASK;
    }

    fn next_bits(&mut self, bits: u32) -> u32 {
        self.calls = self.calls.wrapping_add(1);
        self.seed = self
            .seed
            .wrapping_mul(JAVA_RANDOM_MULTIPLIER)
            .wrapping_add(JAVA_RANDOM_ADDEND)
            & JAVA_RANDOM_MASK;
        (self.seed >> (48 - bits)) as u32
    }

    pub fn next_int(&mut self) -> i32 {
        self.next_bits(32) as i32
    }

    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Java random bound must be positive");
        if (bound & bound.wrapping_sub(1)) == 0 {
            return ((i64::from(bound) * i64::from(self.next_bits(31))) >> 31) as i32;
        }

        loop {
            let sample = self.next_bits(31) as i32;
            let modulo = sample % bound;
            if sample
                .wrapping_sub(modulo)
                .wrapping_add(bound.wrapping_sub(1))
                >= 0
            {
                return modulo;
            }
        }
    }

    pub fn next_long(&mut self) -> i64 {
        let upper = i64::from(self.next_int());
        let lower = i64::from(self.next_int());
        upper.wrapping_shl(32).wrapping_add(lower)
    }

    pub fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * (1.0 / (1_u32 << 24) as f32)
    }

    pub fn next_double(&mut self) -> f64 {
        let upper = u64::from(self.next_bits(26));
        let lower = u64::from(self.next_bits(27));
        ((upper << 27) + lower) as f64 * (1.0 / (1_u64 << 53) as f64)
    }

    /// Number of underlying `next(bits)` calls, matching `WorldgenRandom`.
    pub fn call_count(&self) -> u64 {
        self.calls
    }

    pub fn fork(&mut self) -> Self {
        Self::new(self.next_long())
    }

    pub fn set_large_feature_seed(&mut self, level_seed: i64, chunk_x: i32, chunk_z: i32) {
        self.set_seed(level_seed);
        let x_scale = self.next_long();
        let z_scale = self.next_long();
        let seed = i64::from(chunk_x).wrapping_mul(x_scale)
            ^ i64::from(chunk_z).wrapping_mul(z_scale)
            ^ level_seed;
        self.set_seed(seed);
    }

    pub fn set_large_feature_with_salt(&mut self, level_seed: i64, x: i32, z: i32, salt: i32) {
        let seed = i64::from(x)
            .wrapping_mul(LARGE_FEATURE_X_MULTIPLIER)
            .wrapping_add(i64::from(z).wrapping_mul(LARGE_FEATURE_Z_MULTIPLIER))
            .wrapping_add(level_seed)
            .wrapping_add(i64::from(salt));
        self.set_seed(seed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocateOffset {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrequencyReductionMethod {
    #[default]
    Default,
    LegacyType1,
    LegacyType2,
    LegacyType3,
}

impl FrequencyReductionMethod {
    /// Applies the exact 26.2 frequency reducer after placement-chunk selection.
    pub fn should_generate(
        self,
        level_seed: i64,
        salt: i32,
        source_x: i32,
        source_z: i32,
        probability: f32,
    ) -> bool {
        let mut random = JavaLegacyRandom::new(0);
        match self {
            Self::Default => {
                // StructurePlacement intentionally passes salt as the first coordinate.
                random.set_large_feature_with_salt(level_seed, salt, source_x, source_z);
                random.next_float() < probability
            }
            Self::LegacyType1 => {
                let cx = source_x >> 4;
                let cz = source_z >> 4;
                let mixed_chunks = cx ^ cz.wrapping_shl(4);
                random.set_seed(i64::from(mixed_chunks) ^ level_seed);
                random.next_int();
                let bound = (1.0_f32 / probability) as i32;
                random.next_int_bound(bound) == 0
            }
            Self::LegacyType2 => {
                random.set_large_feature_with_salt(
                    level_seed,
                    source_x,
                    source_z,
                    LEGACY_ARBITRARY_SALT,
                );
                random.next_float() < probability
            }
            Self::LegacyType3 => {
                random.set_large_feature_seed(level_seed, source_x, source_z);
                random.next_double() < f64::from(probability)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StructureSetId {
    Villages,
    DesertPyramids,
    Igloos,
    JungleTemples,
    SwampHuts,
    PillagerOutposts,
    AncientCities,
    OceanMonuments,
    WoodlandMansions,
    BuriedTreasures,
    Mineshafts,
    RuinedPortals,
    Shipwrecks,
    OceanRuins,
    Strongholds,
    TrailRuins,
    TrialChambers,
}

impl StructureSetId {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Villages => "minecraft:villages",
            Self::DesertPyramids => "minecraft:desert_pyramids",
            Self::Igloos => "minecraft:igloos",
            Self::JungleTemples => "minecraft:jungle_temples",
            Self::SwampHuts => "minecraft:swamp_huts",
            Self::PillagerOutposts => "minecraft:pillager_outposts",
            Self::AncientCities => "minecraft:ancient_cities",
            Self::OceanMonuments => "minecraft:ocean_monuments",
            Self::WoodlandMansions => "minecraft:woodland_mansions",
            Self::BuriedTreasures => "minecraft:buried_treasures",
            Self::Mineshafts => "minecraft:mineshafts",
            Self::RuinedPortals => "minecraft:ruined_portals",
            Self::Shipwrecks => "minecraft:shipwrecks",
            Self::OceanRuins => "minecraft:ocean_ruins",
            Self::Strongholds => "minecraft:strongholds",
            Self::TrailRuins => "minecraft:trail_ruins",
            Self::TrialChambers => "minecraft:trial_chambers",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExclusionZone {
    pub other_set: StructureSetId,
    pub chunk_count: i32,
}

impl ExclusionZone {
    /// Mirrors `hasStructureChunkInRange`; the callback must evaluate the
    /// referenced set's complete placement candidate rules at the given chunk.
    pub fn is_forbidden<F>(self, source_x: i32, source_z: i32, mut is_candidate: F) -> bool
    where
        F: FnMut(StructureSetId, i32, i32) -> bool,
    {
        for test_x in source_x - self.chunk_count..=source_x + self.chunk_count {
            for test_z in source_z - self.chunk_count..=source_z + self.chunk_count {
                if is_candidate(self.other_set, test_x, test_z) {
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementCommon {
    pub locate_offset: LocateOffset,
    pub frequency_reduction_method: FrequencyReductionMethod,
    pub frequency: f32,
    pub salt: i32,
    pub exclusion_zone: Option<ExclusionZone>,
}

impl PlacementCommon {
    pub fn locate_pos(self, chunk: ChunkPos) -> BlockPos {
        BlockPos::new(
            chunk.x.wrapping_mul(16).wrapping_add(self.locate_offset.x),
            self.locate_offset.y,
            chunk.z.wrapping_mul(16).wrapping_add(self.locate_offset.z),
        )
    }

    pub fn passes_frequency(self, level_seed: i64, source_x: i32, source_z: i32) -> bool {
        self.frequency >= 1.0
            || self.frequency_reduction_method.should_generate(
                level_seed,
                self.salt,
                source_x,
                source_z,
                self.frequency,
            )
    }

    pub fn exclusion_allows<F>(self, source_x: i32, source_z: i32, is_candidate: F) -> bool
    where
        F: FnMut(StructureSetId, i32, i32) -> bool,
    {
        self.exclusion_zone
            .map(|zone| !zone.is_forbidden(source_x, source_z, is_candidate))
            .unwrap_or(true)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RandomSpreadType {
    #[default]
    Linear,
    Triangular,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandomSpreadPlacement {
    pub common: PlacementCommon,
    pub spacing: i32,
    pub separation: i32,
    pub spread_type: RandomSpreadType,
}

impl RandomSpreadPlacement {
    /// Returns the one potential chunk for the source chunk's spacing region.
    pub fn potential_chunk(self, level_seed: i64, source_x: i32, source_z: i32) -> ChunkPos {
        assert!(
            self.spacing > self.separation && self.separation >= 0,
            "random-spread spacing must exceed non-negative separation"
        );
        let grid_x = source_x.div_euclid(self.spacing);
        let grid_z = source_z.div_euclid(self.spacing);
        let mut random = JavaLegacyRandom::new(0);
        random.set_large_feature_with_salt(level_seed, grid_x, grid_z, self.common.salt);
        let limit = self.spacing - self.separation;
        let spread_x = self.evaluate_spread(&mut random, limit);
        let spread_z = self.evaluate_spread(&mut random, limit);
        ChunkPos::new(
            grid_x.wrapping_mul(self.spacing).wrapping_add(spread_x),
            grid_z.wrapping_mul(self.spacing).wrapping_add(spread_z),
        )
    }

    fn evaluate_spread(self, random: &mut JavaLegacyRandom, limit: i32) -> i32 {
        match self.spread_type {
            RandomSpreadType::Linear => random.next_int_bound(limit),
            RandomSpreadType::Triangular => {
                random
                    .next_int_bound(limit)
                    .wrapping_add(random.next_int_bound(limit))
                    / 2
            }
        }
    }

    pub fn is_placement_chunk(self, level_seed: i64, source_x: i32, source_z: i32) -> bool {
        self.potential_chunk(level_seed, source_x, source_z) == ChunkPos::new(source_x, source_z)
    }

    /// Evaluates random spread and frequency, but deliberately leaves exclusion
    /// evaluation to `PlacementCommon::exclusion_allows`.
    pub fn is_candidate_before_exclusion(
        self,
        level_seed: i64,
        source_x: i32,
        source_z: i32,
    ) -> bool {
        self.is_placement_chunk(level_seed, source_x, source_z)
            && self.common.passes_frequency(level_seed, source_x, source_z)
    }

    pub fn is_candidate<F>(
        self,
        level_seed: i64,
        source_x: i32,
        source_z: i32,
        is_other_set_candidate: F,
    ) -> bool
    where
        F: FnMut(StructureSetId, i32, i32) -> bool,
    {
        self.is_candidate_before_exclusion(level_seed, source_x, source_z)
            && self
                .common
                .exclusion_allows(source_x, source_z, is_other_set_candidate)
    }
}

pub const STRONGHOLD_PREFERRED_BIOMES: &str = "#minecraft:stronghold_biased_to";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrongholdBiomeSearch {
    pub initial_chunk: ChunkPos,
    pub center: BlockPos,
    pub horizontal_radius: i32,
    pub preferred_biomes: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConcentricRingsPlacement {
    pub common: PlacementCommon,
    pub distance: i32,
    pub spread: i32,
    pub count: i32,
    pub preferred_biomes: &'static str,
}

impl ConcentricRingsPlacement {
    pub fn is_placement_chunk(self, positions: &[ChunkPos], source_x: i32, source_z: i32) -> bool {
        positions.contains(&ChunkPos::new(source_x, source_z))
    }

    /// Generates ring candidates and delegates Java's preferred-biome search.
    ///
    /// The callback corresponds exactly to `findBiomeHorizontal`: it receives
    /// the initial chunk center at Y=0, radius 112, the biome tag, and the
    /// per-candidate forked legacy RNG. Return the found biome block position,
    /// or `None` to preserve Java's initial ring chunk when no biome is found.
    pub fn candidates<F>(self, level_seed: i64, mut relocate: F) -> Vec<ChunkPos>
    where
        F: FnMut(StrongholdBiomeSearch, &mut JavaLegacyRandom) -> Option<BlockPos>,
    {
        if self.count <= 0 {
            return Vec::new();
        }

        let mut random = JavaLegacyRandom::new(level_seed);
        let tau = std::f64::consts::PI * 2.0;
        let mut angle = random.next_double() * tau;
        let mut position_in_circle = 0;
        let mut circle = 0;
        let mut spread = self.spread;
        let mut positions = Vec::with_capacity(self.count as usize);

        for i in 0..self.count {
            let distance = 4.0 * f64::from(self.distance)
                + f64::from(self.distance * circle * 6)
                + (random.next_double() - 0.5) * (f64::from(self.distance) * 2.5);
            let initial = ChunkPos::new(
                java_round_to_i32(angle.cos() * distance),
                java_round_to_i32(angle.sin() * distance),
            );
            let mut biome_random = random.fork();
            let request = StrongholdBiomeSearch {
                initial_chunk: initial,
                center: BlockPos::new(
                    initial.x.wrapping_mul(16).wrapping_add(8),
                    0,
                    initial.z.wrapping_mul(16).wrapping_add(8),
                ),
                horizontal_radius: 112,
                preferred_biomes: self.preferred_biomes,
            };
            let position = relocate(request, &mut biome_random)
                .map(|block| ChunkPos::new(block.x.div_euclid(16), block.z.div_euclid(16)))
                .unwrap_or(initial);
            positions.push(position);

            angle += tau / f64::from(spread);
            position_in_circle += 1;
            if position_in_circle == spread {
                circle += 1;
                position_in_circle = 0;
                spread += 2 * spread / (circle + 1);
                spread = spread.min(self.count - i);
                angle += random.next_double() * tau;
            }
        }
        positions
    }
}

fn java_round_to_i32(value: f64) -> i32 {
    (value + 0.5).floor() as i64 as i32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StructurePlacement {
    RandomSpread(RandomSpreadPlacement),
    ConcentricRings(ConcentricRingsPlacement),
}

impl StructurePlacement {
    pub const fn common(self) -> PlacementCommon {
        match self {
            Self::RandomSpread(placement) => placement.common,
            Self::ConcentricRings(placement) => placement.common,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructureEntry {
    pub structure: &'static str,
    pub weight: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructureSet {
    pub id: StructureSetId,
    pub structures: &'static [StructureEntry],
    pub placement: StructurePlacement,
}

impl StructureSet {
    /// Returns Java's weighted no-replacement attempt order for this chunk.
    /// Biome and start generation decide which attempted entry, if any, succeeds.
    pub fn weighted_structure_order(
        self,
        level_seed: i64,
        chunk: ChunkPos,
    ) -> Vec<&'static StructureEntry> {
        weighted_structure_order(self.structures, level_seed, chunk)
    }
}

fn weighted_structure_order(
    entries: &'static [StructureEntry],
    level_seed: i64,
    chunk: ChunkPos,
) -> Vec<&'static StructureEntry> {
    assert!(entries.iter().all(|entry| entry.weight > 0));
    let mut options: Vec<&StructureEntry> = entries.iter().collect();
    let mut total = options
        .iter()
        .fold(0_i32, |sum, entry| sum.wrapping_add(entry.weight));
    assert!(total > 0, "structure weights must fit a positive Java int");

    let mut random = JavaLegacyRandom::new(0);
    random.set_large_feature_seed(level_seed, chunk.x, chunk.z);
    let mut ordered = Vec::with_capacity(options.len());
    while !options.is_empty() {
        let mut choice = random.next_int_bound(total);
        let mut index = 0;
        for option in &options {
            choice -= option.weight;
            if choice < 0 {
                break;
            }
            index += 1;
        }
        let selected = options.remove(index);
        total -= selected.weight;
        ordered.push(selected);
    }
    ordered
}

const fn common(salt: i32) -> PlacementCommon {
    PlacementCommon {
        locate_offset: LocateOffset { x: 0, y: 0, z: 0 },
        frequency_reduction_method: FrequencyReductionMethod::Default,
        frequency: 1.0,
        salt,
        exclusion_zone: None,
    }
}

const fn random_spread(
    spacing: i32,
    separation: i32,
    spread_type: RandomSpreadType,
    salt: i32,
) -> StructurePlacement {
    StructurePlacement::RandomSpread(RandomSpreadPlacement {
        common: common(salt),
        spacing,
        separation,
        spread_type,
    })
}

const VILLAGES: &[StructureEntry] = &[
    StructureEntry {
        structure: "minecraft:village_plains",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:village_desert",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:village_savanna",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:village_snowy",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:village_taiga",
        weight: 1,
    },
];
const DESERT_PYRAMIDS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:desert_pyramid",
    weight: 1,
}];
const IGLOOS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:igloo",
    weight: 1,
}];
const JUNGLE_TEMPLES: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:jungle_pyramid",
    weight: 1,
}];
const SWAMP_HUTS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:swamp_hut",
    weight: 1,
}];
const PILLAGER_OUTPOSTS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:pillager_outpost",
    weight: 1,
}];
const ANCIENT_CITIES: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:ancient_city",
    weight: 1,
}];
const OCEAN_MONUMENTS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:monument",
    weight: 1,
}];
const WOODLAND_MANSIONS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:mansion",
    weight: 1,
}];
const BURIED_TREASURES: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:buried_treasure",
    weight: 1,
}];
const MINESHAFTS: &[StructureEntry] = &[
    StructureEntry {
        structure: "minecraft:mineshaft",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:mineshaft_mesa",
        weight: 1,
    },
];
const RUINED_PORTALS: &[StructureEntry] = &[
    StructureEntry {
        structure: "minecraft:ruined_portal",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ruined_portal_desert",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ruined_portal_jungle",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ruined_portal_swamp",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ruined_portal_mountain",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ruined_portal_ocean",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ruined_portal_nether",
        weight: 1,
    },
];
const SHIPWRECKS: &[StructureEntry] = &[
    StructureEntry {
        structure: "minecraft:shipwreck",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:shipwreck_beached",
        weight: 1,
    },
];
const OCEAN_RUINS: &[StructureEntry] = &[
    StructureEntry {
        structure: "minecraft:ocean_ruin_cold",
        weight: 1,
    },
    StructureEntry {
        structure: "minecraft:ocean_ruin_warm",
        weight: 1,
    },
];
const STRONGHOLDS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:stronghold",
    weight: 1,
}];
const TRAIL_RUINS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:trail_ruins",
    weight: 1,
}];
const TRIAL_CHAMBERS: &[StructureEntry] = &[StructureEntry {
    structure: "minecraft:trial_chambers",
    weight: 1,
}];

/// All structure sets available to the normal 26.2 Overworld biome source.
pub const OVERWORLD_STRUCTURE_SETS: &[StructureSet] = &[
    StructureSet {
        id: StructureSetId::Villages,
        structures: VILLAGES,
        placement: random_spread(34, 8, RandomSpreadType::Linear, 10_387_312),
    },
    StructureSet {
        id: StructureSetId::DesertPyramids,
        structures: DESERT_PYRAMIDS,
        placement: random_spread(32, 8, RandomSpreadType::Linear, 14_357_617),
    },
    StructureSet {
        id: StructureSetId::Igloos,
        structures: IGLOOS,
        placement: random_spread(32, 8, RandomSpreadType::Linear, 14_357_618),
    },
    StructureSet {
        id: StructureSetId::JungleTemples,
        structures: JUNGLE_TEMPLES,
        placement: random_spread(32, 8, RandomSpreadType::Linear, 14_357_619),
    },
    StructureSet {
        id: StructureSetId::SwampHuts,
        structures: SWAMP_HUTS,
        placement: random_spread(32, 8, RandomSpreadType::Linear, 14_357_620),
    },
    StructureSet {
        id: StructureSetId::PillagerOutposts,
        structures: PILLAGER_OUTPOSTS,
        placement: StructurePlacement::RandomSpread(RandomSpreadPlacement {
            common: PlacementCommon {
                locate_offset: LocateOffset { x: 0, y: 0, z: 0 },
                frequency_reduction_method: FrequencyReductionMethod::LegacyType1,
                frequency: 0.2,
                salt: 165_745_296,
                exclusion_zone: Some(ExclusionZone {
                    other_set: StructureSetId::Villages,
                    chunk_count: 10,
                }),
            },
            spacing: 32,
            separation: 8,
            spread_type: RandomSpreadType::Linear,
        }),
    },
    StructureSet {
        id: StructureSetId::AncientCities,
        structures: ANCIENT_CITIES,
        placement: random_spread(24, 8, RandomSpreadType::Linear, 20_083_232),
    },
    StructureSet {
        id: StructureSetId::OceanMonuments,
        structures: OCEAN_MONUMENTS,
        placement: random_spread(32, 5, RandomSpreadType::Triangular, 10_387_313),
    },
    StructureSet {
        id: StructureSetId::WoodlandMansions,
        structures: WOODLAND_MANSIONS,
        placement: random_spread(80, 20, RandomSpreadType::Triangular, 10_387_319),
    },
    StructureSet {
        id: StructureSetId::BuriedTreasures,
        structures: BURIED_TREASURES,
        placement: StructurePlacement::RandomSpread(RandomSpreadPlacement {
            common: PlacementCommon {
                locate_offset: LocateOffset { x: 9, y: 0, z: 9 },
                frequency_reduction_method: FrequencyReductionMethod::LegacyType2,
                frequency: 0.01,
                salt: 0,
                exclusion_zone: None,
            },
            spacing: 1,
            separation: 0,
            spread_type: RandomSpreadType::Linear,
        }),
    },
    StructureSet {
        id: StructureSetId::Mineshafts,
        structures: MINESHAFTS,
        placement: StructurePlacement::RandomSpread(RandomSpreadPlacement {
            common: PlacementCommon {
                locate_offset: LocateOffset { x: 0, y: 0, z: 0 },
                frequency_reduction_method: FrequencyReductionMethod::LegacyType3,
                frequency: 0.004,
                salt: 0,
                exclusion_zone: None,
            },
            spacing: 1,
            separation: 0,
            spread_type: RandomSpreadType::Linear,
        }),
    },
    StructureSet {
        id: StructureSetId::RuinedPortals,
        structures: RUINED_PORTALS,
        placement: random_spread(40, 15, RandomSpreadType::Linear, 34_222_645),
    },
    StructureSet {
        id: StructureSetId::Shipwrecks,
        structures: SHIPWRECKS,
        placement: random_spread(24, 4, RandomSpreadType::Linear, 165_745_295),
    },
    StructureSet {
        id: StructureSetId::OceanRuins,
        structures: OCEAN_RUINS,
        placement: random_spread(20, 8, RandomSpreadType::Linear, 14_357_621),
    },
    StructureSet {
        id: StructureSetId::Strongholds,
        structures: STRONGHOLDS,
        placement: StructurePlacement::ConcentricRings(ConcentricRingsPlacement {
            common: PlacementCommon {
                locate_offset: LocateOffset { x: 0, y: 0, z: 0 },
                frequency_reduction_method: FrequencyReductionMethod::Default,
                frequency: 1.0,
                salt: 0,
                exclusion_zone: None,
            },
            distance: 32,
            spread: 3,
            count: 128,
            preferred_biomes: STRONGHOLD_PREFERRED_BIOMES,
        }),
    },
    StructureSet {
        id: StructureSetId::TrailRuins,
        structures: TRAIL_RUINS,
        placement: random_spread(34, 8, RandomSpreadType::Linear, 83_469_867),
    },
    StructureSet {
        id: StructureSetId::TrialChambers,
        structures: TRIAL_CHAMBERS,
        placement: random_spread(34, 12, RandomSpreadType::Linear, 94_251_327),
    },
];

pub fn overworld_structure_set(id: StructureSetId) -> &'static StructureSet {
    OVERWORLD_STRUCTURE_SETS
        .iter()
        .find(|set| set.id == id)
        .expect("every Overworld structure-set ID must have static 26.2 data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_random_matches_java_vectors_and_worldgen_consumption() {
        let mut random = JavaLegacyRandom::new(123_456_789);
        assert_eq!(random.next_int(), -1_442_945_365);
        assert_eq!(random.next_float().to_bits(), 0.763_316_45_f32.to_bits());
        assert_eq!(random.next_int_bound(17), 9);
        assert_eq!(random.next_long(), 4_701_514_676_984_888_228);
        assert_eq!(random.call_count(), 5);

        let mut worldgen = JavaLegacyRandom::new(0);
        worldgen.set_large_feature_seed(-912_345_678_901_234_567, -31, -47);
        assert_eq!(worldgen.call_count(), 4);
        assert_eq!(worldgen.next_float().to_bits(), 0.843_473_2_f32.to_bits());
        assert_eq!(worldgen.next_int_bound(16), 7);
        assert_eq!(worldgen.next_long(), -7_077_954_664_962_767_630);
        assert_eq!(worldgen.call_count(), 8);
    }

    fn random_placement(id: StructureSetId) -> RandomSpreadPlacement {
        match overworld_structure_set(id).placement {
            StructurePlacement::RandomSpread(placement) => placement,
            StructurePlacement::ConcentricRings(_) => panic!("expected random-spread placement"),
        }
    }

    fn stronghold_placement() -> ConcentricRingsPlacement {
        match overworld_structure_set(StructureSetId::Strongholds).placement {
            StructurePlacement::ConcentricRings(placement) => placement,
            StructurePlacement::RandomSpread(_) => panic!("expected concentric-rings placement"),
        }
    }

    #[test]
    fn linear_spread_matches_fixed_java_candidates_on_both_sides_of_zero() {
        let villages = random_placement(StructureSetId::Villages);
        assert_eq!(
            villages.potential_chunk(123_456_789, 100, -45),
            ChunkPos::new(91, -45)
        );
        assert_eq!(
            villages.potential_chunk(123_456_789, -100, -45),
            ChunkPos::new(-94, -62)
        );
        assert!(villages.is_placement_chunk(123_456_789, -94, -62));
    }

    #[test]
    fn triangular_spread_matches_fixed_java_candidate() {
        let monuments = random_placement(StructureSetId::OceanMonuments);
        assert_eq!(
            monuments.potential_chunk(-9_876_543_212_345, -81, 159),
            ChunkPos::new(-90, 136)
        );
    }

    #[test]
    fn all_frequency_reducers_match_fixed_java_results() {
        let seed = 123_456_789;
        assert!(FrequencyReductionMethod::Default.should_generate(seed, 42, -17, 33, 0.35));
        assert!(!FrequencyReductionMethod::Default.should_generate(seed, 42, -200, -200, 0.35));
        assert!(FrequencyReductionMethod::LegacyType1.should_generate(
            seed,
            165_745_296,
            -200,
            -200,
            0.2
        ));
        assert!(!FrequencyReductionMethod::LegacyType1.should_generate(seed, 42, -17, 33, 0.2));
        assert!(FrequencyReductionMethod::LegacyType2.should_generate(seed, 0, -915, -1000, 0.01));
        assert!(!FrequencyReductionMethod::LegacyType2.should_generate(seed, 42, -17, 33, 0.01));
        assert!(FrequencyReductionMethod::LegacyType3.should_generate(seed, 0, -819, -1000, 0.004));
        assert!(!FrequencyReductionMethod::LegacyType3.should_generate(seed, 42, -17, 33, 0.004));
    }

    #[test]
    fn exclusion_zone_scans_the_exact_inclusive_chunk_square() {
        let zone = random_placement(StructureSetId::PillagerOutposts)
            .common
            .exclusion_zone
            .unwrap();
        assert_eq!(zone.other_set, StructureSetId::Villages);
        assert_eq!(zone.chunk_count, 10);
        assert!(zone.is_forbidden(-5, 7, |set, x, z| {
            set == StructureSetId::Villages && x == -15 && z == -3
        }));

        let mut visited = 0;
        assert!(!zone.is_forbidden(-5, 7, |set, _, _| {
            assert_eq!(set, StructureSetId::Villages);
            visited += 1;
            false
        }));
        assert_eq!(visited, 21 * 21);
    }

    #[test]
    fn weighted_selection_matches_fixed_java_no_replacement_order() {
        static WEIGHTED: &[StructureEntry] = &[
            StructureEntry {
                structure: "test:a",
                weight: 1,
            },
            StructureEntry {
                structure: "test:b",
                weight: 3,
            },
            StructureEntry {
                structure: "test:c",
                weight: 2,
            },
            StructureEntry {
                structure: "test:d",
                weight: 7,
            },
        ];
        let order = weighted_structure_order(WEIGHTED, 246_813_579, ChunkPos::new(-31, 47));
        assert_eq!(
            order
                .iter()
                .map(|entry| entry.structure)
                .collect::<Vec<_>>(),
            vec!["test:d", "test:b", "test:c", "test:a"]
        );
    }

    #[test]
    fn stronghold_rings_match_fixed_unrelocated_java_candidates() {
        let expected = [
            ChunkPos::new(-13, -106),
            ChunkPos::new(121, 52),
            ChunkPos::new(-92, 69),
            ChunkPos::new(-75, -342),
            ChunkPos::new(223, -203),
            ChunkPos::new(278, 89),
            ChunkPos::new(69, 316),
            ChunkPos::new(-213, 194),
            ChunkPos::new(-298, -95),
            ChunkPos::new(-138, -492),
            ChunkPos::new(185, -498),
            ChunkPos::new(449, -299),
        ];
        let mut searches = Vec::new();
        let positions = stronghold_placement().candidates(0, |search, _| {
            searches.push(search);
            None
        });
        assert_eq!(positions.len(), 128);
        assert_eq!(&positions[..expected.len()], &expected);
        assert_eq!(searches[0].initial_chunk, expected[0]);
        assert_eq!(searches[0].center, BlockPos::new(-200, 0, -1688));
        assert_eq!(searches[0].horizontal_radius, 112);
        assert_eq!(searches[0].preferred_biomes, STRONGHOLD_PREFERRED_BIOMES);
    }

    #[test]
    fn stronghold_biome_relocation_is_an_explicit_block_to_chunk_boundary() {
        let mut placement = stronghold_placement();
        placement.count = 1;
        let positions = placement.candidates(0, |_, random| {
            assert_eq!(random.next_int(), -584_661_012);
            Some(BlockPos::new(-1, 42, -17))
        });
        assert_eq!(positions, vec![ChunkPos::new(-1, -2)]);
    }

    #[test]
    fn static_data_contains_every_java_26_2_overworld_structure_set() {
        assert_eq!(OVERWORLD_STRUCTURE_SETS.len(), 17);
        let ids = OVERWORLD_STRUCTURE_SETS
            .iter()
            .map(|set| set.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                StructureSetId::Villages,
                StructureSetId::DesertPyramids,
                StructureSetId::Igloos,
                StructureSetId::JungleTemples,
                StructureSetId::SwampHuts,
                StructureSetId::PillagerOutposts,
                StructureSetId::AncientCities,
                StructureSetId::OceanMonuments,
                StructureSetId::WoodlandMansions,
                StructureSetId::BuriedTreasures,
                StructureSetId::Mineshafts,
                StructureSetId::RuinedPortals,
                StructureSetId::Shipwrecks,
                StructureSetId::OceanRuins,
                StructureSetId::Strongholds,
                StructureSetId::TrailRuins,
                StructureSetId::TrialChambers,
            ]
        );
        assert_eq!(
            overworld_structure_set(StructureSetId::RuinedPortals)
                .structures
                .len(),
            7
        );
        assert!(OVERWORLD_STRUCTURE_SETS
            .iter()
            .flat_map(|set| set.structures)
            .all(|entry| entry.weight == 1));

        let expected_random_spread = [
            (
                StructureSetId::Villages,
                34,
                8,
                RandomSpreadType::Linear,
                10_387_312,
            ),
            (
                StructureSetId::DesertPyramids,
                32,
                8,
                RandomSpreadType::Linear,
                14_357_617,
            ),
            (
                StructureSetId::Igloos,
                32,
                8,
                RandomSpreadType::Linear,
                14_357_618,
            ),
            (
                StructureSetId::JungleTemples,
                32,
                8,
                RandomSpreadType::Linear,
                14_357_619,
            ),
            (
                StructureSetId::SwampHuts,
                32,
                8,
                RandomSpreadType::Linear,
                14_357_620,
            ),
            (
                StructureSetId::PillagerOutposts,
                32,
                8,
                RandomSpreadType::Linear,
                165_745_296,
            ),
            (
                StructureSetId::AncientCities,
                24,
                8,
                RandomSpreadType::Linear,
                20_083_232,
            ),
            (
                StructureSetId::OceanMonuments,
                32,
                5,
                RandomSpreadType::Triangular,
                10_387_313,
            ),
            (
                StructureSetId::WoodlandMansions,
                80,
                20,
                RandomSpreadType::Triangular,
                10_387_319,
            ),
            (
                StructureSetId::BuriedTreasures,
                1,
                0,
                RandomSpreadType::Linear,
                0,
            ),
            (
                StructureSetId::Mineshafts,
                1,
                0,
                RandomSpreadType::Linear,
                0,
            ),
            (
                StructureSetId::RuinedPortals,
                40,
                15,
                RandomSpreadType::Linear,
                34_222_645,
            ),
            (
                StructureSetId::Shipwrecks,
                24,
                4,
                RandomSpreadType::Linear,
                165_745_295,
            ),
            (
                StructureSetId::OceanRuins,
                20,
                8,
                RandomSpreadType::Linear,
                14_357_621,
            ),
            (
                StructureSetId::TrailRuins,
                34,
                8,
                RandomSpreadType::Linear,
                83_469_867,
            ),
            (
                StructureSetId::TrialChambers,
                34,
                12,
                RandomSpreadType::Linear,
                94_251_327,
            ),
        ];
        for (id, spacing, separation, spread_type, salt) in expected_random_spread {
            let placement = random_placement(id);
            assert_eq!(
                (placement.spacing, placement.separation),
                (spacing, separation)
            );
            assert_eq!(placement.spread_type, spread_type);
            assert_eq!(placement.common.salt, salt);
        }

        let buried = random_placement(StructureSetId::BuriedTreasures).common;
        assert_eq!(
            buried.frequency_reduction_method,
            FrequencyReductionMethod::LegacyType2
        );
        assert_eq!(buried.frequency, 0.01);
        assert_eq!(buried.locate_offset, LocateOffset { x: 9, y: 0, z: 9 });
        let mineshafts = random_placement(StructureSetId::Mineshafts).common;
        assert_eq!(
            mineshafts.frequency_reduction_method,
            FrequencyReductionMethod::LegacyType3
        );
        assert_eq!(mineshafts.frequency, 0.004);
        let outposts = random_placement(StructureSetId::PillagerOutposts).common;
        assert_eq!(
            outposts.frequency_reduction_method,
            FrequencyReductionMethod::LegacyType1
        );
        assert_eq!(outposts.frequency, 0.2);

        let strongholds = stronghold_placement();
        assert_eq!(
            (strongholds.distance, strongholds.spread, strongholds.count),
            (32, 3, 128)
        );
        assert_eq!(strongholds.preferred_biomes, STRONGHOLD_PREFERRED_BIOMES);
    }
}
