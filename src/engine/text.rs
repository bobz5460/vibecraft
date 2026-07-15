use crate::assets::reader::AssetReader;
use wgpu::*;

pub(crate) const ASCII_FIRST: u32 = 32;
pub(crate) const ASCII_LAST: u32 = 126;
pub(crate) const ASCII_GLYPH_COUNT: usize = (ASCII_LAST - ASCII_FIRST + 1) as usize;
pub(crate) const GLYPH_SIZE: f32 = 8.0;


pub struct FontTexture {
    #[allow(dead_code)]
    pub texture: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
    /// Total texture width (read from image at load time)
    pub font_width: f32,
    /// Total texture height (read from image at load time)
    pub font_height: f32,
    glyph_advances: [f32; ASCII_GLYPH_COUNT],
    white_ref_uv: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl TextVertex {
    const ATTRIBS: [VertexAttribute; 3] = vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];

    pub fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

impl FontTexture {
    pub fn new(device: &Device, queue: &Queue, reader: &AssetReader) -> Self {
        let img = if reader.exists("textures/font/ascii.png") {
            reader.read_image("textures/font/ascii.png")
                .map(|i| i.into_rgba8())
                .unwrap_or_else(|| {
                    log::warn!("Failed to load font PNG, using fallback");
                    Self::fallback_font()
                })
        } else {
            log::warn!("Texture textures/font/ascii.png not found, using fallback");
            Self::fallback_font()
        };

        let glyph_advances = Self::glyph_advances(&img);
        let white_ref_uv = Self::opaque_ref_uv(&img);
        let (font_width, font_height) = (img.width(), img.height());
        let pixels = img.into_raw();

        let texture_size = Extent3d {
            width: font_width,
            height: font_height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("font"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(font_width * 4),
                rows_per_image: Some(font_height),
            },
            texture_size,
        );

        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("font_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        FontTexture {
            texture,
            view,
            sampler,
            font_width: font_width as f32,
            font_height: font_height as f32,
            glyph_advances,
            white_ref_uv,
        }
    }

    fn fallback_font() -> image::RgbaImage {
        let mut img = image::RgbaImage::new(128, 128);
        img.put_pixel(127, 127, image::Rgba([255, 255, 255, 255]));
        img
    }

