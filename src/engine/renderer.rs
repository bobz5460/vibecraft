use std::sync::Arc;
use crate::assets::reader::AssetReader;
use crate::assets::gui_atlas::GuiAtlas;
use wgpu::*;
use winit::window::Window;
use crate::assets::LoadedTextureManager;
use crate::engine::camera::Camera;
use crate::engine::text::{FontTexture, TextVertex};
use crate::world::mesh::{ChunkMesh, set_model_assets, set_texture_lookups};
use crate::ui::{UiCommand, UiFrame};

#[derive(Debug, thiserror::Error)]
pub enum RendererInitError {
    #[error("failed to create rendering surface: {0}")]
    Surface(#[from] CreateSurfaceError),
    #[error("no compatible GPU adapter was found")]
    AdapterUnavailable,
    #[error("failed to initialize GPU device: {0}")]
    Device(#[from] RequestDeviceError),
    #[error("the selected GPU exposes no usable surface formats")]
    NoSurfaceFormat,
    #[error("failed to load destroy-stage texture {path}: {source}")]
    DestroyTexture {
        path: String,
        #[source]
        source: image::ImageError,
    },
    #[error("destroy-stage texture {path} is {width}x{height}, expected 16x16")]
    InvalidDestroyTexture {
        path: String,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiBatchKind {
    Sprite,
    Text,
}

struct UiBatch {
    kind: UiBatchKind,
    vertices: Vec<TextVertex>,
    indices: Vec<u32>,
}

struct UiDrawBatch {
    kind: UiBatchKind,
    index_start: u32,
    index_end: u32,
}

fn append_ui_geometry(
    batches: &mut Vec<UiBatch>,
    kind: UiBatchKind,
    vertices: Vec<TextVertex>,
    indices: Vec<u32>,
) {
    if vertices.is_empty() {
        return;
    }
    if !matches!(batches.last(), Some(batch) if batch.kind == kind) {
        batches.push(UiBatch { kind, vertices: Vec::new(), indices: Vec::new() });
    }
    if let Some(batch) = batches.last_mut() {
        let base = batch.vertices.len() as u32;
        batch.vertices.extend(vertices);
        batch.indices.extend(indices.into_iter().map(|index| index + base));
    }
}

fn load_destroy_texture(
    device: &Device,
    queue: &Queue,
    reader: &AssetReader,
) -> Result<(Texture, TextureView, Sampler), RendererInitError> {
    const FRAME_SIZE: u32 = 16;
    const FRAME_COUNT: u32 = 10;

    let mut pixels = vec![0; (FRAME_SIZE * FRAME_SIZE * FRAME_COUNT * 4) as usize];
    for frame in 0..FRAME_COUNT {
        let rel_path = format!("textures/block/destroy_stage_{frame}.png");
        let data = reader.read(&rel_path)
            .ok_or_else(|| RendererInitError::DestroyTexture {
                path: rel_path.clone(),
                source: image::ImageError::Decoding(image::error::DecodingError::new(
                    image::error::ImageFormatHint::Unknown,
                    std::io::Error::new(std::io::ErrorKind::NotFound, "asset not found"),
                )),
            })?;
        let image = image::load_from_memory(&data).map_err(|source| RendererInitError::DestroyTexture {
            path: rel_path.clone(),
            source,
        })?.into_rgba8();
        if image.width() != FRAME_SIZE || image.height() != FRAME_SIZE {
            return Err(RendererInitError::InvalidDestroyTexture {
                path: rel_path,
                width: image.width(),
                height: image.height(),
            });
        }

        for row in 0..FRAME_SIZE {
            let source_start = (row * FRAME_SIZE * 4) as usize;
            let destination_start =
                (row * FRAME_SIZE * FRAME_COUNT * 4 + frame * FRAME_SIZE * 4) as usize;
            pixels[destination_start..destination_start + (FRAME_SIZE * 4) as usize]
                .copy_from_slice(&image.as_raw()[source_start..source_start + (FRAME_SIZE * 4) as usize]);
        }
    }

    let size = Extent3d {
        width: FRAME_SIZE * FRAME_COUNT,
        height: FRAME_SIZE,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("destroy_texture"),
        size,
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
            bytes_per_row: Some(size.width * 4),
            rows_per_image: Some(size.height),
        },
        size,
    );
    let view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("destroy_sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        ..Default::default()
    });

    Ok((texture, view, sampler))
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub tex_index: u32,
    pub light_data: u32,
}

impl Vertex {
    const ATTRIBS: [VertexAttribute; 5] = vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
        2 => Float32x3,
        3 => Uint32,
        4 => Uint32,
    ];

    pub fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub vp_matrix: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub light_direction: [f32; 4],
    pub night_factor: [f32; 4],
    pub shadow_vp_matrix: [[f32; 4]; 4],
    pub inv_vp_matrix: [[f32; 4]; 4],
    pub fog_params: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BreakUniforms {
    vp_matrix: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_direction: [f32; 4],
    break_progress: [f32; 4],
}

#[derive(Clone)]
pub struct ChunkRenderData {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub num_indices: u32,
    pub transparent_vertex_buffer: Buffer,
    pub transparent_index_buffer: Buffer,
    pub transparent_num_indices: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OutlineVertex {
    pub pos: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BreakVertex {
    pos: [f32; 3],
    uv: [f32; 2],
}

pub struct HighlightData {
    pub vertex_buffer: Buffer,
    pub num_indices: u32,
}

pub struct BreakOverlay {
    pub vertex_buffer: Buffer,
    pub num_vertices: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MoonVertex { pub pos: [f32; 3], pub uv: [f32; 2] }

pub struct HotbarItem {
    pub name: String,
    pub count: u16,
    pub selected: bool,
    pub is_empty: bool,
    pub tex_tile: u32,
}

pub struct InventorySlot {
    pub name: String,
    pub count: u16,
    pub tex_tile: u32,
    pub is_empty: bool,
}

pub struct NametagRender {
    pub screen_x: f32,
    pub screen_y: f32,
    pub text: String,
}

pub struct RenderContext<'a> {
    pub camera: &'a Camera,
    pub chunk_data: &'a [(i32, i32, ChunkRenderData)],
    pub all_chunk_data: &'a [(i32, i32, ChunkRenderData)],
    pub highlight: Option<&'a HighlightData>,
    pub break_overlay: Option<&'a BreakOverlay>,
    pub chunk_borders: Option<&'a (Buffer, u32)>,
    pub debug_overlay: Option<&'a [String]>,
    pub hotbar_text: &'a str,
    pub chat_lines: &'a [String],
    pub feedback_line: Option<&'a str>,
    pub night_factor: f32,
    pub fog_params: [f32; 4],
    pub shadow_vp: &'a [[f32; 4]; 4],
    pub light_dir: &'a nalgebra::Vector3<f32>,
    pub game_time: f32,
    pub vibrant: bool,
    pub hotbar_items: Option<&'a [HotbarItem]>,
    pub inventory_open: bool,
    pub inventory_items: Option<&'a [InventorySlot]>,
    pub cursor_pos: Option<(f32, f32)>,
    pub carried_item: Option<&'a InventorySlot>,
    pub health: f32,
    pub hunger: f32,
    pub break_progress: f32,
    pub ui_frame: Option<&'a UiFrame>,
    pub ui_captures_gameplay: bool,
    pub nametags: &'a [NametagRender],
}

pub struct Renderer {
    pub surface: Surface<'static>,
    pub device: Device,
    pub queue: Queue,
    pub config: SurfaceConfiguration,
    pub size: (u32, u32),
    #[allow(dead_code)]
    pub tex_manager: LoadedTextureManager,
    pub uniform_bind_group: BindGroup,
    pub uniform_buffer: Buffer,
    pub shadow_bind_group: BindGroup,
    pub pipeline: RenderPipeline,
    pub transparent_pipeline: RenderPipeline,
    pub shadow_pipeline: RenderPipeline,
    pub star_pipeline: RenderPipeline,
    pub sky_gradient_pipeline: RenderPipeline,
    pub moon_pipeline: RenderPipeline,
    pub highlight_pipeline: RenderPipeline,
    pub break_pipeline: RenderPipeline,
    pub highlight_bind_group: BindGroup,
    break_bind_group: BindGroup,
    break_uniform_buffer: Buffer,
    destroy_texture: Texture,
    pub font: FontTexture,
    pub text_pipeline: RenderPipeline,
    pub text_bind_group: BindGroup,
    pub text_uniform_buffer: Buffer,
    gui_atlas: GuiAtlas,
    gui_pipeline: RenderPipeline,
    gui_bind_group: BindGroup,
    gui_vb: Buffer,
    gui_ib: Buffer,
    gui_vb_cap: usize,
    gui_ib_cap: usize,
    star_vertex_buffer: Buffer,
    star_count: u32,
    moon_vertex_buffer: Buffer,
    moon_index_buffer: Buffer,
    depth_texture: Texture,
    depth_view: TextureView,
    shadow_texture: Texture,
    shadow_view: TextureView,
    shadow_sampler: Sampler,
    pub gui_dirty: bool,
    pub item_vb: Buffer,
    pub item_ib: Buffer,
    pub item_vb_cap: usize,
    pub item_ib_cap: usize,
    overlay_vb: Buffer,
    overlay_ib: Buffer,
    overlay_vb_cap: usize,
    overlay_ib_cap: usize,
    break_overlay_vb: Option<Buffer>,
    break_overlay_vb_cap: u64,
    cube_outline_vb: Option<Buffer>,
    cube_outline_vb_cap: u64,
    screenshot_path: Option<String>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, reader: &AssetReader) -> Result<Self, RendererInitError> {
        let size = window.inner_size();

        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.ok_or(RendererInitError::AdapterUnavailable)?;

        let (device, queue) = adapter.request_device(
            &DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                required_limits: Limits::default(),
                memory_hints: MemoryHints::Performance,
            },
            None,
        ).await?;

        let capabilities = surface.get_capabilities(&adapter);
        let surface_format = capabilities
            .formats
            .iter()
            .copied()
            .find(TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(RendererInitError::NoSurfaceFormat)?;
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::AutoVsync,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let tex_manager = LoadedTextureManager::new(&device, &queue, reader);
        let (destroy_texture, destroy_view, destroy_sampler) =
            load_destroy_texture(&device, &queue, reader)?;

        let face_map = tex_manager.face_map().clone();
        let crossed_map = tex_manager.crossed_map().clone();
        set_texture_lookups(face_map, crossed_map);
        set_model_assets(tex_manager.mesh_assets());

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("uniform_buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Shadow map
        let shadow_texture = device.create_texture(&TextureDescriptor {
            label: Some("shadow_map"),
            size: Extent3d { width: 2048, height: 2048, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[TextureFormat::Depth32Float],
        });
        let shadow_view = shadow_texture.create_view(&TextureViewDescriptor::default());
        let shadow_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("shadow_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            compare: Some(CompareFunction::LessEqual),
            ..Default::default()
        });

        let uniform_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("uniform_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        // The shadow pass needs the atlas to alpha-test cutout vegetation.
        let shadow_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("shadow_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shadow_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("shadow_bg"),
            layout: &shadow_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&tex_manager.atlas_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&tex_manager.atlas_sampler),
                },
            ],
        });

        let uniform_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("uniform_bg"),
            layout: &uniform_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&tex_manager.atlas_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&tex_manager.atlas_sampler),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(&shadow_view),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/chunk.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("opaque_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let shadow_pl_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("shadow_pl_layout"),
            bind_group_layouts: &[&shadow_bgl],
            push_constant_ranges: &[],
        });

