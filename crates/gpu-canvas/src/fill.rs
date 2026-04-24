//! GPU 塗りつぶし（flood fill / lasso fill）のディスパッチャ。
//!
//! `flood_fill_step.wgsl` をピンポンマスクで反復実行し、`fill_apply.wgsl` で
//! アクティブレイヤーへ source-over ブレンドで fill color を書き込む。
//! lasso は `lasso_fill_mark.wgsl` でポリゴン内ピクセルをマスクし、同じ
//! `fill_apply.wgsl` を使う。

use std::sync::Arc;

use crate::gpu::GpuLayerTexture;

/// flood_fill_step の uniform バッファサイズ（32 bytes）。
const FLOOD_FILL_PARAMS_SIZE: u64 = 32;
/// fill_apply の uniform バッファサイズ（32 bytes）。
const FILL_APPLY_PARAMS_SIZE: u64 = 32;
/// lasso_fill_mark の uniform バッファサイズ（32 bytes）。
const LASSO_MARK_PARAMS_SIZE: u64 = 32;
/// 収束検出を試す iteration 間隔。
const CONVERGENCE_CHECK_INTERVAL: u32 = 32;
/// Flood fill の最大 iteration 数の安全上限。
const FLOOD_FILL_ITERATION_CAP: u32 = 8192;

/// Flood fill 後に返される実行メトリクス。
pub struct FloodFillOutcome {
    pub iterations: u32,
    pub pixels_changed: u32,
}

/// GPU 塗りつぶしパイプラインを管理するディスパッチャ。
pub struct GpuFillDispatch {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    flood_step_pipeline: wgpu::ComputePipeline,
    flood_step_bgl: wgpu::BindGroupLayout,
    lasso_pipeline: wgpu::ComputePipeline,
    lasso_bgl: wgpu::BindGroupLayout,
    apply_pipeline: wgpu::ComputePipeline,
    apply_bgl: wgpu::BindGroupLayout,
}

impl GpuFillDispatch {
    /// 計算パイプラインと BGL を初期化する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let flood_step_bgl = create_flood_step_bgl(&device);
        let lasso_bgl = create_lasso_bgl(&device);
        let apply_bgl = create_apply_bgl(&device);

        let flood_step_pipeline = build_pipeline(
            &device,
            &flood_step_bgl,
            include_str!("shaders/flood_fill_step.wgsl"),
            "flood_fill_step",
        );
        let lasso_pipeline = build_pipeline(
            &device,
            &lasso_bgl,
            include_str!("shaders/lasso_fill_mark.wgsl"),
            "lasso_fill_mark",
        );
        let apply_pipeline = build_pipeline(
            &device,
            &apply_bgl,
            include_str!("shaders/fill_apply.wgsl"),
            "fill_apply",
        );

