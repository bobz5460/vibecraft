# Spec: overworld-parity

Scope: feature

# Overworld Parity Slice

## Acceptance

- The active generator uses the supplied minecraft-26 Overworld source as its algorithmic authority, not handcrafted climate thresholds or post-hoc terrain normalization.
- Seed initialization and coordinate transforms are stable for positive and negative chunk coordinates.
- Representative fixed seed/coordinate fixtures cover density, surface, biome, aquifer/fluid, cave, ore, feature, and structure outcomes.
- Generation order does not change a chunk's result when the reference result is local/deterministic; chunk-edge writes follow an explicit ownership policy.
- Async generation remains compatible with `ChunkManager` revision checks and does not publish stale or partially initialized chunks.

## Deliberate boundaries

- This feature is Overworld-only.
- Native Rust chunk storage, block IDs, persistence, and networking remain non-Java-compatible.
- Missing block-state, block-entity, data-pack, or structure representations must be reported as limitations and tested at the boundary.
- Exact parity cannot be claimed for a subsystem until its reference data inputs are also ported; source-code shape alone is insufficient.