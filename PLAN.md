# Vibecraft Roadmap

Vibecraft is a native Rust reimplementation of **Minecraft: Java Edition**. The immediate product goal is a playable native multiplayer demo; behavioral and visual parity for a pinned modern Java Edition data version remains the long-term goal.

## How To Use This Plan

- This is the authoritative roadmap and active-work log. Keep it current when starting or completing a meaningful feature.
- `Foundation` means code exists and is exercised in-game. It does **not** mean vanilla-parity complete.
- `Partial` means an end-to-end path exists but has documented gaps. Do not mark work complete merely because types, commands, or placeholder rendering exist.
- `Not started` means no usable implementation exists.
- Work in dependency order. Do not add a consumer before its data model, simulation rules, and persistence path exist.
- Before opening a broad task, define the target Java Edition version and use its data and behavior as the reference. The active world-generation parity target is Minecraft Java Edition 26.2 (world version 4903, data pack 107.1); other subsystems may still be pinned separately until migrated.
- For gameplay or rendering changes, verify with `cargo build && timeout 15 cargo run --release`. There is currently no automated test suite; add focused tests whenever logic can be tested without a GPU.
- Keep implementation constraints and architectural gotchas in `AGENTS.md`; keep roadmap status and next work here.
- Until the multiplayer demo exit criteria are met, prioritize end-to-end multiplayer capabilities over parity, breadth, presentation polish, and isolated singleplayer-only features. New features must either be part of the demo slice or be explicitly deferred.

## Agent Operating Procedure

Use this procedure for every implementation task. It is intentionally explicit so an agent can make progress without relying on unstated project conventions.

1. Read `AGENTS.md`, this roadmap's active queue, and the relevant milestone before proposing or changing code.
2. Inspect `git status --short`. The worktree is often shared and dirty. Preserve unrelated work and never reset, checkout, or reformat files outside the task.
3. State the work item in one sentence: subsystem, supported behavior, non-goals, and the observable acceptance check. If it spans more than one milestone, split it before coding.
4. Trace the existing path from input/data source through simulation, rendering, persistence, and UI. Search call sites before changing public structs, `ChunkManager`, shader uniforms, or registries.
5. Implement the smallest vertical slice that is usable from normal gameplay. Do not add a command-only implementation and call the feature complete.
6. Verify the affected layer first, then run the project baseline. For example, test a pure block-state function before running a graphical block placement check.
7. Update this file only when priority, scope, dependency, or status changes. Record durable architecture rules in `AGENTS.md`; record bugs and investigations in `ISSUES.md`.

### Task Sizing Rules

- A small task changes one behavior inside one subsystem and has one clear acceptance check.
- A medium task may cross a data type and its consumers, but must preserve existing behavior and include migration/call-site updates.
- A large task changes a shared abstraction such as block state, persistence, ticking, entities, or rendering data. Before coding, add an active work entry with scope, non-goals, and a staged delivery order.
- Stop and ask for a decision when the task needs a target-version choice, data-format commitment, public compatibility promise, or a rewrite of an existing shared system.
- Do not use a broad feature name such as "add redstone" or "add mobs" as a task. Name the required platform capability and one vertical slice instead.

### Evidence To Record

When completing a work item, report all applicable evidence:

- Build command and result.
- Runtime scenario: seed, coordinates, input sequence, command, or fixture used.
- Behavior before and after, including intentional deviations from the reference.
- Files or systems that must remain synchronized.
- Follow-up work that is now unblocked, or a reason the item remains partial.

## Definition Of Done

A parity item is complete only when all applicable conditions hold:

1. Its rules match the pinned Java Edition reference, including state transitions and edge cases.
2. It works through normal gameplay, not only through debug commands or manually constructed state.
3. It renders and sounds correct enough for its supported assets and participates in lighting, collision, drops, saving, and networking where relevant.
4. It is deterministic where the reference is deterministic and does not introduce panics, stale async results, or unbounded work.
5. It has a regression test, reproducible in-game check, or explicit manual acceptance steps recorded in the implementing PR/commit.

## Current Baseline

| Area | Status | What exists | Main gap |
|---|---|---|---|
| Engine and terrain renderer | Foundation | wgpu/winit loop, chunk GPU cache, frustum culling, shadows, fog, sky, post-processing | no occlusion culling/LOD or configurable settings |
| Overworld terrain | Partial | seeded terrain, biomes, caves, ores, decorations, async generation | not data-version-faithful; no natural structures or cave biomes |
| Blocks and lighting | Partial | registry/state and resolved-model foundations, chunk lighting, greedy cube/crossed/slab/stair meshes | metadata does not yet drive all gameplay/rendering; no generic model-element or block-entity system |
| Player survival | Partial | movement, collision, modes, health/hunger, effects, basic fluids | formulas and interactions are incomplete; no entity combat |
| Items and inventory | Partial | item registry, stack counts, hotbar/inventory textures, block/item drop billboards | no durability, equipment, crafting, containers, or generic item/entity-model rendering |
| UI and audio | Partial | text, basic HUD/inventory data, commands, block sounds | graphical vanilla screens/HUD, chat, settings, positional audio |
| Simulation content | Not started | dropped items, XP, and specialized fluid updates | no general entity or block-tick framework, mobs, projectiles, or redstone |
| Persistence and dimensions | Partial | versioned native Overworld level/player/chunk persistence with atomic saves | no dimensions, block-entity/general-entity persistence, or scheduled-event persistence |
| Multiplayer | Partial | versioned native protocol, bounded TCP sessions, headless 20 TPS server, atomic level/chunk persistence, in-game direct-address join, handshake and keep-alive validation | dropped-item/container replication, reconnect edge cases, and two-client release scenario remain |

## Multiplayer Demo Contract

**Goal:** ship a native, server-authoritative demo quickly. Two or more local or remote clients can join the same persistent Overworld, see one another, move, chat, place and break supported blocks, and reconnect without losing authoritative player or world changes.

