//! Bounded Minecraft 26.2 Overworld structure geometry.
//!
//! Placement candidates, frequency reducers, exclusion zones, and weighted
//! entry order come from `structures`. Geometry is deliberately narrower than
//! Java piece generation: each accepted start owns one bounded native shape or
//! one real template slice, and every target chunk independently projects the
//! starts that can intersect it. No loaded neighbor is read or mutated.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::world::block::{Block, BlockId};
use crate::world::chunk::{Chunk, CHUNK_SIZE};
use crate::world::world_gen::generator::VanillaWorldGenerator;
use crate::world::world_gen::structures::{
    overworld_structure_set, BlockPos, ChunkPos, JavaLegacyRandom, StructurePlacement,
    StructureSet, StructureSetId, StrongholdBiomeSearch, OVERWORLD_STRUCTURE_SETS,
};
use crate::world::world_gen::template::{
    FlattenedTemplateBlock, Rotation, StructureTemplate, TemplateLimits, TemplateTransform,
};
use crate::world::world_gen::Biome;

/// The largest supported footprint reaches at most 32 blocks from its owner
/// chunk's center. Three chunks covers that footprint on both axes, including
/// negative coordinates and a rotated 64-block template.
pub const STRUCTURE_OWNER_RADIUS: i32 = 3;
/// Candidate starts admitted while producing one target chunk.
pub const MAX_STRUCTURE_STARTS: usize = 128;
/// Target-local writes admitted across all starts.
pub const MAX_PROJECTED_STRUCTURE_BLOCKS: usize = 65_536;
/// Maximum native/template block positions evaluated for one target chunk.
pub const MAX_STRUCTURE_BLOCK_EVALUATIONS: usize = 1_048_576;
/// Templates larger than this are skipped rather than exceeding the owner scan.
pub const MAX_TEMPLATE_HORIZONTAL_SPAN: i32 = 64;
/// Decode and iteration bound for a selected template slice.
pub const MAX_TEMPLATE_BLOCKS: usize = 32_768;

pub const BASE_TERRAIN_REPLACEMENT_RULES: &[&str] = &[
    "solid template blocks may replace base air, fluid, or natural terrain",
    "destructive structure air, ignored legacy air, and underwater fluid/terrain preservation are explicit per template source",
    "structure_void and jigsaw markers never write",
    "native underground rooms may carve base terrain",
    "bedrock is never replaced",
    "writes outside the target chunk or Java-profile build height are discarded",
    "later placed-feature writes use their own replacement predicates",
];

