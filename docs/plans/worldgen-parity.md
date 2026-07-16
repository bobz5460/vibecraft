---
plan name: worldgen-parity
plan description: Port Overworld generation faithfully
plan status: active
---

## Idea
Replace the repository's approximate world generator with an evidence-driven Overworld implementation matching the supplied minecraft-26 Java source. Preserve the existing asynchronous chunk-generation boundary and native chunk storage, but port the reference's seed plumbing, dimension/noise settings, biome source, density and aquifer pipeline, surface rules, carvers, ores, configured/placed features, and structures in dependency order. Use targeted tests and fixed seed/coordinate fixtures to prevent plausible-looking but incorrect terrain from being accepted. Track unsupported Java data-driven or block-registry behavior explicitly rather than silently approximating it.

## Implementation
- Inventory the supplied minecraft-26 Overworld levelgen classes, registries/data inputs, seed/random APIs, and current Rust generator call sites; document the Java-to-Rust mapping and compatibility gaps in the deepwork record.
- Pin the repository's generation target to minecraft-26 in PLAN.md and add a staged M4 work entry with non-goals, acceptance fixtures, and the native chunk/storage boundary.
- Port and test the reference seed/random, noise parameter, octave, density-function, NoiseRouter, terrain spline, and interpolation behavior; remove ad hoc normalization and approximate climate logic from the active path.
- Port the Overworld biome source, aquifer, cave/carver, surface-rule, deepslate, fluid, and stone replacement behavior using the reference configuration and valid world bounds.
- Port deterministic ore, vegetation, tree, aquatic, and other configured/placed feature placement with cross-chunk ownership/ordering rules, then implement structures only where the existing block and chunk APIs can represent them.
- Integrate the new generator with ChunkManager's async revision and neighbor contracts, preserving negative coordinates, chunk-edge determinism, fluid bookkeeping, and worker safety.
- Add reference fixtures for fixed seeds and coordinates, chunk-order independence, boundary continuity, biome/surface outcomes, and representative caves/ores/features; run targeted tests, cargo build, and the release startup smoke test.
- Review remaining differences against the supplied source, update PLAN.md and ISSUES.md with verified limitations, and report exact supported behavior plus any blockers to claiming full parity.

## Required Specs
<!-- SPECS_START -->
- worldgen-target
- overworld-parity
<!-- SPECS_END -->