**Demo scope:** headless dedicated server; custom versioned native protocol; LAN/direct-address connection; authoritative 20 TPS simulation; player/chunk/block/inventory replication; basic player visuals; text chat; connect/disconnect feedback; save-on-disconnect and clean server shutdown; bounded and observable networking failures.

**Demo non-goals:** Java protocol/account compatibility, matchmaking, public server browser, encryption/authentication beyond an explicit demo policy, client prediction, combat/mobs, complete inventories/containers, dimensions, redstone, broad block parity, graphical menus, moderation tooling, and packet-loss optimization. These are follow-up work, not blockers for the demo.

**Release bar:** verify one server plus at least two independently launched clients: both clients join, occupy the same world, observe movement and block edits across chunk boundaries, exchange chat, disconnect/reconnect, and retain changes after a server restart. No client may authoritatively mutate the world or inventory.

## Active Queue

Do not start more than one large cross-cutting item at once. Update this list while work is active.

| Priority | Status | Work item | Depends on | Acceptance criteria |
|---|---|---|---|---|
| P0 | complete | Define the native multiplayer demo protocol and authority boundary | M2 fixed tick/persistence | Versioned handshake, message limits, ownership rules, disconnect behavior, and reject/error paths are documented and covered by serialization tests. |
| P0 | complete | Extract and run a headless authoritative server | native simulation and persistence | A server owns the 20 TPS world, accepts a connection, saves safely, and runs without winit/wgpu. |
| P0 | in progress | Deliver two-client join, spawn, chunk, movement, and block-edit replication | server, protocol | Server-side sessions, authoritative movement, initial player/inventory/chunk messages, multi-player-centered chunk retention, and stale/reach-checked block requests work; chunk-boundary release validation remains. |
| P0 | in progress | Deliver demo chat, inventory synchronization, reconnect, and manual release scenario | session lifecycle and replication | Chat, hotbar/cursor inventory synchronization, automatic reconnect, and native player persistence exist; container actions, reconnect edge cases, and the two-client release bar remain. |
| P1 | pending | Add client interpolation, prediction, reconciliation, and packet-loss handling | authoritative replication telemetry | Movement remains responsive and converges under controlled latency/loss without client-authoritative mutations. |
| P1 | in progress | Complete graphical HUD, inventory, chat, and pause/settings screens | GUI atlas and input | Normal play no longer relies on debug/text fallback for supported screens. |
| P2 | pending | Generalize block-state/model rendering and fluid simulation | asset pipeline, block-state representation | Expanded world content replicates with deterministic state and stable rendering. |

### Priority And Dependency Rules

- P0 is the multiplayer-demo critical path. Do not start parity, rendering, content, or UI expansion that does not directly unblock a P0 demo acceptance criterion.
- P1 improves the demo after the release bar is met. A graphical placeholder is acceptable during development but is not a roadmap completion.
- P2 resumes deferred breadth and parity work once it can consume the multiplayer foundation it needs.
- Work from top to bottom within a milestone unless the task explicitly has no dependency on earlier bullets.
- A later milestone can receive research, small isolated bug fixes, or test fixtures, but not production feature work that would force a new foundation.
- If a feature exposes missing data, add the data-pipeline work to the earliest applicable milestone rather than hiding it in renderer or UI special cases.

### Cross-Cutting Change Matrix

Use this matrix before editing. A change usually needs every listed layer, not just the first file that compiles.

| Change | Inspect and update as needed |
|---|---|
| Block behavior/state | `world/block.rs`, chunk storage, `ChunkManager` mutation/lighting, mesh classification, texture mapping, drops, collision, inventory item mapping, worldgen, UI/debug names |
| Block appearance | blockstate/model loader, texture atlas, mesh vertex data/material flags, Rust uniform layout, WGSL shader, opaque/translucent pass selection |
| Chunk or lighting data | `Chunk`, `ChunkManager`, async task snapshots/revisions, mesh invalidation, GPU render cache, chunk-boundary behavior |
| Item/inventory rule | `inventory/item.rs`, `inventory/mod.rs`, main input/UI integration, dropped-item behavior, player mechanics, serialization design |
| Player mechanic | `player/`, camera/input integration, collision and world queries, HUD feedback, gamemode and difficulty behavior |
| GUI/input change | `engine/input.rs`, `main.rs` event handling, GUI atlas/renderer/text, cursor grab/release, window resize and focus-loss paths |
| GPU/shader change | Rust `#[repr(C)]` data layout, bind groups/pipelines, WGSL struct layout, all passes using the data, resize/recreate paths |
| Asset change | `assets/`, `VIBECRAFT_ASSETS` layout, fallback/error path, atlas capacity, shader atlas constants, real asset smoke test |

## Milestones

### M0: Product Contract And Engineering Guardrails

**Purpose:** make later work measurable and prevent incompatible one-off systems.

- [x] Pin a Java Edition target version and document whether Java save/protocol compatibility is a goal or only gameplay compatibility. General assets/gameplay foundations remain 1.21.1-oriented, while the active world-generation parity slice targets Java Edition 26.2 (world version 4903, data pack 107.1) from `/Users/dac63/Downloads/minecraft-26`. Java save/protocol compatibility is not a goal; persistence and networking formats remain native and versioned.
- [x] Add CLI/config support for seed, world directory, render distance, graphics settings, and keybindings. `vibecraft.json` and command-line overrides are validated at startup; the world directory is created now and reserved for M2 persistence.
- [x] Split reusable simulation logic from the executable entry point so it can be tested without window/GPU setup. `src/lib.rs` exports the simulation and configuration modules; `main.rs` is the windowed application shell.
- [x] Add deterministic unit tests for block state, inventory, raycast, lighting, world generation, and player damage/movement rules. Tests run through the library target without window or GPU setup.
- [x] Add crash-resistant error paths for assets, GPU setup, worker failures, and corrupt save data. Startup validates configuration, asset root, window/event-loop, surface, adapter, device, and surface formats; worker panics become logged retriable failures. Save data remains an M2 feature, so the currently applicable corrupt on-disk input is JSON configuration.
- [x] Create a reproducible screenshot/manual-check scene for renderer regressions. `RENDER_CHECK.md` fixes the seed, settings, launch command, capture timing, and comparison baseline.