pub const UNSUPPORTED_STRUCTURE_OUTPUTS: &[&str] = &[
    "jigsaw expansion and complete Java piece graphs",
    "general template processors, terrain adaptation, and unsupported block-state families",
    "loot tables, archaeology, trial spawners, vaults, mobs, entities, and block entities",
    "structure references, locate metadata, and Java save/protocol representation",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureGeometryKind {
    TemplateBacked,
    NativeBounded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StructureGeometryCoverage {
    pub set: StructureSetId,
    pub placement: &'static str,
    pub geometry: StructureGeometryKind,
    pub geometry_scope: &'static str,
}

/// Placement is exact through the existing 26.2 candidate layer. Geometry is
/// explicitly not jigsaw, processor, piece, state, loot, mob, entity, or block-
/// entity parity.
pub const MINECRAFT26_STRUCTURE_GEOMETRY_COVERAGE: &[StructureGeometryCoverage] = &[
    coverage(StructureSetId::Villages, StructureGeometryKind::TemplateBacked, "one biome-matched 26.2 town-center template; no jigsaw expansion"),
    coverage(StructureSetId::DesertPyramids, StructureGeometryKind::NativeBounded, "recognizable stepped sandstone pyramid; native geometry"),
    coverage(StructureSetId::Igloos, StructureGeometryKind::TemplateBacked, "26.2 igloo top template; no basement processor chain"),
    coverage(StructureSetId::JungleTemples, StructureGeometryKind::NativeBounded, "12x15 mossy temple shell; native geometry"),
    coverage(StructureSetId::SwampHuts, StructureGeometryKind::NativeBounded, "stilted hut footprint; native geometry"),
    coverage(StructureSetId::PillagerOutposts, StructureGeometryKind::TemplateBacked, "26.2 watchtower template; no jigsaw features or mobs"),
    coverage(StructureSetId::AncientCities, StructureGeometryKind::TemplateBacked, "one weighted 26.2 city-center template at the Java start height; no jigsaw city"),
    coverage(StructureSetId::OceanMonuments, StructureGeometryKind::NativeBounded, "58x58 tiered prismarine monument; native geometry"),
    coverage(StructureSetId::WoodlandMansions, StructureGeometryKind::NativeBounded, "bounded multi-floor dark-oak mansion; native geometry"),
    coverage(StructureSetId::BuriedTreasures, StructureGeometryKind::NativeBounded, "buried chest marker without loot or block entity; native geometry"),
    coverage(StructureSetId::Mineshafts, StructureGeometryKind::NativeBounded, "bounded crossing corridors and timber supports; native geometry"),
    coverage(StructureSetId::RuinedPortals, StructureGeometryKind::TemplateBacked, "one weighted 26.2 ruined-portal template; processors omitted"),
    coverage(StructureSetId::Shipwrecks, StructureGeometryKind::TemplateBacked, "one weighted 26.2 plural-palette shipwreck template; loot and block entities omitted"),
    coverage(StructureSetId::OceanRuins, StructureGeometryKind::TemplateBacked, "one biome-matched 26.2 underwater-ruin template; clusters omitted"),
    coverage(StructureSetId::Strongholds, StructureGeometryKind::NativeBounded, "bounded corridors, crossing, and portal-room footprint; native geometry"),
    coverage(StructureSetId::TrailRuins, StructureGeometryKind::TemplateBacked, "one buried 26.2 tower template; no jigsaw or archaeology processors"),
    coverage(StructureSetId::TrialChambers, StructureGeometryKind::TemplateBacked, "one weighted 26.2 corridor/end start template; no jigsaw, vault, or spawner behavior"),
];

const fn coverage(
    set: StructureSetId,
    geometry: StructureGeometryKind,
    geometry_scope: &'static str,
) -> StructureGeometryCoverage {
    StructureGeometryCoverage {
        set,
        placement: "exact 26.2 candidate/frequency/exclusion/weighted order; native biome and terrain gate",
        geometry,
        geometry_scope,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WorldPos {
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Clone, Copy)]
struct ProjectedWrite {
    pos: WorldPos,
    block: Block,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TemplateAirPolicy {
    Destructive,
    Ignore,
    Underwater,
}

#[derive(Clone, Copy)]
struct TemplateSelection {
    path: &'static str,
    air_policy: TemplateAirPolicy,
}

struct StructureStage<'a> {
    generator: &'a VanillaWorldGenerator,
    world_seed: i64,
    min_y: i32,
    max_y: i32,
    target_x: i32,
    target_z: i32,
    asset_root: PathBuf,
    starts: usize,
    block_evaluations: usize,
    writes: Vec<ProjectedWrite>,
    base_columns: HashMap<(i32, i32), Vec<Block>>,
    target_world_surface: [i32; CHUNK_SIZE * CHUNK_SIZE],
    target_ocean_floor: [i32; CHUNK_SIZE * CHUNK_SIZE],
    target_buried_treasure: [Option<i32>; CHUNK_SIZE * CHUNK_SIZE],
}

impl<'a> StructureStage<'a> {
    fn new(
        generator: &'a VanillaWorldGenerator,
        world_seed: u64,
        min_y: i32,
        height: i32,
        chunk: &Chunk,
    ) -> Self {
        Self::new_with_asset_root(
            generator,
            world_seed,
            min_y,
            height,
            chunk,
            structure_asset_root(),
        )
    }

    fn new_with_asset_root(
        generator: &'a VanillaWorldGenerator,
        world_seed: u64,
        min_y: i32,
        height: i32,
        chunk: &Chunk,
        asset_root: PathBuf,
    ) -> Self {
        let mut target_world_surface = [min_y; CHUNK_SIZE * CHUNK_SIZE];
        let mut target_ocean_floor = [min_y; CHUNK_SIZE * CHUNK_SIZE];
        let mut target_buried_treasure = [None; CHUNK_SIZE * CHUNK_SIZE];
        for local_x in 0..CHUNK_SIZE {
            for local_z in 0..CHUNK_SIZE {
                let index = local_x * CHUNK_SIZE + local_z;
                let mut found_world_surface = false;
                let mut found_ocean_floor = false;
                for local_y in (0..crate::world::chunk::CHUNK_HEIGHT).rev() {
                    let block = chunk.get_block(local_x, local_y, local_z);
                    if !found_world_surface && !block.is_air() {
                        target_world_surface[index] = min_y + local_y as i32 + 1;
                        found_world_surface = true;
                    }
                    if !block.is_air() && !matches!(block.id, BlockId::Water | BlockId::Lava) {
                        if !found_ocean_floor {
                            target_ocean_floor[index] = min_y + local_y as i32 + 1;
                            found_ocean_floor = true;
                        }
                        if target_buried_treasure[index].is_none()
                            && is_buried_treasure_support(block.id)
                        {
                            target_buried_treasure[index] = Some(min_y + local_y as i32 + 1);
                        }
                    }
                    if found_world_surface
                        && found_ocean_floor
                        && target_buried_treasure[index].is_some()
                    {
                        break;
                    }
                }
            }
        }
        Self {
            generator,
            world_seed: world_seed as i64,
            min_y,
            max_y: min_y.saturating_add(height),
            target_x: chunk.cx,
            target_z: chunk.cz,
            asset_root,
            starts: 0,
            block_evaluations: 0,
            writes: Vec::new(),
            base_columns: HashMap::new(),
            target_world_surface,
            target_ocean_floor,
            target_buried_treasure,
        }
    }

    fn push(&mut self, x: i32, y: i32, z: i32, block: BlockId) {
        self.push_block(x, y, z, Block::new(block));
    }

    fn push_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if self.block_evaluations >= MAX_STRUCTURE_BLOCK_EVALUATIONS {
            return;
        }
        self.block_evaluations += 1;
        if self.writes.len() >= MAX_PROJECTED_STRUCTURE_BLOCKS
            || y < self.min_y
            || y >= self.max_y
            || x.div_euclid(CHUNK_SIZE as i32) != self.target_x
            || z.div_euclid(CHUNK_SIZE as i32) != self.target_z
        {
            return;
        }
        self.writes.push(ProjectedWrite {
            pos: WorldPos { x, y, z },
            block,
        });
    }

    fn claim_evaluation(&mut self) -> bool {
        if self.block_evaluations >= MAX_STRUCTURE_BLOCK_EVALUATIONS {
            return false;
        }
        self.block_evaluations += 1;
        true
    }

    fn base_column(&mut self, x: i32, z: i32) -> &[Block] {
        self.base_columns
            .entry((x, z))
            .or_insert_with(|| self.generator.geometry_base_column(x, z))
    }

    fn final_world_surface(&mut self, x: i32, z: i32) -> i32 {
        if x.div_euclid(CHUNK_SIZE as i32) == self.target_x
            && z.div_euclid(CHUNK_SIZE as i32) == self.target_z
        {
            return self.target_world_surface[target_column_index(x, z)];
        }
        let min_y = self.min_y;
        self.base_column(x, z)
            .iter()
            .rposition(|block| !block.is_air())
            .map_or(min_y, |local_y| min_y + local_y as i32 + 1)
    }

    fn final_ocean_floor(&mut self, x: i32, z: i32) -> i32 {
        if x.div_euclid(CHUNK_SIZE as i32) == self.target_x
            && z.div_euclid(CHUNK_SIZE as i32) == self.target_z
        {
            return self.target_ocean_floor[target_column_index(x, z)];
        }
        let min_y = self.min_y;
        self.base_column(x, z)
            .iter()
            .rposition(|block| !block.is_air() && !matches!(block.id, BlockId::Water | BlockId::Lava))
            .map_or(min_y, |local_y| min_y + local_y as i32 + 1)
    }

    fn buried_treasure_y(&mut self, x: i32, z: i32) -> Option<i32> {
        if x.div_euclid(CHUNK_SIZE as i32) == self.target_x
            && z.div_euclid(CHUNK_SIZE as i32) == self.target_z
        {
            return self.target_buried_treasure[target_column_index(x, z)];
        }
        let min_y = self.min_y;
        let ocean_floor = self.final_ocean_floor(x, z);
        let column = self.base_column(x, z);
        for y in (min_y + 1..=ocean_floor).rev() {
            let support = column[(y - 1 - min_y) as usize].id;
            if is_buried_treasure_support(support) {
                return Some(y);
            }
        }
        None
    }

    fn apply(self, chunk: &mut Chunk) {
        for write in self.writes {
            let local_x = write.pos.x.rem_euclid(CHUNK_SIZE as i32) as usize;
            let local_y = (write.pos.y - self.min_y) as usize;
            let local_z = write.pos.z.rem_euclid(CHUNK_SIZE as i32) as usize;
            if chunk.get_block(local_x, local_y, local_z).id != BlockId::Bedrock {
                chunk.set_block(local_x, local_y, local_z, write.block);
            }
        }
        chunk.recount_fluids();
    }
}

fn target_column_index(x: i32, z: i32) -> usize {
    x.rem_euclid(CHUNK_SIZE as i32) as usize * CHUNK_SIZE
        + z.rem_euclid(CHUNK_SIZE as i32) as usize
}

fn is_buried_treasure_support(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Stone
            | BlockId::Sandstone
            | BlockId::Andesite
            | BlockId::Granite
            | BlockId::Diorite
    )
}

/// Applies structure starts after base terrain and before the current
/// monolithic placed-feature pass. Java interleaves underground/surface
/// structures with decoration steps; placing both structure steps here is the
/// closest ordering available without introducing proto-chunk feature stages.
pub(crate) fn apply_minecraft26_structure_geometry(
    generator: &VanillaWorldGenerator,
    world_seed: u64,
    min_y: i32,
    height: i32,
    chunk: &mut Chunk,
) {
    let mut stage = StructureStage::new(generator, world_seed, min_y, height, chunk);
    plan_target(&mut stage);
    stage.apply(chunk);
}

fn plan_target(stage: &mut StructureStage<'_>) {
    let min_owner_x = stage.target_x.saturating_sub(STRUCTURE_OWNER_RADIUS);
    let max_owner_x = stage.target_x.saturating_add(STRUCTURE_OWNER_RADIUS);
    let min_owner_z = stage.target_z.saturating_sub(STRUCTURE_OWNER_RADIUS);
    let max_owner_z = stage.target_z.saturating_add(STRUCTURE_OWNER_RADIUS);
    let strongholds = nearby_stronghold_candidates(
        stage,
        min_owner_x,
        max_owner_x,
        min_owner_z,
        max_owner_z,
    );

    for set in OVERWORLD_STRUCTURE_SETS {
        match set.placement {
            StructurePlacement::RandomSpread(_) => {
                for owner_z in min_owner_z..=max_owner_z {
                    for owner_x in min_owner_x..=max_owner_x {
                        if stage.starts == MAX_STRUCTURE_STARTS {
                            return;
                        }
                        if is_random_candidate(stage.world_seed, set.id, owner_x, owner_z) {
                            plan_start(stage, set, ChunkPos::new(owner_x, owner_z));
                        }
                    }
                }
            }
            StructurePlacement::ConcentricRings(_) => {
                for owner in &strongholds {
                    if stage.starts == MAX_STRUCTURE_STARTS {
                        return;
                    }
                    plan_start(stage, set, *owner);
                }
            }
        }
    }
}

fn is_random_candidate(level_seed: i64, id: StructureSetId, x: i32, z: i32) -> bool {
    let set = overworld_structure_set(id);
    let StructurePlacement::RandomSpread(placement) = set.placement else {
        return false;
    };
    placement.is_candidate(level_seed, x, z, |other, other_x, other_z| {
        let other_set = overworld_structure_set(other);
        match other_set.placement {
            StructurePlacement::RandomSpread(other_placement) => {
                other_placement.is_candidate_before_exclusion(level_seed, other_x, other_z)
            }
            StructurePlacement::ConcentricRings(_) => false,
        }
    })
}

fn nearby_stronghold_candidates(
    stage: &StructureStage<'_>,
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
) -> Vec<ChunkPos> {
    let set = overworld_structure_set(StructureSetId::Strongholds);
    let StructurePlacement::ConcentricRings(placement) = set.placement else {
        return Vec::new();
    };
    // Relocation cannot move farther than 112 blocks (seven chunks). Far ring
    // positions cannot enter this target's fixed owner window and need no biome
    // work. Potentially relevant positions use Java's exact quart-grid scan and
    // reservoir sampling order.
    let search_halo = 7;
    placement
        .candidates(stage.world_seed, |search, random| {
            if search.initial_chunk.x < min_x.saturating_sub(search_halo)
                || search.initial_chunk.x > max_x.saturating_add(search_halo)
                || search.initial_chunk.z < min_z.saturating_sub(search_halo)
                || search.initial_chunk.z > max_z.saturating_add(search_halo)
            {
                None
            } else {
                relocate_stronghold(stage.generator, search, random)
            }
        })
        .into_iter()
        .filter(|position| {
            (min_x..=max_x).contains(&position.x) && (min_z..=max_z).contains(&position.z)
        })
        .collect()
}

fn relocate_stronghold(
    generator: &VanillaWorldGenerator,
    search: StrongholdBiomeSearch,
    random: &mut JavaLegacyRandom,
) -> Option<BlockPos> {
    let center_quart_x = search.center.x.div_euclid(4);
    let center_quart_z = search.center.z.div_euclid(4);
    let radius = search.horizontal_radius.div_euclid(4);
    let mut selected = None;
    let mut found = 0;
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let quart_x = center_quart_x.wrapping_add(dx);
            let quart_z = center_quart_z.wrapping_add(dz);
            let biome = generator.get_biome_at(
                quart_x.wrapping_mul(4),
                search.center.y,
                quart_z.wrapping_mul(4),
            );
            if is_stronghold_preferred(biome) {
                found += 1;
                if selected.is_none() || random.next_int_bound(found) == 0 {
                    selected = Some(BlockPos::new(
                        quart_x.wrapping_mul(4),
                        search.center.y,
                        quart_z.wrapping_mul(4),
                    ));
                }
            }
        }
    }
    selected
}