        Self {
            device,
            queue,
            flood_step_pipeline,
            flood_step_bgl,
            lasso_pipeline,
            lasso_bgl,
            apply_pipeline,
            apply_bgl,
        }
    }

    /// 指定座標の seed 色に一致する連結成分を塗りつぶす。
    ///
    /// - `source`: seed 色と連結成分の判定に使うテクスチャ。CPU 実装の
    ///   `composited_bitmap` に相当（多レイヤー時は panel の composite テクスチャを
    ///   渡す。単一レイヤー時は active layer テクスチャで等価）。
    /// - `target`: 実際に塗り色を書き込むレイヤーテクスチャ（active layer）。
    /// - `source` と `target` は同じサイズである必要がある。
    pub fn dispatch_flood_fill(
        &self,
        source: &GpuLayerTexture,
        target: &GpuLayerTexture,
        seed: (u32, u32),
        fill_rgba: [f32; 4],
    ) -> FloodFillOutcome {
        let w = target.width;
        let h = target.height;
        if w == 0 || h == 0 || seed.0 >= w || seed.1 >= h {
            return FloodFillOutcome {
                iterations: 0,
                pixels_changed: 0,
            };
        }
        if source.width != w || source.height != h {
            return FloodFillOutcome {
                iterations: 0,
                pixels_changed: 0,
            };
        }

        let mark_a = self.create_mask_texture(w, h, "flood-mark-a");
        let mark_b = self.create_mask_texture(w, h, "flood-mark-b");

        let source_view = source.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("flood-source-view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let view_a = mark_a.create_view(&wgpu::TextureViewDescriptor {
            label: Some("flood-mark-a-view"),
            ..Default::default()
        });
        let view_b = mark_b.create_view(&wgpu::TextureViewDescriptor {
            label: Some("flood-mark-b-view"),
            ..Default::default()
        });

        let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("flood-fill-params"),
            size: FLOOD_FILL_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &params_buf,
            0,
            &build_flood_fill_params_bytes(seed.0, seed.1, w, h),
        );

        let counter_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("flood-fill-counter"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("flood-fill-counter-readback"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bg_ab = self.make_flood_bind_group(
            &params_buf,
            &source_view,
            &view_a,
            &view_b,
            &counter_buf,
        );
        let bg_ba = self.make_flood_bind_group(
            &params_buf,
            &source_view,
            &view_b,
            &view_a,
            &counter_buf,
        );

        let wg_x = w.div_ceil(8);
        let wg_y = h.div_ceil(8);
        let max_iter = FLOOD_FILL_ITERATION_CAP.min(w + h + 4);

        let mut iterations = 0u32;
        let mut total_changed = 0u32;
        // swap_flag は「次 iter で src として読むマスクはどちらか」を示す:
        //   swap_flag == false → bg_ab (src=a, dst=b) を使う → 書き込み先は b
        //   swap_flag == true  → bg_ba (src=b, dst=a) を使う → 書き込み先は a
        // 各 iter の dispatch 後に swap_flag をトグルする。
        // ループ終了後、直近の書き込み先は: swap_flag==true なら b、false なら a。
        let mut swap_flag = false;
        for iter in 0..max_iter {
            iterations += 1;

            // Reset atomic counter.
            self.queue.write_buffer(&counter_buf, 0, &0u32.to_le_bytes());

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("flood-fill-step-encoder"),
                });
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("flood-fill-step-pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.flood_step_pipeline);
                let bg = if swap_flag { &bg_ba } else { &bg_ab };
                cpass.set_bind_group(0, bg, &[]);
                cpass.dispatch_workgroups(wg_x, wg_y, 1);
            }

            // Copy counter → readback buffer.
            let needs_check = iter % CONVERGENCE_CHECK_INTERVAL == CONVERGENCE_CHECK_INTERVAL - 1;
            if needs_check {
                encoder.copy_buffer_to_buffer(&counter_buf, 0, &readback_buf, 0, 4);
            }
            self.queue.submit(std::iter::once(encoder.finish()));

            if needs_check {
                let slice = readback_buf.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
                let _ = self.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                });
                if rx.recv().ok().and_then(|r| r.ok()).is_some() {
                    let data = slice.get_mapped_range();
                    let changed =
                        u32::from_le_bytes(data[0..4].try_into().unwrap_or([0u8; 4]));
                    drop(data);
                    readback_buf.unmap();
                    total_changed += changed;
                    if changed == 0 {
                        // Converged.
                        swap_flag = !swap_flag;
                        break;
                    }
                }
            }
            swap_flag = !swap_flag;
        }

        // 直近の書き込み先マスクを選ぶ。swap_flag==true のとき最後の dst は mark_b。
        let final_mark = if swap_flag { &mark_b } else { &mark_a };

        self.apply_mark_to_layer(final_mark, target, fill_rgba);

        FloodFillOutcome {
            iterations,
            pixels_changed: total_changed,
        }
    }

    /// 指定ポリゴン内ピクセルを塗りつぶす。`polygon_aabb` は `(x0, y0, x1, y1)` の包括的
    /// バウンディングボックス（x1/y1 は最終ピクセル座標）。
    pub fn dispatch_lasso_fill(
        &self,
        active_layer: &GpuLayerTexture,
        polygon: &[(f32, f32)],
        polygon_aabb: (u32, u32, u32, u32),
        fill_rgba: [f32; 4],
    ) {
        let w = active_layer.width;
        let h = active_layer.height;
        if w == 0 || h == 0 || polygon.len() < 3 {
            return;
        }

        let mark = self.create_mask_texture(w, h, "lasso-mark");
        let mark_view = mark.create_view(&wgpu::TextureViewDescriptor {
            label: Some("lasso-mark-view"),
            ..Default::default()
        });

        // Pack polygon into storage buffer (vec2<f32>; stride 8).
        let mut poly_bytes = Vec::with_capacity(polygon.len() * 8);
        for &(x, y) in polygon {
            poly_bytes.extend_from_slice(&x.to_le_bytes());
            poly_bytes.extend_from_slice(&y.to_le_bytes());
        }
        let poly_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lasso-polygon"),
            size: poly_bytes.len() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&poly_buf, 0, &poly_bytes);

        let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lasso-mark-params"),
            size: LASSO_MARK_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &params_buf,
            0,
            &build_lasso_mark_params_bytes(polygon.len() as u32, w, h, polygon_aabb),
        );

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lasso-mark-bg"),
            layout: &self.lasso_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: poly_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&mark_view),
                },
            ],
        });

        let wg_x = w.div_ceil(8);
        let wg_y = h.div_ceil(8);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lasso-mark-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("lasso-mark-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.lasso_pipeline);
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));

        self.apply_mark_to_layer(&mark, active_layer, fill_rgba);
    }

    fn apply_mark_to_layer(
        &self,
        mark: &wgpu::Texture,
        active_layer: &GpuLayerTexture,
        fill_rgba: [f32; 4],
    ) {
        let w = active_layer.width;
        let h = active_layer.height;

        let mark_view = mark.create_view(&wgpu::TextureViewDescriptor {
            label: Some("fill-apply-mark-view"),
            ..Default::default()
        });
        let layer_view = active_layer.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("fill-apply-layer-view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });

        let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fill-apply-params"),
            size: FILL_APPLY_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &params_buf,
            0,
            &build_fill_apply_params_bytes(fill_rgba, w, h),
        );

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fill-apply-bg"),
            layout: &self.apply_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mark_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&layer_view),
                },
            ],
        });

        let wg_x = w.div_ceil(8);
        let wg_y = h.div_ceil(8);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fill-apply-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("fill-apply-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.apply_pipeline);
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn create_mask_texture(&self, w: u32, h: u32, label: &str) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    fn make_flood_bind_group(
        &self,
        params: &wgpu::Buffer,
        source_view: &wgpu::TextureView,
        in_view: &wgpu::TextureView,
        out_view: &wgpu::TextureView,
        counter: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("flood-step-bg"),
            layout: &self.flood_step_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(in_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(out_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: counter.as_entire_binding(),
                },
            ],
        })
    }
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