**Exit criteria:** a world can be reproduced by seed and settings; core rules have tests; failures are reported rather than hidden by silent fallback or panic.

**Recommended delivery order:** pin the reference version and asset source; make seed/config reproducible; extract pure logic into testable modules; add fixture tests; then define persistence boundaries. Avoid choosing a final save format while block state and item representations are still unstable.

### M1: Data-Driven World Representation And Rendering

**Purpose:** replace special cases with the data needed to support vanilla blocks correctly.

### 2026-07-13: M1 stateful placement and connected-model completion
- Owner: OpenCode
- Scope: Add the missing stateful placement, neighbor-state recomputation, generic model families, fluid surface mesh, tint data, texture animation/mip chain, and transparent ordering foundations. Block-entity persistence, gameplay interaction, and scheduled simulation remain M2/M3/M6 work.
- Depends on: M1 registry and resolved-model foundation
- Acceptance: Representative stateful blocks place through normal input, resolve to generic model geometry, and rebuild correctly at chunk boundaries; renderer consumes tint and texture-animation data without asset I/O in workers.
- Status: in progress

### 2026-07-12: M1 registry and resolved-model foundation
- Owner: OpenCode
- Scope: Introduce registry-backed block definitions and compact property states, then extend deterministic blockstate/model resolution. Existing cube/crossed/slab/stair rendering remains as a compatibility path; fluids, biome tint, texture animation, and generic geometry follow this foundation.
- Depends on: Java Edition 1.21.1 target policy
- Acceptance: Normal placement retains default state behavior; registry state schemas round-trip for supported families; resolver tests cover deterministic variants, weighted selection, rotations, multipart conditions, and multi-element metadata.
- Status: partial
- Notes: Native save/protocol formats are explicitly not Java-compatible. Parent inheritance, texture variables, UV/tint metadata, and all model elements are represented by the resolver. Startup now caches resolved slab/stair model assets and their textures for worker-safe generic element emission; the terrain mesher retains legacy cube/crossed/fluid and unsupported-family paths. Fluids, tint propagation, texture animation, mipmaps, block entities, and generic connected models remain required for M1 exit.

- [ ] Replace enum-only block behavior with a registry-backed block definition, state schema, tags, hardness, collision shape, light opacity/emission, drops, and sound group.
- [ ] Represent arbitrary block properties rather than overloading one data byte for unrelated states.
- [ ] Load blockstates/models deterministically, including weighted variants, rotations, multipart conditions, parent inheritance, texture variables, tint indices, and all model elements.
- [ ] Mesh cube, crossed, fluid, and arbitrary model-element geometry from resolved block state and neighbor state.
- [ ] Carry biome color data to vertices/fragments for grass, foliage, water, fog, and sky instead of baking a plains tint into the atlas.
- [ ] Implement texture animation metadata, mipmaps, cutout/translucent material rules, destroy stages, and robust translucent ordering.
- [ ] Validate and extend frustum culling, add occlusion/LOD where justified, and profile generation, meshing, upload, and draw costs before raising render distance.

**Exit criteria:** representative vanilla blocks (doors, fences, plants, fluids, redstone dust, chests, stairs, slabs, tinted blocks) can be described and rendered without adding a new meshing special case per block family.

**Recommended delivery order:** define registry/state data first; resolve blockstate/model data deterministically; make one generic model-element path; add material/tint data to vertices; then support fluid geometry and texture animation. Keep the current cube/crossed/slab/stair paths working until the generic path has visual regression coverage.

### M2: Singleplayer Game Loop And Persistence

**Purpose:** make a world worth playing and safe to evolve.

### 2026-07-13: M2 native singleplayer foundation
- Owner: OpenCode
- Scope: Add a fixed simulation boundary, versioned native level/player/chunk persistence, atomic writes, chunk unload safety, persistent daylight rule, and world-spawn respawn. Beds, block entities, general entities, and scheduled-event persistence remain deferred because their runtime models do not exist.
- Depends on: M0 persistence policy and current chunk lifecycle
- Acceptance: A changed chunk, player inventory/position, world seed, time, and daylight rule survive a clean quit/reopen; corrupt data is reported without regeneration; simulation advances at 20 TPS independently of render frames.
- Status: complete
- Notes: Native JSON envelopes carry format/data versions and use atomic file replacement. Data version 2 migrates version-1 level saves without replacing corrupt data. Changed chunks save before stream unload and flush on autosave, `/save`, and `/quit`; `/quit` keeps the process alive when a save fails. Saved lighting, meshes, worker state, and GPU data are rebuilt rather than serialized. Fixed 20 TPS scheduling persists water/lava work and specialized dropped-item/XP state, while deferred unloaded work remains queued. World spawn selection and `doDaylightCycle` are persistent and command-backed. Beds, block entities, per-player spawn points, and general entities remain M3/M5/M6 prerequisites rather than hidden one-off systems.

- [x] Introduce a fixed 20 TPS simulation clock with clear client/render interpolation boundaries.
- [ ] Define tick scheduling for blocks, fluids, random ticks, entities, and scheduled events with chunk-load safety.
- [ ] Save/load level metadata, chunks, player data, inventories, block entities, scheduled ticks, and world seed.
- [x] Implement autosave, explicit save/quit, atomic writes, corruption handling, and backward data-version migration.
- [ ] Add spawn selection, beds/spawn points, death/respawn, gamerules, and command behavior backed by the simulation state.
- [x] Add configurable render distance and safe chunk loading/unloading around the player.