fn plan_start(stage: &mut StructureStage<'_>, set: &StructureSet, owner: ChunkPos) {
    let common = set.placement.common();
    let locate = common.locate_pos(owner);
    let anchor_x = if common.locate_offset.x == 0 {
        locate.x.wrapping_add(8)
    } else {
        locate.x
    };
    let anchor_z = if common.locate_offset.z == 0 {
        locate.z.wrapping_add(8)
    } else {
        locate.z
    };
    let surface_y = stage
        .final_world_surface(anchor_x, anchor_z)
        .clamp(stage.min_y + 1, stage.max_y - 2);
    let ocean_floor_y = stage
        .final_ocean_floor(anchor_x, anchor_z)
        .clamp(stage.min_y + 1, stage.max_y - 2);
    let mut random = JavaLegacyRandom::new(0);
    random.set_large_feature_with_salt(
        stage.world_seed,
        owner.x,
        owner.z,
        geometry_salt(set.id),
    );
    let start_y = start_y(set.id, surface_y, ocean_floor_y, &mut random);
    let biome = stage.generator.get_biome_at(anchor_x, start_y, anchor_z);

    let Some(entry) = set
        .weighted_structure_order(stage.world_seed, owner)
        .into_iter()
        .find(|entry| entry_allowed(stage, entry.structure, biome, surface_y, anchor_x, anchor_z))
    else {
        return;
    };
    stage.starts += 1;

    match set.id {
        StructureSetId::Villages
        | StructureSetId::Igloos
        | StructureSetId::PillagerOutposts
        | StructureSetId::AncientCities
        | StructureSetId::RuinedPortals
        | StructureSetId::Shipwrecks
        | StructureSetId::OceanRuins
        | StructureSetId::TrailRuins
        | StructureSetId::TrialChambers => {
            if let Some(selection) = template_selection(entry.structure, &mut random) {
                plan_template(stage, selection, anchor_x, start_y, anchor_z, &mut random);
            }
        }
        StructureSetId::DesertPyramids => emit_desert_pyramid(stage, anchor_x, surface_y, anchor_z),
        StructureSetId::JungleTemples => emit_jungle_temple(stage, anchor_x, surface_y, anchor_z, &mut random),
        StructureSetId::SwampHuts => emit_swamp_hut(stage, anchor_x, surface_y, anchor_z),
        StructureSetId::OceanMonuments => emit_monument(stage, anchor_x, surface_y, anchor_z),
        StructureSetId::WoodlandMansions => emit_mansion(stage, anchor_x, surface_y, anchor_z),
        StructureSetId::BuriedTreasures => {
            if let Some(y) = stage.buried_treasure_y(anchor_x, anchor_z) {
                emit_buried_treasure(stage, anchor_x, y, anchor_z);
            }
        }
        StructureSetId::Mineshafts => emit_mineshaft(
            stage,
            anchor_x,
            start_y,
            anchor_z,
            entry.structure == "minecraft:mineshaft_mesa",
            &mut random,
        ),
        StructureSetId::Strongholds => emit_stronghold(stage, anchor_x, start_y, anchor_z),
    }
}

fn geometry_salt(id: StructureSetId) -> i32 {
    0x5600_0000_u32.wrapping_add(id as u32) as i32
}

fn start_y(
    id: StructureSetId,
    surface_y: i32,
    ocean_floor_y: i32,
    random: &mut JavaLegacyRandom,
) -> i32 {
    match id {
        StructureSetId::AncientCities => -27,
        StructureSetId::Mineshafts => -48 + random.next_int_bound(81),
        StructureSetId::Strongholds => -32,
        StructureSetId::TrailRuins => surface_y - 15,
        StructureSetId::TrialChambers => -40 + random.next_int_bound(21),
        StructureSetId::Shipwrecks | StructureSetId::OceanRuins | StructureSetId::OceanMonuments => {
            ocean_floor_y
        }
        StructureSetId::BuriedTreasures => ocean_floor_y,
        _ => surface_y,
    }
}

fn entry_allowed(
    stage: &StructureStage<'_>,
    structure: &str,
    biome: Biome,
    _surface_y: i32,
    x: i32,
    z: i32,
) -> bool {
    if !biome_allows(structure, biome) {
        return false;
    }
    match structure {
        "minecraft:monument" => {
            [(-29, -29), (-29, 29), (29, -29), (29, 29)]
                    .into_iter()
                    .all(|(dx, dz)| {
                        let surrounding = stage.generator.get_biome_at(
                            x + dx,
                            stage.generator.aquifer.sea_level,
                            z + dz,
                        );
                        is_ocean(surrounding)
                            || matches!(surrounding, Biome::River | Biome::FrozenRiver)
                    })
        }
        _ => true,
    }
}

