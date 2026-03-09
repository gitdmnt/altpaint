//! `wgpu` を使って UI ベースフレーム・GPU キャンバス・オーバーレイを提示する。

use anyhow::{Context, Result};
use desktop_support::PresentTimings;
use render::RenderFrame;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::frame::{Rect, TextureQuad};

#[derive(Debug, Clone, Copy)]
pub struct TextureSource<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

impl<'a> From<&'a RenderFrame> for TextureSource<'a> {
    fn from(frame: &'a RenderFrame) -> Self {
        Self {
            width: frame.width as u32,
            height: frame.height as u32,
            pixels: frame.pixels.as_slice(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameLayer<'a> {
    pub source: TextureSource<'a>,
    pub upload_region: Option<UploadRegion>,
}

#[derive(Debug, Clone, Copy)]
pub struct CanvasLayer<'a> {
    pub source: TextureSource<'a>,
    pub upload_region: Option<UploadRegion>,
    pub quad: TextureQuad,
}

#[derive(Debug, Clone, Copy)]
pub struct PresentScene<'a> {
    pub base_layer: FrameLayer<'a>,
    pub overlay_layer: FrameLayer<'a>,
    pub canvas_layer: Option<CanvasLayer<'a>>,
}

const PRESENT_SHADER: &str = r#"
struct LayerUniform {
    rect_min: vec2<f32>,
    rect_max: vec2<f32>,
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
};

@group(0) @binding(0)
var present_texture: texture_2d<f32>;

@group(0) @binding(1)
var present_sampler: sampler;

@group(0) @binding(2)
var<uniform> layer_uniform: LayerUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var unit = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );

    let current = unit[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(
        mix(layer_uniform.rect_min.x, layer_uniform.rect_max.x, current.x),
        mix(layer_uniform.rect_min.y, layer_uniform.rect_max.y, current.y),
        0.0,
        1.0,
    );
    output.uv = vec2<f32>(
        mix(layer_uniform.uv_min.x, layer_uniform.uv_max.x, current.x),
        mix(layer_uniform.uv_min.y, layer_uniform.uv_max.y, current.y),
    );
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(present_texture, present_sampler, input.uv);
}
"#;

#[derive(Debug)]
struct UploadedLayerTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    needs_full_upload: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LayerUploadStats {
    duration: Duration,
    bytes: u64,
}

pub struct WgpuPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    base_layer: Option<UploadedLayerTexture>,
    overlay_layer: Option<UploadedLayerTexture>,
    canvas_layer: Option<UploadedLayerTexture>,
}

fn preferred_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    [
        wgpu::PresentMode::Mailbox,
        wgpu::PresentMode::Immediate,
        wgpu::PresentMode::FifoRelaxed,
        wgpu::PresentMode::Fifo,
    ]
    .into_iter()
    .find(|mode| modes.contains(mode))
    .unwrap_or(wgpu::PresentMode::Fifo)
}

impl WgpuPresenter {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .context("failed to create surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("failed to acquire adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("altpaint-device"),
                required_features: wgpu::Features::empty(),
                experimental_features: Default::default(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::default(),
            })
            .await
            .context("failed to create device")?;

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(surface_capabilities.formats[0]);
        let present_mode = preferred_present_mode(surface_capabilities.present_modes.as_slice());
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("altpaint-present-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("altpaint-present-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("altpaint-present-shader"),
            source: wgpu::ShaderSource::Wgsl(PRESENT_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("altpaint-present-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("altpaint-present-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            sampler,
            bind_group_layout,
            base_layer: None,
            overlay_layer: None,
            canvas_layer: None,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, scene: PresentScene<'_>) -> Result<PresentTimings> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(PresentTimings {
                upload: Duration::default(),
                encode_and_submit: Duration::default(),
                present: Duration::default(),
                base_upload: Duration::default(),
                overlay_upload: Duration::default(),
                canvas_upload: Duration::default(),
                base_upload_bytes: 0,
                overlay_upload_bytes: 0,
                canvas_upload_bytes: 0,
            });
        }

        Self::ensure_layer_texture(
            &self.device,
            &self.sampler,
            &self.bind_group_layout,
            &mut self.base_layer,
            scene.base_layer.source.width,
            scene.base_layer.source.height,
            "base",
        );
        Self::ensure_layer_texture(
            &self.device,
            &self.sampler,
            &self.bind_group_layout,
            &mut self.overlay_layer,
            scene.overlay_layer.source.width,
            scene.overlay_layer.source.height,
            "overlay",
        );
        if let Some(canvas_layer) = scene.canvas_layer {
            Self::ensure_layer_texture(
                &self.device,
                &self.sampler,
                &self.bind_group_layout,
                &mut self.canvas_layer,
                canvas_layer.source.width,
                canvas_layer.source.height,
                "canvas",
            );
        }

