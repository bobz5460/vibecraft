pub mod blockstate;
pub mod model;
pub mod texture_map;
pub mod reader;
pub mod gui_atlas;
pub mod item_atlas;
pub mod lang;

use std::collections::HashMap;
use std::sync::Arc;
use wgpu::{Device, Queue, Texture, TextureView, Sampler};
use crate::world::block::{BlockId, BlockFace};
use crate::world::block_registry::registry;
use crate::assets::blockstate::{load_blockstate, resolve_models, BlockstateFile, ResolvedBlockstateModel};
use crate::assets::model::{resolve_model, ResolvedModel};
use crate::assets::reader::AssetReader;

pub const ATLAS_TILE_SIZE: u32 = 16;
pub const ATLAS_TILES_PER_ROW: u32 = 32;
pub const ATLAS_SIZE: u32 = ATLAS_TILE_SIZE * ATLAS_TILES_PER_ROW;

/// Immutable model data shared by every async mesh worker. Only block families
/// whose normal gameplay states are represented by ordinary block models are
/// cached here; fluids and block entities retain their dedicated paths.
#[derive(Clone)]
pub struct BlockMeshAssets {
    blocks: HashMap<BlockId, CachedBlockstate>,
    texture_tiles: HashMap<String, u32>,
}

#[derive(Clone)]
struct CachedBlockstate {
    state: BlockstateFile,
    models: HashMap<String, ResolvedModel>,
}

impl BlockMeshAssets {
    fn load(reader: &AssetReader) -> Self {
        let supported = [
            (BlockId::StoneSlab, "stone_slab"),
            (BlockId::OakSlab, "oak_slab"),
            (BlockId::StoneStairs, "cobblestone_stairs"),
            (BlockId::OakStairs, "oak_stairs"),
            (BlockId::OakDoor, "oak_door"),
            (BlockId::OakFence, "oak_fence"),
            (BlockId::RedstoneDust, "redstone_wire"),
        ];
        let mut blocks = HashMap::new();
        for (id, name) in supported {
            let Some(state) = load_blockstate(reader, name) else {
                log::warn!("missing blockstate for generic mesh {name}");
                continue;
            };
            let mut model_names = Vec::new();
            for value in state.variants.values().chain(state.multipart.iter().map(|part| &part.apply)) {
                match value {
                    crate::assets::blockstate::VariantValue::Single(variant) => model_names.push(variant.model.as_str()),
                    crate::assets::blockstate::VariantValue::Array(variants) => {
                        model_names.extend(variants.iter().map(|variant| variant.model.as_str()));
                    }
                }
            }
            model_names.sort_unstable();
            model_names.dedup();
            let mut resolved = HashMap::new();
            for model_name in model_names {
                let normalized = model_name
                    .strip_prefix("minecraft:block/")
                    .or_else(|| model_name.strip_prefix("block/"))
                    .unwrap_or(model_name);
                if let Some(model) = resolve_model(reader, normalized) {
                    resolved.insert(normalized.to_string(), model);
                } else {
                    log::warn!("failed to resolve generic block model {normalized}");
                }
            }
            blocks.insert(id, CachedBlockstate { state, models: resolved });
        }
        Self { blocks, texture_tiles: HashMap::new() }
    }

    fn texture_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for cached in self.blocks.values() {
            for model in cached.models.values() {
                for element in &model.elements {
                    names.extend(element.faces.iter().map(|face| face.texture.clone()));
                }
            }
        }
        names.sort_unstable();
        names.dedup();
        names
    }

    fn set_texture_tiles(&mut self, tiles: &[texture_map::TextureTile]) {
        self.texture_tiles = tiles
            .iter()
            .filter(|tile| tile.frame == 0)
            .map(|tile| (tile.path.clone(), tile.tile_index))
            .collect();
    }

    pub fn has_model(&self, id: BlockId) -> bool {
        self.blocks.contains_key(&id)
    }

    pub fn resolve<'a>(&'a self, id: BlockId, state: u16, position: (i32, i32, i32)) -> Vec<(&'a ResolvedModel, ResolvedBlockstateModel)> {
        let Some(cached) = self.blocks.get(&id) else { return Vec::new(); };
        let Some(properties) = registry().properties_for_state(id, state) else { return Vec::new(); };
        let properties = properties.into_iter().map(|(key, value)| (key.to_string(), value.to_string())).collect();
        resolve_models(&cached.state, &properties, position)
            .into_iter()
            .filter_map(|resolved| cached.models.get(&resolved.model).map(|model| (model, resolved)))
            .collect()
    }

    pub fn texture_tile(&self, texture: &str) -> Option<u32> {
        self.texture_tiles.get(texture).copied()
    }
}

