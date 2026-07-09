pub mod blockstate;
pub mod model;
pub mod texture_map;

use std::collections::HashMap;
use wgpu::{Device, Queue, Texture, TextureView, Sampler};
use crate::world::block::{BlockId, BlockFace};

pub const ATLAS_TILE_SIZE: u32 = 16;
pub const ATLAS_TILES_PER_ROW: u32 = 32;

pub struct LoadedTextureManager {
    pub atlas_texture: Texture,
    pub atlas_view: TextureView,
    pub atlas_sampler: Sampler,
    pub tile_count: u32,
    face_tiles: HashMap<(BlockId, BlockFace), u32>,
    crossed_tiles: HashMap<BlockId, u32>,
}

impl LoadedTextureManager {
    pub fn new(device: &Device, queue: &Queue, asset_path: &str) -> Self {
        let mapping = texture_map::build_texture_mapping(asset_path);
        let tiles = mapping.build_tile_list();
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
        }
    }

    pub fn face_map(&self) -> &HashMap<(BlockId, BlockFace), u32> {
        &self.face_tiles
    }

    pub fn crossed_map(&self) -> &HashMap<BlockId, u32> {
        &self.crossed_tiles
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
