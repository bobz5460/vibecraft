use std::collections::HashMap;
use std::sync::Mutex;

use crate::assets::reader::AssetReader;
use crate::engine::text::TextVertex;

pub const ITEM_ATLAS_TILE_SIZE: u32 = 16;
pub const ITEM_ATLAS_TILES_PER_ROW: u32 = 64;
pub const ITEM_ATLAS_SIZE: u32 = ITEM_ATLAS_TILE_SIZE * ITEM_ATLAS_TILES_PER_ROW;

/// Dedicated fixed-tile atlas for item icons and block-item fallback sprites.
/// Keys use the same names consumed by the UI: `item/<stem>` or `block/<stem>`.
pub struct ItemAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    sprites: HashMap<String, [f32; 4]>,
    missing_sprites: Mutex<Vec<String>>,
}

impl ItemAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, reader: &AssetReader) -> Self {
        let mut entries = Vec::new();
        entries.push(("_missing".to_string(), Self::missing_sprite()));
        Self::load_directory(reader, "textures/item", "item/", &mut entries);
        Self::load_directory(reader, "textures/block", "block/", &mut entries);

        let capacity = (ITEM_ATLAS_TILES_PER_ROW * ITEM_ATLAS_TILES_PER_ROW) as usize;
        if entries.len() > capacity {
            log::error!(
                "item atlas has {} sprites but capacity is {}; excess sprites will be unavailable",
                entries.len(),
                capacity
            );
            entries.truncate(capacity);
        }

        let mut pixels = vec![0; (ITEM_ATLAS_SIZE * ITEM_ATLAS_SIZE * 4) as usize];
        let mut sprites = HashMap::with_capacity(entries.len());
        for (index, (key, image)) in entries.into_iter().enumerate() {
            let tile_x = index as u32 % ITEM_ATLAS_TILES_PER_ROW;
            let tile_y = index as u32 / ITEM_ATLAS_TILES_PER_ROW;
            let image = Self::normalize(image);
            for py in 0..ITEM_ATLAS_TILE_SIZE {
                let src = (py * ITEM_ATLAS_TILE_SIZE * 4) as usize;
                let dst = (((tile_y * ITEM_ATLAS_TILE_SIZE + py) * ITEM_ATLAS_SIZE
                    + tile_x * ITEM_ATLAS_TILE_SIZE)
                    * 4) as usize;
                pixels[dst..dst + (ITEM_ATLAS_TILE_SIZE * 4) as usize]
                    .copy_from_slice(&image.as_raw()[src..src + (ITEM_ATLAS_TILE_SIZE * 4) as usize]);
            }
            let scale = ITEM_ATLAS_SIZE as f32;
            // Sample texel centers to prevent an icon's edge from picking up
            // the adjacent sprite in the tightly packed nearest-filtered atlas.
            let inset = 0.5 / scale;
            sprites.insert(
                key,
                [
                    (tile_x * ITEM_ATLAS_TILE_SIZE) as f32 / scale + inset,
                    (tile_y * ITEM_ATLAS_TILE_SIZE) as f32 / scale + inset,
                    ((tile_x + 1) * ITEM_ATLAS_TILE_SIZE) as f32 / scale - inset,
                    ((tile_y + 1) * ITEM_ATLAS_TILE_SIZE) as f32 / scale - inset,
                ],
            );
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("item_atlas"),
            size: wgpu::Extent3d {
                width: ITEM_ATLAS_SIZE,
                height: ITEM_ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
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
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ITEM_ATLAS_SIZE * 4),
                rows_per_image: Some(ITEM_ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ITEM_ATLAS_SIZE,
                height: ITEM_ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("item_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            sprites,
            missing_sprites: Mutex::new(Vec::new()),
        }
    }

    pub fn sprite_uv(&self, name: &str) -> Option<[f32; 4]> {
        if let Some(uv) = self.sprites.get(name) {
            return Some(*uv);
        }
        if let Ok(mut missing) = self.missing_sprites.lock() {
            if !missing.iter().any(|sprite| sprite == name) {
                log::warn!("item atlas has no sprite {name}; using diagnostic fallback");
                missing.push(name.to_string());
            }
        }
        self.sprites.get("_missing").copied()
    }

    pub fn build_sprite(
        &self,
        name: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) -> Option<(Vec<TextVertex>, Vec<u32>)> {
        let [u0, v0, u1, v1] = self.sprite_uv(name)?;
        Some((
            vec![
                TextVertex { pos: [x, y], uv: [u0, v0], color },
                TextVertex { pos: [x + w, y], uv: [u1, v0], color },
                TextVertex { pos: [x + w, y + h], uv: [u1, v1], color },
                TextVertex { pos: [x, y + h], uv: [u0, v1], color },
            ],
            vec![0, 1, 2, 0, 2, 3],
        ))
    }

    fn load_directory(
        reader: &AssetReader,
        directory: &str,
        prefix: &str,
        entries: &mut Vec<(String, image::RgbaImage)>,
    ) {
        if !reader.exists(directory) {
            log::warn!("missing item atlas source directory {directory}");
            return;
        }
        for name in reader.read_dir(directory) {
            let Some(stem) = name.strip_suffix(".png") else { continue; };
            let path = format!("{directory}/{name}");
            match reader.read_image(&path) {
                Some(image) => entries.push((format!("{prefix}{stem}"), image.into_rgba8())),
                None => log::warn!("failed to decode item atlas sprite {path}"),
            }
        }
    }

    fn normalize(image: image::RgbaImage) -> image::RgbaImage {
        let (width, height) = image.dimensions();
        if width == ITEM_ATLAS_TILE_SIZE && height == ITEM_ATLAS_TILE_SIZE {
            image
        } else if width == ITEM_ATLAS_TILE_SIZE && height >= ITEM_ATLAS_TILE_SIZE {
            image::imageops::crop_imm(&image, 0, 0, ITEM_ATLAS_TILE_SIZE, ITEM_ATLAS_TILE_SIZE)
                .to_image()
        } else {
            image::imageops::resize(
                &image,
                ITEM_ATLAS_TILE_SIZE,
                ITEM_ATLAS_TILE_SIZE,
                image::imageops::FilterType::Nearest,
            )
        }
    }

    fn missing_sprite() -> image::RgbaImage {
        image::RgbaImage::from_fn(ITEM_ATLAS_TILE_SIZE, ITEM_ATLAS_TILE_SIZE, |x, y| {
            if (x / 4 + y / 4) % 2 == 0 {
                image::Rgba([255, 0, 255, 255])
            } else {
                image::Rgba([20, 0, 20, 255])
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::item::ItemRegistry;

    #[test]
    fn registered_non_block_items_resolve_to_real_assets() {
        let root = std::path::Path::new("/tmp/opencode/minecraft-assets/assets/minecraft");
        if !root.is_dir() {
            return;
        }
        let reader = AssetReader::new(root.to_path_buf());
        let registry = ItemRegistry::new();
        for id in 1..registry.items.len() as u16 {
            if !registry.is_valid(id) {
                continue;
            }
            // Reserved registry padding has no gameplay item or asset.
            if registry.name(id) == "Block" {
                continue;
            }
            if registry.block_from_item(id).is_some() {
                continue;
            }
            let Some(texture) = registry.texture_stem(id) else {
                continue;
            };
            assert!(
                reader.exists(&format!("textures/item/{texture}.png")),
                "item {id:?} ({}) maps to missing texture {texture}",
                registry.name(id)
            );
        }
    }
}
