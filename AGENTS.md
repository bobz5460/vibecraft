# Vibecraft Agent Guide

This repository is a native Rust implementation of Minecraft-like Java Edition gameplay and rendering. It is an actively evolving single-crate prototype, not a stable engine or a finished game. Favor small, correct, end-to-end changes over broad feature lists or placeholder scaffolding.

## Read First

1. Read this file and the relevant section of `PLAN.md` before changing code.
2. Run `git status --short` before editing. The worktree is shared and is commonly dirty.
3. Treat unrelated changes as owned by another person or agent. Do not revert, reset, checkout, or reformat them.
4. Read `ISSUES.md` when working in a subsystem with existing correctness, performance, or shader concerns.
5. Search for all call sites before changing a public struct, shader uniform, `ChunkManager`, `BlockId`, `ItemId`, or renderer cache contract.

## Scope And Documentation

- `PLAN.md` is the dynamic roadmap and cross-cutting work log. Update it when a task changes priority, scope, dependency, or completion status.
- `AGENTS.md` is the durable implementation guide. Add rules here only when they are repeatedly useful and verified against source.
- `ISSUES.md` is the detailed bug and risk log. Do not turn `PLAN.md` into another issue tracker. Its historical entries can lag source, so confirm an issue against current code before acting on it.
- A feature is not complete because an enum variant, command, debug path, or placeholder renderer exists. It must work through normal gameplay and meet its roadmap acceptance criteria.
- The project intends Java Edition parity, but no exact target data version has been pinned. Do not claim exact parity, Java protocol compatibility, or Anvil/NBT compatibility unless the target version and tests are explicitly defined.

## Build And Verify

- **Baseline for code changes:** `cargo build`
- **Gameplay/rendering baseline:** `cargo build && timeout 15 cargo run --release`
- Always build before running. The 15-second run catches startup panics, worker deadlocks, GPU validation errors, and asset-loading failures.
- A `timeout` exit after an otherwise healthy windowed run is expected. A panic, validation error, freeze, or error log is not expected; identify and fix its root cause.
- Run targeted unit tests when they exist: `cargo test <name>`. There is no established test suite, CI job, lint job, or git hook yet.
- Release builds use `opt-level = 3`, LTO, and one codegen unit. `.cargo/config.toml` enables incremental compilation.
- Do not run a formatter, clippy, or bulk rewrite across unrelated files unless the task requires it. The shared worktree makes broad formatting changes expensive to review.

### Assets And Runtime Environment

- The default Minecraft asset checkout is `/tmp/opencode/minecraft-assets`.
- Override the asset root with `VIBECRAFT_ASSETS=/path/to/minecraft-assets`.
- The root must contain `assets/minecraft/...`; do not point the variable directly at `assets/minecraft`.
- Startup loads terrain textures, font assets, and common sounds. Test with real assets after changing an asset path, loader, atlas, or shader sampling rule.
- If startup fails because assets are absent, report that as an environment blocker. Do not silently replace a production asset failure with arbitrary placeholder content unless the task specifically adds a documented fallback.

## Repository Map

| Path | Responsibility | Change notes |
|---|---|---|
| `src/main.rs` | application setup, winit event loop, gameplay wiring, commands, input-to-action flow, render-cache coordination | Large orchestration file. Keep reusable game rules out of it when adding new code. |
| `src/engine/camera.rs` | camera vectors, view/projection, frustum test, shadow matrix | Camera yaw `0` looks toward `+Z`; projection converts OpenGL depth to wgpu depth. |
| `src/engine/input.rs` | keyboard/mouse state and focus reset | Input must be cleared/released on focus loss and UI transitions. |
| `src/engine/renderer.rs` | wgpu initialization, terrain buffers, passes, overlays, screenshots | Rust buffer layouts and WGSL layouts must change together. |
| `src/engine/text.rs` | font texture and text/colored-quad geometry | UI coordinates and atlas assumptions are shared with renderer/UI code. |
| `src/engine/audio.rs` | rodio loading and sound playback | Current audio is a foundation, not positional vanilla audio. |
| `src/assets/` | blockstate/model parsing and terrain texture mapping | Terrain asset data is resolved at startup into the texture atlas; GUI/lang helpers are still under construction. |
| `src/world/block.rs` | block IDs, block flags, faces, compact `Block` value | Current block state is `BlockId + u8 data`; it is not a general vanilla state system. |
| `src/world/chunk.rs` | 16x384x16 chunk storage, light arrays, fluid tracking | Chunk-local storage must not own global lighting policy. |
| `src/world/chunk_manager.rs` | async generation, async lighting/meshing, revisions, loaded chunks and meshes | The authoritative world mutation and invalidation boundary. |
| `src/world/mesh.rs` | greedy and special-case terrain mesh generation | Emits world-space positions and separate opaque/translucent meshes. |
| `src/world/raycast.rs` | `ChunkManager::raycast` DDA implementation | Keep this extension impl aligned with `ChunkManager` APIs. |
| `src/world/world_gen.rs` | terrain, biome, cave, ore, and decoration generation | Generation is custom and not target-version-faithful yet. |
| `src/world/dropped_item.rs` | dropped item and XP orb behavior | This is not a general entity framework. |
| `src/inventory/` | item registry, stacks, inventory slots and operations | Current item stack has ID/count only; durability/equipment are future work. |
| `src/player/` | movement, collision, survival values, effects | Player code queries the world through `ChunkManager`. |
| `src/shaders/` | WGSL terrain, lighting, sky, GUI, text, highlight, and break passes | WGSL is compiled/validated at runtime by wgpu. |