fn biome_allows(structure: &str, biome: Biome) -> bool {
    match structure {
        "minecraft:village_plains" => matches!(biome, Biome::Plains | Biome::Meadow),
        "minecraft:village_desert" | "minecraft:desert_pyramid" => biome == Biome::Desert,
        "minecraft:village_savanna" => biome == Biome::Savanna,
        "minecraft:village_snowy" => biome == Biome::SnowyPlains,
        "minecraft:village_taiga" => biome == Biome::Taiga,
        "minecraft:igloo" => matches!(biome, Biome::SnowyTaiga | Biome::SnowyPlains | Biome::SnowySlopes),
        "minecraft:jungle_pyramid" => matches!(biome, Biome::Jungle | Biome::BambooJungle),
        "minecraft:swamp_hut" => biome == Biome::Swamp,
        "minecraft:pillager_outpost" => matches!(biome, Biome::Desert | Biome::Plains | Biome::Savanna | Biome::SnowyPlains | Biome::Taiga | Biome::Grove) || is_mountain(biome),
        "minecraft:ancient_city" => biome == Biome::DeepDark,
        "minecraft:monument" => is_deep_ocean(biome),
        "minecraft:mansion" => biome == Biome::DarkForest,
        "minecraft:buried_treasure" | "minecraft:shipwreck_beached" => is_beach(biome),
        "minecraft:mineshaft_mesa" => is_badlands(biome),
        "minecraft:mineshaft" => has_mineshaft(biome),
        "minecraft:ruined_portal_desert" => biome == Biome::Desert,
        "minecraft:ruined_portal_jungle" => is_jungle(biome),
        "minecraft:ruined_portal_swamp" => matches!(biome, Biome::Swamp | Biome::MangroveSwamp),
        "minecraft:ruined_portal_mountain" => is_mountain(biome) || is_badlands(biome) || matches!(biome, Biome::SavannaPlateau | Biome::WindsweptSavanna | Biome::StonyShore),
        "minecraft:ruined_portal_ocean" => is_ocean(biome),
        "minecraft:ruined_portal_nether" => false,
        "minecraft:ruined_portal" => is_standard_portal_biome(biome),
        "minecraft:shipwreck" => is_ocean(biome),
        "minecraft:ocean_ruin_cold" => matches!(biome, Biome::FrozenOcean | Biome::ColdOcean | Biome::Ocean | Biome::DeepFrozenOcean | Biome::DeepColdOcean | Biome::DeepOcean),
        "minecraft:ocean_ruin_warm" => matches!(biome, Biome::LukewarmOcean | Biome::WarmOcean | Biome::DeepLukewarmOcean),
        "minecraft:stronghold" => is_overworld_26_2(biome),
        "minecraft:trail_ruins" => matches!(biome, Biome::Taiga | Biome::SnowyTaiga | Biome::OldGrowthPineTaiga | Biome::OldGrowthSpruceTaiga | Biome::OldGrowthBirchForest | Biome::Jungle),
        "minecraft:trial_chambers" => is_trial_chambers_biome(biome),
        _ => false,
    }
}

fn is_standard_portal_biome(biome: Biome) -> bool {
    is_beach(biome)
        || is_forest(biome)
        || matches!(biome, Biome::River | Biome::FrozenRiver | Biome::Taiga | Biome::SnowyTaiga | Biome::OldGrowthPineTaiga | Biome::OldGrowthSpruceTaiga | Biome::MushroomFields | Biome::IceSpikes | Biome::DripstoneCaves | Biome::LushCaves | Biome::Savanna | Biome::SnowyPlains | Biome::Plains | Biome::SunflowerPlains)
}

fn is_stronghold_preferred(biome: Biome) -> bool {
    matches!(
        biome,
        Biome::Plains
            | Biome::SunflowerPlains
            | Biome::SnowyPlains
            | Biome::IceSpikes
            | Biome::Desert
            | Biome::Forest
            | Biome::FlowerForest
            | Biome::BirchForest
            | Biome::DarkForest
            | Biome::PaleGarden
            | Biome::OldGrowthBirchForest
            | Biome::OldGrowthPineTaiga
            | Biome::OldGrowthSpruceTaiga
            | Biome::Taiga
            | Biome::SnowyTaiga
            | Biome::Savanna
            | Biome::SavannaPlateau
            | Biome::WindsweptHills
            | Biome::WindsweptGravellyHills
            | Biome::WindsweptForest
            | Biome::WindsweptSavanna
            | Biome::Jungle
            | Biome::SparseJungle
            | Biome::BambooJungle
            | Biome::Badlands
            | Biome::ErodedBadlands
            | Biome::WoodedBadlands
            | Biome::Meadow
            | Biome::Grove
            | Biome::SnowySlopes
            | Biome::FrozenPeaks
            | Biome::JaggedPeaks
            | Biome::StonyPeaks
            | Biome::MushroomFields
            | Biome::DripstoneCaves
            | Biome::LushCaves
    )
}

fn is_overworld_26_2(biome: Biome) -> bool {
    biome != Biome::SulfurCaves
}

fn is_trial_chambers_biome(biome: Biome) -> bool {
    is_overworld_26_2(biome) && biome != Biome::DeepDark
}

fn has_mineshaft(biome: Biome) -> bool {
    !matches!(
        biome,
        Biome::Badlands
            | Biome::WoodedBadlands
            | Biome::ErodedBadlands
            | Biome::DeepDark
            | Biome::SulfurCaves
    )
}

fn is_ocean(biome: Biome) -> bool {
    matches!(biome, Biome::Ocean | Biome::DeepOcean | Biome::WarmOcean | Biome::LukewarmOcean | Biome::ColdOcean | Biome::FrozenOcean | Biome::DeepLukewarmOcean | Biome::DeepColdOcean | Biome::DeepFrozenOcean)
}

fn is_deep_ocean(biome: Biome) -> bool {
    matches!(biome, Biome::DeepOcean | Biome::DeepLukewarmOcean | Biome::DeepColdOcean | Biome::DeepFrozenOcean)
}

fn is_beach(biome: Biome) -> bool {
    matches!(biome, Biome::Beach | Biome::SnowyBeach)
}

fn is_badlands(biome: Biome) -> bool {
    matches!(biome, Biome::Badlands | Biome::WoodedBadlands | Biome::ErodedBadlands)
}

fn is_jungle(biome: Biome) -> bool {
    matches!(biome, Biome::Jungle | Biome::SparseJungle | Biome::BambooJungle)
}

fn is_forest(biome: Biome) -> bool {
    matches!(biome, Biome::Forest | Biome::BirchForest | Biome::OldGrowthBirchForest | Biome::FlowerForest | Biome::DarkForest | Biome::PaleGarden | Biome::Grove)
}

fn is_mountain(biome: Biome) -> bool {
    matches!(biome, Biome::Meadow | Biome::SnowySlopes | Biome::JaggedPeaks | Biome::FrozenPeaks | Biome::StonyPeaks | Biome::CherryGrove)
}

