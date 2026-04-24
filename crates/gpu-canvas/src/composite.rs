//! GPU 多レイヤー合成ディスパッチャ。
//!
//! `composite_clear.wgsl` で dirty 領域を透明クリアし、bottom → top で
//! `layer_composite.wgsl` を 1 レイヤーずつ dispatch する。blend mode は
//! `Normal=0 / Multiply=1 / Screen=2 / Add=3` の u32 code で渡す。

use std::sync::Arc;

use crate::gpu::GpuLayerTexture;

const CLEAR_PARAMS_SIZE: u64 = 32;
const COMPOSITE_PARAMS_SIZE: u64 = 32;

/// 合成する 1 レイヤー分の情報。
pub struct CompositeLayerEntry<'a> {
    pub color: &'a GpuLayerTexture,
    pub mask: Option<&'a wgpu::Texture>,
    pub blend_code: u32,
    pub visible: bool,
}

/// 多レイヤー合成パイプラインを管理するディスパッチャ。
pub struct GpuLayerCompositor {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    clear_pipeline: wgpu::ComputePipeline,
    clear_bgl: wgpu::BindGroupLayout,
    composite_pipeline: wgpu::ComputePipeline,
    composite_bgl: wgpu::BindGroupLayout,
    dummy_mask: wgpu::Texture,
}

impl GpuLayerCompositor {
    /// 計算パイプラインと BGL を初期化する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let clear_bgl = create_clear_bgl(&device);
        let composite_bgl = create_composite_bgl(&device);
        let clear_pipeline = build_pipeline(
            &device,
            &clear_bgl,
            include_str!("shaders/composite_clear.wgsl"),
            "composite_clear",
        );
        let composite_pipeline = build_pipeline(
            &device,
            &composite_bgl,
            include_str!("shaders/layer_composite.wgsl"),
            "layer_composite",
        );
        let dummy_mask = create_dummy_mask(&device);
        Self {
            device,
            queue,
            clear_pipeline,
            clear_bgl,
            composite_pipeline,
            composite_bgl,
            dummy_mask,
        }
    }

    /// dirty 範囲内で composite テクスチャを再合成する。
    ///
    /// `layers` は bottom → top 順。`visible == false` のエントリはスキップする。
    /// `dirty` は `(x0, y0, x1, y1)` の半開区間。範囲外は書き換えない。
    pub fn recomposite(
        &self,
        composite: &GpuLayerTexture,
        layers: &[CompositeLayerEntry<'_>],
        dirty: (u32, u32, u32, u32),
    ) {
        let w = composite.width;
        let h = composite.height;
        let dirty = clamp_dirty(dirty, w, h);
        if dirty.0 >= dirty.2 || dirty.1 >= dirty.3 {
            return;
        }

        let composite_view = composite.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("composite-rw-view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });

        // 1. Clear pass.
        let clear_params = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite-clear-params"),
            size: CLEAR_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &clear_params,
            0,
            &build_clear_params_bytes(dirty, w, h),
        );
        let clear_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite-clear-bg"),
            layout: &self.clear_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: clear_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&composite_view),
                },
            ],
        });

        let dirty_w = dirty.2 - dirty.0;
        let dirty_h = dirty.3 - dirty.1;
        let wg_x = dirty_w.div_ceil(8);
        let wg_y = dirty_h.div_ceil(8);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("composite-clear-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("composite-clear-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.clear_pipeline);
            cpass.set_bind_group(0, &clear_bg, &[]);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));

        // 2. Iterative composite passes (bottom → top).
        let dummy_view = self.dummy_mask.create_view(&wgpu::TextureViewDescriptor {
            label: Some("composite-dummy-mask-view"),
            ..Default::default()
        });
        for entry in layers {
            if !entry.visible {
                continue;
            }
            let layer_view = entry.color.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("composite-layer-color-view"),
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            });
            let mask_view = match entry.mask {
                Some(m) => m.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("composite-mask-view"),
                    ..Default::default()
                }),
                None => dummy_view.clone(),
            };
            let has_mask = u32::from(entry.mask.is_some());

            let params = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("composite-params"),
                size: COMPOSITE_PARAMS_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(
                &params,
                0,
                &build_composite_params_bytes(dirty, w, h, entry.blend_code, has_mask),
            );

            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite-layer-bg"),
                layout: &self.composite_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&layer_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&mask_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&composite_view),
                    },
                ],
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("composite-layer-encoder"),
                });
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("composite-layer-pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.composite_pipeline);
                cpass.set_bind_group(0, &bg, &[]);
                cpass.dispatch_workgroups(wg_x, wg_y, 1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }
}

fn clamp_dirty(dirty: (u32, u32, u32, u32), w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x0 = dirty.0.min(w);
    let y0 = dirty.1.min(h);
    let x1 = dirty.2.min(w);
    let y1 = dirty.3.min(h);
    (x0, y0, x1, y1)
}

fn create_dummy_mask(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("composite-dummy-mask"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn build_pipeline(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    wgsl: &str,
    label: &str,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bgl],
        immediate_size: 0,
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn create_clear_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("composite-clear-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    })
}

fn create_composite_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("composite-layer-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::ReadWrite,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    })
}

pub(crate) fn build_clear_params_bytes(
    dirty: (u32, u32, u32, u32),
    layer_w: u32,
    layer_h: u32,
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0..4].copy_from_slice(&dirty.0.to_le_bytes());
    buf[4..8].copy_from_slice(&dirty.1.to_le_bytes());
    buf[8..12].copy_from_slice(&dirty.2.to_le_bytes());
    buf[12..16].copy_from_slice(&dirty.3.to_le_bytes());
    buf[16..20].copy_from_slice(&layer_w.to_le_bytes());
    buf[20..24].copy_from_slice(&layer_h.to_le_bytes());
    buf
}

pub(crate) fn build_composite_params_bytes(
    dirty: (u32, u32, u32, u32),
    layer_w: u32,
    layer_h: u32,
    blend_code: u32,
    has_mask: u32,
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0..4].copy_from_slice(&dirty.0.to_le_bytes());
    buf[4..8].copy_from_slice(&dirty.1.to_le_bytes());
    buf[8..12].copy_from_slice(&dirty.2.to_le_bytes());
    buf[12..16].copy_from_slice(&dirty.3.to_le_bytes());
    buf[16..20].copy_from_slice(&layer_w.to_le_bytes());
    buf[20..24].copy_from_slice(&layer_h.to_le_bytes());
    buf[24..28].copy_from_slice(&blend_code.to_le_bytes());
    buf[28..32].copy_from_slice(&has_mask.to_le_bytes());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_params_layout_matches_wgsl() {
        let bytes = build_clear_params_bytes((1, 2, 100, 200), 256, 256);
        assert_eq!(bytes.len(), 32);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 100);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 200);
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 256);
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 256);
    }

    #[test]
    fn composite_params_layout_matches_wgsl() {
        let bytes = build_composite_params_bytes((0, 0, 64, 64), 128, 128, 2, 1);
        assert_eq!(bytes.len(), 32);
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 128);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 1);
    }
}