        let shadow_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("shadow_pipeline"),
            layout: Some(&shadow_pl_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let transparent_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("transparent_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent::OVER,
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Sky shader for stars and moon
        let sky_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/sky.wgsl").into()),
        });

        let star_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("star_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &sky_shader,
                entry_point: Some("vs_star"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute { format: VertexFormat::Float32x3, offset: 0, shader_location: 0 }],
                }],
            },
            fragment: Some(FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_star"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::PointList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::Always,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let moon_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("moon_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &sky_shader,
                entry_point: Some("vs_moon"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<MoonVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute { format: VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                        VertexAttribute { format: VertexFormat::Float32x2, offset: 12, shader_location: 1 },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_moon"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::Always,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Sky gradient pipeline — full-screen pass for per-pixel sky colors
        let sky_gradient_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sky_gradient_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/sky_gradient.wgsl").into()),
        });

        let sky_gradient_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("sky_gradient_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &sky_gradient_shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &sky_gradient_shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::Always,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Generate star positions (random points on a dome 200 blocks up)
        use rand::Rng;
        let mut rng = rand::rng();
        let mut star_positions: Vec<[f32; 3]> = Vec::with_capacity(300);
        for _ in 0..300 {
            let theta = rng.random::<f32>() * std::f32::consts::TAU;
            let phi = rng.random::<f32>() * std::f32::consts::PI * 0.45;
            let r: f32 = 512.0;
            let x = r * phi.cos() * theta.cos();
            let y = r * phi.sin() + 64.0;
            let z = r * phi.cos() * theta.sin();
            star_positions.push([x, y, z]);
        }
        let star_vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("star_vb"),
            size: (star_positions.len() as u64) * std::mem::size_of::<[f32; 3]>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&star_vertex_buffer, 0, bytemuck::cast_slice(&star_positions));

        // Moon geometry: a 4×4 quad facing upward
        let moon_verts: [MoonVertex; 4] = [
            MoonVertex { pos: [-2.0, 128.0, -2.0], uv: [0.0, 0.0] },
            MoonVertex { pos: [ 2.0, 128.0, -2.0], uv: [1.0, 0.0] },
            MoonVertex { pos: [ 2.0, 128.0,  2.0], uv: [1.0, 1.0] },
            MoonVertex { pos: [-2.0, 128.0,  2.0], uv: [0.0, 1.0] },
        ];
        let moon_indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
        let moon_vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("moon_vb"),
            size: std::mem::size_of::<[MoonVertex; 4]>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&moon_vertex_buffer, 0, bytemuck::cast_slice(&moon_verts));
        let moon_index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("moon_ib"),
            size: std::mem::size_of::<[u32; 6]>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&moon_index_buffer, 0, bytemuck::cast_slice(&moon_indices));

        let highlight_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("highlight_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/highlight.wgsl").into()),
        });

        let break_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("break_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/break.wgsl").into()),
        });

        let highlight_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("highlight_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let highlight_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("highlight_pl"),
            bind_group_layouts: &[&highlight_bgl],
            push_constant_ranges: &[],
        });

        let highlight_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("highlight_pipeline"),
            layout: Some(&highlight_pipeline_layout),
            vertex: VertexState {
                module: &highlight_shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            fragment: Some(FragmentState {
                module: &highlight_shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::LessEqual,
                stencil: StencilState::default(),
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let break_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("break_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let break_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("break_pl"),
            bind_group_layouts: &[&break_bgl],
            push_constant_ranges: &[],
        });
        let break_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("break_pipeline"),
            layout: Some(&break_pipeline_layout),
            vertex: VertexState {
                module: &break_shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<BreakVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: std::mem::size_of::<[f32; 3]>() as u64,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                module: &break_shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::LessEqual,
                stencil: StencilState::default(),
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let highlight_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("highlight_bg"),
            layout: &highlight_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let break_uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("break_uniform_buffer"),
            size: std::mem::size_of::<BreakUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let break_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("break_bg"),
            layout: &break_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: break_uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&destroy_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&destroy_sampler),
                },
            ],
        });

        let font = FontTexture::new(&device, &queue, reader);

        const OVERLAY_VERT_CAP: usize = 32768;
        let overlay_vb = device.create_buffer(&BufferDescriptor {
            label: Some("ov_vb"),
            size: (OVERLAY_VERT_CAP as u64) * std::mem::size_of::<TextVertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let overlay_ib = device.create_buffer(&BufferDescriptor {
            label: Some("ov_ib"),
            size: (OVERLAY_VERT_CAP as u64) * std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        const ITEM_VERT_CAP: usize = 8192;
        let item_vb = device.create_buffer(&BufferDescriptor {
            label: Some("item_vb"),
            size: (ITEM_VERT_CAP as u64) * std::mem::size_of::<Vertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let item_ib = device.create_buffer(&BufferDescriptor {
            label: Some("item_ib"),
            size: (ITEM_VERT_CAP as u64) * std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let depth_texture = device.create_texture(&TextureDescriptor {
            label: Some("depth_texture"),
            size: Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[TextureFormat::Depth32Float],
        });
        let depth_view = depth_texture.create_view(&TextureViewDescriptor::default());

        let text_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/text.wgsl").into()),
        });

        let text_uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("text_ub"),
            size: std::mem::size_of::<[f32; 16]>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let text_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("text_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let text_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("text_pl"),
            bind_group_layouts: &[&text_bgl],
            push_constant_ranges: &[],
        });

        let text_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("text_pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: VertexState {
                module: &text_shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[TextVertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let text_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("text_bg"),
            layout: &text_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: text_uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&font.view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&font.sampler),
                },
            ],
        });

        let gui_atlas = GuiAtlas::new(&device, &queue, reader);
        let gui_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("gui_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/gui.wgsl").into()),
        });
        let gui_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("gui_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let gui_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("gui_pipeline_layout"),
            bind_group_layouts: &[&gui_bgl],
            push_constant_ranges: &[],
        });
        let gui_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("gui_pipeline"),
            layout: Some(&gui_pipeline_layout),
            vertex: VertexState {
                module: &gui_shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[TextVertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &gui_shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let gui_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("gui_bind_group"),
            layout: &gui_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: text_uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&gui_atlas.view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&gui_atlas.sampler),
                },
            ],
        });
        const GUI_VERT_CAP: usize = 8192;
        let gui_vb = device.create_buffer(&BufferDescriptor {
            label: Some("gui_vb"),
            size: (GUI_VERT_CAP * std::mem::size_of::<TextVertex>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let gui_ib = device.create_buffer(&BufferDescriptor {
            label: Some("gui_ib"),
            size: (GUI_VERT_CAP * std::mem::size_of::<u32>()) as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Renderer {
            surface,
            device,
            queue,
            config,
            size: (size.width, size.height),
            tex_manager,
            uniform_bind_group,
            uniform_buffer,
            shadow_bind_group,
            pipeline,
            transparent_pipeline,
            shadow_pipeline,
            star_pipeline,
            sky_gradient_pipeline,
            moon_pipeline,
            highlight_pipeline,
            break_pipeline,
            highlight_bind_group,
            break_bind_group,
            break_uniform_buffer,
            destroy_texture,
            star_vertex_buffer,
            star_count: 300,
            moon_vertex_buffer,
            moon_index_buffer,
            font,
            text_pipeline,
            text_bind_group,
            text_uniform_buffer,
            gui_atlas,
            gui_pipeline,
            gui_bind_group,
            gui_vb,
            gui_ib,
            gui_vb_cap: GUI_VERT_CAP,
            gui_ib_cap: GUI_VERT_CAP,
            depth_texture,
            depth_view,
            shadow_texture,
            shadow_view,
            shadow_sampler,
            gui_dirty: false,
            item_vb,
            item_ib,
            item_vb_cap: ITEM_VERT_CAP,
            item_ib_cap: ITEM_VERT_CAP,
            overlay_vb,
            overlay_ib,
            overlay_vb_cap: OVERLAY_VERT_CAP,
            overlay_ib_cap: OVERLAY_VERT_CAP,
            break_overlay_vb: None,
            break_overlay_vb_cap: 0,
            cube_outline_vb: None,
            cube_outline_vb_cap: 0,
            screenshot_path: None,
        })
    }

    pub fn request_screenshot(&mut self, path: &str) {
        self.screenshot_path = Some(path.to_string());
    }

    pub fn create_cube_outline(&mut self, x: f32, y: f32, z: f32) -> HighlightData {
        let corners = [
            [x, y, z], [x + 1.0, y, z], [x + 1.0, y, z + 1.0], [x, y, z + 1.0],
            [x, y + 1.0, z], [x + 1.0, y + 1.0, z], [x + 1.0, y + 1.0, z + 1.0], [x, y + 1.0, z + 1.0],
        ];
        let edges: [[usize; 2]; 12] = [
            [0, 1], [1, 2], [2, 3], [3, 0],
            [4, 5], [5, 6], [6, 7], [7, 4],
            [0, 4], [1, 5], [2, 6], [3, 7],
        ];

        let mut verts = Vec::with_capacity(24);
        for &[a, b] in &edges {
            verts.push(corners[a]);
            verts.push(corners[b]);
        }

        let vert_size = (verts.len() as u64) * std::mem::size_of::<[f32; 3]>() as u64;
        let vertex_buffer = match &self.cube_outline_vb {
            Some(buf) if buf.size() >= vert_size => buf.clone(),
            _ => {
                let buf = self.device.create_buffer(&BufferDescriptor {
                    label: Some("highlight_vb"),
                    size: vert_size.max(self.cube_outline_vb_cap).next_power_of_two(),
                    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.cube_outline_vb_cap = buf.size();
                self.cube_outline_vb = Some(buf.clone());
                buf
            }
        };
        self.queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts));

        HighlightData {
            vertex_buffer,
            num_indices: verts.len() as u32,
        }
    }

    pub fn create_break_overlay(&mut self, x: f32, y: f32, z: f32) -> BreakOverlay {
        let c = |dx: f32, dy: f32, dz: f32| [x + dx, y + dy, z + dz];
        let mut positions = Vec::with_capacity(36);
        positions.extend_from_slice(&[
            c(0., 0., 0.), c(1., 0., 0.), c(1., 0., 1.),
            c(0., 0., 0.), c(1., 0., 1.), c(0., 0., 1.),
        ]);
        positions.extend_from_slice(&[
            c(0., 1., 0.), c(1., 1., 1.), c(1., 1., 0.),
            c(0., 1., 0.), c(0., 1., 1.), c(1., 1., 1.),
        ]);
        positions.extend_from_slice(&[
            c(0., 0., 0.), c(0., 1., 0.), c(1., 0., 0.),
            c(1., 0., 0.), c(0., 1., 0.), c(1., 1., 0.),
        ]);
        positions.extend_from_slice(&[
            c(1., 0., 1.), c(1., 1., 1.), c(0., 0., 1.),
            c(0., 0., 1.), c(1., 1., 1.), c(0., 1., 1.),
        ]);
        positions.extend_from_slice(&[
            c(0., 0., 0.), c(0., 0., 1.), c(0., 1., 1.),
            c(0., 0., 0.), c(0., 1., 1.), c(0., 1., 0.),
        ]);
        positions.extend_from_slice(&[
            c(1., 0., 0.), c(1., 1., 0.), c(1., 0., 1.),
            c(1., 0., 1.), c(1., 1., 0.), c(1., 1., 1.),
        ]);

        let face_uvs = [
            [0.0, 0.0], [1.0, 0.0], [1.0, 1.0],
            [0.0, 0.0], [1.0, 1.0], [0.0, 1.0],
        ];
        let verts: Vec<_> = positions
            .into_iter()
            .enumerate()
            .map(|(index, pos)| BreakVertex {
                pos,
                uv: face_uvs[index % face_uvs.len()],
            })
            .collect();

        let vert_size = (verts.len() as u64) * std::mem::size_of::<BreakVertex>() as u64;
        let vertex_buffer = match &self.break_overlay_vb {
            Some(buf) if buf.size() >= vert_size => buf.clone(),
            _ => {
                let buf = self.device.create_buffer(&BufferDescriptor {
                    label: Some("break_vb"),
                    size: vert_size.max(self.break_overlay_vb_cap).next_power_of_two(),
                    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.break_overlay_vb_cap = buf.size();
                self.break_overlay_vb = Some(buf.clone());
                buf
            }
        };
        self.queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts));

        BreakOverlay {
            vertex_buffer,
            num_vertices: verts.len() as u32,
        }
    }

    pub fn resize(&mut self, new_size: (u32, u32)) {
        self.size = new_size;
        self.config.width = new_size.0.max(1);
        self.config.height = new_size.1.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = self.device.create_texture(&TextureDescriptor {
            label: Some("depth_texture"),
            size: Extent3d {
                width: new_size.0.max(1),
                height: new_size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[TextureFormat::Depth32Float],
        });
        self.depth_view = self.depth_texture.create_view(&TextureViewDescriptor::default());
    }

    pub fn update_uniforms(
        &mut self,
        camera: &Camera,
        night_factor: f32,
        shadow_vp: &[[f32; 4]; 4],
        light_dir: &nalgebra::Vector3<f32>,
        fog_params: [f32; 4],
    ) {
        let vp: [[f32; 4]; 4] = camera.vp_matrix().into();
        let vp_mat = nalgebra::Matrix4::from(vp);
        let inv_vp = vp_mat.try_inverse().unwrap_or(nalgebra::Matrix4::identity());
        let uniforms = Uniforms {
            vp_matrix: vp,
            camera_pos: [camera.position.x, camera.position.y, camera.position.z, 0.0],
            light_direction: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            night_factor: [night_factor, 0.0, 0.0, 0.0],
            shadow_vp_matrix: *shadow_vp,
            inv_vp_matrix: inv_vp.into(),
            fog_params,
        };
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );
    }

    pub fn create_chunk_data(&self, mesh: &ChunkMesh) -> ChunkRenderData {
        let vertex_size = std::mem::size_of::<Vertex>() as u64;
        let index_size = std::mem::size_of::<u32>() as u64;

        let vertices: Vec<Vertex> = mesh.vertices.iter().map(|v| Vertex {
            pos: v.pos,
            uv: v.uv,
            normal: v.normal,
            tex_index: v.tex_index,
            light_data: v.light_data,
        }).collect();

        let transparent_vertices: Vec<Vertex> = mesh.transparent_vertices.iter().map(|v| Vertex {
            pos: v.pos,
            uv: v.uv,
            normal: v.normal,
            tex_index: v.tex_index,
            light_data: v.light_data,
        }).collect();

        let vertex_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_vb"),
            size: ((vertices.len() as u64) * vertex_size).max(vertex_size),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            self.queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }

        let index_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_ib"),
            size: ((mesh.indices.len() as u64) * index_size).max(index_size),
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !mesh.indices.is_empty() {
            self.queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&mesh.indices));
        }

        let transparent_vertex_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_tvb"),
            size: ((transparent_vertices.len() as u64) * vertex_size).max(vertex_size),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !transparent_vertices.is_empty() {
            self.queue.write_buffer(&transparent_vertex_buffer, 0, bytemuck::cast_slice(&transparent_vertices));
        }

        let transparent_index_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_tib"),
            size: ((mesh.transparent_indices.len() as u64) * index_size).max(index_size),
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !mesh.transparent_indices.is_empty() {
            self.queue.write_buffer(&transparent_index_buffer, 0, bytemuck::cast_slice(&mesh.transparent_indices));
        }

        ChunkRenderData {
            vertex_buffer,
            index_buffer,
            num_indices: mesh.indices.len() as u32,
            transparent_vertex_buffer,
            transparent_index_buffer,
            transparent_num_indices: mesh.transparent_indices.len() as u32,
        }
    }

    fn ensure_gui_capacity(&mut self, vertex_count: usize, index_count: usize) {
        if vertex_count > self.overlay_vb_cap {
            self.overlay_vb_cap = vertex_count.next_power_of_two();
            self.overlay_vb = self.device.create_buffer(&BufferDescriptor {
                label: Some("ui_overlay_vb"),
                size: (self.overlay_vb_cap * std::mem::size_of::<TextVertex>()) as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if index_count > self.overlay_ib_cap {
            self.overlay_ib_cap = index_count.next_power_of_two();
            self.overlay_ib = self.device.create_buffer(&BufferDescriptor {
                label: Some("ui_overlay_ib"),
                size: (self.overlay_ib_cap * std::mem::size_of::<u32>()) as u64,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if vertex_count > self.gui_vb_cap {
            self.gui_vb_cap = vertex_count.next_power_of_two();
            self.gui_vb = self.device.create_buffer(&BufferDescriptor {
                label: Some("ui_gui_vb"),
                size: (self.gui_vb_cap * std::mem::size_of::<TextVertex>()) as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if index_count > self.gui_ib_cap {
            self.gui_ib_cap = index_count.next_power_of_two();
            self.gui_ib = self.device.create_buffer(&BufferDescriptor {
                label: Some("ui_gui_ib"),
                size: (self.gui_ib_cap * std::mem::size_of::<u32>()) as u64,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
    }

    fn render_ui(&mut self, encoder: &mut CommandEncoder, view: &TextureView, frame: &UiFrame) {
        let mut batches = Vec::new();
        for command in &frame.commands {
            match command {
                UiCommand::Rect { x, y, w, h, color } => {
                    let (quad, quad_indices) = self.font.build_colored_rect(*x, *y, *w, *h, *color);
                    append_ui_geometry(&mut batches, UiBatchKind::Text, quad, quad_indices);
                }
                UiCommand::Text { x, y, size, text, color } => {
                    let (mut text_vertices, text_indices) = self.font.build_text(text, *x, *y, *size);
                    for vertex in &mut text_vertices {
                        vertex.color = *color;
                    }
                    append_ui_geometry(&mut batches, UiBatchKind::Text, text_vertices, text_indices);
                }
                UiCommand::CenteredText { center_x, y, size, text, color } => {
                    let (mut text_vertices, text_indices) = self.font.build_text_centered(text, *center_x, *y, *size);
                    for vertex in &mut text_vertices {
                        vertex.color = *color;
                    }
                    append_ui_geometry(&mut batches, UiBatchKind::Text, text_vertices, text_indices);
                }
                UiCommand::Sprite { name, x, y, w, h, color } => {
                    if let Some((quad, quad_indices)) = self.gui_atlas.build_sprite(name, *x, *y, *w, *h, *color) {
                        append_ui_geometry(&mut batches, UiBatchKind::Sprite, quad, quad_indices);
                    }
                }
                UiCommand::SpriteProgress { name, x, y, w, h, progress, color } => {
                    if let Some((quad, quad_indices)) = self.gui_atlas.build_sprite_progress(name, *x, *y, *w, *h, *progress, *color) {
                        append_ui_geometry(&mut batches, UiBatchKind::Sprite, quad, quad_indices);
                    }
                }
                UiCommand::NineSlice { sprite, x, y, w, h, border, color } => {
                    if let Some((quad, quad_indices)) = self.gui_atlas.build_nine_slice(sprite, *x, *y, *w, *h, *border, *color) {
                        append_ui_geometry(&mut batches, UiBatchKind::Sprite, quad, quad_indices);
                    }
                }
                UiCommand::Item { x, y, size, name: _, sprite, count, hint } => {
                    if let Some((quad, quad_indices)) = self.gui_atlas.build_sprite(sprite, *x, *y, *size, *size, [1.0, 1.0, 1.0, 1.0]) {
                        append_ui_geometry(&mut batches, UiBatchKind::Sprite, quad, quad_indices);
                    } else {
                        let hue = (*hint % 7) as f32 / 7.0;
                        let color = [0.35 + hue * 0.45, 0.45 + (1.0 - hue) * 0.35, 0.55, 1.0];
                        let (quad, quad_indices) = self.font.build_colored_rect(*x, *y, *size, *size, color);
                        append_ui_geometry(&mut batches, UiBatchKind::Text, quad, quad_indices);
                    }
                    if *count > 1 {
                        let count_text = count.to_string();
                        let cw = *size * 0.28;
                        let count_w = self.font.measure_text(&count_text, cw);
                        let (mut count_vertices, count_indices) = self.font.build_text(&count_text, *x + *size - count_w - 1.0, *y + *size - cw - 1.0, cw);
                        for vertex in &mut count_vertices {
                            vertex.color = [1.0, 1.0, 1.0, 1.0];
                        }
                        append_ui_geometry(&mut batches, UiBatchKind::Text, count_vertices, count_indices);
                    }
                }
            }
        }
        if batches.is_empty() {
            return;
        }
        let mut text_vertices = Vec::new();
        let mut text_indices = Vec::new();
        let mut sprite_vertices = Vec::new();
        let mut sprite_indices = Vec::new();
        let mut draw_batches = Vec::with_capacity(batches.len());
        for batch in batches {
            let (vertices, indices) = match batch.kind {
                UiBatchKind::Sprite => (&mut sprite_vertices, &mut sprite_indices),
                UiBatchKind::Text => (&mut text_vertices, &mut text_indices),
            };
            let vertex_base = vertices.len() as u32;
            let index_start = indices.len() as u32;
            vertices.extend(batch.vertices);
            indices.extend(batch.indices.into_iter().map(|index| index + vertex_base));
            draw_batches.push(UiDrawBatch {
                kind: batch.kind,
                index_start,
                index_end: indices.len() as u32,
            });
        }
        self.ensure_gui_capacity(
            text_vertices.len().max(sprite_vertices.len()),
            text_indices.len().max(sprite_indices.len()),
        );
        let width = self.size.0.max(1) as f32;
        let height = self.size.1.max(1) as f32;
        let ortho: [[f32; 4]; 4] = [
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0, 1.0],
        ];
        self.queue.write_buffer(&self.text_uniform_buffer, 0, bytemuck::cast_slice(&[ortho]));
        if !sprite_vertices.is_empty() {
            self.queue.write_buffer(&self.gui_vb, 0, bytemuck::cast_slice(&sprite_vertices));
            self.queue.write_buffer(&self.gui_ib, 0, bytemuck::cast_slice(&sprite_indices));
        }
        if !text_vertices.is_empty() {
            self.queue.write_buffer(&self.overlay_vb, 0, bytemuck::cast_slice(&text_vertices));
            self.queue.write_buffer(&self.overlay_ib, 0, bytemuck::cast_slice(&text_indices));
        }
        for batch in draw_batches {
            match batch.kind {
                UiBatchKind::Sprite => {
                    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("ui_sprite_pass"),
                        color_attachments: &[Some(RenderPassColorAttachment { view, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.gui_pipeline);
                    pass.set_bind_group(0, &self.gui_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.gui_vb.slice(..));
                    pass.set_index_buffer(self.gui_ib.slice(..), IndexFormat::Uint32);
                    pass.draw_indexed(batch.index_start..batch.index_end, 0, 0..1);
                }
                UiBatchKind::Text => {
                    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("ui_text_pass"),
                        color_attachments: &[Some(RenderPassColorAttachment { view, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.text_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.overlay_vb.slice(..));
                    pass.set_index_buffer(self.overlay_ib.slice(..), IndexFormat::Uint32);
                    pass.draw_indexed(batch.index_start..batch.index_end, 0, 0..1);
                }
            }
        }
    }

    fn render_overlay(
        &self,
        encoder: &mut CommandEncoder,
        view: &TextureView,
        lines: &[String],
        screen_w: f32,
        screen_h: f32,
        start_y: f32,
    ) {
        if lines.is_empty() { return; }
        let cw = screen_w / 160.0;
        let ch = cw;
        let mut all_verts = Vec::new();
        let mut all_indices = Vec::new();

        let bg_w = screen_w * 0.5;
        let bg_h = lines.len() as f32 * (ch + 4.0) + 4.0;
        let bg = self.font.build_text_bg(0.0, start_y, bg_w, bg_h);
        all_verts.extend(bg.0);
        all_indices.extend(bg.1);

        for (i, line) in lines.iter().enumerate() {
            let (verts, indices) = self.font.build_text(line, 4.0, start_y + 4.0 + i as f32 * (ch + 4.0), cw);
            if !verts.is_empty() {
                let inds: Vec<u32> = indices.iter().map(|idx| idx + all_verts.len() as u32).collect();
                all_verts.extend(verts);
                all_indices.extend(inds);
            }
        }

        if all_verts.is_empty() { return; }

        let vb = self.device.create_buffer(&BufferDescriptor {
            label: Some("ov_vb"),
            size: (all_verts.len() as u64) * std::mem::size_of::<TextVertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vb, 0, bytemuck::cast_slice(&all_verts));

        let ib = self.device.create_buffer(&BufferDescriptor {
            label: Some("ov_ib"),
            size: (all_indices.len() as u64) * std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&ib, 0, bytemuck::cast_slice(&all_indices));

        let ortho: [[f32; 4]; 4] = [
            [2.0 / screen_w, 0.0, 0.0, 0.0],
            [0.0, -2.0 / screen_h, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0, 1.0],
        ];
        self.queue.write_buffer(&self.text_uniform_buffer, 0, bytemuck::cast_slice(&[ortho]));

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ov_rpass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&self.text_pipeline);
        rpass.set_bind_group(0, &self.text_bind_group, &[]);
        rpass.set_vertex_buffer(0, vb.slice(..));
        rpass.set_index_buffer(ib.slice(..), IndexFormat::Uint32);
        rpass.draw_indexed(0..all_indices.len() as u32, 0, 0..1);
    }

    fn render_crosshair(&mut self, encoder: &mut CommandEncoder, view: &TextureView) {
        let cx = self.size.0 as f32 * 0.5;
        let cy = self.size.1 as f32 * 0.5;
        let mut vertices = Vec::with_capacity(16);
        let mut indices = Vec::with_capacity(24);
        for (x, y, w, h, color) in [
            (cx - 6.0, cy - 1.5, 12.0, 3.0, [0.0, 0.0, 0.0, 0.75]),
            (cx - 1.5, cy - 6.0, 3.0, 12.0, [0.0, 0.0, 0.0, 0.75]),
            (cx - 5.0, cy - 0.5, 10.0, 1.0, [1.0, 1.0, 1.0, 1.0]),
            (cx - 0.5, cy - 5.0, 1.0, 10.0, [1.0, 1.0, 1.0, 1.0]),
        ] {
            let (quad, quad_indices) = self.font.build_colored_rect(x, y, w, h, color);
            let base = vertices.len() as u32;
            vertices.extend(quad);
            indices.extend(quad_indices.into_iter().map(|index| index + base));
        }
        self.queue.write_buffer(&self.overlay_vb, 0, bytemuck::cast_slice(&vertices));
        self.queue.write_buffer(&self.overlay_ib, 0, bytemuck::cast_slice(&indices));

        let width = self.size.0 as f32;
        let height = self.size.1 as f32;
        let ortho: [[f32; 4]; 4] = [
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0, 1.0],
        ];
        self.queue.write_buffer(&self.text_uniform_buffer, 0, bytemuck::cast_slice(&[ortho]));

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("crosshair_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations { load: LoadOp::Load, store: StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.text_pipeline);
        pass.set_bind_group(0, &self.text_bind_group, &[]);
        pass.set_vertex_buffer(0, self.overlay_vb.slice(..));
        pass.set_index_buffer(self.overlay_ib.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }

    /// Temporary first-person arm silhouette. It deliberately uses the shared
    /// UI pipeline so it is depth-independent and remains visible while the
    /// dedicated skinned hand/item model pass is developed.
    fn render_first_person_hand(&mut self, encoder: &mut CommandEncoder, view: &TextureView) {
        let width = self.size.0 as f32;
        let height = self.size.1 as f32;
        let arm_h = height * 0.24;
        let arm_w = arm_h / 3.0;
        let (vertices, indices) = self.font.build_colored_rect(
            width - arm_w - width * 0.04,
            height - arm_h + height * 0.03,
            arm_w,
            arm_h,
            [0.78, 0.55, 0.38, 1.0],
        );
        self.queue.write_buffer(&self.overlay_vb, 0, bytemuck::cast_slice(&vertices));
        self.queue.write_buffer(&self.overlay_ib, 0, bytemuck::cast_slice(&indices));
        let ortho = [[2.0 / width, 0.0, 0.0, 0.0], [0.0, -2.0 / height, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [-1.0, 1.0, 0.0, 1.0]];
        self.queue.write_buffer(&self.text_uniform_buffer, 0, bytemuck::cast_slice(&[ortho]));
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("first_person_hand"),
            color_attachments: &[Some(RenderPassColorAttachment { view, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.text_pipeline);
        pass.set_bind_group(0, &self.text_bind_group, &[]);
        pass.set_vertex_buffer(0, self.overlay_vb.slice(..));
        pass.set_index_buffer(self.overlay_ib.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }

    fn render_nametags(&self, encoder: &mut CommandEncoder, view: &TextureView, nametags: &[NametagRender]) {
        if nametags.is_empty() { return; }
        let mut all_verts = Vec::new();
        let mut all_indices = Vec::new();
        let cw = 8.0;
        let ch = cw;
        for nametag in nametags {
            let text_w = self.font.measure_text(&nametag.text, cw);
            let pad = 3.0;
            let bg_w = text_w + pad * 2.0;
            let bg_h = ch + pad * 1.5;
            let bg_x = nametag.screen_x - bg_w * 0.5;
            let bg_y = nametag.screen_y - bg_h;
            // Dark background pill (Minecraft-style)
            let (bg_verts, bg_inds) = self.font.build_colored_rect(bg_x, bg_y, bg_w, bg_h, [0.06, 0.06, 0.08, 0.65]);
            let base = all_verts.len() as u32;
            all_verts.extend(bg_verts);
            all_indices.extend(bg_inds.into_iter().map(|i| i + base));
            // Text shadow (1px offset, dark color)
            let (mut shadow_tv, shadow_ti) = self.font.build_text(&nametag.text, bg_x + pad + 1.0, bg_y + pad * 0.75 + 1.0, cw);
            for v in &mut shadow_tv { v.color = [0.05, 0.05, 0.05, 1.0]; }
            let base = all_verts.len() as u32;
            all_verts.extend(shadow_tv);
            all_indices.extend(shadow_ti.into_iter().map(|i| i + base));
            // White text
            let (mut tv, ti) = self.font.build_text(&nametag.text, bg_x + pad, bg_y + pad * 0.75, cw);
            for v in &mut tv { v.color = [1.0, 1.0, 1.0, 1.0]; }
            let base = all_verts.len() as u32;
            all_verts.extend(tv);
            all_indices.extend(ti.into_iter().map(|i| i + base));
        }
        let width = self.size.0 as f32;
        let height = self.size.1 as f32;
        let ortho: [[f32; 4]; 4] = [
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0, 1.0],
        ];
        self.queue.write_buffer(&self.text_uniform_buffer, 0, bytemuck::cast_slice(&[ortho]));
        let vb = self.device.create_buffer(&BufferDescriptor {
            label: Some("nametag_vb"),
            size: (all_verts.len() as u64) * std::mem::size_of::<TextVertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vb, 0, bytemuck::cast_slice(&all_verts));
        let ib = self.device.create_buffer(&BufferDescriptor {
            label: Some("nametag_ib"),
            size: (all_indices.len() as u64) * std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&ib, 0, bytemuck::cast_slice(&all_indices));
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("nametag_pass"),
            color_attachments: &[Some(RenderPassColorAttachment { view, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.text_pipeline);
        pass.set_bind_group(0, &self.text_bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..all_indices.len() as u32, 0, 0..1);
    }

    pub fn render(&mut self, ctx: &RenderContext) -> Result<(), SurfaceError> {
        self.update_uniforms(
            ctx.camera,
            ctx.night_factor,
            ctx.shadow_vp,
            ctx.light_dir,
            ctx.fog_params,
        );

        let break_uniforms = BreakUniforms {
            vp_matrix: ctx.camera.vp_matrix().into(),
            camera_pos: [ctx.camera.position.x, ctx.camera.position.y, ctx.camera.position.z, 0.0],
            light_direction: [ctx.light_dir.x, ctx.light_dir.y, ctx.light_dir.z, 0.0],
            break_progress: [ctx.break_progress, 0.0, 0.0, 0.0],
        };
        self.queue.write_buffer(
            &self.break_uniform_buffer,
            0,
            bytemuck::cast_slice(&[break_uniforms]),
        );

        // Update moon position: opposite the sun, centered on camera
        let moon_dir = -*ctx.light_dir;
        let moon_center = ctx.camera.position.coords + moon_dir * 300.0;
        let mh = 3.0;
        let right = ctx.camera.right() * mh;
        let up = right.cross(&ctx.camera.forward()).normalize() * mh;
        let moon_verts: [MoonVertex; 4] = [
            MoonVertex { pos: (moon_center - right + up).into(), uv: [0.0, 0.0] },
            MoonVertex { pos: (moon_center + right + up).into(), uv: [1.0, 0.0] },
            MoonVertex { pos: (moon_center + right - up).into(), uv: [1.0, 1.0] },
            MoonVertex { pos: (moon_center - right - up).into(), uv: [0.0, 1.0] },
        ];
        self.queue.write_buffer(&self.moon_vertex_buffer, 0, bytemuck::cast_slice(&moon_verts));

        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });

        // Shadow pass: render depth from sun's POV
        {
            let mut spass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("shadow_rpass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.shadow_bind_group, &[]);
            for &(_, _, ref data) in ctx.chunk_data {
                if data.num_indices > 0 {
                    spass.set_vertex_buffer(0, data.vertex_buffer.slice(..));
                    spass.set_index_buffer(data.index_buffer.slice(..), IndexFormat::Uint32);
                    spass.draw_indexed(0..data.num_indices, 0, 0..1);
                }
            }
        }

        // Main pass
        {
            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main_rpass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Sky gradient: full-screen per-pixel sky (always passes depth test)
            rpass.set_pipeline(&self.sky_gradient_pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
            rpass.draw(0..3, 0..1);

            // Render stars (always passes depth test, appears behind everything)
            rpass.set_pipeline(&self.star_pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
            rpass.set_vertex_buffer(0, self.star_vertex_buffer.slice(..));
            rpass.draw(0..self.star_count, 0..1);

            // Render moon
            rpass.set_pipeline(&self.moon_pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
            rpass.set_vertex_buffer(0, self.moon_vertex_buffer.slice(..));
            rpass.set_index_buffer(self.moon_index_buffer.slice(..), IndexFormat::Uint32);
            rpass.draw_indexed(0..6, 0, 0..1);

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for &(_, _, ref data) in ctx.chunk_data {
                if data.num_indices > 0 {
                    rpass.set_vertex_buffer(0, data.vertex_buffer.slice(..));
                    rpass.set_index_buffer(data.index_buffer.slice(..), IndexFormat::Uint32);
                    rpass.draw_indexed(0..data.num_indices, 0, 0..1);
                }
            }

            rpass.set_pipeline(&self.transparent_pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);

            let cam_pos = ctx.camera.position.coords;
            let mut sorted: Vec<usize> = (0..ctx.chunk_data.len()).collect();
            sorted.sort_by(|&a, &b| {
                let ca = &ctx.chunk_data[a];
                let cb = &ctx.chunk_data[b];
                let da = (ca.0 as f32 * 16.0 + 8.0 - cam_pos.x).powi(2)
                       + (-cam_pos.y).powi(2)
                       + (ca.1 as f32 * 16.0 + 8.0 - cam_pos.z).powi(2);
                let db = (cb.0 as f32 * 16.0 + 8.0 - cam_pos.x).powi(2)
                       + (-cam_pos.y).powi(2)
                       + (cb.1 as f32 * 16.0 + 8.0 - cam_pos.z).powi(2);
                db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
            });
            for &idx in &sorted {
                let data = &ctx.chunk_data[idx].2;
                if data.transparent_num_indices > 0 {
                    rpass.set_vertex_buffer(0, data.transparent_vertex_buffer.slice(..));
                    rpass.set_index_buffer(data.transparent_index_buffer.slice(..), IndexFormat::Uint32);
                    rpass.draw_indexed(0..data.transparent_num_indices, 0, 0..1);
                }
            }
            if let Some(hl) = ctx.highlight {
                rpass.set_pipeline(&self.highlight_pipeline);
                rpass.set_bind_group(0, &self.highlight_bind_group, &[]);
                rpass.set_vertex_buffer(0, hl.vertex_buffer.slice(..));
                rpass.draw(0..hl.num_indices, 0..1);
            }
            if let Some(bo) = ctx.break_overlay {
                rpass.set_pipeline(&self.break_pipeline);
                rpass.set_bind_group(0, &self.break_bind_group, &[]);
                rpass.set_vertex_buffer(0, bo.vertex_buffer.slice(..));
                rpass.draw(0..bo.num_vertices, 0..1);
            }
            if let Some((ref border_buf, border_count)) = ctx.chunk_borders {
                rpass.set_pipeline(&self.highlight_pipeline);
                rpass.set_bind_group(0, &self.highlight_bind_group, &[]);
                rpass.set_vertex_buffer(0, border_buf.slice(..));
                rpass.draw(0..*border_count, 0..1);
            }
        }

        let sw = self.size.0 as f32;
        let sh = self.size.1 as f32;
        if let Some(lines) = ctx.debug_overlay {
            self.render_overlay(&mut encoder, &view, lines, sw, sh, 0.0);
        }
        if ctx.ui_frame.is_none() && !ctx.hotbar_text.is_empty() {
            let bar = vec![ctx.hotbar_text.to_string()];
            self.render_overlay(&mut encoder, &view, &bar, sw, sh, sh - 30.0);
        }
        if let Some(frame) = ctx.ui_frame {
            self.render_ui(&mut encoder, &view, frame);
        }
        self.render_nametags(&mut encoder, &view, ctx.nametags);
        if !ctx.ui_captures_gameplay {
            self.render_first_person_hand(&mut encoder, &view);
            if ctx.ui_frame.is_none() {
                self.render_crosshair(&mut encoder, &view);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex() -> TextVertex {
        TextVertex { pos: [0.0, 0.0], uv: [0.0, 0.0], color: [1.0; 4] }
    }

    #[test]
    fn ui_batches_preserve_text_sprite_text_order() {
        let mut batches = Vec::new();
        append_ui_geometry(&mut batches, UiBatchKind::Text, vec![vertex()], vec![0]);
        append_ui_geometry(&mut batches, UiBatchKind::Sprite, vec![vertex()], vec![0]);
        append_ui_geometry(&mut batches, UiBatchKind::Text, vec![vertex()], vec![0]);

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].kind, UiBatchKind::Text);
        assert_eq!(batches[1].kind, UiBatchKind::Sprite);
        assert_eq!(batches[2].kind, UiBatchKind::Text);
    }
}