fn template_selection(structure: &str, random: &mut JavaLegacyRandom) -> Option<TemplateSelection> {
    let path = match structure {
        "minecraft:village_plains" => "data/minecraft/structure/village/plains/town_centers/plains_meeting_point_1.nbt",
        "minecraft:village_desert" => "data/minecraft/structure/village/desert/town_centers/desert_meeting_point_1.nbt",
        "minecraft:village_savanna" => "data/minecraft/structure/village/savanna/town_centers/savanna_meeting_point_1.nbt",
        "minecraft:village_snowy" => "data/minecraft/structure/village/snowy/town_centers/snowy_meeting_point_1.nbt",
        "minecraft:village_taiga" => "data/minecraft/structure/village/taiga/town_centers/taiga_meeting_point_1.nbt",
        "minecraft:pillager_outpost" => "data/minecraft/structure/pillager_outpost/watchtower.nbt",
        "minecraft:igloo" => {
            return Some(TemplateSelection {
                path: "data/minecraft/structure/igloo/top.nbt",
                air_policy: TemplateAirPolicy::Destructive,
            });
        }
        "minecraft:ancient_city" => {
            let path = match random.next_int_bound(3) {
                0 => "data/minecraft/structure/ancient_city/city_center/city_center_1.nbt",
                1 => "data/minecraft/structure/ancient_city/city_center/city_center_2.nbt",
                _ => "data/minecraft/structure/ancient_city/city_center/city_center_3.nbt",
            };
            return Some(TemplateSelection {
                path,
                air_policy: TemplateAirPolicy::Destructive,
            });
        }
        "minecraft:ruined_portal" | "minecraft:ruined_portal_desert" | "minecraft:ruined_portal_jungle" | "minecraft:ruined_portal_swamp" | "minecraft:ruined_portal_mountain" | "minecraft:ruined_portal_ocean" => match random.next_int_bound(10) {
            0 => "data/minecraft/structure/ruined_portal/portal_1.nbt",
            1 => "data/minecraft/structure/ruined_portal/portal_2.nbt",
            2 => "data/minecraft/structure/ruined_portal/portal_3.nbt",
            3 => "data/minecraft/structure/ruined_portal/portal_4.nbt",
            4 => "data/minecraft/structure/ruined_portal/portal_5.nbt",
            5 => "data/minecraft/structure/ruined_portal/portal_6.nbt",
            6 => "data/minecraft/structure/ruined_portal/portal_7.nbt",
            7 => "data/minecraft/structure/ruined_portal/portal_8.nbt",
            8 => "data/minecraft/structure/ruined_portal/portal_9.nbt",
            _ => "data/minecraft/structure/ruined_portal/portal_10.nbt",
        },
        "minecraft:shipwreck" => {
            let variants = [
                "data/minecraft/structure/shipwreck/with_mast.nbt",
                "data/minecraft/structure/shipwreck/with_mast_degraded.nbt",
                "data/minecraft/structure/shipwreck/rightsideup_full.nbt",
                "data/minecraft/structure/shipwreck/rightsideup_full_degraded.nbt",
                "data/minecraft/structure/shipwreck/sideways_full.nbt",
                "data/minecraft/structure/shipwreck/sideways_full_degraded.nbt",
                "data/minecraft/structure/shipwreck/upsidedown_full.nbt",
                "data/minecraft/structure/shipwreck/upsidedown_full_degraded.nbt",
            ];
            return Some(TemplateSelection {
                path: variants[random.next_int_bound(variants.len() as i32) as usize],
                air_policy: TemplateAirPolicy::Underwater,
            });
        }
        "minecraft:shipwreck_beached" => {
            return Some(TemplateSelection {
                path: if random.next_int_bound(2) == 0 {
                    "data/minecraft/structure/shipwreck/rightsideup_full.nbt"
                } else {
                    "data/minecraft/structure/shipwreck/rightsideup_full_degraded.nbt"
                },
                air_policy: TemplateAirPolicy::Destructive,
            });
        }
        "minecraft:ocean_ruin_cold" => {
            return Some(TemplateSelection {
                path: "data/minecraft/structure/underwater_ruin/brick_1.nbt",
                air_policy: TemplateAirPolicy::Underwater,
            });
        }
        "minecraft:ocean_ruin_warm" => {
            return Some(TemplateSelection {
                path: "data/minecraft/structure/underwater_ruin/warm_1.nbt",
                air_policy: TemplateAirPolicy::Underwater,
            });
        }
        "minecraft:trail_ruins" => {
            return Some(TemplateSelection {
                path: "data/minecraft/structure/trail_ruins/tower/tower_1.nbt",
                air_policy: TemplateAirPolicy::Destructive,
            });
        }
        "minecraft:trial_chambers" => {
            return Some(TemplateSelection {
                path: if random.next_int_bound(2) == 0 {
                    "data/minecraft/structure/trial_chambers/corridor/end_1.nbt"
                } else {
                    "data/minecraft/structure/trial_chambers/corridor/end_2.nbt"
                },
                air_policy: TemplateAirPolicy::Destructive,
            });
        }
        _ => return None,
    };
    let air_policy = if matches!(
        structure,
        "minecraft:village_plains"
            | "minecraft:village_desert"
            | "minecraft:village_savanna"
            | "minecraft:village_snowy"
            | "minecraft:village_taiga"
            | "minecraft:pillager_outpost"
    ) {
        TemplateAirPolicy::Ignore
    } else {
        TemplateAirPolicy::Destructive
    };
    Some(TemplateSelection { path, air_policy })
}

fn plan_template(
    stage: &mut StructureStage<'_>,
    selection: TemplateSelection,
    anchor_x: i32,
    base_y: i32,
    anchor_z: i32,
    random: &mut JavaLegacyRandom,
) {
    let relative_path = selection.path;
    let Some(template) = load_cached_template(&stage.asset_root, Path::new(relative_path)) else {
        return;
    };
    plan_loaded_template(stage, &template, selection, anchor_x, base_y, anchor_z, random);
}

fn plan_loaded_template(
    stage: &mut StructureStage<'_>,
    template: &StructureTemplate,
    selection: TemplateSelection,
    anchor_x: i32,
    base_y: i32,
    anchor_z: i32,
    random: &mut JavaLegacyRandom,
) {
    let relative_path = selection.path;
    let transform = TemplateTransform {
        rotation: match random.next_int_bound(4) {
            0 => Rotation::None,
            1 => Rotation::Clockwise90,
            2 => Rotation::Clockwise180,
            _ => Rotation::Counterclockwise90,
        },
        ..TemplateTransform::default()
    };
    let selected_palette = random.next_int_bound(template.palettes.len() as i32) as usize;
    let palette = &template.palettes[selected_palette];
    if stage
        .block_evaluations
        .checked_add(template.blocks.len())
        .is_none_or(|evaluations| evaluations > MAX_STRUCTURE_BLOCK_EVALUATIONS)
    {
        return;
    }
    stage.block_evaluations += template.blocks.len();
    let supported_non_air = template.blocks.iter().any(|block| {
        matches!(
            palette[block.palette_index].flatten(transform),
            Ok(FlattenedTemplateBlock::Place(block)) if block.id != BlockId::Air
        )
    });
    if !supported_non_air {
        warn_template_once(
            stage.asset_root.join(relative_path),
            format!("selected palette {selected_palette} has zero supported non-air blocks"),
        );
        return;
    }
    let size = template.transformed_size(transform);
    if size[0] > MAX_TEMPLATE_HORIZONTAL_SPAN || size[2] > MAX_TEMPLATE_HORIZONTAL_SPAN {
        warn_template_once(
            stage.asset_root.join(relative_path),
            format!("template span {}x{} exceeds bounded {}-block limit", size[0], size[2], MAX_TEMPLATE_HORIZONTAL_SPAN),
        );
        return;
    }
    let origin = [
        i64::from(anchor_x - size[0] / 2),
        i64::from(base_y),
        i64::from(anchor_z - size[2] / 2),
    ];
    let Some(projected_blocks) = template.blocks_in_chunk_with_palette(
        origin,
        transform,
        [stage.target_x, stage.target_z],
        selected_palette,
    ) else {
        return;
    };
    for projected in projected_blocks {
        let Ok(projected) = projected else {
            if !stage.claim_evaluation() {
                break;
            }
            continue;
        };
        let [x, y, z] = projected.world_position;
        let (Ok(x), Ok(y), Ok(z)) = (i32::try_from(x), i32::try_from(y), i32::try_from(z)) else {
            if !stage.claim_evaluation() {
                break;
            }
            continue;
        };
        match projected.palette.flatten(transform) {
            Ok(FlattenedTemplateBlock::NoOp) | Err(_) => {
                if !stage.claim_evaluation() {
                    break;
                }
            }
            Ok(FlattenedTemplateBlock::Place(block)) if block.id == BlockId::Air => {
                match selection.air_policy {
                    TemplateAirPolicy::Destructive => stage.push_block(x, y, z, block),
                    TemplateAirPolicy::Ignore => {
                        if !stage.claim_evaluation() {
                            break;
                        }
                    }
                    TemplateAirPolicy::Underwater => {
                        if !stage.claim_evaluation() {
                            break;
                        }
                    }
                }
            }
            Ok(FlattenedTemplateBlock::Place(block)) => stage.push_block(x, y, z, block),
        }
    }
}

