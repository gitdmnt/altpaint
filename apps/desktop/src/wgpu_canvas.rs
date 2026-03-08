use anyhow::{Context, Result};
use render::RenderFrame;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::window::Window;

const PRESENT_SHADER: &str = r#"
@group(0) @binding(0)
var present_texture: texture_2d<f32>;

@group(0) @binding(1)
var present_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0)
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0)
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(present_texture, present_sampler, input.uv);
}
"#;

#[derive(Debug)]
struct UploadedFrameTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentTimings {
    pub upload: Duration,
    pub encode_and_submit: Duration,
    pub present: Duration,
}

pub struct WgpuPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    uploaded_frame: Option<UploadedFrameTexture>,
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
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("altpaint-present-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
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
                    blend: Some(wgpu::BlendState::REPLACE),
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
            uploaded_frame: None,
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

    pub fn render(
        &mut self,
        frame: &RenderFrame,
        upload_region: Option<UploadRegion>,
    ) -> Result<PresentTimings> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(PresentTimings {
                upload: Duration::default(),
                encode_and_submit: Duration::default(),
                present: Duration::default(),
            });
        }

        self.ensure_uploaded_frame(frame.width as u32, frame.height as u32);
        let upload_started = Instant::now();
        match upload_region {
            Some(region) => self.upload_frame_region(frame, region),
            None => self.upload_frame(frame),
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
        let uploaded_frame = self
            .uploaded_frame
            .as_ref()
            .context("uploaded frame texture is missing")?;

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
            pass.set_bind_group(0, &uploaded_frame.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        let encode_and_submit = encode_started.elapsed();
        let present_started = Instant::now();
        surface_texture.present();
        Ok(PresentTimings {
            upload,
            encode_and_submit,
            present: present_started.elapsed(),
        })
    }

    fn ensure_uploaded_frame(&mut self, width: u32, height: u32) {
        let needs_rebuild = self
            .uploaded_frame
            .as_ref()
            .is_none_or(|frame| frame.width != width || frame.height != height);
        if !needs_rebuild {
            return;
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("altpaint-present-texture"),
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("altpaint-present-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.uploaded_frame = Some(UploadedFrameTexture {
            texture,
            bind_group,
            width,
            height,
        });
    }

    fn upload_frame(&mut self, frame: &RenderFrame) {
        let Some(uploaded_frame) = self.uploaded_frame.as_ref() else {
            return;
        };

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &uploaded_frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.pixels.as_slice(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((frame.width * 4) as u32),
                rows_per_image: Some(frame.height as u32),
            },
            wgpu::Extent3d {
                width: frame.width as u32,
                height: frame.height as u32,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_frame_region(&mut self, frame: &RenderFrame, region: UploadRegion) {
        let Some(uploaded_frame) = self.uploaded_frame.as_ref() else {
            return;
        };
        if region.width == 0 || region.height == 0 {
            return;
        }

        let max_width = (frame.width as u32).saturating_sub(region.x);
        let max_height = (frame.height as u32).saturating_sub(region.y);
        let copy_width = region.width.min(max_width);
        let copy_height = region.height.min(max_height);
        if copy_width == 0 || copy_height == 0 {
            return;
        }

        let mut packed = vec![0; (copy_width * copy_height * 4) as usize];
        for row in 0..copy_height as usize {
            let src_start = (((region.y as usize + row) * frame.width) + region.x as usize) * 4;
            let src_end = src_start + copy_width as usize * 4;
            let dst_start = row * copy_width as usize * 4;
            let dst_end = dst_start + copy_width as usize * 4;
            packed[dst_start..dst_end].copy_from_slice(&frame.pixels[src_start..src_end]);
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &uploaded_frame.texture,
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
    }
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
    fn presenter_shader_mentions_texture_sampling() {
        assert!(PRESENT_SHADER.contains("textureSample"));
        assert!(PRESENT_SHADER.contains("vs_main"));
        assert!(PRESENT_SHADER.contains("fs_main"));
    }
}