fn create_flood_step_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("flood-step-bgl"),
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
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_lasso_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("lasso-mark-bgl"),
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
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
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

fn create_apply_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fill-apply-bgl"),
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

/// FloodFillStepParams を 32-byte LE シリアライズする。WGSL のメモリレイアウトに一致。
pub(crate) fn build_flood_fill_params_bytes(
    seed_x: u32,
    seed_y: u32,
    w: u32,
    h: u32,
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0..4].copy_from_slice(&seed_x.to_le_bytes());
    buf[4..8].copy_from_slice(&seed_y.to_le_bytes());
    buf[8..12].copy_from_slice(&w.to_le_bytes());
    buf[12..16].copy_from_slice(&h.to_le_bytes());
    buf
}

/// FillApplyParams を 32-byte LE シリアライズする。
pub(crate) fn build_fill_apply_params_bytes(color: [f32; 4], w: u32, h: u32) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0..4].copy_from_slice(&color[0].to_le_bytes());
    buf[4..8].copy_from_slice(&color[1].to_le_bytes());
    buf[8..12].copy_from_slice(&color[2].to_le_bytes());
    buf[12..16].copy_from_slice(&color[3].to_le_bytes());
    buf[16..20].copy_from_slice(&w.to_le_bytes());
    buf[20..24].copy_from_slice(&h.to_le_bytes());
    buf
}

/// LassoMarkParams を 32-byte LE シリアライズする。
pub(crate) fn build_lasso_mark_params_bytes(
    polygon_count: u32,
    w: u32,
    h: u32,
    aabb: (u32, u32, u32, u32),
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0..4].copy_from_slice(&polygon_count.to_le_bytes());
    buf[4..8].copy_from_slice(&w.to_le_bytes());
    buf[8..12].copy_from_slice(&h.to_le_bytes());
    buf[12..16].copy_from_slice(&aabb.0.to_le_bytes());
    buf[16..20].copy_from_slice(&aabb.1.to_le_bytes());
    buf[20..24].copy_from_slice(&aabb.2.to_le_bytes());
    buf[24..28].copy_from_slice(&aabb.3.to_le_bytes());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_fill_params_layout_matches_wgsl() {
        let bytes = build_flood_fill_params_bytes(3, 5, 128, 256);
        assert_eq!(bytes.len(), 32);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 5);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 128);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 256);
        assert_eq!(&bytes[16..32], &[0u8; 16]);
    }

    #[test]
    fn fill_apply_params_layout_matches_wgsl() {
        let bytes = build_fill_apply_params_bytes([1.0, 0.5, 0.25, 0.75], 64, 32);
        assert_eq!(bytes.len(), 32);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0.25);
        assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), 0.75);
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 64);
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 32);
        assert_eq!(&bytes[24..32], &[0u8; 8]);
    }

    #[test]
    fn lasso_mark_params_layout_matches_wgsl() {
        let bytes = build_lasso_mark_params_bytes(7, 320, 240, (10, 20, 100, 200));
        assert_eq!(bytes.len(), 32);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 320);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 240);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 10);
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 100);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 200);
    }
}