**Exit criteria:** a changed singleplayer world can be quit, reopened, and continue identically without losing blocks, player state, inventories, or scheduled behavior.

**Recommended delivery order:** establish the fixed tick boundary; introduce serializable DTOs separate from runtime GPU/thread state; persist simple level/player/chunk data; add atomic write/recovery; finally persist scheduled ticks and block entities. Never serialize wgpu handles, channels, `Arc` worker state, or cache-only mesh data.

### M3: Player, Items, And Core Progression

**Purpose:** provide the survival loop before filling out content.

### 2026-07-13: M3 survival-loop implementation
- Owner: OpenCode
- Scope: Replace the unsafe shared block/item ID assumption; make item stacks, drops, equipment, mining, recipes, furnace processing, player crafting, and survival death state reusable and persisted. The user explicitly authorized prerequisite M5/M6/M8 platform work needed to complete M3 literally: entity/projectile lifecycle, redstone-facing container behavior, and first-person presentation. Full mob content, AI families, Java data-pack compatibility, and broad UI/accessibility work remain separately scoped after the required substrate.
- Depends on: M2 fixed simulation and native persistence
- Acceptance: Normal mouse/keyboard play supports gathering, crafting, smelting, equipping, block interaction, item pickup, death/respawn, and inventory-loss rules without debug commands; pure rules have regression tests.
- Status: in progress
- Notes: Item identity is now independent from block IDs; stacks carry durability and persist, dropped stacks merge/pick up without loss, and local/server mining use shared harvest, mapped-drop, quantity, and tool-damage rules. The inventory screen now exposes cursor-based 2x2 shaped crafting, preserves damaged stacks, and returns unfinished ingredients safely when closed. Player crafting and furnace rules, persisted chest/furnace entities, fixed-tick furnace processing, direct chest/furnace interaction, armor equipment with slot validation, and `keepInventory` persistence are present. A tested entity/projectile substrate, visible training dummy, basic melee path, and temporary first-person hand overlay exist. Graphical table/chest/furnace screens, held-item animation, broader recipe/loot coverage, automation/brewing blocks, entity persistence, shields/projectile input, and target-version parity remain required before checking any M3 item complete.

### 2026-07-14: M3 survival interaction slice
- Owner: OpenCode
- Scope: Connect shaped player crafting, harvest-aware drops, server-side mining validation, durable inventory cursor movement, and armor-slot validation to normal mouse/keyboard play. Container-specific screens, broad recipe/loot data, projectile input, and entity persistence remain non-goals for this slice.
- Depends on: M2 fixed simulation and native persistence; existing M3 progression and inventory foundations
- Acceptance: A survival player can open the inventory, place ingredients in the rendered 2x2 grid, collect a valid shaped output, close without losing ingredients, mine stone/coal with correct mapped drops, and equip armor only in its matching slot; local and network block breaks reject unharvestable tools.
- Status: complete
- Notes: Added pure recipe/harvest regression tests and retained the parent M3 status as in progress because the remaining M3 gaps are still material.

- [x] Match player collision, stepping, crouching, swimming, climbing, fall damage, exhaustion, oxygen, and status-effect rules to the target version.
- [~] Swimming detection fixed: head-submersion + sprint/forward intent activates swim pose (0.6 hitbox + 0.52 eye height); water physics applied whenever `in_water` (not just in swim pose); jump blocked in water; sprint multiplier excluded from swimming speeds; buoyancy, vertical controls (space=rise, sneak=sink) wired.
- [ ] Render first-person hand and held items with use, swing, equip, hurt, and view-bob animations.
- [ ] Implement canonical item stacks: max sizes, durability, damage, cooldowns, attributes, food effects, equipment, and offhand behavior.
- [ ] Implement mining time, harvest requirements, tool effectiveness, drops, loot tables, and experience.
- [ ] Implement 2x2 and 3x3 crafting, recipe data, furnace-family processing, fuel, and recipe-book foundations.
- [ ] Implement reusable container and block-entity foundations, then the survival-loop blocks: chest, furnace, crafting table, hopper, dispenser/dropper, and brewing stand.
- [ ] Implement combat primitives: melee, knockback, shields, projectiles, armor, enchantment hooks, and damage sources.

**Exit criteria:** a player can gather, craft, smelt, equip, fight, die, respawn, and retain or lose items according to configured gamerules without debug commands.

**Recommended delivery order:** correct item stack/equipment invariants; make mining and drops data-driven; add crafting/smelting; add containers; then implement combat/projectiles. Each step should work through mouse input and the inventory UI, not only `/give` or debug state.

### M4: Overworld Content And Environmental Simulation

**Purpose:** make the Overworld generate and evolve like the target version.

### 2026-07-15: Restore and stabilize biome-column terrain
- Owner: OpenCode
- Scope: Shelve the experimental density generator, restore the existing biome-column terrain, and fix deterministic surface/feature generation without cross-chunk writes or Java seed compatibility.
- Depends on: Current async chunk-generation contract
- Acceptance: The prior generator remains active; surface material and shoreline decisions are deterministic across chunk borders; decoration, structure, and ocean feature RNG streams are independent; generation order does not alter a chunk's result.
- Status: complete
- Notes: The removed density prototype is retained in Git stash `shelved-continuous-terrain-2026-07-15`. The restored generator now uses deterministic surface-material tie-breaking, world-space shoreline queries, and salted per-stage RNG streams. Exact noise-router, aquifer, cave-biome, and cross-chunk feature parity remain M4 work.

### 2026-07-15: Minecraft 26.2 Overworld parity port