pub struct LoadedTextureManager {
    pub atlas_texture: Texture,
    pub atlas_view: TextureView,
    pub atlas_sampler: Sampler,
    pub tile_count: u32,
    face_tiles: HashMap<(BlockId, BlockFace), u32>,
    crossed_tiles: HashMap<BlockId, u32>,
    mesh_assets: Arc<BlockMeshAssets>,
}

impl LoadedTextureManager {
    pub fn new(device: &Device, queue: &Queue, reader: &AssetReader) -> Self {
        let mapping = texture_map::build_texture_mapping(reader);
        let mut mesh_assets = BlockMeshAssets::load(reader);
        let tiles = mapping.build_tile_list(&mesh_assets.texture_names());
        mesh_assets.set_texture_tiles(&tiles);
        let pixels = mapping.load_all_pngs(&tiles);

        let pixel_data = pack_atlas(&pixels, ATLAS_TILE_SIZE, ATLAS_TILES_PER_ROW);
        let atlas_size = ATLAS_TILE_SIZE * ATLAS_TILES_PER_ROW;

        let texture_size = wgpu::Extent3d {
            width: atlas_size,
            height: atlas_size,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("minecraft_atlas"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Rgba8UnormSrgb is the correct format for Minecraft textures because
            // they are authored in sRGB color space. The hardware will apply
            // sRGB-to-linear conversion during sampling, ensuring correct
            // lighting calculations, and linear-to-sRGB conversion during
            // framebuffer write. Using Rgba8Unorm (linear) would result in
            // incorrect colors since the source data is already sRGB-encoded.
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixel_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_size * 4),
                rows_per_image: Some(atlas_size),
            },
            texture_size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let tile_count = tiles.len() as u32;
        let face_tiles = mapping.resolve_face_tiles(&tiles);
        let crossed_tiles = mapping.resolve_crossed_tiles(&tiles);

        LoadedTextureManager {
            atlas_texture: texture,
            atlas_view: view,
            atlas_sampler: sampler,
            tile_count,
            face_tiles,
            crossed_tiles,
            mesh_assets: Arc::new(mesh_assets),
        }
    }

    pub fn face_map(&self) -> &HashMap<(BlockId, BlockFace), u32> {
        &self.face_tiles
    }

    pub fn crossed_map(&self) -> &HashMap<BlockId, u32> {
        &self.crossed_tiles
    }

    pub fn mesh_assets(&self) -> Arc<BlockMeshAssets> {
        Arc::clone(&self.mesh_assets)
    }

}

fn pack_atlas(tile_images: &[Vec<u8>], tile_size: u32, tiles_per_row: u32) -> Vec<u8> {
    let atlas_size = tile_size * tiles_per_row;
    let mut pixels = vec![0u8; (atlas_size * atlas_size * 4) as usize];

    for (idx, tile) in tile_images.iter().enumerate() {
        let tile_x = (idx as u32) % tiles_per_row;
        let tile_y = (idx as u32) / tiles_per_row;

        for py in 0..tile_size {
            for px in 0..tile_size {
                let src_i = ((py * tile_size + px) * 4) as usize;
                let dst_x = tile_x * tile_size + px;
                let dst_y = tile_y * tile_size + py;
                let dst_i = ((dst_y * atlas_size + dst_x) * 4) as usize;

                if src_i + 3 < tile.len() {
                    pixels[dst_i] = tile[src_i];
                    pixels[dst_i + 1] = tile[src_i + 1];
                    pixels[dst_i + 2] = tile[src_i + 2];
                    pixels[dst_i + 3] = tile[src_i + 3];
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_textures_fit_the_fixed_atlas() {
        let root = "/tmp/opencode/minecraft-assets";
        if !std::path::Path::new(root).join("assets/minecraft").is_dir() {
            return;
        }
        let reader = AssetReader::new(std::path::PathBuf::from(root).join("assets/minecraft"));
        let mapping = texture_map::build_texture_mapping(&reader);
        let assets = BlockMeshAssets::load(&reader);
        let tiles = mapping.build_tile_list(&assets.texture_names());
        assert!(tiles.len() <= (ATLAS_TILES_PER_ROW * ATLAS_TILES_PER_ROW) as usize, "{} tiles exceed atlas capacity", tiles.len());
    }
}
