//! Pure, bounded inputs for the native decoration preview stage.
//!
//! This module intentionally has no `Chunk` or generator dependency and no
//! operation-application API. The generator owns candidate selection and
//! projection so this module remains reusable and independent of chunk workers.

use crate::world::block::Block;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::world_gen::noise::NoiseSeed;
use std::collections::BTreeSet;

/// Prevent a malformed or future data-driven feature list from growing a
/// single chunk plan without bound.
pub const MAX_DECORATION_CANDIDATES: usize = 64;
pub const MAX_DECORATION_OPERATIONS: usize = 512;

/// A block position in the public world coordinate system.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorldPosition {
    x: i32,
    y: i32,
    z: i32,
}

impl WorldPosition {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn x(self) -> i32 {
        self.x
    }

    pub const fn y(self) -> i32 {
        self.y
    }

    pub const fn z(self) -> i32 {
        self.z
    }

    pub const fn chunk(self) -> ChunkPosition {
        ChunkPosition::from_world(self)
    }
}

/// The stable chunk owner of a feature candidate or operation target.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ChunkPosition {
    x: i32,
    z: i32,
}

impl ChunkPosition {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub const fn from_world(position: WorldPosition) -> Self {
        Self {
            x: position.x.div_euclid(CHUNK_SIZE as i32),
            z: position.z.div_euclid(CHUNK_SIZE as i32),
        }
    }

    pub const fn x(self) -> i32 {
        self.x
    }

    pub const fn z(self) -> i32 {
        self.z
    }
}

/// Feature labels used by the native preview planner. Neither label implies
/// Java placed-feature/index compatibility.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PlannedFeature {
    Tree,
    DesertWell,
}

/// A bounded, immutable feature candidate. Its owner is the chunk containing
/// its world-coordinate origin, even when its prospective operations target
/// neighboring chunks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeatureCandidate {
    feature: PlannedFeature,
    owner: ChunkPosition,
    origin: WorldPosition,
}

impl FeatureCandidate {
    pub const fn new(feature: PlannedFeature, origin: WorldPosition) -> Self {
        Self {
            feature,
            owner: origin.chunk(),
            origin,
        }
    }

    pub const fn feature(self) -> PlannedFeature {
        self.feature
    }

    pub const fn owner(self) -> ChunkPosition {
        self.owner
    }

    pub const fn origin(self) -> WorldPosition {
        self.origin
    }
}

/// A prospective block change owned by one candidate. Targets remain public
/// world positions and are not restricted to their candidate's owning chunk.
/// This is descriptive data only; applying it requires a future explicit
/// chunk-mutation stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationOperation {
    SetBlock {
        candidate_index: u8,
        sequence: u16,
        target: WorldPosition,
        block: Block,
    },
}

impl DecorationOperation {
    pub const fn set_block(
        candidate_index: u8,
        sequence: u16,
        target: WorldPosition,
        block: Block,
    ) -> Self {
        Self::SetBlock {
            candidate_index,
            sequence,
            target,
            block,
        }
    }

    pub const fn candidate_index(self) -> usize {
        match self {
            Self::SetBlock {
                candidate_index, ..
            } => candidate_index as usize,
        }
    }

    pub const fn sequence(self) -> u16 {
        match self {
            Self::SetBlock { sequence, .. } => sequence,
        }
    }

    pub const fn target(self) -> WorldPosition {
        match self {
            Self::SetBlock { target, .. } => target,
        }
    }

    pub const fn target_owner(self) -> ChunkPosition {
        self.target().chunk()
    }

    pub const fn block(self) -> Block {
        match self {
            Self::SetBlock { block, .. } => block,
        }
    }
}

/// Validation failures while assembling a bounded decoration plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationPlanError {
    TooManyCandidates,
    TooManyOperations,
    UnknownCandidate,
    /// Planning rejects competing writes rather than depending on application order.
    DuplicateTarget,
}

/// An immutable, canonically ordered description of future decoration work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecorationPlan {
    candidates: Vec<FeatureCandidate>,
    operations: Vec<DecorationOperation>,
}

