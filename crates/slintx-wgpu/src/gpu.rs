//! Pipelines and per-frame GPU work: upload the scene, draw it, resolve MSAA.

use std::cell::RefCell;
use std::collections::HashMap;

use wgpu::util::DeviceExt;

use crate::glyphs::GlyphCache;
use crate::images::Uploader;
use crate::scene::{RAMP_ROWS, RAMP_WIDTH, Scene, Vertex};

const SAMPLES: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    viewport: [f32; 2],
    ramp_rows: f32,
    _pad: f32,
}

struct Msaa {
    format: wgpu::TextureFormat,
    size: [u32; 2],
    view: wgpu::TextureView,
}

struct Atlas {
    size: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

pub(crate) struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    globals_layout: wgpu::BindGroupLayout,
    pub image_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: RefCell<HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>>,
    pub smooth: wgpu::Sampler,
    pub nearest: wgpu::Sampler,
    ramp: wgpu::Texture,
    ramp_view: wgpu::TextureView,
    atlas: RefCell<Atlas>,
    no_image: wgpu::BindGroup,
    msaa: RefCell<Option<Msaa>>,
}

impl Gpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slintx-globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1),
                storage(2),
                storage(3),
                texture(4, true),
                sampler(5),
                texture(6, false),
            ],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slintx-image"),
            entries: &[texture(0, true), sampler(1)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("slintx"),
            bind_group_layouts: &[Some(&globals_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let make_sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("slintx"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: filter,
                min_filter: filter,
                ..wgpu::SamplerDescriptor::default()
            })
        };
        let smooth = make_sampler(wgpu::FilterMode::Linear);
        let nearest = make_sampler(wgpu::FilterMode::Nearest);
        let ramp = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slintx-ramps"),
            size: wgpu::Extent3d {
                width: RAMP_WIDTH as u32,
                height: RAMP_ROWS as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let ramp_view = ramp.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas = RefCell::new(Self::make_atlas(&device, 1));
        let gpu_less = Uploader {
            device: &device,
            queue: &queue,
            layout: &image_layout,
            smooth: &smooth,
            nearest: &nearest,
        };
        let no_image = gpu_less.upload(1, 1, &[0, 0, 0, 0]).smooth.clone();
        Self {
            device,
            queue,
            shader,
            globals_layout,
            image_layout,
            pipeline_layout,
            pipelines: RefCell::new(HashMap::new()),
            smooth,
            nearest,
            ramp,
            ramp_view,
            atlas,
            no_image,
            msaa: RefCell::new(None),
        }
    }

    pub fn uploader(&self) -> Uploader<'_> {
        Uploader {
            device: &self.device,
            queue: &self.queue,
            layout: &self.image_layout,
            smooth: &self.smooth,
            nearest: &self.nearest,
        }
    }

    fn make_atlas(device: &wgpu::Device, size: u32) -> Atlas {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slintx-glyphs"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Atlas {
            size,
            texture,
            view,
        }
    }

    fn pipeline(&self, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some(pipeline) = self.pipelines.borrow().get(&format) {
            return pipeline.clone();
        }
        let blend = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("slintx"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState {
                            color: blend,
                            alpha: blend,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..wgpu::MultisampleState::default()
                },
                multiview_mask: None,
                cache: None,
            });
        self.pipelines.borrow_mut().insert(format, pipeline.clone());
        pipeline
    }

    fn msaa_view(&self, format: wgpu::TextureFormat, size: [u32; 2]) -> wgpu::TextureView {
        let mut slot = self.msaa.borrow_mut();
        if let Some(msaa) = slot.as_ref()
            && msaa.format == format
            && msaa.size == size
        {
            return msaa.view.clone();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slintx-msaa"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        *slot = Some(Msaa {
            format,
            size,
            view: view.clone(),
        });
        view
    }

    fn sync_atlas(&self, glyphs: &mut GlyphCache) {
        let mut atlas = self.atlas.borrow_mut();
        if atlas.size != glyphs.size {
            *atlas = Self::make_atlas(&self.device, glyphs.size);
            glyphs.dirty = true;
        }
        if !glyphs.dirty {
            return;
        }
        let size = wgpu::Extent3d {
            width: atlas.size,
            height: atlas.size,
            depth_or_array_layers: 1,
        };
        self.queue.write_texture(
            atlas.texture.as_image_copy(),
            &glyphs.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.size),
                rows_per_image: Some(atlas.size),
            },
            size,
        );
        glyphs.dirty = false;
    }

    /// Draws `scene` over a transparent clear into `target`.
    pub fn draw(&self, scene: &Scene, glyphs: &mut GlyphCache, target: &wgpu::Texture) {
        let size = [target.width().max(1), target.height().max(1)];
        let format = target.format();
        self.sync_atlas(glyphs);
        if !scene.ramps.is_empty() {
            let rows = (scene.ramps.len() / (RAMP_WIDTH * 4)) as u32;
            self.queue.write_texture(
                self.ramp.as_image_copy(),
                &scene.ramps,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(RAMP_WIDTH as u32 * 4),
                    rows_per_image: Some(rows),
                },
                wgpu::Extent3d {
                    width: RAMP_WIDTH as u32,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
        }

        let globals = Globals {
            viewport: [size[0] as f32, size[1] as f32],
            ramp_rows: RAMP_ROWS as f32,
            _pad: 0.0,
        };
        let buffer = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        // Storage bindings cannot be empty.
        let non_empty = |bytes: &[u8], stride: usize| -> Vec<u8> {
            if bytes.is_empty() {
                vec![0; stride]
            } else {
                bytes.to_vec()
            }
        };
        let globals_buffer = buffer(
            "slintx-globals",
            bytemuck::bytes_of(&globals),
            wgpu::BufferUsages::UNIFORM,
        );
        let prims = buffer(
            "slintx-prims",
            &non_empty(bytemuck::cast_slice(&scene.prims), 112),
            wgpu::BufferUsages::STORAGE,
        );
        let brushes = buffer(
            "slintx-brushes",
            &non_empty(bytemuck::cast_slice(&scene.brushes), 48),
            wgpu::BufferUsages::STORAGE,
        );
        let clips = buffer(
            "slintx-clips",
            &non_empty(bytemuck::cast_slice(&scene.clips), 48),
            wgpu::BufferUsages::STORAGE,
        );
        let vertices = buffer(
            "slintx-vertices",
            &non_empty(bytemuck::cast_slice(&scene.vertices), 4),
            wgpu::BufferUsages::VERTEX,
        );
        let atlas = self.atlas.borrow();
        let globals_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("slintx-globals"),
            layout: &self.globals_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: prims.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: brushes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: clips.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.ramp_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.smooth),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
            ],
        });

        let pipeline = self.pipeline(format);
        let msaa = self.msaa_view(format, size);
        let resolve = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("slintx"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("slintx"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa,
                    depth_slice: None,
                    resolve_target: Some(&resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &globals_group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            for batch in &scene.batches {
                if batch.start == batch.end {
                    continue;
                }
                let group = match &batch.image {
                    Some((image, true)) => &image.nearest,
                    Some((image, false)) => &image.smooth,
                    None => &self.no_image,
                };
                pass.set_bind_group(1, group, &[]);
                pass.draw(batch.start..batch.end, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Copies an `Rgba8Unorm` texture back to the CPU, row by row.
    pub fn read_back(&self, texture: &wgpu::Texture) -> Result<Vec<u8>, String> {
        let (width, height) = (texture.width(), texture.height());
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("slintx-readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("slintx-readback"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|err| err.to_string())?;
        receiver
            .recv()
            .map_err(|err| err.to_string())?
            .map_err(|err| err.to_string())?;
        let mapped = slice.get_mapped_range().map_err(|err| err.to_string())?;
        let mut out = Vec::with_capacity((unpadded * height) as usize);
        for row in mapped.chunks(padded as usize) {
            out.extend_from_slice(&row[..unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(out)
    }
}
