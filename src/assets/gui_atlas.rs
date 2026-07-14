use std::collections::HashMap;
use crate::assets::reader::AssetReader;
use crate::engine::text::TextVertex;

fn sprite_id(name: &str) -> u64 {
    let mut h = 0x9e3779b97f4a7c15u64;
    for b in name.bytes() {
        h = h.wrapping_mul(0xbf58476d1ce4e5b9).wrapping_add(b as u64);
    }
    h
}

pub struct GuiAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub sprites: HashMap<u64, [f32; 4]>,
    atlas_size: u32,
}

impl GuiAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, reader: &AssetReader) -> Self {
        let mut entries: Vec<(String, image::RgbaImage)> = Vec::new();

        // HUD sprites
        if reader.exists("textures/gui/sprites/hud") {
            Self::load_pngs(reader, "textures/gui/sprites/hud", &mut entries, "hud/");
        }

        // Heart sprites (subdirectory)
        if reader.exists("textures/gui/sprites/hud/heart") {
            Self::load_pngs(reader, "textures/gui/sprites/hud/heart", &mut entries, "hud/heart/");
        }

        // Locator bar dots
        if reader.exists("textures/gui/sprites/hud/locator_bar_dot") {
            Self::load_pngs(reader, "textures/gui/sprites/hud/locator_bar_dot", &mut entries, "hud/locator_bar_dot/");
        }

        // Container GUIs (inventory, crafting table, chest)
        for name in reader.read_dir("textures/gui/container") {
            if name.ends_with(".png") {
                if let Some(img) = reader.read_image(&format!("textures/gui/container/{name}")) {
                    let fname = name.strip_suffix(".png").unwrap_or(&name).to_string();
                    entries.push((format!("container/{}", fname), img.into_rgba8()));
                }
            }
        }

        // Core screens must be packed before the broad item/block icon sets. The
        // latter can exceed the bounded atlas, while missing widgets make every
        // menu unusable.
        if reader.exists("textures/gui/title") {
            Self::load_pngs(reader, "textures/gui/title", &mut entries, "title/");
        }
        if reader.exists("textures/gui/sprites/widget") {
            Self::load_pngs(reader, "textures/gui/sprites/widget", &mut entries, "widget/");
        }
        if reader.exists("textures/gui/sprites/popup") {
            Self::load_pngs(reader, "textures/gui/sprites/popup", &mut entries, "popup/");
        }

        // Item icons are part of the official GUI presentation. Keep them in
        // the same nearest-filtered atlas as HUD and container sprites so the
        // UI never falls back to terrain-only placeholders for known items.
        if reader.exists("textures/item") {
            Self::load_pngs(reader, "textures/item", &mut entries, "item/");
        }

        // Block-backed inventory stacks use the authored block face texture,
        // matching the terrain and dropped-block presentation.
        if reader.exists("textures/block") {
            Self::load_pngs(reader, "textures/block", &mut entries, "block/");
        }

        // Mob effect icons
        if reader.exists("textures/mob_effect") {
            Self::load_pngs(reader, "textures/mob_effect", &mut entries, "mob_effect/");
        }

        // Misc textures (only specific ones)
        if reader.exists("textures/misc") {
            Self::load_single_png(reader, "textures/misc", &mut entries, "misc/", "vignette");
            Self::load_single_png(reader, "textures/misc", &mut entries, "misc/", "enchanted_glint_item");
        }

        // Environment textures (clouds, rain, snow)
        if reader.exists("textures/environment") {
            Self::load_single_png(reader, "textures/environment", &mut entries, "environment/", "clouds");
            Self::load_single_png(reader, "textures/environment", &mut entries, "environment/", "rain");
            Self::load_single_png(reader, "textures/environment", &mut entries, "environment/", "snow");
        }

        // Colormap textures (foliage, grass)
        if reader.exists("textures/colormap") {
            Self::load_single_png(reader, "textures/colormap", &mut entries, "colormap/", "foliage");
            Self::load_single_png(reader, "textures/colormap", &mut entries, "colormap/", "grass");
        }

        // Add white reference pixel for colored quads (synthetic 1x1 white sprite)
        entries.push(("_white".to_string(), image::RgbaImage::from_pixel(1, 1, image::Rgba([255u8, 255, 255, 255]))));