impl DecorationPlan {
    pub fn new(
        candidate_inputs: impl IntoIterator<Item = FeatureCandidate>,
        operation_inputs: impl IntoIterator<Item = DecorationOperation>,
    ) -> Result<Self, DecorationPlanError> {
        let mut candidates = Vec::new();
        for candidate in candidate_inputs {
            if candidates.len() == MAX_DECORATION_CANDIDATES {
                return Err(DecorationPlanError::TooManyCandidates);
            }
            candidates.push(candidate);
        }

        let mut targets = BTreeSet::new();
        let mut operations = Vec::new();
        for operation in operation_inputs {
            if operations.len() == MAX_DECORATION_OPERATIONS {
                return Err(DecorationPlanError::TooManyOperations);
            }
            if operation.candidate_index() >= candidates.len() {
                return Err(DecorationPlanError::UnknownCandidate);
            }
            if !targets.insert(operation.target()) {
                return Err(DecorationPlanError::DuplicateTarget);
            }
            operations.push(operation);
        }
        operations.sort_by_key(|operation| {
            (
                operation.candidate_index(),
                operation.sequence(),
                operation.target(),
            )
        });

        Ok(Self {
            candidates,
            operations,
        })
    }

    pub fn candidates(&self) -> &[FeatureCandidate] {
        &self.candidates
    }

    pub fn operations(&self) -> &[DecorationOperation] {
        &self.operations
    }
}

/// The opaque seed value established for one chunk's decoration pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecorationSeed(u64);

impl DecorationSeed {
    /// Construct the repository's existing Java-style RNG from this seed.
    pub fn random(self) -> NoiseSeed {
        NoiseSeed::new(self.0)
    }
}

/// Derive a per-chunk decoration seed using the shape of Java's legacy
/// `setDecorationSeed` calculation. Coordinates are chunk origins in blocks,
/// not chunk indices. This is deterministic plumbing, not parity evidence.
pub fn decoration_seed(
    world_seed: u64,
    chunk_start_x: i32,
    chunk_start_z: i32,
) -> DecorationSeed {
    let mut random = NoiseSeed::new(world_seed);
    let x_multiplier = random.next_long() | 1;
    let z_multiplier = random.next_long() | 1;
    let mixed = (chunk_start_x as i64)
        .wrapping_mul(x_multiplier as i64)
        .wrapping_add((chunk_start_z as i64).wrapping_mul(z_multiplier as i64))
        as u64
        ^ world_seed;
    DecorationSeed(mixed)
}