    /// Build an atlas-textured icon quad at screen position (x,y) with size (s,s)
    /// uv_rect = (u0, v0, u1, v1) in atlas UV space
    pub fn build_icon_quad(
        x: f32,
        y: f32,
        s: f32,
        uv_rect: &[f32; 4],
    ) -> (Vec<TextVertex>, Vec<u32>) {
        let [u0, v0, u1, v1] = *uv_rect;
        let verts = vec![
            TextVertex {
                pos: [x, y],
                uv: [u0, v0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            TextVertex {
                pos: [x + s, y],
                uv: [u1, v0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            TextVertex {
                pos: [x + s, y + s],
                uv: [u1, v1],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            TextVertex {
                pos: [x, y + s],
                uv: [u0, v1],
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        (verts, indices)
    }

    fn white_ref_uv(&self) -> [f32; 2] {
        self.white_ref_uv
    }

    /// Build a solid-colored rectangle using an opaque pixel from the loaded font.
    pub fn build_colored_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) -> (Vec<TextVertex>, Vec<u32>) {
        let [u_white, v_white] = self.white_ref_uv();
        let verts = vec![
            TextVertex {
                pos: [x, y],
                uv: [u_white, v_white],
                color,
            },
            TextVertex {
                pos: [x + w, y],
                uv: [u_white, v_white],
                color,
            },
            TextVertex {
                pos: [x + w, y + h],
                uv: [u_white, v_white],
                color,
            },
            TextVertex {
                pos: [x, y + h],
                uv: [u_white, v_white],
                color,
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        (verts, indices)
    }

    /// Build a bordered slot rectangle (darker bg + lighter border)
    pub fn build_slot_rect(
        &self,
        x: f32,
        y: f32,
        s: f32,
        highlighted: bool,
    ) -> Vec<(Vec<TextVertex>, Vec<u32>)> {
        let bg_color = if highlighted {
            [0.3, 0.3, 0.4, 0.8]
        } else {
            [0.1, 0.1, 0.15, 0.8]
        };
        let border_color = if highlighted {
            [1.0, 1.0, 1.0, 0.9]
        } else {
            [0.4, 0.4, 0.4, 0.7]
        };
        let b = 1.0; // border width
        let mut parts = Vec::with_capacity(5);
        // Background fill
        parts.push(self.build_colored_rect(x + b, y + b, s - 2.0 * b, s - 2.0 * b, bg_color));
        // Top border
        parts.push(self.build_colored_rect(x, y, s, b, border_color));
        // Bottom border
        parts.push(self.build_colored_rect(x, y + s - b, s, b, border_color));
        // Left border
        parts.push(self.build_colored_rect(x, y, b, s, border_color));
        // Right border
        parts.push(self.build_colored_rect(x + s - b, y, b, s, border_color));
        parts
    }

    /// Build a dark semi-transparent background quad using the white reference pixel
    pub fn build_text_bg(&self, x: f32, y: f32, w: f32, h: f32) -> (Vec<TextVertex>, Vec<u32>) {
        let bg = [0.0, 0.0, 0.0, 0.5];
        let [u_ref, v_ref] = self.white_ref_uv();
        let verts = vec![
            TextVertex {
                pos: [x, y],
                uv: [u_ref, v_ref],
                color: bg,
            },
            TextVertex {
                pos: [x + w, y],
                uv: [u_ref, v_ref],
                color: bg,
            },
            TextVertex {
                pos: [x + w, y + h],
                uv: [u_ref, v_ref],
                color: bg,
            },
            TextVertex {
                pos: [x, y + h],
                uv: [u_ref, v_ref],
                color: bg,
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        (verts, indices)
    }

    /// Measure text in screen pixels for a square 8px Minecraft glyph cell scaled to `size`.
    pub fn measure_text(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .filter_map(|ch| self.glyph_advance(ch))
            .map(|advance| advance * size / GLYPH_SIZE)
            .sum()
    }

    /// Build text vertex data for a string at screen position (x, y).
    /// The Mojangles bitmap glyph cells remain square; their advances are read from
    /// the official ASCII texture so narrow glyphs do not stretch or mis-center text.
    pub fn build_text(
        &self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
    ) -> (Vec<TextVertex>, Vec<u32>) {
        let fw = self.font_width;
        let fh = self.font_height;

        let mut verts = Vec::with_capacity(text.len() * 4);
        let mut indices = Vec::with_capacity(text.len() * 6);

        let mut cursor_x = x;
        for ch in text.chars() {
            let code = ch as u32;
            if !(ASCII_FIRST..=ASCII_LAST).contains(&code) {
                continue;
            }
            let Some([u0, v0, u1, v1]) = Self::glyph_uv(code, fw, fh) else {
                continue;
            };

            let base = verts.len() as u32;
            let white = [1.0, 1.0, 1.0, 1.0];
            verts.push(TextVertex {
                pos: [cursor_x, y],
                uv: [u0, v0],
                color: white,
            });
            verts.push(TextVertex {
                pos: [cursor_x + size, y],
                uv: [u1, v0],
                color: white,
            });
            verts.push(TextVertex {
                pos: [cursor_x + size, y + size],
                uv: [u1, v1],
                color: white,
            });
            verts.push(TextVertex {
                pos: [cursor_x, y + size],
                uv: [u0, v1],
                color: white,
            });
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

            cursor_x += self.glyph_advance(ch).unwrap_or_default() * size / GLYPH_SIZE;
        }

        (verts, indices)
    }

    pub fn build_text_with_shadow(
        &self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
    ) -> (Vec<TextVertex>, Vec<u32>) {
        // Keep the shadow directly adjacent to the bitmap glyph. Scaling this
        // offset with larger labels left a visible gap between the glyph and
        // its shadow.
        let shadow_offset = 1.0;
        let shadow_color = Self::shadow_color(color);
        let (shadow_verts, shadow_idx) = self.build_text(text, x + shadow_offset, y + shadow_offset, size);
        let shadow_verts: Vec<TextVertex> = shadow_verts
            .into_iter()
            .map(|mut v| { v.color = shadow_color; v })
            .collect();
        let base = shadow_verts.len() as u32;
        let (fg_verts, fg_idx) = self.build_text(text, x, y, size);
        let fg_verts: Vec<TextVertex> = fg_verts
            .into_iter()
            .map(|mut v| { v.color = color; v })
            .collect();
        let mut verts = shadow_verts;
        verts.extend(fg_verts);
        let mut indices = shadow_idx;
        indices.extend(fg_idx.into_iter().map(|i| i + base));
        (verts, indices)
    }

    pub fn build_text_centered_with_shadow(
        &self,
        text: &str,
        center_x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
    ) -> (Vec<TextVertex>, Vec<u32>) {
        self.build_text_with_shadow(
            text,
            center_x - self.measure_text(text, size) * 0.5,
            y,
            size,
            color,
        )
    }

    pub fn build_text_centered(
        &self,
        text: &str,
        center_x: f32,
        y: f32,
        size: f32,
    ) -> (Vec<TextVertex>, Vec<u32>) {
        self.build_text(text, center_x - self.measure_text(text, size) * 0.5, y, size)
    }

    pub fn glyph_advances_slice(&self) -> &[f32; ASCII_GLYPH_COUNT] {
        &self.glyph_advances
    }

    fn glyph_advance(&self, ch: char) -> Option<f32> {
        let code = ch as u32;
        (ASCII_FIRST..=ASCII_LAST)
            .contains(&code)
            .then(|| self.glyph_advances[(code - ASCII_FIRST) as usize])
    }

    fn shadow_color(_color: [f32; 4]) -> [f32; 4] {
        [0.05, 0.05, 0.05, 1.0]
    }

    fn glyph_advances(img: &image::RgbaImage) -> [f32; ASCII_GLYPH_COUNT] {
        let mut advances = [GLYPH_SIZE; ASCII_GLYPH_COUNT];
        advances[0] = 4.0; // The default font's space provider is four pixels wide.
        if img.width() < 128 || img.height() < 128 {
            return advances;
        }
        for (index, advance) in advances.iter_mut().enumerate().skip(1) {
            let column = (index % 16) as u32;
            let row = 2 + (index / 16) as u32;
            let mut rightmost = None;
            for y in 0..8 {
                for x in 0..8 {
                    if img.get_pixel(column * 8 + x, row * 8 + y)[3] != 0 {
                        rightmost = Some(rightmost.map_or(x, |current: u32| current.max(x)));
                    }
                }
            }
            if let Some(x) = rightmost {
                *advance = x as f32 + 2.0;
            }
        }
        advances
    }

    fn glyph_uv(code: u32, font_width: f32, font_height: f32) -> Option<[f32; 4]> {
        if !(ASCII_FIRST..=ASCII_LAST).contains(&code) || font_width <= 0.0 || font_height <= 0.0 {
            return None;
        }
        let index = code - ASCII_FIRST;
        let column = (index % 16) as f32;
        // The bitmap provider places U+0020..U+007E in atlas rows 2 through 7.
        let row = (2 + index / 16) as f32;
        Some([
            column * GLYPH_SIZE / font_width,
            row * GLYPH_SIZE / font_height,
            (column + 1.0) * GLYPH_SIZE / font_width,
            (row + 1.0) * GLYPH_SIZE / font_height,
        ])
    }

    fn opaque_ref_uv(img: &image::RgbaImage) -> [f32; 2] {
        for y in 0..img.height() {
            for x in 0..img.width() {
                let pixel = img.get_pixel(x, y);
                if pixel[3] != 0 && pixel[0] != 0 && pixel[1] != 0 && pixel[2] != 0 {
                    return [(x as f32 + 0.5) / img.width() as f32, (y as f32 + 0.5) / img.height() as f32];
                }
            }
        }
        [0.5 / img.width().max(1) as f32, 0.5 / img.height().max(1) as f32]
    }
}

pub(crate) fn measure_text_width(text: &str, size: f32, glyph_advances: &[f32; ASCII_GLYPH_COUNT]) -> f32 {
    text.chars()
        .filter_map(|ch| {
            let code = ch as u32;
            (ASCII_FIRST..=ASCII_LAST).contains(&code).then(|| glyph_advances[(code - ASCII_FIRST) as usize])
        })
        .map(|advance| advance * size / GLYPH_SIZE)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_metrics_keep_narrow_glyphs_narrow() {
        let mut image = image::RgbaImage::new(128, 128);
        // A reaches x=4, while i reaches x=0 in their respective font cells.
        image.put_pixel(8 + 4, 4 * 8, image::Rgba([255, 255, 255, 255]));
        image.put_pixel(9 * 8, 6 * 8 + 2, image::Rgba([255, 255, 255, 255]));
        let advances = FontTexture::glyph_advances(&image);
        assert_eq!(advances[('A' as u32 - ASCII_FIRST) as usize], 6.0);
        assert_eq!(advances[('i' as u32 - ASCII_FIRST) as usize], 2.0);
        assert_eq!(advances[0], 4.0);
    }

    #[test]
    fn opaque_reference_avoids_transparent_font_padding() {
        let mut image = image::RgbaImage::new(128, 128);
        image.put_pixel(8, 16, image::Rgba([255, 255, 255, 255]));
        let uv = FontTexture::opaque_ref_uv(&image);
        assert_eq!(uv, [8.5 / 128.0, 16.5 / 128.0]);
    }

    #[test]
    fn ascii_uvs_match_the_official_bitmap_provider_layout() {
        assert_eq!(FontTexture::glyph_uv('A' as u32, 128.0, 128.0), Some([8.0 / 128.0, 32.0 / 128.0, 16.0 / 128.0, 40.0 / 128.0]));
    }

    #[test]
    fn shadow_color_is_fixed_dark_gray() {
        assert_eq!(FontTexture::shadow_color([0.8, 0.4, 0.2, 0.5]), [0.05, 0.05, 0.05, 1.0]);
        assert_eq!(FontTexture::shadow_color([1.0, 1.0, 1.0, 1.0]), [0.05, 0.05, 0.05, 1.0]);
    }
}