- Owner: OpenCode
- Scope: Replace approximate active Overworld generation with the supplied Minecraft 26.2 noise, biome, aquifer, surface, carver, feature, and structure pipeline while preserving native chunk storage and async generation boundaries. Nether, End, Java save/protocol compatibility, and unsupported block-entity/template behavior remain out of scope for this slice.
- Depends on: M1 block/state representation and current async chunk-generation contract
- Acceptance: Fixed Minecraft 26.2 source/data fixtures cover seed/random initialization, negative coordinates, density/interpolation, biome/surface/aquifer decisions, representative carvers/ores/features, and generation-order independence; `cargo build` and release startup smoke test pass without stale worker publication.
- Status: in progress
- Notes: The supplied tree is Minecraft 26.2 (`world_version` 4903, data pack 107.1) and includes worldgen JSON plus binary structure templates. New worlds now use persisted Java-profile coordinates `-64..319` over unchanged local storage `0..383`; migrated existing saves retain legacy local-Y coordinates. A separate persisted generation profile keeps migrated worlds on pre-corrected interpolation while new local/server worlds use the corrected Minecraft-26 base, preventing old/new streaming seams without claiming Java output parity. Existing uncommitted generator edits are preserved pending golden-vector validation.

### 2026-07-16: Native future-world decoration preview

- Owner: OpenCode
- Scope: Versioned new-world-only deterministic oak/spruce tree and desert-well projection after the undecorated native base pass. Existing legacy and `minecraft26_base` worlds remain undecorated through persistence migration; local UI and server worlds choose `minecraft26_native_decoration_preview`.
- Acceptance: Candidate ownership, seed streams, and target projection are independent of generation order, work has fixed bounds, and base terrain is never replaced. Preview snapshots remain isolated from `ChunkManager` and no runtime mutation APIs are used.
- Status: complete
- Validation: `cargo test world::world_gen::generator::tests --lib` (19 passed), targeted persistence/network migration tests, `cargo build`, `cargo build --release`, and a 15-second release startup window completed without a panic or GPU validation error. macOS lacks the `timeout` utility, so the tool process timeout supplied the expected termination.
- Notes: This is deliberately not Java placed-feature/index compatibility. Trees support only vertical default-data logs and default leaf state; unsupported log axes and leaf-distance/persistence states are not represented. Wells intentionally omit sandstone slabs, suspicious sand, loot, and block-entity behavior.

- [~] Port vanilla noise/density pipeline: ImprovedNoise, PerlinNoise, NormalNoise, DensityFunction framework, NoiseRouter, TerrainProvider splines, cave density functions, and cell-based trilinear interpolation chunk fill. Integrated as VanillaWorldGenerator replacing the old WorldGenerator in ChunkManager; candidate-only until executable Java 26.2 fixture coverage validates seed plumbing and chunk output.
- [ ] Port or faithfully reproduce target-version biome, density, surface, aquifer, cave, ore, and feature rules using reference seeds for comparison. The active path now uses the 26.2 biome source, aquifer, seeded surface system, and ore-vein material rule; carvers and configured/placed features remain pending.
- [~] Correct density-cell banding in the active terrain fill: explicit interpolated density markers now use global cell coordinates, and material boundaries receive exact point refinement without post-generation smoothing. The fill loop now matches Java's delayed X-slice swap and aquifers use the named `minecraft:aquifer` random fork; Java runtime fixtures remain required before declaring the path exact.
- [ ] Add missing overworld biomes and biome-dependent colors, precipitation, features, and spawn rules.
- [ ] Add naturally generated structures in dependency order: small features/geodes, villages/outposts, dungeons/mineshafts, temples, monuments, strongholds, ancient cities, trail ruins, and trial chambers.
- [ ] Implement crop growth, farmland, leaves, fire, snow/ice, weather, lightning, and other random/scheduled block behavior.
- [ ] Implement weather simulation and visual/audio effects, including biome-dependent rain/snow and thunder behavior.
- [ ] Add world-generation snapshot tests for stable seed/coordinate fixtures, negative-Y mapping, representative caves/ores/features, and structure-placement checks.

**Exit criteria:** normal exploration produces stable, biome-appropriate terrain and structures, and environmental mechanics continue correctly after save/load and chunk reload.

### 2026-07-15: Fix ocean terrain height (density pipeline fixes)

- **Assignee:** Last agent session
- **Scope:** `src/world/world_gen/`, `noise_router.rs`, `generator.rs`, `terrain.rs`

**Changes:**

1. **Reverted `terrain.rs` ocean spline values** from the patched (0.35/0.4) back to original vanilla values (0.044, -0.2222) — the positive patch made total_offset too positive, pushing the gradient-driven density surface too low.

2. **Fixed BlendedNoise (`base_3d_noise`) amplitude bug** in `noise_router.rs`: removed spurious `/ 128.0` divisor from the return value of `compute()`. The vanilla BlendedNoise returns `blend_min/512 + (blend_max/512 - blend_min/512) * factor` which produces output in roughly [-128, 128]. Our code was dividing by an extra 128, reducing the noise to near-zero (±0.03), making the density function rely entirely on a weak gradient for terrain shaping.

3. **Replaced the simple height cap with `normalize_terrain`** in `generator.rs`:
   - **Height cap:** prevents sky-high blocks (density > 0 above sea_level + 48) by replacing them with air, then re-scans for the real column surface.
   - **Ocean floor guarantee:** any column whose highest solid block is below `sea_level - 8` and below `MIN_OCEAN_FLOOR=30` gets a minimum stone floor from y=4 up to y=29, giving submerged columns a consistent walkable bottom.

**Results (seed=42):**

| Chunk | Solid | Surface range | Description |
|-------|-------|---------------|-------------|
| (0,0) | 11606 | y=33-97 (avg 56) | Coastal/land near sea level |
| (1,0) | 5363 | y=29-56 (avg 31) | Ocean floor transitioning upward |
| (2,2) | 6912 | y=29 (flat) | Deep ocean floor at y=29 |
| (5,5) | 6912 | y=29 (flat) | Deep ocean floor at y=29 |
| (10,10) | 6016 | y=29-63 (avg 34) | Ocean floor with some rising terrain |

