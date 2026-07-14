use crate::assets::reader::AssetReader;
use wgpu::*;

pub struct FontTexture {
    #[allow(dead_code)]
    pub texture: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
    /// Total texture width (read from image at load time)
    pub font_width: f32,
    /// Total texture height (read from image at load time)
    pub font_height: f32,
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

    /// Build a solid-colored rectangle using a white reference pixel in the font texture
    /// Uses the bottom-right corner pixel as white reference.
    /// UV coordinate of the bottom-right texel, expected to be pure white in both
    /// the fallback font and ascii.png. Used as a white reference for tinted quads.
    fn white_ref_uv(&self) -> [f32; 2] {
        let u = (self.font_width - 0.5) / self.font_width;
        let v = (self.font_height - 0.5) / self.font_height;
        [u, v]
    }

    /// Build a solid-colored rectangle. Uses the bottom-right pixel of the font
    /// texture as a white reference (both the fallback font and ascii.png have
    /// one). For true solid fills, the reference coordinates are baked at
    /// (width-0.5, height-0.5) in texel space.
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

    /// Build text vertex data for a string at screen position (x, y)
    /// Character size (char_w × char_h) in screen pixels.
    /// Uses Minecraft font layout: ascii.png is 128×128 with 16×16 glyph grid,
    /// each glyph cell = 8×8. ASCII 32-127 occupies rows 2-7.
    pub fn build_text(
        &self,
        text: &str,
        x: f32,
        y: f32,
        char_w: f32,
        char_h: f32,
    ) -> (Vec<TextVertex>, Vec<u32>) {
        let fw = self.font_width;
        let fh = self.font_height;

        let mut verts = Vec::with_capacity(text.len() * 4);
        let mut indices = Vec::with_capacity(text.len() * 6);

        let mut cursor_x = x;
        for ch in text.chars() {
            let code = ch as u32;
            if code < 32 || code > 127 {
                continue;
            }
            let char_idx = code - 32;
            let tc = (char_idx % 16) as f32;
            // Minecraft font: ASCII chars start at glyph row 2 (0-indexed)
            let tr = (2 + char_idx / 16) as f32;

            let u0 = (tc * 8.0) / fw;
            let v0 = (tr * 8.0) / fh;
            let u1 = ((tc + 1.0) * 8.0) / fw;
            let v1 = ((tr + 1.0) * 8.0) / fh;

            let base = verts.len() as u32;
            let white = [1.0, 1.0, 1.0, 1.0];
            verts.push(TextVertex {
                pos: [cursor_x, y],
                uv: [u0, v0],
                color: white,
            });
            verts.push(TextVertex {
                pos: [cursor_x + char_w, y],
                uv: [u1, v0],
                color: white,
            });
            verts.push(TextVertex {
                pos: [cursor_x + char_w, y + char_h],
                uv: [u1, v1],
                color: white,
            });
            verts.push(TextVertex {
                pos: [cursor_x, y + char_h],
                uv: [u0, v1],
                color: white,
            });
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

            cursor_x += char_w;
        }

        (verts, indices)
    }
}