/// Derive a feature-local RNG from a decoration seed using Java's legacy
/// feature-step/index shape. Java overflows the signed 32-bit step expression
/// before adding its sign-extended result to the long decoration seed.
pub fn feature_seed(
    decoration_seed: DecorationSeed,
    decoration_step: i32,
    feature_index: i32,
) -> NoiseSeed {
    let offset = decoration_step
        .wrapping_mul(10_000)
        .wrapping_add(feature_index);
    NoiseSeed::new(
        decoration_seed
            .0
            .wrapping_add((offset as i64) as u64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::BlockId;

    #[test]
    fn decoration_seeds_are_deterministic_for_negative_chunk_origins() {
        let seed = decoration_seed(0x5EED, -272, -144);
        let mut first = feature_seed(seed, 3, 7);
        let mut second = feature_seed(seed, 3, 7);

        assert_eq!(first.next_long(), second.next_long());
        assert_eq!(first.next_long(), second.next_long());
    }

    #[test]
    fn feature_seed_sign_extends_the_overflowed_java_int_offset() {
        let base = DecorationSeed(0x0123_4567_89ab_cdef);
        let step = i32::MAX;
        let index = i32::MAX;
        let expected_offset = step.wrapping_mul(10_000).wrapping_add(index);
        let expected = base.0.wrapping_add((expected_offset as i64) as u64);

        assert_eq!(feature_seed(base, step, index).initial_seed(), expected);
        assert_ne!(
            feature_seed(base, step, index).initial_seed(),
            base.0
                .wrapping_add(index as u32 as u64)
                .wrapping_add((step as u32 as u64).wrapping_mul(10_000)),
        );
    }

    #[test]
    fn feature_seed_preserves_negative_java_int_offsets() {
        let base = DecorationSeed(0);
        assert_eq!(feature_seed(base, -1, -1).initial_seed(), (-10_001_i64) as u64);
        assert_eq!(
            feature_seed(base, i32::MIN, 0).initial_seed(),
            (i32::MIN.wrapping_mul(10_000) as i64) as u64
        );
    }

    #[test]
    fn decoration_plan_allows_cross_chunk_targets_but_rejects_duplicate_writes() {
        let tree = FeatureCandidate::new(PlannedFeature::Tree, WorldPosition::new(15, 70, 15));
        let well = FeatureCandidate::new(PlannedFeature::DesertWell, WorldPosition::new(-1, 70, 0));
        let boundary_target = WorldPosition::new(16, 71, 16);
        let plan = DecorationPlan::new(
            [tree, well],
            [
                DecorationOperation::set_block(1, 0, WorldPosition::new(-16, 70, 0), Block::new(BlockId::Sandstone)),
                DecorationOperation::set_block(0, 0, boundary_target, Block::new(BlockId::OakLog)),
            ],
        )
        .unwrap();

        assert_eq!(plan.candidates()[0].owner(), ChunkPosition::new(0, 0));
        assert_eq!(plan.candidates()[1].owner(), ChunkPosition::new(-1, 0));
        assert_eq!(plan.operations()[0].target_owner(), ChunkPosition::new(1, 1));
        assert_eq!(plan.operations()[1].target_owner(), ChunkPosition::new(-1, 0));
        assert_eq!(
            DecorationPlan::new(
                [tree],
                [
                    DecorationOperation::set_block(0, 0, boundary_target, Block::new(BlockId::OakLog)),
                    DecorationOperation::set_block(0, 1, boundary_target, Block::new(BlockId::OakLeaves)),
                ],
            ),
            Err(DecorationPlanError::DuplicateTarget),
        );
    }

    #[test]
    fn decoration_plan_enforces_bounds_and_canonical_operation_order() {
        let tree = FeatureCandidate::new(PlannedFeature::Tree, WorldPosition::new(4, 70, 4));
        let well = FeatureCandidate::new(PlannedFeature::DesertWell, WorldPosition::new(8, 70, 8));
        let plan = DecorationPlan::new(
            [tree, well],
            [
                DecorationOperation::set_block(1, 2, WorldPosition::new(8, 70, 8), Block::new(BlockId::Sandstone)),
                DecorationOperation::set_block(0, 1, WorldPosition::new(4, 71, 4), Block::new(BlockId::OakLog)),
                DecorationOperation::set_block(0, 0, WorldPosition::new(4, 70, 4), Block::new(BlockId::OakLog)),
            ],
        )
        .unwrap();

        assert_eq!(plan.operations().len(), 3);
        assert_eq!(plan.operations()[0].candidate_index(), 0);
        assert_eq!(plan.operations()[0].sequence(), 0);
        assert_eq!(plan.operations()[1].candidate_index(), 0);
        assert_eq!(plan.operations()[1].sequence(), 1);
        assert_eq!(plan.operations()[2].candidate_index(), 1);
        assert_eq!(plan.operations()[2].sequence(), 2);

        let too_many_operations: Vec<_> = (0..=MAX_DECORATION_OPERATIONS)
            .map(|index| {
                DecorationOperation::set_block(
                    0,
                    index as u16,
                    WorldPosition::new(index as i32, 0, 0),
                    Block::new(BlockId::Stone),
                )
            })
            .collect();
        assert_eq!(
            DecorationPlan::new([tree], too_many_operations),
            Err(DecorationPlanError::TooManyOperations),
        );
        assert_eq!(
            DecorationPlan::new(vec![tree; MAX_DECORATION_CANDIDATES + 1], []),
            Err(DecorationPlanError::TooManyCandidates),
        );
        assert_eq!(
            DecorationPlan::new(
                [tree],
                [DecorationOperation::set_block(
                    1,
                    0,
                    WorldPosition::new(0, 0, 0),
                    Block::new(BlockId::Stone),
                )],
            ),
            Err(DecorationPlanError::UnknownCandidate),
        );
    }
}