## Current Architecture Boundaries

### Application And Frame Flow

- `main.rs` creates the window, renderer, world/chunk manager, player, inventory, audio, and per-frame render data.
- The event loop owns raw winit events. `InputState` collects them; gameplay code consumes them once per frame.
- `main.rs` coordinates chunk streaming, completed async work, physics, interactions, fluids, dropped items, UI state, and rendering. It is currently the integration point, not a reusable simulation API.
- `Renderer::render` accepts `RenderContext`; it should not reach back into `ChunkManager` or gameplay state.
- The current game loop is frame-driven. A fixed 20 TPS simulation boundary is roadmap work, not an existing invariant.

### World Coordinates And Chunks

- Chunks are columns: `CHUNK_SIZE = 16`, `CHUNK_HEIGHT = 384`, and `CHUNK_VOLUME = 98,304`.
- Chunk-local Y is always `0..CHUNK_HEIGHT`. Public world Y is selected by immutable `WorldCoordinateProfile`: legacy saves use `0..383`; new Java-profile Overworlds use `-64..319`. Keep chunk storage, lighting, fluid work, and chunk wire payloads local; convert only at `ChunkManager`'s public world-coordinate boundary and mesh output.
- `WorldGenerationProfile` is persisted separately from `WorldCoordinateProfile`. Migrated worlds retain legacy pre-corrected interpolation; new local/server worlds use the corrected Minecraft-26 base. Do not infer or change one profile from the other, and thread generation behavior through `ChunkManager` workers so old/new chunk seams remain stable.
- `Chunk::index(x, y, z)` is `(y * 16 + z) * 16 + x`. Preserve this ordering for any parallel block/light arrays.
- Convert negative world coordinates with `div_euclid` and `rem_euclid`, as `ChunkManager::get_block` and `set_block` do. Never use `/` and `%` for chunk lookup at negative positions.
- `Chunk` contains blocks, sky light, block light, dirty flags, and fluid bookkeeping. It does not decide cross-chunk lighting propagation.

### Safe Block Mutation

For normal gameplay, mutate blocks through `ChunkManager::set_block`, not by directly editing a loaded chunk.

`set_block` performs these responsibilities:

1. Validates world Y and resolves chunk/local coordinates.
2. Updates the block and chunk-local fluid bookkeeping.
3. Marks the chunk dirty and increments its revision.
4. Compares light signatures and schedules relighting only when opacity/emission changes.
5. Marks affected neighbor chunks dirty at chunk boundaries; cardinal neighbors relight when needed and diagonal neighbors remesh for AO sampling.

If an interaction needs immediate visual correctness, follow the mutation with the existing synchronous rebuild path (`rebuild_chunk_now` or `rebuild_chunks_now`) and evict/recreate the corresponding GPU cache entries. Background work is appropriate for streaming; immediate place/break feedback is a separate requirement.

When adding or changing a block, inspect all relevant behavior rather than only `BlockId`:

- `BlockId` uses `#[repr(u16)]`; `from_repr` uses a bounded unsafe transmute. Update its maximum discriminant and maintain contiguous variants if adding IDs.
- Update solidity, transparency, crossed geometry, climbing, slab/stair, light emission, and all bitmasks or match tables that apply.
- `is_transparent()` controls render/culling behavior. Light opacity is separately defined in `chunk_manager.rs`; do not infer one from the other.
- Add face/crossed texture mapping and choose opaque, cutout, or translucent handling in `mesh.rs`.
- Update drops, item mapping, collision, world generation, commands, and UI names when the new block is meant to be normally playable.