        // Pack into atlas
        let atlas_size = Self::find_atlas_size(&entries, device.limits().max_texture_dimension_2d);
        let (atlas_data, sprite_uvs) = Self::pack_atlas(&entries, atlas_size);
        for name in ["widget/button", "widget/button_highlighted", "widget/text_field"] {
            if !sprite_uvs.contains_key(&sprite_id(name)) {
                log::warn!("GUI atlas is missing required sprite {name}");
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui_atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
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
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_size * 4),
                rows_per_image: Some(atlas_size),
            },
            wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gui_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        GuiAtlas {
            texture,
            view,
            sampler,
            sprites: sprite_uvs,
            atlas_size,
        }
    }

    fn load_pngs(reader: &AssetReader, rel_dir: &str, entries: &mut Vec<(String, image::RgbaImage)>, prefix: &str) {
        for name in reader.read_dir(rel_dir) {
            if name.ends_with(".png") {
                let stem = name.strip_suffix(".png").unwrap_or(&name);
                if let Some(img) = reader.read_image(&format!("{rel_dir}/{name}")) {
                    entries.push((format!("{prefix}{stem}"), img.into_rgba8()));
                }
            }
        }
    }

    fn load_single_png(
        reader: &AssetReader,
        rel_dir: &str,
        entries: &mut Vec<(String, image::RgbaImage)>,
        prefix: &str,
        name: &str,
    ) {
        let rel_path = format!("{rel_dir}/{name}.png");
        if reader.exists(&rel_path) {
            if let Some(img) = reader.read_image(&rel_path) {
                entries.push((format!("{prefix}{name}"), img.into_rgba8()));
            }
        }
    }

    fn find_atlas_size(entries: &[(String, image::RgbaImage)], max_texture_size: u32) -> u32 {
        let total_area: u64 = entries
            .iter()
            .map(|(_, img)| img.width() as u64 * img.height() as u64)
            .sum();
        let mut size = 64;
        while (size as u64) * (size as u64) < total_area * 2 {
            size *= 2;
        }
        let max_texture_size = max_texture_size.max(256);
        if size > max_texture_size {
            log::warn!("GUI atlas requires {size}px but the GPU supports only {max_texture_size}px");
            max_texture_size
        } else {
            size.max(256)
        }
    }

    fn pack_atlas(
        entries: &[(String, image::RgbaImage)],
        atlas_size: u32,
    ) -> (Vec<u8>, HashMap<u64, [f32; 4]>) {
        let mut pixels = vec![0u8; (atlas_size * atlas_size * 4) as usize];
        let mut uvs = HashMap::new();

        let mut cursor_x = 0u32;
        let mut cursor_y = 0u32;
        let mut row_h = 0u32;

        for (name, img) in entries {
            let (w, h) = (img.width(), img.height());
            if cursor_x + w > atlas_size {
                cursor_x = 0;
                cursor_y += row_h;
                row_h = 0;
            }
            if cursor_y + h > atlas_size {
                log::warn!("GUI atlas overflow for sprite: {}", name);
                continue;
            }

            let data = img.as_raw();
            let row_bytes = (w * 4) as usize;
            for py in 0..h {
                let py = py as usize;
                let src_row_start = py * row_bytes;
                let dst_y = (cursor_y as usize) + py;
                let dst_row_start = (dst_y * atlas_size as usize + cursor_x as usize) * 4;
                pixels[dst_row_start..dst_row_start + row_bytes]
                    .copy_from_slice(&data[src_row_start..src_row_start + row_bytes]);
            }

            let af = atlas_size as f32;
            uvs.insert(
                sprite_id(name),
                [
                    cursor_x as f32 / af,
                    cursor_y as f32 / af,
                    (cursor_x + w) as f32 / af,
                    (cursor_y + h) as f32 / af,
                ],
            );

            cursor_x += w;
            row_h = row_h.max(h);
        }

        (pixels, uvs)
    }

    fn get_uv(&self, name: &str) -> Option<[f32; 4]> {
        self.sprites.get(&sprite_id(name)).copied()
    }

    pub fn sprite_size(&self, name: &str) -> (f32, f32) {
        self.sprites
            .get(&sprite_id(name))
            .map(|uv| {
                let af = self.atlas_size as f32;
                ((uv[2] - uv[0]) * af, (uv[3] - uv[1]) * af)
            })
            .unwrap_or((0.0, 0.0))
    }

    /// Build vertex data for a sprite quad at screen position (x,y) with given dimensions
    pub fn build_sprite(
        &self,
        name: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) -> Option<(Vec<crate::engine::text::TextVertex>, Vec<u32>)> {
        let uv = self.get_uv(name)?;
        let [u0, v0, u1, v1] = uv;
        let verts = vec![
            crate::engine::text::TextVertex {
                pos: [x, y],
                uv: [u0, v0],
                color,
            },
            crate::engine::text::TextVertex {
                pos: [x + w, y],
                uv: [u1, v0],
                color,
            },
            crate::engine::text::TextVertex {
                pos: [x + w, y + h],
                uv: [u1, v1],
                color,
            },
            crate::engine::text::TextVertex {
                pos: [x, y + h],
                uv: [u0, v1],
                color,
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        Some((verts, indices))
    }

    /// Builds the leftmost `progress` fraction of a horizontal sprite without
    /// stretching the full source image into the shortened destination width.
    pub fn build_sprite_progress(
        &self,
        name: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        progress: f32,
        color: [f32; 4],
    ) -> Option<(Vec<TextVertex>, Vec<u32>)> {
        let progress = progress.clamp(0.0, 1.0);
        if progress <= 0.0 {
            return None;
        }
        let [u0, v0, u1, v1] = self.get_uv(name)?;
        let right = u0 + (u1 - u0) * progress;
        let w = w * progress;
        let verts = vec![
            TextVertex { pos: [x, y], uv: [u0, v0], color },
            TextVertex { pos: [x + w, y], uv: [right, v0], color },
            TextVertex { pos: [x + w, y + h], uv: [right, v1], color },
            TextVertex { pos: [x, y + h], uv: [u0, v1], color },
        ];
        Some((verts, vec![0, 1, 2, 0, 2, 3]))
    }

    /// Build nine-slice geometry at screen position (x,y) with dimensions (w,h).
    /// `border` is the pixel border from the source sprite edges.
    /// Returns 9 quads (vertices + indices) representing the 9-slice layout.
    pub fn build_nine_slice(
        &self,
        name: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        border: f32,
        color: [f32; 4],
    ) -> Option<(Vec<TextVertex>, Vec<u32>)> {
        let uv = self.get_uv(name)?;
        let [u0, v0, u1, v1] = uv;

        // Source sprite pixel size from atlas UV
        let af = self.atlas_size as f32;
        let src_w = (u1 - u0) * af;
        let src_h = (v1 - v0) * af;

        // Scale border proportionally to the larger of the two scaling factors
        let scale_x = w / src_w;
        let scale_y = h / src_h;
        let bp = border * scale_x.max(scale_y);

        let (um, vm) = Self::nine_slice_uv_boundaries(uv, af, border)?;

        // Pixel boundaries
        let xm = [x, x + bp, x + w - bp, x + w];
        let ym = [y, y + bp, y + h - bp, y + h];

        let mut vertices = Vec::with_capacity(36);
        let mut indices = Vec::with_capacity(54);

        for row in 0..3 {
            for col in 0..3 {
                let u_left = um[col];
                let u_right = um[col + 1];
                let v_top = vm[row];
                let v_bot = vm[row + 1];
                let px = xm[col];
                let py = ym[row];
                let pw = xm[col + 1] - px;
                let ph = ym[row + 1] - py;

                let base = vertices.len() as u32;
                vertices.push(TextVertex { pos: [px, py], uv: [u_left, v_top], color });
                vertices.push(TextVertex { pos: [px + pw, py], uv: [u_right, v_top], color });
                vertices.push(TextVertex { pos: [px + pw, py + ph], uv: [u_right, v_bot], color });
                vertices.push(TextVertex { pos: [px, py + ph], uv: [u_left, v_bot], color });
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }

        Some((vertices, indices))
    }

    fn nine_slice_uv_boundaries(uv: [f32; 4], atlas_size: f32, border: f32) -> Option<([f32; 4], [f32; 4])> {
        if atlas_size <= 0.0 || border < 0.0 {
            return None;
        }
        let [u0, v0, u1, v1] = uv;
        let border_uv = border / atlas_size;
        if u1 - u0 < border_uv * 2.0 || v1 - v0 < border_uv * 2.0 {
            return None;
        }
        Some(([u0, u0 + border_uv, u1 - border_uv, u1], [v0, v0 + border_uv, v1 - border_uv, v1]))
    }

    /// Build vertex data using a white reference pixel for solid color quads
    pub fn build_colored_quad(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) -> (Vec<crate::engine::text::TextVertex>, Vec<u32>) {
        let af = self.atlas_size as f32;
        let u_ref = (af - 0.5) / af;
        let v_ref = (af - 0.5) / af;
        let verts = vec![
            crate::engine::text::TextVertex {
                pos: [x, y],
                uv: [u_ref, v_ref],
                color,
            },
            crate::engine::text::TextVertex {
                pos: [x + w, y],
                uv: [u_ref, v_ref],
                color,
            },
            crate::engine::text::TextVertex {
                pos: [x + w, y + h],
                uv: [u_ref, v_ref],
                color,
            },
            crate::engine::text::TextVertex {
                pos: [x, y + h],
                uv: [u_ref, v_ref],
                color,
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        (verts, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn nine_slice_border_uses_atlas_uv_space() {
        let atlas_size = 2048.0;
        let uv = [100.0 / atlas_size, 400.0 / atlas_size, 300.0 / atlas_size, 420.0 / atlas_size];
        let (u, v) = GuiAtlas::nine_slice_uv_boundaries(uv, atlas_size, 3.0).unwrap();
        assert_eq!((u[1] - u[0]) * atlas_size, 3.0);
        assert_eq!((v[1] - v[0]) * atlas_size, 3.0);
        assert!(u[0] < u[1] && u[1] < u[2] && u[2] < u[3]);
        assert!(v[0] < v[1] && v[1] < v[2] && v[2] < v[3]);
    }

    #[test]
    fn widget_button_png_registers_under_its_render_key() {
        let root = std::env::temp_dir().join(format!(
            "vibecraft-gui-atlas-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(),
        ));
        let widget_dir = root.join("textures/gui/sprites/widget");
        std::fs::create_dir_all(&widget_dir).unwrap();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))
            .save(widget_dir.join("button.png"))
            .unwrap();

        let reader = AssetReader::new(root.clone());
        let mut entries = Vec::new();
        GuiAtlas::load_pngs(&reader, "textures/gui/sprites/widget", &mut entries, "widget/");
        let (_, sprites) = GuiAtlas::pack_atlas(&entries, 256);
        assert!(sprites.contains_key(&sprite_id("widget/button")));

        std::fs::remove_dir_all(root).unwrap();
    }
}