fn structure_asset_root() -> PathBuf {
    std::env::var_os("VIBECRAFT_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/opencode/minecraft-assets"))
}

type TemplateCache = HashMap<PathBuf, Arc<StructureTemplate>>;

fn template_cache() -> &'static RwLock<TemplateCache> {
    static CACHE: OnceLock<RwLock<TemplateCache>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn template_warnings() -> &'static Mutex<HashSet<(PathBuf, String)>> {
    static WARNINGS: OnceLock<Mutex<HashSet<(PathBuf, String)>>> = OnceLock::new();
    WARNINGS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn load_cached_template(asset_root: &Path, relative_path: &Path) -> Option<Arc<StructureTemplate>> {
    let full_path = asset_root.join(relative_path);
    if let Some(template) = template_cache()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&full_path)
        .cloned()
    {
        return Some(template);
    }

    let mut limits = TemplateLimits::default();
    limits.max_dimension = MAX_TEMPLATE_HORIZONTAL_SPAN;
    limits.max_volume = u64::from(MAX_TEMPLATE_HORIZONTAL_SPAN as u32).pow(3);
    limits.max_blocks = MAX_TEMPLATE_BLOCKS;
    let template = match StructureTemplate::load_from_asset_root_with_limits(
        asset_root,
        relative_path,
        limits,
    ) {
        Ok(template) => Arc::new(template),
        Err(error) => {
            warn_template_once(full_path, format!("failed to decode: {error}"));
            return None;
        }
    };
    report_unsupported_palette(&full_path, &template);
    let mut cache = template_cache()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Some(
        cache
            .entry(full_path)
            .or_insert_with(|| template.clone())
            .clone(),
    )
}

fn report_unsupported_palette(path: &Path, template: &StructureTemplate) {
    let mut unsupported = BTreeMap::<String, usize>::new();
    for palette in &template.palettes {
        for block in &template.blocks {
            if let Err(reason) = palette[block.palette_index].flatten(TemplateTransform::default()) {
                *unsupported.entry(reason.to_string()).or_default() += 1;
            }
        }
    }
    for (reason, count) in unsupported {
        warn_template_once(path.to_path_buf(), format!("skipping {count} blocks: {reason}"));
    }
}

fn warn_template_once(path: PathBuf, message: String) {
    let inserted = template_warnings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((path.clone(), message.clone()));
    if inserted {
        log::warn!("structure template {}: {message}", path.display());
    }
}

fn emit_desert_pyramid(stage: &mut StructureStage<'_>, x: i32, y: i32, z: i32) {
    for layer in 0_i32..=10 {
        let radius: i32 = 10 - layer;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if layer == 0 || dx.abs() == radius || dz.abs() == radius {
                    let block = if layer == 0 && (dx == 0 || dz == 0) {
                        BlockId::OrangeTerracotta
                    } else {
                        BlockId::Sandstone
                    };
                    stage.push(x + dx, y + layer, z + dz, block);
                }
            }
        }
    }
    for dy in 1..=7 {
        for dx in -2..=2 {
            for dz in -2..=2 {
                stage.push(x + dx, y + dy, z + dz, BlockId::Air);
            }
        }
    }
}

fn emit_jungle_temple(
    stage: &mut StructureStage<'_>,
    x: i32,
    y: i32,
    z: i32,
    random: &mut JavaLegacyRandom,
) {
    emit_hollow_box(stage, x - 6, y, z - 7, x + 5, y + 8, z + 7, BlockId::MossyCobblestone);
    for dx in -4_i32..=3 {
        for dz in -5_i32..=5 {
            if random.next_int_bound(4) == 0 {
                stage.push(x + dx, y, z + dz, BlockId::Cobblestone);
            }
        }
    }
    stage.push(x, y + 1, z + 4, BlockId::Chest);
}

fn emit_swamp_hut(stage: &mut StructureStage<'_>, x: i32, y: i32, z: i32) {
    for dx in [-4, 4] {
        for dz in [-3, 3] {
            for dy in -4..=1 {
                stage.push(x + dx, y + dy, z + dz, BlockId::OakLog);
            }
        }
    }
    emit_hollow_box(stage, x - 4, y + 1, z - 3, x + 4, y + 5, z + 3, BlockId::SprucePlanks);
    for dx in -5..=5 {
        for dz in -4..=4 {
            stage.push(x + dx, y + 6, z + dz, BlockId::SprucePlanks);
        }
    }
}

fn emit_monument(stage: &mut StructureStage<'_>, x: i32, surface_y: i32, z: i32) {
    let base_y = surface_y.min(stage.generator.aquifer.sea_level - 12);
    for layer in 0..3 {
        let radius = 29 - layer * 7;
        emit_hollow_box(
            stage,
            x - radius,
            base_y + layer * 5,
            z - radius,
            x + radius,
            base_y + layer * 5 + 5,
            z + radius,
            if layer == 1 { BlockId::PrismarineBricks } else { BlockId::Prismarine },
        );
    }
    for dx in [-20, 20] {
        for dz in [-20, 20] {
            for dy in 1..=12 {
                stage.push(x + dx, base_y + dy, z + dz, BlockId::DarkPrismarine);
            }
        }
    }
    stage.push(x, base_y + 8, z, BlockId::SeaLantern);
}

fn emit_mansion(stage: &mut StructureStage<'_>, x: i32, y: i32, z: i32) {
    for floor in 0..3 {
        let floor_y = y + floor * 6;
        emit_hollow_box(stage, x - 16, floor_y, z - 12, x + 16, floor_y + 5, z + 12, BlockId::DarkOakPlanks);
        for dx in (-12..=12).step_by(8) {
            for dy in 1..=4 {
                stage.push(x + dx, floor_y + dy, z - 12, BlockId::Glass);
                stage.push(x + dx, floor_y + dy, z + 12, BlockId::Glass);
            }
        }
    }
    for dx in -17..=17 {
        for dz in -13..=13 {
            stage.push(x + dx, y + 18, z + dz, BlockId::DarkOakPlanks);
        }
    }
}

fn emit_buried_treasure(stage: &mut StructureStage<'_>, x: i32, y: i32, z: i32) {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                stage.push(x + dx, y + dy, z + dz, if dy == -1 { BlockId::Stone } else { BlockId::Sand });
            }
        }
    }
    stage.push(x, y, z, BlockId::Chest);
}

fn emit_mineshaft(
    stage: &mut StructureStage<'_>,
    x: i32,
    y: i32,
    z: i32,
    mesa: bool,
    random: &mut JavaLegacyRandom,
) {
    let support = if mesa { BlockId::DarkOakLog } else { BlockId::OakLog };
    for axis in 0..2 {
        for along in -24..=24 {
            for across in -1..=1 {
                for dy in 0..=2 {
                    let (wx, wz) = if axis == 0 { (x + along, z + across) } else { (x + across, z + along) };
                    stage.push(wx, y + dy, wz, BlockId::Air);
                }
            }
            if along.rem_euclid(5) == 0 {
                for across in [-2, 2] {
                    let (wx, wz) = if axis == 0 { (x + along, z + across) } else { (x + across, z + along) };
                    for dy in 0..=3 {
                        stage.push(wx, y + dy, wz, support);
                    }
                }
            }
        }
    }
    if random.next_int_bound(3) == 0 {
        stage.push(x + 6, y, z, BlockId::Chest);
    }
}