**Notes:** All chunks produce consistent, walkable terrain. Deep ocean columns get a flat floor at y=29 (above the void y=0). Near-shore columns (1,0) show a convincing ocean floor slope. Chunk (0,0) has proper surface variation near sea level without sky-high outliers. The ocean floor is uniformly flat at y=29 because the `normalize_terrain` pass overwrites all submerged columns — future work should add biome-aware floor variation. This is a scaffold, not a full surface-rule solution; the density pipeline still produces Swiss-cheese interior (the underground path oscillates due to post_process mapping everything in (-1,1) to solid).

**Recommended delivery order:** create fixed seed fixtures before changing generation; validate density/surface/caves/ores; add biome-specific features; add small structures; add large structures with spacing rules; then add scheduled environmental behavior. Do not let decoration depend on chunk-generation order.

### M5: Entity Platform And Vanilla Mobs

**Purpose:** build one scalable entity system before adding mob lists.

- [ ] Implement entity identity, lifecycle, components/state, spatial queries, collision, interpolation, serialization, and chunk activation/despawning.
- [ ] Implement entity rendering, model animation, hitboxes, nametags, particles, sounds, and projectile support.
- [ ] Implement AI goals, sensing, navigation, target selection, breeding, loot, equipment, and spawn caps/rules.
- [ ] Ship a representative vertical slice: zombie, skeleton, creeper, cow, sheep, chicken, villager, and boat/minecart.
- [ ] Expand by shared behavior families, then implement raids, trading, bosses, and version-specific mobs.
- [ ] Add deterministic AI/combat/spawn tests and stress tests for entity counts.

**Exit criteria:** entities are persistent, render and collide correctly, simulate under the 20 TPS budget, and representative passive/hostile/neutral gameplay works in naturally generated worlds.

**Recommended delivery order:** entity identity and storage; collision/spatial queries; rendering; lifecycle/save-load; projectiles and damage; AI goals/pathing; representative mobs; then content expansion. Avoid a second ad-hoc entity representation for every mob family.

### M6: Block Entities, Redstone, And Automation

**Purpose:** support stateful blocks and Java Edition automation semantics.

- [ ] Extend block-entity support for redstone-facing inventories, ticking, serialization, rendering, and neighbor updates.
- [ ] Implement redstone power propagation, component update scheduling, repeaters, comparators, observers, lamps, and container signals.
- [ ] Implement pistons, movable block rules, block events, quasi-connectivity policy, and Java-specific update-order behavior.
- [ ] Implement transport and automation: hoppers, dispensers, droppers, rails/minecarts, boats, and item transfer rules.
- [ ] Add redstone fixture worlds with expected per-tick traces before claiming Java parity.

**Exit criteria:** a documented suite of common circuits and automation fixtures behaves deterministically at 20 TPS, including save/load and chunk-boundary cases.

**Recommended delivery order:** neighbor update/event queue semantics; power queries; basic components; container signals; pistons; rails/transport. Record tick-by-tick expected results for each fixture because Java redstone behavior is sensitive to update ordering.

### M7: Dimensions And Endgame

**Purpose:** complete progression without creating dimension-specific forks of core systems.

- [ ] Add dimension definitions, coordinate transforms, portals, independent time/weather policies, and per-dimension save data.
- [ ] Implement Nether generation, biomes, structures, portal linking, mobs, and netherite progression.
- [ ] Implement End generation, stronghold/end portal flow, dragon fight, gateways, end cities, shulkers, and elytra loop.
- [ ] Verify portal, respawn-anchor, bed-explosion, and cross-dimension entity/item edge cases.

**Exit criteria:** a survival player can reach the Nether and End through normal progression, defeat the dragon, and retain a coherent saved world across all dimensions.

**Recommended delivery order:** dimension abstraction and save keys; portal linking; Nether terrain/content; stronghold and End portal progression; End terrain/content; dragon/gateway flow. Do not encode dimension assumptions in Overworld-only worldgen, chunk storage, or player respawn code.

### M8: Complete Presentation And Accessibility

**Purpose:** make supported gameplay feel complete rather than debug-driven.

- [ ] Complete title, world-select, multiplayer, pause, options, controls, accessibility, chat, command suggestions, tooltips, toasts, subtitles, statistics, and advancements.
- [ ] Implement positional sound, sound categories, music, ambience, weather, entity sounds, and resource-driven sound events.
- [ ] Implement resource packs first, then data packs for recipes, loot tables, tags, advancements, functions, and worldgen where feasible.
- [ ] Add localization support and remove hardcoded English-only gameplay strings.
- [ ] Profile and improve culling, LOD, texture streaming/mipmaps, entity visibility, and dynamic quality without changing simulation semantics.

**Exit criteria:** a player can configure, navigate, understand, and play the supported game without developer-only UI or hardcoded asset assumptions.

**Recommended delivery order:** core menus/settings and input rebinding; chat/command UX; inventory/container polish; sound categories and positional events; accessibility/localization; packs; then performance options. UI screens must own their focus/cursor behavior and not leak gameplay input.

### 2026-07-14: GUI state, HUD, inventory, and menu presentation
- Owner: OpenCode
- Scope: Restore the reusable UI state/layout layer; render the HUD, health/hunger meters, chat, toasts, inventory slots, carried items, tooltips, pause, options, controls, and accessibility screens through the orthographic GUI path; release gameplay input while a screen or chat owns focus.
- Depends on: GUI atlas and input plumbing
- Acceptance: Escape opens a navigable pause screen, supported menu actions work with mouse and keyboard, inventory clicks use the rendered slot layout, chat/cursor focus does not rotate or move the player, and UI rendering survives resize.
- Status: partial
- Notes: HUD, inventory backgrounds, status icons, crosshair, and known item icons now come from the official 1.21.1 asset checkout through the GUI atlas. Hotbar now includes experience bar, armor bar, and selected item name above the bar, matching vanilla layout. Settings screens (options, controls, accessibility) have been updated with more Minecraft-like button layouts and additional options (view bobbing, auto-jump). Title/world-select/multiplayer screen lifecycle, runtime key rebinding, and full localization remain follow-up work.