        let upload_started = Instant::now();
        let base_upload = Self::upload_layer(
            &self.queue,
            self.base_layer.as_mut(),
            scene.base_layer.source,
            scene.base_layer.upload_region,
        );
        let overlay_upload = Self::upload_layer(
            &self.queue,
            self.overlay_layer.as_mut(),
            scene.overlay_layer.source,
            scene.overlay_layer.upload_region,
        );
        let canvas_upload = if let Some(canvas_layer) = scene.canvas_layer {
            Self::upload_layer(
                &self.queue,
                self.canvas_layer.as_mut(),
                canvas_layer.source,
                canvas_layer.upload_region,
            )
        } else {
            LayerUploadStats::default()
        };

        Self::update_quad_uniform(
            &self.queue,
            self.base_layer.as_ref(),
            fullscreen_quad(self.config.width, self.config.height),
            self.config.width,
            self.config.height,
        );
        Self::update_quad_uniform(
            &self.queue,
            self.overlay_layer.as_ref(),
            fullscreen_quad(self.config.width, self.config.height),
            self.config.width,
            self.config.height,
        );
        if let Some(canvas_layer) = scene.canvas_layer {
            Self::update_quad_uniform(
                &self.queue,
                self.canvas_layer.as_ref(),
                canvas_layer.quad,
                self.config.width,
                self.config.height,
            );
        }
        let upload = upload_started.elapsed();

        let surface_texture = match self.surface.get_current_texture() {
            Ok(surface_texture) => surface_texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .context("failed to acquire surface texture after reconfigure")?
            }
            Err(wgpu::SurfaceError::Timeout) => {
                return Ok(PresentTimings {
                    upload,
                    encode_and_submit: Duration::default(),
                    present: Duration::default(),
                    base_upload: base_upload.duration,
                    overlay_upload: overlay_upload.duration,
                    canvas_upload: canvas_upload.duration,
                    base_upload_bytes: base_upload.bytes,
                    overlay_upload_bytes: overlay_upload.bytes,
                    canvas_upload_bytes: canvas_upload.bytes,
                });
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                anyhow::bail!("out of memory while acquiring surface texture")
            }
            Err(other) => return Err(other).context("failed to acquire surface texture"),
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("altpaint-present-encoder"),
            });
        let encode_started = Instant::now();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("altpaint-present-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            Self::draw_layer(&mut pass, self.base_layer.as_ref());
            if scene.canvas_layer.is_some() {
                Self::draw_layer(&mut pass, self.canvas_layer.as_ref());
            }
            Self::draw_layer(&mut pass, self.overlay_layer.as_ref());
        }

        self.queue.submit([encoder.finish()]);
        let encode_and_submit = encode_started.elapsed();
        let present_started = Instant::now();
        surface_texture.present();
        Ok(PresentTimings {
            upload,
            encode_and_submit,
            present: present_started.elapsed(),
            base_upload: base_upload.duration,
            overlay_upload: overlay_upload.duration,
            canvas_upload: canvas_upload.duration,
            base_upload_bytes: base_upload.bytes,
            overlay_upload_bytes: overlay_upload.bytes,
            canvas_upload_bytes: canvas_upload.bytes,
        })
    }

    fn ensure_layer_texture(
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        bind_group_layout: &wgpu::BindGroupLayout,
        slot: &mut Option<UploadedLayerTexture>,
        width: u32,
        height: u32,
        label: &str,
    ) {
        let needs_rebuild = slot
            .as_ref()
            .is_none_or(|layer| layer.width != width || layer.height != height);
        if !needs_rebuild {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("altpaint-{label}-texture")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("altpaint-{label}-uniform")),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("altpaint-{label}-bind-group")),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        *slot = Some(UploadedLayerTexture {
            texture,
            bind_group,
            uniform_buffer,
            width,
            height,
            needs_full_upload: true,
        });
    }

    fn upload_layer(
        queue: &wgpu::Queue,
        layer: Option<&mut UploadedLayerTexture>,
        source: TextureSource<'_>,
        upload_region: Option<UploadRegion>,
    ) -> LayerUploadStats {
        let Some(layer) = layer else {
            return LayerUploadStats::default();
        };

        let started = Instant::now();

        if layer.needs_full_upload {
            let bytes = upload_full_texture(queue, layer, source);
            layer.needs_full_upload = false;
            return LayerUploadStats {
                duration: started.elapsed(),
                bytes,
            };
        }

        if let Some(region) = upload_region {
            let bytes = upload_texture_region(queue, layer, source, region);
            return LayerUploadStats {
                duration: started.elapsed(),
                bytes,
            };
        }

        LayerUploadStats::default()
    }

    fn update_quad_uniform(
        queue: &wgpu::Queue,
        layer: Option<&UploadedLayerTexture>,
        quad: TextureQuad,
        surface_width: u32,
        surface_height: u32,
    ) {
        let Some(layer) = layer else {
            return;
        };
        queue.write_buffer(
            &layer.uniform_buffer,
            0,
            &quad_uniform_bytes(quad, surface_width, surface_height),
        );
    }

    fn draw_layer<'a>(pass: &mut wgpu::RenderPass<'a>, layer: Option<&'a UploadedLayerTexture>) {
        let Some(layer) = layer else {
            return;
        };
        pass.set_bind_group(0, &layer.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

fn upload_full_texture(
    queue: &wgpu::Queue,
    layer: &UploadedLayerTexture,
    source: TextureSource<'_>,
) -> u64 {
    if source.width == 0 || source.height == 0 {
        return 0;
    }

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &layer.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        source.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width * 4),
            rows_per_image: Some(source.height),
        },
        wgpu::Extent3d {
            width: source.width,
            height: source.height,
            depth_or_array_layers: 1,
        },
    );

    (source.width as u64) * (source.height as u64) * 4
}

