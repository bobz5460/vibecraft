---
plan name: real-item-textures
plan description: Item texture atlas and sprites
plan status: done
---

## Idea
Load all 605+ item PNG textures from the Minecraft assets folder into a dedicated GPU atlas, create an ItemId→texture mapping, wire it into the GUI inventory/hotbar rendering, and render dropped items as 2D billboard sprites instead of block cubes. Stack counts are already supported in the UI (renders count text when >1) but need verification.

## Implementation
- 1. Create ItemId→texture name mapping in inventory/item.rs — similar to minecraft_name() in texture_map.rs but for all item IDs, matching actual filenames in textures/item/
- 2. Create ItemAtlas struct in assets/item_atlas.rs — loads all textures/item/*.png and textures/block/*.png into a dedicated wgpu texture atlas with sprite UV lookup
- 3. Register item atlas in main.rs and wire it into the renderer — create during startup, expose for GUI + world rendering
- 4. Fix ui_slot() in main.rs to use the correct sprite names from the new ItemId mapping so all items show their real textures in inventory/hotbar
- 5. Update UiCommand::Item handling in engine/renderer.rs to render items from the ItemAtlas (or fall back to existing GUI atlas path)
- 6. Replace dropped-item cube mesh (build_item_cube_mesh) with 2D billboard quads facing the camera, using item atlas textures
- 7. Render dropped stacks by ItemId-derived sprite key while retaining persisted block IDs only for legacy save recovery
- 8. Verify stack count display — count>1 renders as an overlaid number in the bottom-right corner of item slots in both hotbar and inventory
- 9. Build and release smoke test — cargo build && timeout 15 cargo run --release with real assets

## Required Specs
<!-- SPECS_START -->
- item-texture-system
- dropped-item-billboards
<!-- SPECS_END -->