### 2026-07-15: Java-style chat and command interaction
- Owner: OpenCode
- Scope: Replace the fixed chat overlay with bounded, wrapped, scrollable history, sent-entry recall, text editing, and command suggestions; align the supported singleplayer command surface with Java Edition syntax and add only operations backed by current simulation state. Server-authoritative command execution, text components, signed chat, clickable/hoverable chat, weather simulation, and unsupported command families remain out of scope.
- Depends on: GUI frame/input plumbing; native multiplayer authority boundary
- Acceptance: `T` and `/` open a 256-character editor without gameplay input; chat wraps to the viewport, retains 100 entries, scrolls and recalls sent input; Tab completes supported commands/arguments; supported Java-style commands validate syntax and mutate the correct local state, while network clients cannot mutate local authority through slash commands.
- Status: complete
- Notes: Chat now has a bounded Unicode editor, message and sent-entry history, cursor editing, wrapping, scrollback, contextual Tab suggestions, and correctly retained cursor focus. Supported local commands gained Java-style `/teleport`, `/setblock`, `/fill`, `/clear`, `/experience`, `/time query`, coordinates, and namespace-aware lookup. Native multiplayer intentionally rejects slash commands until server-side command requests and authorization exist; text components, signed chat, and command families without current simulation support remain deferred.

### M9: Multiplayer Demo And External Compatibility

**Purpose:** ship the playable multiplayer demo first, then evolve it toward resilient multiplayer and any explicitly chosen compatibility commitments.

**Current progress:** the protocol contract and headless server foundation are complete. Server-side authoritative sessions now own movement intent, player spawn/despawn/update messages, initial inventory snapshots, compact loaded-chunk streaming, and revision/reach-checked block requests. The windowed client now consumes this transport for authoritative player/chunk/block/inventory/chat state, can join a direct server from the in-game pause menu, and renders interpolated articulated remote-player models with velocity-driven walk animation; unsupported network drops are rejected without local mutation, and permanent disconnects no longer reconnect forever. The next critical slice is reconnect/release validation, dropped-item replication, and container actions.

### 2026-07-13: Server-side replication substrate
- Owner: OpenCode
- Scope: Add lossless wire block state, bounded VCC1 chunk encoding, nonblocking client transport, authoritative server player sessions, fixed-tick movement, spawn/despawn/update broadcasts, initial inventory snapshots, loaded-chunk streaming, and revision/reach-checked single-block edits. Client world application, prediction, reconnect UX, and full inventory actions remain out of scope.
- Depends on: Native multiplayer protocol contract; headless authoritative server foundation
- Acceptance: Protocol and compact chunk round trips pass; two in-process TCP clients establish sessions; server movement produces authoritative updates; unsupported/stale/unloaded edits are rejected without client authority.
- Status: complete
- Notes: The server streams only chunks currently loaded by its existing streaming lifecycle; the windowed client integration is tracked in the following slice. Multi-center retention and chunk-boundary release validation remain.

### 2026-07-13: Windowed authoritative client slice
- Owner: OpenCode
- Scope: Connect the windowed executable to `ClientTransport`; apply authoritative welcome, chunk, block, inventory, chat, and player messages; send movement, hotbar, cursor-click, chat, and block-edit requests; retain server chunks around all active player centers; prevent local generation, physics, ticking, persistence, and inventory clicks from becoming a second authority.
- Depends on: Server-side replication substrate
- Acceptance: A client launched with `--server IP:PORT` consumes server snapshots and renders streamed chunks and articulated remote player models; local edits are requests and stale snapshots are rejected.
- Status: partial
- Notes: Basic server-cursor inventory clicks, right-click cursor actions, explicit rejection of unsupported network drops, automatic reconnect attempts, duplicate-name session aliases, username-keyed native player persistence, authoritative chunk-session reset, stale-inventory resynchronization, server-driven chunk unload/re-entry, and interpolated remote-player model animation are wired. Permanent disconnect codes stop automatic retries. Dropped-item entity replication, container-specific actions, remaining reconnect edge cases, server gamemode metadata, prediction/reconciliation, and release validation with two independent windowed clients remain.

### 2026-07-14: In-game direct server connection
- Owner: OpenCode
- Scope: Add a pause-menu server address form using the existing native client transport; preserve local state before switching sessions and surface invalid addresses or connection failures in the UI. Public server discovery, authentication, Java protocol compatibility, and background connection workers remain out of scope.
- Depends on: Windowed authoritative client slice
- Acceptance: While playing, open Pause > Join Server, enter an IPv4/IPv6/hostname address with port, connect to a running native server, and transition to authoritative world state without relaunching.
- Status: complete
- Notes: The form defaults to `127.0.0.1:25565`, supports keyboard and mouse submission, resolves hostnames through the system resolver, and resets client-authoritative chunk/cache state after a successful connection.

### 2026-07-14: Remote player model and animation
- Owner: OpenCode
- Scope: Replace the multiplayer remote-player cube proxy with a textured articulated avatar and client-side snapshot interpolation; animate arms and legs from authoritative horizontal velocity without changing server authority or the native wire contract. Player skins, equipment layers, name tags, and local-player first-person animation remain out of scope.
- Depends on: Windowed authoritative client slice
- Acceptance: Two connected clients render one another as visible head/torso/limb models, movement is smoothed between server updates, and walking produces a synchronized readable limb cycle.
- Status: partial
- Notes: Remote avatars use existing terrain-atlas wool textures and the shared lit/shadow terrain pipeline. Local player camera now interpolates smoothly between authoritative server position updates using exponential blending, decoupling render rate from the 20 TPS simulation clock. The release scenario with two independently launched clients is still required.