fn upload_texture_region(
    queue: &wgpu::Queue,
    layer: &UploadedLayerTexture,
    source: TextureSource<'_>,
    region: UploadRegion,
) -> u64 {
    if region.width == 0 || region.height == 0 || source.width == 0 || source.height == 0 {
        return 0;
    }

    let max_width = source.width.saturating_sub(region.x);
    let max_height = source.height.saturating_sub(region.y);
    let copy_width = region.width.min(max_width);
    let copy_height = region.height.min(max_height);
    if copy_width == 0 || copy_height == 0 {
        return 0;
    }

    let mut packed = vec![0; (copy_width * copy_height * 4) as usize];
    let row_pixels = source.width as usize;
    let copy_width_usize = copy_width as usize;
    let region_x = region.x as usize;
    let region_y = region.y as usize;
    for row in 0..copy_height as usize {
        let src_start = ((region_y + row) * row_pixels + region_x) * 4;
        let src_end = src_start + copy_width_usize * 4;
        let dst_start = row * copy_width_usize * 4;
        packed[dst_start..dst_start + copy_width_usize * 4]
            .copy_from_slice(&source.pixels[src_start..src_end]);
    }

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &layer.texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: region.x,
                y: region.y,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        packed.as_slice(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(copy_width * 4),
            rows_per_image: Some(copy_height),
        },
        wgpu::Extent3d {
            width: copy_width,
            height: copy_height,
            depth_or_array_layers: 1,
        },
    );

    (copy_width as u64) * (copy_height as u64) * 4
}

fn fullscreen_quad(width: u32, height: u32) -> TextureQuad {
    TextureQuad {
        destination: Rect {
            x: 0,
            y: 0,
            width: width as usize,
            height: height as usize,
        },
        uv_min: [0.0, 0.0],
        uv_max: [1.0, 1.0],
    }
}

fn quad_uniform_bytes(quad: TextureQuad, surface_width: u32, surface_height: u32) -> [u8; 32] {
    let surface_width = surface_width.max(1) as f32;
    let surface_height = surface_height.max(1) as f32;
    let left = quad.destination.x as f32 / surface_width * 2.0 - 1.0;
    let top = 1.0 - quad.destination.y as f32 / surface_height * 2.0;
    let right = (quad.destination.x + quad.destination.width) as f32 / surface_width * 2.0 - 1.0;
    let bottom = 1.0 - (quad.destination.y + quad.destination.height) as f32 / surface_height * 2.0;
    let values = [
        left,
        top,
        right,
        bottom,
        quad.uv_min[0],
        quad.uv_min[1],
        quad.uv_max[0],
        quad.uv_max[1],
    ];
    let mut bytes = [0u8; 32];
    for (index, value) in values.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_present_mode_prefers_low_latency_modes() {
        let mode = preferred_present_mode(&[wgpu::PresentMode::Fifo, wgpu::PresentMode::Immediate]);

        assert_eq!(mode, wgpu::PresentMode::Immediate);
    }

    #[test]
    fn preferred_present_mode_uses_mailbox_when_available() {
        let mode = preferred_present_mode(&[
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Immediate,
        ]);

        assert_eq!(mode, wgpu::PresentMode::Mailbox);
    }

    #[test]
    fn presenter_shader_mentions_uniform_quad_mapping() {
        assert!(PRESENT_SHADER.contains("LayerUniform"));
        assert!(PRESENT_SHADER.contains("rect_min"));
        assert!(PRESENT_SHADER.contains("textureSample"));
    }

    #[test]
    fn quad_uniform_bytes_maps_fullscreen_quad_to_ndc() {
        let bytes = quad_uniform_bytes(fullscreen_quad(640, 480), 640, 480);
        let mut values = [0.0f32; 8];
        for (index, chunk) in bytes.chunks_exact(4).enumerate() {
            values[index] = f32::from_le_bytes(chunk.try_into().expect("chunk size"));
        }

        assert_eq!(values[0], -1.0);
        assert_eq!(values[1], 1.0);
        assert_eq!(values[2], 1.0);
        assert_eq!(values[3], -1.0);
    }
}
