//! GPU ブラシ計算シェーダーのディスパッチ。
//!
//! `brush_stroke.wgsl` / `erase_stamp.wgsl` を実行し、
//! レイヤーテクスチャに source-over または消去ブレンドを適用する。

use std::sync::Arc;

use app_core::paint_params::MAX_STAMP_STEPS;
use app_core::ToolKind;

use crate::gpu::GpuLayerTexture;

/// BrushStrokeParams uniform buffer の固定バイトサイズ（48 bytes）。
const BRUSH_STROKE_PARAMS_SIZE: u64 = 48;

/// stamp_positions storage buffer の最大バイトサイズ。
/// MAX_STAMP_STEPS+1 個の vec2<f32>（各 8 bytes）。
const STAMP_POSITIONS_SIZE: u64 = (MAX_STAMP_STEPS as u64 + 1) * 8;

/// GPU ブラシ/消しゴム計算シェーダーを管理するディスパッチャ。
///
/// `new` でパイプラインを一度構築し、`dispatch_stroke` を繰り返し呼び出す。
pub struct GpuBrushDispatch {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    stroke_pipeline: wgpu::ComputePipeline,
    erase_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuBrushDispatch {
    /// 計算パイプラインとバインドグループレイアウトを初期化する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let bind_group_layout = Self::create_bind_group_layout(&device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-brush-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let stroke_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("brush_stroke"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/brush_stroke.wgsl").into(),
            ),
        });
        let erase_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("erase_stamp"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/erase_stamp.wgsl").into(),
            ),
        });

        let stroke_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("brush-stroke-pipeline"),
            layout: Some(&pipeline_layout),
            module: &stroke_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let erase_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("erase-stamp-pipeline"),
            layout: Some(&pipeline_layout),
            module: &erase_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            device,
            queue,
            stroke_pipeline,
            erase_pipeline,
            bind_group_layout,
        }
    }

    /// 指定スタンプ位置群をレイヤーテクスチャへ描画する。
    ///
    /// - `tool_kind == ToolKind::Eraser` なら消去シェーダーを使用する。
    /// - `positions` は `canvas::compute_stamp_positions` の戻り値を `(x, y)` に変換して渡す。
    /// - `positions` が空の場合は何もしない。
    pub fn dispatch_stroke(
        &self,
        layer_texture: &GpuLayerTexture,
        positions: &[(f32, f32)],
        color_rgba: [f32; 4],
        radius: f32,
        opacity: f32,
        antialias: bool,
        tool_kind: ToolKind,
    ) {
        let stamp_count = positions.len().min(MAX_STAMP_STEPS + 1) as u32;
        if stamp_count == 0 {
            return;
        }

        let params_bytes = build_stroke_params_bytes(
            color_rgba,
            radius,
            opacity,
            antialias,
            stamp_count,
            layer_texture.width,
            layer_texture.height,
        );
        let positions_bytes = build_positions_bytes(positions, MAX_STAMP_STEPS + 1);

        let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("brush-stroke-params"),
            size: BRUSH_STROKE_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buf, 0, &params_bytes);

        let positions_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("brush-stamp-positions"),
            size: STAMP_POSITIONS_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&positions_buf, 0, &positions_bytes);

        let texture_view = layer_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                label: Some("brush-layer-view"),
                ..Default::default()
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("brush-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: positions_buf.as_entire_binding(),
                },
            ],
        });

        let pipeline = match tool_kind {
            ToolKind::Eraser => &self.erase_pipeline,
            _ => &self.stroke_pipeline,
        };

        let wg_x = (layer_texture.width + 7) / 8;
        let wg_y = (layer_texture.height + 7) / 8;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("brush-stroke-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("brush-stroke-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu-brush-bgl"),
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
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }
}

/// BrushStrokeParams を 48-byte リトルエンディアンで直列化する。
///
/// WGSL のメモリレイアウトと完全に一致させる。CPU テストで検証可能。
fn build_stroke_params_bytes(
    color_rgba: [f32; 4],
    radius: f32,
    opacity: f32,
    antialias: bool,
    stamp_count: u32,
    layer_width: u32,
    layer_height: u32,
) -> [u8; 48] {
    let mut buf = [0u8; 48];
    // color: vec4<f32> at offset 0 (16 bytes)
    buf[0..4].copy_from_slice(&color_rgba[0].to_le_bytes());
    buf[4..8].copy_from_slice(&color_rgba[1].to_le_bytes());
    buf[8..12].copy_from_slice(&color_rgba[2].to_le_bytes());
    buf[12..16].copy_from_slice(&color_rgba[3].to_le_bytes());
    // radius: f32 at offset 16
    buf[16..20].copy_from_slice(&radius.to_le_bytes());
    // opacity: f32 at offset 20
    buf[20..24].copy_from_slice(&opacity.to_le_bytes());
    // antialias: u32 at offset 24
    buf[24..28].copy_from_slice(&(antialias as u32).to_le_bytes());
    // stamp_count: u32 at offset 28
    buf[28..32].copy_from_slice(&stamp_count.to_le_bytes());
    // layer_width: u32 at offset 32
    buf[32..36].copy_from_slice(&layer_width.to_le_bytes());
    // layer_height: u32 at offset 36
    buf[36..40].copy_from_slice(&layer_height.to_le_bytes());
    // _pad0 at offset 40, _pad1 at offset 44 — zeros (already zeroed)
    buf
}

/// スタンプ位置を `max_count` 個までフラットな f32 LE バイト列へ直列化する。
///
/// `STAMP_POSITIONS_SIZE` バイトの固定長バッファを返す。
fn build_positions_bytes(positions: &[(f32, f32)], max_count: usize) -> Vec<u8> {
    let mut buf = vec![0u8; max_count * 8];
    for (i, &(x, y)) in positions.iter().take(max_count).enumerate() {
        let offset = i * 8;
        buf[offset..offset + 4].copy_from_slice(&x.to_le_bytes());
        buf[offset + 4..offset + 8].copy_from_slice(&y.to_le_bytes());
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_bytes_layout_matches_wgsl() {
        let bytes = build_stroke_params_bytes(
            [1.0, 0.5, 0.25, 1.0],
            10.0,
            0.8,
            true,
            3,
            512,
            512,
        );
        assert_eq!(bytes.len(), 48);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0.25);
        assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[16..20].try_into().unwrap()), 10.0);
        assert_eq!(f32::from_le_bytes(bytes[20..24].try_into().unwrap()), 0.8);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 1u32);
        assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 3u32);
        assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 512u32);
        assert_eq!(u32::from_le_bytes(bytes[36..40].try_into().unwrap()), 512u32);
        assert_eq!(&bytes[40..48], &[0u8; 8]);
    }

    #[test]
    fn positions_bytes_layout_is_correct() {
        let positions = vec![(1.0f32, 2.0f32), (3.0, 4.0)];
        let bytes = build_positions_bytes(&positions, MAX_STAMP_STEPS + 1);
        assert_eq!(bytes.len(), (MAX_STAMP_STEPS + 1) * 8);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2.0);
        assert_eq!(f32::from_le_bytes(bytes[8..12].try_into().unwrap()), 3.0);
        assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), 4.0);
        assert!(bytes[16..].iter().all(|&b| b == 0));
    }

    #[test]
    fn params_bytes_antialias_false_is_zero() {
        let bytes = build_stroke_params_bytes([0.0; 4], 1.0, 1.0, false, 1, 1, 1);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 0u32);
    }
}
