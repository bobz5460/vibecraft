# Spec: item-texture-system

Scope: feature

# Item Texture System Design

## Atlas Format

**ItemAtlas** — a dedicated wgpu `Rgba8UnormSrgb` texture containing all 605+ item sprites.

- **Tile size:** 16×16 pixels (matching Minecraft item sprite resolution)
- **Tiles per row:** Configurable, suggested 32 (matching terrain atlas), producing a 512×512 base
- If items exceed 1024 (32×32), the atlas grows to 64 tiles per row (1024×1024)
- **Format:** `Rgba8UnormSrgb` (same as terrain atlas; Minecraft textures are sRGB)
- **Usage:** `TEXTURE_BINDING | COPY_DST`
- **Filtering:** Nearest (pixel-art aesthetic)

Allocates one row at a time. Each 16×16 sprite packs sequentially left-to-right, top-to-bottom. No bin-packing overhead since all sprites are the same size.

## ItemId → Texture Mapping

A new function `item_texture_name(id: ItemId) -> &'static str` in `inventory/item.rs` returns the Minecraft texture filename stem (without `.png` or `item/` prefix). Examples:

| ItemId range | Examples | Texture stem |
|---|---|---|
| Block items (≤74) | Stone, Dirt, Oak Log | `"stone"`, `"dirt"`, `"oak_log"` (reuses block name) |
| Block items (>74) | Deepslate ores, planks | `"deepslate_iron_ore"`, etc. |
| Tools (75–124) | Wooden Pickaxe, Iron Sword | `"wooden_pickaxe"`, `"iron_sword"` |
| Food (125–146) | Apple, Bread, Cooked Beef | `"apple"`, `"bread"`, `"cooked_beef"` |
| Materials (147–172) | Stick, Iron Ingot, Diamond | `"stick"`, `"iron_ingot"`, `"diamond"` |
| Armor (173–204) | Leather Helmet, Diamond Chestplate | `"leather_helmet"`, `"diamond_chestplate"` |
| Misc (205–209) | Shield, Flint and Steel, Shears | `"shield"`, `"flint_and_steel"`, `"shears"` |

For items with multiple texture variants (clock, compass), use the default frame (`clock_00`, `compass_00`).

## ItemAtlas API (pseudocode)

```rust
pub struct ItemAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    /// Maps ItemId → normalized UV rect [u0, v0, u1, v1]
    items: HashMap<ItemId, [f32; 4]>,
    atlas_size: u32,
}

impl ItemAtlas {
    pub fn new(device: &Device, queue: &Queue, reader: &AssetReader) -> Self;
    
    /// Returns normalized UVs for the given item
    pub fn item_uv(&self, id: ItemId) -> Option<[f32; 4]>;
    
    /// Builds a quad with item texture for GUI rendering
    pub fn build_item_quad(&self, id: ItemId, x: f32, y: f32, size: f32) -> Option<(Vec<TextVertex>, Vec<u32>)>;
    
    /// Builds a billboard quad for 3D world rendering (dropped items)
    /// Returns vertex data with world-space positions facing the camera
    pub fn build_billboard(&self, id: ItemId, x: f32, y: f32, z: f32, size: f32, camera_pos: [f32; 3]) -> Option<(Vec<MeshVertex>, Vec<u32>)>;
}
```

## Dropped Item Billboard Rendering

Dropped items switch from `build_item_cube_mesh` to billboard quads:

- A single quad centered at the item's (x, y, z) position
- Always faces the camera (billboard rotation applied in vertex shader or CPU)
- Uses the `ItemAtlas` texture instead of the terrain atlas
- Rendered in the transparent/overlay pass (needs no depth writes, uses alpha testing)
- The quad size is ~0.25 world units (matching vanilla dropped item scale)

For the billboard, we have two options:
1. **CPU billboard**: Pre-rotate vertices toward the camera each frame (simpler, works with existing shaders)
2. **GPU billboard**: Pass a `model_view` matrix or use a geometry shader (more efficient but needs WGSL change)

**Recommendation**: CPU billboard for initial implementation. Main loop already iterates dropped items for physics; adding vertex rotation there is straightforward.

The billboard uses a new vertex type or reuses `MeshVertex` with the item atlas texture index. Since the item atlas is a separate texture from the terrain atlas, we'd need either:
- A separate bind group / pipeline for item billboards
- Or a combined atlas approach

**Recommendation**: Add a new render pass/pipeline for dropped item billboards with its own bind group pointing to the `ItemAtlas`. This keeps the terrain atlas pipeline unchanged.

## Changes to Main Event Loop

### Startup (`main.rs` ~line 669):
```rust
let item_atlas = ItemAtlas::new(&device, &queue, &asset_reader);
```

Pass `item_atlas` into the render context and make it accessible for both GUI and world rendering.

### UI Slot Resolution (`main.rs`):
Replace the heuristic `gui_asset_stem()` with direct lookup:
```rust
fn ui_slot(stack, items, block_sprites, item_atlas, selected) -> UiSlot {
    let sprite_name = if let Some(block) = items.block_from_item(stack.id) {
        format!("block/{}", block.name())
    } else {
        item_texture_name(stack.id).map(|n| format!("item/{n}"))
            .unwrap_or_else(|| "item/missing".to_string())
    };
    // ...
}
```

The `UiSlot` carries the ItemId so the renderer can look up the UV in `ItemAtlas`.

Alternatively, simplify further: remove the `sprite: String` from `UiSlot` and replace with `item_id: Option<ItemId>`, then let the renderer directly look up `ItemAtlas::item_uv(item_id)`.

## Renderer Changes (`engine/renderer.rs`)

### New bind group layout for item atlas:
```
BindGroupLayout {
    entries: [
        TextureView (item atlas),
        Sampler (nearest),
    ]
}
```

### UiCommand::Item handler:
Current: `self.gui_atlas.build_sprite(sprite, ...)` 
Updated: `self.item_atlas.build_item_quad(item_id, ...)` with fallback to colored rect.

### Dropped item world rendering:
After the main terrain opaque/transparent passes, add a billboard pass:
- Iterate visible dropped items
- Build billboard quads using `ItemAtlas::build_billboard()`
- Submit with the item atlas bind group

## Migration and Compatibility

- Block items continue to use block textures from the terrain atlas for both UI and dropped items
- The `inventory_block_sprite_map` in `main.rs` remains for block items
- Item textures in the `GuiAtlas` can be removed once `ItemAtlas` is the authoritative source, but removing them is optional — they'll simply be unused
- The existing `build_item_cube_mesh()` remains for backwards compatibility but is no longer the primary code path for dropped items