## Async Chunk, Lighting, And Mesh Rules

- Generation, lighting, and meshing use background workers. Their results are snapshots and may be stale when received.
- `ChunkManager` revisions are the stale-work guard. Preserve dependency snapshots and revision checks when changing worker tasks or adding data that affects mesh/light output.
- `process_loaded_chunks()` admits a bounded number of generation results and schedules valid lighting before first meshing. Do not publish a mesh built with bootstrap/fabricated lighting.
- `rebuild_dirty_meshes()` applies valid light results, applies valid mesh results, schedules more lighting, then schedules bounded mesh work. Its order is intentional.
- `rebuild_dirty_meshes()` returns chunk keys whose CPU meshes were rebuilt. Any caller caching GPU `ChunkRenderData` must evict those keys before rebuilding render data.
- Mesh tasks need neighboring chunks for face culling and vertex AO/light sampling. Preserve 3x3 neighborhood dependencies and mark boundaries correctly.
- Never move lighting recomputation into `Chunk`; lighting belongs to `ChunkManager` because it crosses chunk boundaries.
- Avoid unbounded queues or per-frame full-world scans. Generation is nearest-first and bounded; meshing and lighting have in-flight limits for a reason.

## Meshing, Lighting, And Rendering Rules

### Terrain Meshes

- Mesh positions are world-space. `build_chunk_mesh` adds `chunk.cx * 16` to X and `chunk.cz * 16` to Z.
- Chunk meshes have distinct opaque and translucent vertex/index arrays. Keep material/pass classification consistent with shader blending and depth behavior.
- Positive-normal faces need the `+1` plane offset: Top uses `y + 1`, Right uses `x + 1`, and Front uses `z + 1`.
- Greedy meshing uses `FrontFace::Ccw`. Current winding is reversed for Top/Right/Back and not reversed for Bottom/Left/Front.
- Vertex lighting samples the adjacent air cell. Negative-normal faces need the extra `-1` sample offset.
- The greedy merge key includes data that affects visual equivalence. Do not merge faces across texture, material, light, AO, or state changes that would produce a visible seam.
- Crossed plants and special geometry are separate from greedy cube geometry. Do not mark a block crossed unless it is emitted by the crossed-geometry path.

### GPU Cache And Culling

- `rebuild_render_data` owns the mapping from CPU chunk meshes to GPU `ChunkRenderData` and prunes entries for unloaded chunks.
- The current render data builder uses camera AABB frustum culling for the main terrain draw list while retaining all chunk data for passes that need it. Do not reintroduce a claim that all chunks are always rendered without checking this path.
- Keep `BufferUsages::COPY_DST` on every buffer written with `queue.write_buffer`.
- Do not allocate a zero-sized wgpu buffer. Skip empty geometry or allocate a safe minimum size as the surrounding code expects.
- Resize paths must recreate size-dependent textures/views and update camera aspect. Test window resize after changing render targets or screen-space passes.

### Shaders And Uniforms

- Rust uniform structs must be layout-compatible with WGSL. Keep field order, alignment, padding, bind group index, and binding index synchronized.
- Use `[f32; 4]` in Rust and `vec4<f32>` in WGSL for uniform vector fields. Do not introduce `vec3` uniforms without deliberately handling their alignment.
- WGSL forbids swizzle assignment. Build and assign a full vector instead.
- `get_tile_uv()` flips V with `1.0 - fract(uv).y`; preserve that convention for atlas texture lookup.
- Atlas constants are defined in `src/assets/mod.rs`; matching constants/assumptions in WGSL must be updated together.
- Test opaque, cutout, and translucent terrain after changing material flags. Water, lava, leaves, and glass have different depth/blending expectations.
- Shader failures may only appear when a pipeline is created at runtime. A successful Rust compile is insufficient verification for WGSL work.

## Asset Pipeline

- `LoadedTextureManager::new` builds the terrain atlas at startup.
- The current path is block ID -> blockstate JSON -> model JSON -> face texture paths -> PNG pixels -> fixed 16px tile atlas upload.
- Atlas constants: `ATLAS_TILE_SIZE = 16`, `ATLAS_TILES_PER_ROW = 32`, and `ATLAS_SIZE = 512`. Atlas overflow/capacity changes require coordinated Rust and shader updates.
- Face texture order is `[up, down, west, east, south, north]`, matching `FACES = [Top, Bottom, Left, Right, Front, Back]`.
- Models currently resolve only the first element for face textures; overlays and general multi-element models are explicitly roadmap work. Do not accidentally claim support for them from a loader-only change.
- Biome-tinted grayscale textures currently receive static/default treatment in `load_all_pngs`; dynamic per-biome tint must travel through the mesh/shader data path.
- Minecraft textures are authored in sRGB. The atlas uses `Rgba8UnormSrgb`; do not change this to linear format without auditing the entire lighting/output pipeline.

