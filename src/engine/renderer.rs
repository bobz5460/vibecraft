use std::sync::Arc;
use wgpu::*;
use winit::window::Window;
use crate::assets::LoadedTextureManager;
use crate::engine::camera::Camera;
use crate::engine::text::{FontTexture, TextVertex};
use crate::world::mesh::{ChunkMesh, set_texture_lookups};

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

pub struct HighlightData {
    pub vertex_buffer: Buffer,
    pub num_indices: u32,
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
    pub pipeline: RenderPipeline,
    pub transparent_pipeline: RenderPipeline,
    pub shadow_pipeline: RenderPipeline,
    pub highlight_pipeline: RenderPipeline,
    pub highlight_bind_group: BindGroup,
    pub font: FontTexture,
    pub text_pipeline: RenderPipeline,
    pub text_bind_group: BindGroup,
    pub text_uniform_buffer: Buffer,
    depth_texture: Texture,
    depth_view: TextureView,
    shadow_texture: Texture,
    shadow_view: TextureView,
    shadow_sampler: Sampler,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, asset_path: &str) -> Self {
        let size = window.inner_size();

        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &DeviceDescriptor {
                label: None,
                required_features: Features::POLYGON_MODE_LINE,
                required_limits: Limits::default(),
                memory_hints: MemoryHints::Performance,
            },
            None,
        ).await.unwrap();

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_capabilities(&adapter).formats[0],
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::AutoVsync,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let tex_manager = LoadedTextureManager::new(&device, &queue, asset_path);

        let face_map = tex_manager.face_map().clone();
        let crossed_map = tex_manager.crossed_map().clone();
        set_texture_lookups(face_map, crossed_map);

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

        let shadow_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("shadow_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: None,
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
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let highlight_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("highlight_shader"),
            source: ShaderSource::Wgsl(include_str!("../shaders/highlight.wgsl").into()),
        });

        let highlight_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("highlight_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
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
                    blend: Some(BlendState::REPLACE),
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
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
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

        let font = FontTexture::new(&device, &queue);

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

        Renderer {
            surface,
            device,
            queue,
            config,
            size: (size.width, size.height),
            tex_manager,
            uniform_bind_group,
            uniform_buffer,
            pipeline,
            transparent_pipeline,
            shadow_pipeline,
            highlight_pipeline,
            highlight_bind_group,
            font,
            text_pipeline,
            text_bind_group,
            text_uniform_buffer,
            depth_texture,
            depth_view,
            shadow_texture,
            shadow_view,
            shadow_sampler,
        }
    }

    pub fn create_cube_outline(&self, x: f32, y: f32, z: f32) -> HighlightData {
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

        let vertex_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("highlight_vb"),
            size: (verts.len() as u64) * std::mem::size_of::<[f32; 3]>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts));

        HighlightData {
            vertex_buffer,
            num_indices: verts.len() as u32,
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

    pub fn update_uniforms(&mut self, camera: &Camera, night_factor: f32, shadow_vp: &[[f32; 4]; 4]) {
        let dir = [0.5f32, -0.85, 0.5, 0.0];
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let uniforms = Uniforms {
            vp_matrix: camera.vp_matrix().into(),
            camera_pos: [camera.position.x, camera.position.y, camera.position.z, 0.0],
            light_direction: [dir[0] / len, dir[1] / len, dir[2] / len, 0.0],
            night_factor: [night_factor, 0.0, 0.0, 0.0],
            shadow_vp_matrix: *shadow_vp,
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
            size: (vertices.len() as u64) * vertex_size,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            self.queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }

        let index_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_ib"),
            size: (mesh.indices.len() as u64) * index_size,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !mesh.indices.is_empty() {
            self.queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&mesh.indices));
        }

        let transparent_vertex_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_tvb"),
            size: (transparent_vertices.len() as u64) * vertex_size,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !transparent_vertices.is_empty() {
            self.queue.write_buffer(&transparent_vertex_buffer, 0, bytemuck::cast_slice(&transparent_vertices));
        }

        let transparent_index_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("chunk_tib"),
            size: (mesh.transparent_indices.len() as u64) * index_size,
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
        let ch = cw * 1.5;
        let mut all_verts = Vec::new();
        let mut all_indices = Vec::new();

        let bg_w = screen_w * 0.5;
        let bg_h = lines.len() as f32 * (ch + 4.0) + 4.0;
        let bg = self.font.build_text_bg(0.0, start_y, bg_w, bg_h);
        all_verts.extend(bg.0);
        all_indices.extend(bg.1);

        for (i, line) in lines.iter().enumerate() {
            let (verts, indices) = self.font.build_text(line, 4.0, start_y + 4.0 + i as f32 * (ch + 4.0), cw, ch);
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

    pub fn render(
        &mut self,
        camera: &Camera,
        chunk_data: &[(i32, i32, ChunkRenderData)],
        highlight: Option<&HighlightData>,
        chunk_borders: Option<&(Buffer, u32)>,
        debug_overlay: Option<&[String]>,
        hotbar_text: &str,
        night_factor: f32,
        shadow_vp: &[[f32; 4]; 4],
    ) -> Result<(), SurfaceError> {
        self.update_uniforms(camera, night_factor, shadow_vp);

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
            spass.set_bind_group(0, &self.uniform_bind_group, &[]);
            for &(_, _, ref data) in chunk_data {
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
                        load: LoadOp::Clear(wgpu::Color { r: 0.5, g: 0.65, b: 0.85, a: 1.0 }),
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

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for &(_, _, ref data) in chunk_data {
                if data.num_indices > 0 {
                    rpass.set_vertex_buffer(0, data.vertex_buffer.slice(..));
                    rpass.set_index_buffer(data.index_buffer.slice(..), IndexFormat::Uint32);
                    rpass.draw_indexed(0..data.num_indices, 0, 0..1);
                }
            }

            rpass.set_pipeline(&self.transparent_pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);

            let cam_pos = camera.position.coords;
            let mut sorted: Vec<usize> = (0..chunk_data.len()).collect();
            sorted.sort_by(|&a, &b| {
                let ca = &chunk_data[a];
                let cb = &chunk_data[b];
                let da = ((ca.0 * 16 + 8) as f32 - cam_pos.x).powi(2)
                       + (-cam_pos.y).powi(2)
                       + ((ca.1 * 16 + 8) as f32 - cam_pos.z).powi(2);
                let db = ((cb.0 * 16 + 8) as f32 - cam_pos.x).powi(2)
                       + (-cam_pos.y).powi(2)
                       + ((cb.1 * 16 + 8) as f32 - cam_pos.z).powi(2);
                db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
            });
            for &idx in &sorted {
                let data = &chunk_data[idx].2;
                if data.transparent_num_indices > 0 {
                    rpass.set_vertex_buffer(0, data.transparent_vertex_buffer.slice(..));
                    rpass.set_index_buffer(data.transparent_index_buffer.slice(..), IndexFormat::Uint32);
                    rpass.draw_indexed(0..data.transparent_num_indices, 0, 0..1);
                }
            }
            if let Some(hl) = highlight {
                rpass.set_pipeline(&self.highlight_pipeline);
                rpass.set_bind_group(0, &self.highlight_bind_group, &[]);
                rpass.set_vertex_buffer(0, hl.vertex_buffer.slice(..));
                rpass.draw(0..hl.num_indices, 0..1);
            }
            if let Some((ref border_buf, border_count)) = chunk_borders {
                rpass.set_pipeline(&self.highlight_pipeline);
                rpass.set_bind_group(0, &self.highlight_bind_group, &[]);
                rpass.set_vertex_buffer(0, border_buf.slice(..));
                rpass.draw(0..*border_count, 0..1);
            }
        }

        let sw = self.size.0 as f32;
        let sh = self.size.1 as f32;
        if let Some(lines) = debug_overlay {
            self.render_overlay(&mut encoder, &view, lines, sw, sh, 0.0);
        }
        if !hotbar_text.is_empty() {
            let bar = vec![hotbar_text.to_string()];
            self.render_overlay(&mut encoder, &view, &bar, sw, sh, sh - 30.0);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