fn emit_stronghold(stage: &mut StructureStage<'_>, x: i32, y: i32, z: i32) {
    emit_hollow_box(stage, x - 15, y, z - 4, x + 15, y + 5, z + 4, BlockId::StoneBricks);
    emit_hollow_box(stage, x - 4, y, z - 15, x + 4, y + 5, z + 15, BlockId::StoneBricks);
    emit_hollow_box(stage, x + 10, y - 1, z - 8, x + 24, y + 7, z + 8, BlockId::StoneBricks);
    for dz in -2..=2 {
        stage.push(x + 20, y, z + dz, BlockId::Lava);
    }
    stage.push(x + 18, y + 1, z, BlockId::Spawner);
    for dx in [-10, 10] {
        stage.push(x + dx, y + 1, z, BlockId::Bookshelf);
    }
}

fn emit_hollow_box(
    stage: &mut StructureStage<'_>,
    min_x: i32,
    min_y: i32,
    min_z: i32,
    max_x: i32,
    max_y: i32,
    max_z: i32,
    wall: BlockId,
) {
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let shell = x == min_x || x == max_x || y == min_y || y == max_y || z == min_z || z == max_z;
                stage.push(x, y, z, if shell { wall } else { BlockId::Air });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generation::WorldGenerationProfile;
    use crate::world::world_gen::template::{TemplateBlock, TemplatePaletteEntry};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn generator(seed: u64) -> VanillaWorldGenerator {
        VanillaWorldGenerator::from_seed(seed, WorldGenerationProfile::Minecraft26Geometry)
    }

    fn template(
        size: [i32; 3],
        palette: Vec<TemplatePaletteEntry>,
        blocks: Vec<TemplateBlock>,
    ) -> StructureTemplate {
        StructureTemplate {
            size,
            palettes: vec![palette],
            blocks,
        }
    }

    fn palette(name: &str, properties: &[(&str, &str)]) -> TemplatePaletteEntry {
        TemplatePaletteEntry {
            name: name.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn coverage_has_all_17_structure_sets_once() {
        assert_eq!(MINECRAFT26_STRUCTURE_GEOMETRY_COVERAGE.len(), 17);
        let covered = MINECRAFT26_STRUCTURE_GEOMETRY_COVERAGE
            .iter()
            .map(|entry| entry.set)
            .collect::<HashSet<_>>();
        let expected = OVERWORLD_STRUCTURE_SETS
            .iter()
            .map(|set| set.id)
            .collect::<HashSet<_>>();
        assert_eq!(covered, expected);
        assert!(MINECRAFT26_STRUCTURE_GEOMETRY_COVERAGE
            .iter()
            .all(|entry| !entry.geometry_scope.is_empty()));
    }

    #[test]
    fn biome_rejection_does_not_fall_through_weighted_entries() {
        assert!(!biome_allows("minecraft:desert_pyramid", Biome::Forest));
        assert!(!biome_allows("minecraft:village_plains", Biome::Desert));
        assert!(!biome_allows("minecraft:monument", Biome::Ocean));
        assert!(biome_allows("minecraft:monument", Biome::DeepOcean));
        assert!(!biome_allows("minecraft:ruined_portal_nether", Biome::Plains));
    }

    #[test]
    fn biome_gates_match_the_supplied_26_2_tags() {
        assert!(!biome_allows("minecraft:mansion", Biome::PaleGarden));
        assert!(biome_allows("minecraft:mansion", Biome::DarkForest));
        assert!(biome_allows("minecraft:pillager_outpost", Biome::CherryGrove));
        assert!(biome_allows("minecraft:pillager_outpost", Biome::Grove));
        assert!(!biome_allows("minecraft:ruined_portal", Biome::CherryGrove));
        assert!(biome_allows("minecraft:ruined_portal", Biome::Grove));
        assert!(!biome_allows("minecraft:stronghold", Biome::SulfurCaves));
        assert!(!is_stronghold_preferred(Biome::CherryGrove));
        assert!(is_stronghold_preferred(Biome::Grove));
        assert!(!is_stronghold_preferred(Biome::SulfurCaves));
    }

    #[test]
    fn legacy_and_underwater_air_are_non_destructive() {
        let generator = generator(44);
        let structure = template(
            [2, 1, 1],
            vec![palette("minecraft:air", &[]), palette("minecraft:stone", &[])],
            vec![
                TemplateBlock { position: [0, 0, 0], palette_index: 0 },
                TemplateBlock { position: [1, 0, 0], palette_index: 1 },
            ],
        );
        let project = |air_policy| {
            let mut chunk = Chunk::new(0, 0);
            let initial = if air_policy == TemplateAirPolicy::Underwater {
                BlockId::Water
            } else {
                BlockId::Dirt
            };
            for x in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    chunk.set_block(x, 64, z, Block::new(initial));
                }
            }
            let mut stage = StructureStage::new_with_asset_root(
                &generator,
                44,
                -64,
                384,
                &chunk,
                PathBuf::from("/missing"),
            );
            plan_loaded_template(
                &mut stage,
                &structure,
                TemplateSelection { path: "test.nbt", air_policy },
                1,
                0,
                1,
                &mut JavaLegacyRandom::new(0),
            );
            stage.apply(&mut chunk);
            chunk
        };
        let legacy = project(TemplateAirPolicy::Ignore);
        assert!((0..CHUNK_SIZE).all(|x| (0..CHUNK_SIZE)
            .all(|z| legacy.get_block(x, 64, z).id != BlockId::Air)));
        assert!(legacy.blocks.iter().any(|block| block.id == BlockId::Stone));

        let underwater = project(TemplateAirPolicy::Underwater);
        assert!(underwater.blocks.iter().any(|block| block.id == BlockId::Water));
        assert!(underwater.blocks.iter().any(|block| block.id == BlockId::Stone));
        assert!((0..CHUNK_SIZE).all(|x| (0..CHUNK_SIZE)
            .all(|z| underwater.get_block(x, 64, z).id != BlockId::Air)));
    }

    #[test]
    fn unsupported_only_template_cannot_carve_a_cavity() {
        let generator = generator(45);
        let structure = template(
            [2, 1, 1],
            vec![
                palette("minecraft:air", &[]),
                palette("minecraft:carved_pumpkin", &[("facing", "north")]),
            ],
            vec![
                TemplateBlock { position: [0, 0, 0], palette_index: 0 },
                TemplateBlock { position: [1, 0, 0], palette_index: 1 },
            ],
        );
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(0, 64, 0, Block::new(BlockId::Dirt));
        let mut stage = StructureStage::new_with_asset_root(
            &generator,
            45,
            -64,
            384,
            &chunk,
            PathBuf::from("/missing"),
        );
        plan_loaded_template(
            &mut stage,
            &structure,
            TemplateSelection { path: "trial-test.nbt", air_policy: TemplateAirPolicy::Destructive },
            1,
            0,
            1,
            &mut JavaLegacyRandom::new(0),
        );
        assert!(stage.writes.is_empty());
    }

    #[test]
    fn buried_treasure_searches_down_to_supported_base_material() {
        let generator = generator(47);
        let chunk = Chunk::new(0, 0);
        let mut stage = StructureStage::new_with_asset_root(
            &generator,
            47,
            -64,
            384,
            &chunk,
            PathBuf::from("/missing"),
        );
        let mut column = vec![Block::air(); 384];
        column[70] = Block::new(BlockId::Stone);
        for block in &mut column[71..128] {
            *block = Block::new(BlockId::Water);
        }
        stage.base_columns.insert((25, 9), column);
        assert_eq!(stage.final_ocean_floor(25, 9), 7);
        assert_eq!(stage.buried_treasure_y(25, 9), Some(7));
    }

    #[test]
    fn template_projection_crosses_a_negative_chunk_border() {
        let generator = generator(46);
        let structure = template(
            [2, 1, 2],
            vec![palette("minecraft:stone", &[])],
            vec![
                TemplateBlock { position: [0, 0, 0], palette_index: 0 },
                TemplateBlock { position: [1, 0, 0], palette_index: 0 },
                TemplateBlock { position: [0, 0, 1], palette_index: 0 },
                TemplateBlock { position: [1, 0, 1], palette_index: 0 },
            ],
        );
        let project = |cx| {
            let mut chunk = Chunk::new(cx, 0);
            let mut stage = StructureStage::new_with_asset_root(
                &generator,
                46,
                -64,
                384,
                &chunk,
                PathBuf::from("/missing"),
            );
            plan_loaded_template(
                &mut stage,
                &structure,
                TemplateSelection { path: "border-test.nbt", air_policy: TemplateAirPolicy::Ignore },
                0,
                0,
                1,
                &mut JavaLegacyRandom::new(0),
            );
            stage.apply(&mut chunk);
            chunk.blocks.iter().filter(|block| block.id == BlockId::Stone).count()
        };
        assert_eq!(project(-1), 2);
        assert_eq!(project(0), 2);
    }

    #[test]
    fn negative_candidate_projects_across_its_chunk_border() {
        let villages = overworld_structure_set(StructureSetId::Villages);
        let StructurePlacement::RandomSpread(placement) = villages.placement else {
            unreachable!();
        };
        let owner = placement.potential_chunk(123_456_789, -100, -45);
        assert_eq!(owner, ChunkPos::new(-94, -62));
        assert!(is_random_candidate(123_456_789, StructureSetId::Villages, owner.x, owner.z));

        let generator = generator(1);
        let mut target = Chunk::new(-95, -62);
        let mut stage = StructureStage::new_with_asset_root(
            &generator,
            1,
            -64,
            384,
            &target,
            PathBuf::from("/missing"),
        );
        emit_hollow_box(
            &mut stage,
            owner.x * 16 - 2,
            64,
            owner.z * 16 + 4,
            owner.x * 16 + 2,
            67,
            owner.z * 16 + 8,
            BlockId::StoneBricks,
        );
        stage.apply(&mut target);
        assert!(target.blocks.iter().any(|block| block.id == BlockId::StoneBricks));
    }

    #[test]
    fn native_geometry_projects_both_sides_of_a_border() {
        let generator = generator(2);
        let mut left = Chunk::new(-1, 0);
        let mut right = Chunk::new(0, 0);
        let mut left_stage = StructureStage::new_with_asset_root(&generator, 2, -64, 384, &left, PathBuf::from("/missing"));
        let mut right_stage = StructureStage::new_with_asset_root(&generator, 2, -64, 384, &right, PathBuf::from("/missing"));
        emit_swamp_hut(&mut left_stage, 0, 64, 8);
        emit_swamp_hut(&mut right_stage, 0, 64, 8);
        left_stage.apply(&mut left);
        right_stage.apply(&mut right);
        assert!(left.blocks.iter().any(|block| block.id == BlockId::SprucePlanks));
        assert!(right.blocks.iter().any(|block| block.id == BlockId::SprucePlanks));
    }

    #[test]
    fn projection_has_no_generation_order_state() {
        let generator = generator(3);
        let project = |cx, cz| {
            let mut chunk = Chunk::new(cx, cz);
            let mut stage = StructureStage::new_with_asset_root(&generator, 3, -64, 384, &chunk, PathBuf::from("/missing"));
            emit_mineshaft(&mut stage, -1, -20, 8, false, &mut JavaLegacyRandom::new(9));
            stage.apply(&mut chunk);
            chunk.blocks
        };
        let expected = project(-1, 0);
        let _unrelated = project(12, -9);
        assert_eq!(project(-1, 0), expected);
    }

    #[test]
    fn old_profiles_do_not_select_the_structure_stage() {
        assert!(!WorldGenerationProfile::legacy().uses_minecraft26_geometry());
        assert!(!WorldGenerationProfile::Minecraft26Base.uses_minecraft26_geometry());
        assert!(!WorldGenerationProfile::Minecraft26NativeDecorationPreview.uses_minecraft26_geometry());
        assert!(WorldGenerationProfile::Minecraft26Geometry.uses_minecraft26_geometry());
    }

    #[test]
    fn missing_template_is_recoverable_and_not_cached_as_success() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "vibecraft-missing-structure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let relative = Path::new("data/minecraft/structure/missing.nbt");
        assert!(load_cached_template(&root, relative).is_none());
        assert!(load_cached_template(&root, relative).is_none());
        assert!(!template_cache()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(&root.join(relative)));
    }

    #[test]
    fn relevant_stronghold_candidates_are_deterministic() {
        let generator = generator(0);
        let chunk = Chunk::new(-13, -106);
        let stage = StructureStage::new_with_asset_root(&generator, 0, -64, 384, &chunk, PathBuf::from("/missing"));
        let first = nearby_stronghold_candidates(&stage, -16, -10, -109, -103);
        let second = nearby_stronghold_candidates(&stage, -16, -10, -109, -103);
        assert_eq!(first, second);
    }

    #[test]
    #[ignore = "requires Minecraft 26.2 data/minecraft/structure assets"]
    fn loads_and_projects_a_real_26_2_template() {
        let root = structure_asset_root();
        let relative = Path::new("data/minecraft/structure/igloo/top.nbt");
        let template = load_cached_template(&root, relative)
            .expect("set VIBECRAFT_ASSETS to the Minecraft 26.2 asset root");
        assert!(!template.blocks.is_empty());
        let projected = template
            .blocks_in_chunk([0, 64, 0], TemplateTransform::default(), [0, 0])
            .filter_map(Result::ok)
            .count();
        assert!(projected > 0);
    }

    #[test]
    #[ignore = "requires Minecraft 26.2 data/minecraft/structure assets"]
    fn real_shipwreck_and_start_slices_are_supported() {
        let root = structure_asset_root();
        let shipwreck = StructureTemplate::load_from_asset_root(
            &root,
            "data/minecraft/structure/shipwreck/with_mast.nbt",
        )
        .expect("set VIBECRAFT_ASSETS to the Minecraft 26.2 asset root");
        assert!(shipwreck.palettes.len() > 1);
        for selected in 0..shipwreck.palettes.len() {
            assert!(shipwreck.blocks.iter().any(|block| matches!(
                shipwreck.palettes[selected][block.palette_index]
                    .flatten(TemplateTransform::default()),
                Ok(FlattenedTemplateBlock::Place(native)) if native.id != BlockId::Air
            )));
        }

        for path in [
            "data/minecraft/structure/trial_chambers/corridor/end_1.nbt",
            "data/minecraft/structure/trial_chambers/corridor/end_2.nbt",
            "data/minecraft/structure/ancient_city/city_center/city_center_1.nbt",
        ] {
            let template = StructureTemplate::load_from_asset_root(&root, path).unwrap();
            assert!(template.blocks.iter().any(|block| matches!(
                template.palettes[0][block.palette_index].flatten(TemplateTransform::default()),
                Ok(FlattenedTemplateBlock::Place(native)) if native.id != BlockId::Air
            )), "{path} must project supported geometry");
        }
    }
}