### 2026-07-13: Native multiplayer protocol contract
- Owner: OpenCode
- Scope: Define the versioned client/server wire messages, bounded framing, handshake/session guard, rate limits, reject/disconnect codes, authority boundary, and serialization tests. The headless server now consumes this contract; replication and client reconciliation remain separate work.
- Depends on: M2 fixed tick/persistence
- Acceptance: Valid client and server messages round-trip through bounded frames; unsupported versions, malformed/truncated/trailing/oversized frames, invalid state, handshake violations, stale input, and rate-limit violations are rejected deterministically.
- Status: complete
- Notes: Protocol version 1 uses a four-byte big-endian length prefix around a UTF-8 JSON envelope. It is native to Vibecraft and is not Java protocol compatible. See `NETWORK_PROTOCOL.md` and `src/network/mod.rs`.

- [x] Define a small, versioned native protocol with bounded framing, handshake/version rejection, message-size/rate limits, and explicit client/server ownership. Do not implement Java protocol compatibility for the demo.
### 2026-07-15: World select dirt background, delete world, and scrollable list
- Owner: OpenCode
- Scope: `src/ui/mod.rs`, `src/engine/renderer.rs`, `src/assets/gui_atlas.rs`, `src/main.rs`
- Depends on: Existing GUI atlas, UI frame, and renderer
- Acceptance: Title/world-select/create-world screens render a tiled dirt background (Minecraft `menu_background.png`). World list is scrollable with mouse wheel, auto-scrolls to keep selection visible. 4 bottom buttons (Play/Create/Cancel/Delete) are always visible. Delete button uses two-click confirmation, removes the world directory, and refreshes the list.
- Status: complete
- Notes: Added `world_scroll_offset` with `clamp_world_scroll` (auto-scrolls to keep selection visible) and `scroll_world_list` (mouse wheel). `world_row_rects` now takes a scroll offset and computes visible rows dynamically. `button_rects` for WorldSelect uses adaptive bottom spacing to fit 4 buttons. Scroll handling wired in main.rs MouseWheel event.

### 2026-07-13: Headless authoritative server foundation
- Owner: OpenCode
- Scope: Add a winit/wgpu-free TCP server binary using the shared fixed 20 TPS clock, chunk manager, scheduler, and native persistence. Accept bounded protocol sessions, complete handshake/keep-alive, autosave atomically, and reject unsupported client-authoritative edits. Player movement, chunk replication, inventory replication, and client conversion remain separate work.
- Depends on: Native multiplayer protocol contract; M2 fixed tick/persistence
- Acceptance: `vibecraft-server` starts without renderer setup, accepts a versioned client handshake, advances authoritative ticks, echoes keep-alives, and saves level/chunk state on shutdown.
- Status: complete
- Notes: Run `cargo run --bin vibecraft-server -- --world-dir PATH`; type `quit` on its console for a clean save and stop. Replication now lives in the server-side substrate and windowed client slice above; unsupported inventory actions still receive `NotAllowed`.

- [x] Extract a headless authoritative server using the same fixed-tick simulation and persistence code.
- [ ] Make the windowed executable a network client rather than a second simulation authority. `--server IP:PORT` and the in-game Join Server form activate an authoritative client path for snapshots, movement, chat, hotbar selection, and block-edit requests; full inventory/container actions and reconnect UX remain.
- [ ] Implement connection/session lifecycle, player identity/spawn/despawn, server-side input validation, and client-visible errors/disconnect reasons.
- [ ] Replicate initial/streamed chunks, authoritative player transforms, block edits, and supported inventory changes; preserve revisions/order so stale messages cannot overwrite newer state.
- [x] Add basic remote-player presentation and text chat so two connected players can identify and communicate with one another.
- [ ] Verify the Multiplayer Demo Contract with two clients, server restart, chunk-boundary edits, reconnect, and malformed/unsupported message rejection.
- [ ] After the demo, add interpolation, prediction/reconciliation, controlled latency/loss testing, permissions/operators, observability, authentication/moderation, broader entity replication, and multi-client load tests.
- [ ] Decide separately whether any external/Java protocol compatibility is worth supporting; require a pinned protocol version and packet tests before promising it.

**Demo exit criteria:** the Multiplayer Demo Contract release bar passes and server authority is preserved for all supported mutable state. Broader load, security, prediction, and parity requirements remain post-demo work.

**Recommended demo delivery order:** protocol/authority contract; headless server; one client connects and receives spawn/chunks; second client and player transforms; authoritative place/break; chat and inventory synchronization; reconnect/save; two-client release scenario. Do not start prediction, mobs, or protocol compatibility before this path is demonstrably playable.

## Deferred Until Prerequisites Exist

Do not schedule these as isolated features:

- Full block/item/mob checklists before the registry, state, entity, and serialization systems are in place.
- Redstone parity before scheduled ticks, block entities, and deterministic update-order tests exist.
- Natural structures before version-pinned worldgen and persistence exist.
- Multiplayer expansion beyond the demo before replication and the two-client release scenario exist.
- Java protocol, Anvil/NBT, resource-pack, or data-pack compatibility before their target versions and compatibility guarantees are chosen.

## Agent Work Item Template

Add a short entry under the relevant milestone when taking cross-cutting work:

```md
### YYYY-MM-DD: <work item>
- Owner: <agent or user>
- Scope: <files/systems and non-goals>
- Depends on: <milestone/work item>
- Acceptance: <observable checks and tests>
- Status: in progress | blocked | complete
- Notes: <decisions, migration needs, or follow-up>
```

Remove completed entries after their decisions and follow-ups have been folded into the milestone or `AGENTS.md`. Keep this file concise: detailed bug investigations belong in `ISSUES.md`.

## Historical Notes

The previous plan's large per-feature checklist was replaced because it duplicated items, mixed partial scaffolding with completed parity claims, and placed dependent work out of order. Existing implementation details remain discoverable in source, `AGENTS.md`, and `ISSUES.md`; this roadmap intentionally tracks capabilities and prerequisites instead of approximate feature counts.
