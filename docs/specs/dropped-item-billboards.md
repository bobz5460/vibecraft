# Spec: dropped-item-billboards

Scope: feature

# Dropped Item Billboard Design

## All Items Use Billboards (Blocks Too)

**All** dropped items — block items AND non-block items — render as 2D camera-facing billboard sprites, matching vanilla Minecraft behavior. No more block cubes on the ground.

### Block items in the ItemAtlas

Block items (stone, dirt, logs, etc.) do NOT have separate PNGs in `textures/item/`. Instead, the ItemAtlas loads their textures from `textures/block/{name}.png` — the same source as the terrain atlas. Each block texture is available under its `block/<stem>` key.

This means:
- Non-block item → `ItemAtlas::item_uv(ItemId)` → `textures/item/{name}.png`
- Block item → `ItemAtlas::block_uv(BlockId)` → `textures/block/{name}.png`

### DroppedItem data change

`stack.id` is the authoritative render identity. `block_id` remains persisted solely for legacy native-save recovery and is never used to select a render texture. This avoids a save-format migration while keeping rendering correct for non-block stacks.

Block/model-only assets that have no 16px block or item PNG use the atlas diagnostic sprite until generic item-model rendering is implemented.

### Billboard rendering

CPU billboard: vertices rotated toward camera each frame (simple, works with existing shaders). The renderer accumulates billboard quads for all visible dropped items and submits them in a dedicated overlay pass with the ItemAtlas bind group.

### New shader pass

A simple WGSL pass that samples from the ItemAtlas texture. Uses alpha clip to discard transparent pixels from item sprites. No depth writes. Renders after terrain transparent pass but before GUI.