## Camera, Input, UI, And Player Rules

- Camera yaw `0` faces `+Z`. `Camera::forward` uses `+X` for positive yaw and negative pitch for looking upward.
- `main.rs` currently applies `camera.rotate(dx * 0.003, dy * 0.003)`. Preserve user-visible direction unless intentionally changing controls and documenting the migration.
- Pitch is clamped to avoid a degenerate view basis; yaw is wrapped. Do not remove those guards.
- Camera ray origin starts at the near plane. Raycast direction must be nonzero and its zero-axis handling must retain finite DDA progression.
- The player camera position is player position plus current eye height. Movement/collision changes must preserve this relationship, swimming/sneaking heights, and raycast reach behavior.
- GUI hit-testing currently has pixel constants in `screen_pos_to_inventory_slot` tied to the inventory GUI texture. Update rendering and hit testing together if GUI scale, texture, or slot layout changes.
- UI modes must release/grab the cursor deliberately, consume input appropriately, and clear stuck input when focus is lost.
- The inventory has 9 hotbar, 27 main, 4 armor, and 1 offhand slots. Armor/offhand storage exists but mechanics are not complete.

## Performance And Failure Rules

- Do not turn bounded async work into unbounded queues, one thread per chunk, or full chunk scans every frame.
- Avoid cloning full chunks in hot paths. A chunk includes large block and light arrays; snapshot only the scope required by the operation.
- Prefer warnings/errors for missing assets, models, mappings, and worker failures. Do not silently render a random/default texture without a diagnostic.
- Do not add `unwrap()` or `expect()` to normal runtime paths where a recoverable asset, GPU, channel, or filesystem error can occur.
- Preserve stale-result rejection. Faster code that can publish old light or mesh data is a correctness regression.
- Use the F5 profiler when changing chunk generation, meshing, lighting, GPU upload, or draw submission. Report workload changes alongside visual results.

## Common Change Checklists

### New Block Or Block Variant

1. Add the ID safely and update `from_repr` bounds/bitmasks.
2. Define state, collision, render classification, light emission, and light opacity separately.
3. Add asset/model/texture mapping and verify every face orientation.
4. Add meshing behavior, including neighbor culling, AO, and transparent pass selection.
5. Add normal gameplay paths: placement, breaking, drops, inventory item, and world generation if applicable.
6. Test at chunk edges, in darkness, beside emissive blocks, and in front of opaque/translucent neighbors.

### New Shader Or Render Feature

1. Identify its input data and whether it is per-vertex, per-frame uniform, texture, or storage data.
2. Change Rust and WGSL layouts together, including pipeline/bind-group creation.
3. Test with a real asset world in Regular and Vibrant render quality, day and night, and after resize.
4. Check transparent objects, shadowing, fog, and UI layering when the feature touches a shared pass.
5. Run the release startup smoke test and inspect wgpu logs.

### New Player Or Inventory Mechanic

1. Put the rule in `player/` or `inventory/`, not only in input/command code in `main.rs`.
2. Define behavior for every gamemode and difficulty where relevant.
3. Ensure normal input/UI triggers it; commands may be useful for setup but are not the only path.
4. Update HUD, item consumption/drops, effects, and future persistence requirements.
5. Test target, no-target, empty-stack, full-inventory, death/respawn, and chunk-boundary cases as applicable.

### Chunk/Lighting Change

1. Identify whether blocks, light, mesh, or all three change.
2. Use revisions and task dependencies for any asynchronous result.
3. Mark source and required boundary neighbors dirty; include diagonal remesh requirements for AO.
4. Evict affected GPU cache entries after rebuilt mesh keys are returned.
5. Test place/break at local coordinates `0` and `15`, at low/high Y, and next to an emitter/occluder.

## Handoff Expectations

When finishing a task, report:

- What behavior changed and what deliberately remains unsupported.
- The files/subsystems touched and any contract that changed.
- Commands run and their results.
- Manual scenario used for gameplay/rendering validation.
- Follow-up work, migrations, or risks that another agent must know.

Do not claim completion if verification was blocked. State the blocker precisely, including the command, missing dependency, or runtime failure.
