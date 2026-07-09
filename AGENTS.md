# Vibecraft — Dev Guide for Agents

## Build & Run

```sh
cargo build              # debug (~1min first build, ~1s incremental)
cargo build --release    # release (opt-level=3 + LTO + codegen-units=1)
cargo run --release      # recommended for gameplay (debug is slow)
```
To more accurately identify errors, always build the full app and run it at the end of a session.
No test framework, lint config, CI, or pre-commit hooks exist yet.

**Build speed tricks (already configured):**
- Dev profile: `codegen-units=256`, `incremental=true`
- `image` crate: only PNG feature (excludes JPEG/GIF/BMP/WebP decoders)
- `.cargo/config.toml`: incremental builds enabled

## Project Structure

Single Rust crate (not a workspace). Key modules:

- `engine/` — `camera`, `input`, `renderer` (wgpu), `window` (winit)
- `world/` — `block` (BlockId enum), `chunk` (16x384x16), `chunk_manager`, `mesh` (greedy), `world_gen` (noise), `raycast`, `block_registry`
- `shaders/` — `chunk.wgsl`, `highlight.wgsl`
- `main.rs` — event loop, game state, all wiring lives here

All block IDs live in one flat `BlockId` enum (`src/world/block.rs`). `is_transparent()`, `light_level()` are methods on the enum.

## Critical Gotchas

### Chunk mesh positions MUST be in world coordinates
`build_chunk_mesh` emits vertices with `chunk.cx * 16` added to X and `chunk.cz * 16` added to Z. The shader applies VP matrix directly — local coordinates would make every chunk overlap at origin.

### Face winding rules (FrontFace::Ccw)
Greedy meshing uses an `emit_quad` with per-face winding. The `reversed` flag maps as:
- `reversed=true`: Top, Right, Back (positive-normal faces except Front)
- `reversed=false`: Bottom, Left, Front (negative-normal faces plus Front)

Changing this mapping without recomputing cross products for each face will produce culled or inside-out faces.

### Face emission position: positive-normal faces need +1 offset
The block's origin `(bx, by, bz)` is the block's minimum corner. The Top face (normal +Y) must be at `Y=by+1`, not `Y=by`. Same for Right (`X=bx+1`) and Front (`Z=bz+1`). The mesh emits a separate `(fx, fy, fz)` for the face position while keeping `(bx, by, bz)` for block-lookup and skylight.

### Buffer creation requires COPY_DST
All vertex/index buffers written with `queue.write_buffer` must include `BufferUsages::COPY_DST` in their usage flags. The uniform buffer also needs `COPY_DST`.

### Uniform alignment: use vec4, never vec3
WGSL `vec3<f32>` has 16-byte alignment in structs, but Rust `[f32; 3]` has 4-byte alignment. This causes field offset mismatches that silently corrupt uniforms. Always use `vec4<f32>` / `[f32; 4]` for shader uniform fields, even when only 3 components are needed.

### WGSL: no swizzle assignment
`color.rgb *= factor` is illegal. Use `color = vec4<f32>(color.r * factor, color.g * factor, color.b * factor, color.a)`.

### Camera conventions
- `pitch += dy` (mouse-up → negative dy → pitch decreases → looks up)
- `yaw -= dx` (mouse-right → negative delta → yaw decreases)
- A key calls `move_right(speed)` and D calls `move_right(-speed)` (swapped from common convention)
- Starting yaw=0 means looking toward +Z

### Texture atlas
Real Minecraft textures loaded from `minecraft-assets` repo (cloned to `/tmp/opencode/minecraft-assets` by default, overridable via `VIBECRAFT_ASSETS` env var). Atlas is built at startup by resolving blockstate JSON → model JSON → face texture paths → PNG loading → GPU upload. `src/assets/` module handles all of this:
- `blockstate.rs` — parses blockstate JSON, picks default variant
- `model.rs` — parses model JSON, resolves parent chain, resolves face textures
- `texture_map.rs` — maps `BlockId` + `BlockFace` to atlas tile index via `OnceLock` globals in `mesh.rs`
- `mod.rs` — `LoadedTextureManager` builds the GPU atlas from loaded PNGs

Face ordering: faces array is `[up, down, west, east, south, north]` matching `FACES = [Top, Bottom, Left, Right, Front, Back]`. Minecraft `north` = -Z = our `Back`, `south` = +Z = our `Front`.

### Shader get_tile_uv: V is flipped
`get_tile_uv` in `chunk.wgsl` flips V with `1.0 - fract(uv).y` so PNG row 0 (top of image) maps to the top of the geometry face. Without this flip, grass side green strip appears at the bottom of the block.

### Model resolver: only first element
`resolve_face_textures` in `model.rs` only reads the **first** element of each model. Overlay elements (used by grass_block for `tintindex` layers) are skipped. Otherwise the greyscale overlay replaces the colored base texture.

### Biome tinting (greyscale textures)
Many Minecraft textures are stored as grayscale and colored by biome tinting (`tintindex` in block models). The code hardcodes tints for known textures in `load_all_pngs()`:
- `grass_block_top` → green (0.5, 1.0, 0.35)
- Leaves, ferns, vines → various greens
- Water → blue (0.25, 0.5, 0.9)

### `impl ChunkManager` is split across files
Methods in `chunk_manager.rs` and `raycast.rs` both use `impl ChunkManager`. Both files must be kept in sync for field access.

## PLAN.md
Tracks all feature phases. Check it before deciding what to work on next. AGENTS.md is not a dynamic file. It should stay static unless absolutely required for it to change. All dynamic work should be put in PLAN.md
