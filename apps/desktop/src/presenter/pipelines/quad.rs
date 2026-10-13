//! 単一 uniform バインディングで駆動する quad パイプラインの共通実装 (BL-116)。
//!
//! solid / circle / line の 3 パイプラインは、シェーダ・uniform サイズ・uniform
//! バイト列のエンコード方法だけが異なり、bind group layout 構築・slot プール管理・
//! draw 記録は完全に同一だった。本モジュールはその共通部分を `QuadPipeline` 1 つに
//! 統合し、差分はコンストラクタ引数 (シェーダ / uniform サイズ) と `prepare` に渡す
//! エンコーダ関数でパラメータ化する。

/// quad パイプラインの 1 スロット (bind_group + uniform buffer)。
pub(crate) struct QuadSlot {
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

/// `@group(0) @binding(0)` に uniform バッファ 1 個だけを持つ quad 描画パイプライン。
///
/// 使用フロー:
///   1. `prepare(quads, encode)` で必要数の slot を確保し uniform を書き込む
///   2. レンダーパス内で `record(pass, 0..count)` を呼んで draw コールを積む
///
/// slot は連結 Vec として保持されるため、複数レイヤー (背景 / overlay / 前景) が
/// 1 つのパイプラインを共有する場合は `record` の range トークンでレイヤーごとの
/// 区間を指定する (旧 skip/take インデックス演算の置換)。
pub(crate) struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_size: u64,
    label: &'static str,
    slots: Vec<QuadSlot>,
}

impl QuadPipeline {
    /// 指定シェーダと uniform サイズで quad パイプラインを構築する。
    ///
    /// `label` はリソースのデバッグラベルの接頭辞 (例: `"solid-quad"`)。
    pub(crate) fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        shader_source: &str,
        uniform_size: u64,
        label: &'static str,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("altpaint-{label}-bind-group-layout")),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX.union(wgpu::ShaderStages::FRAGMENT),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("altpaint-{label}-shader")),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("altpaint-{label}-pipeline-layout")),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("altpaint-{label}-pipeline")),
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
                    format: surface_format,
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
        Self {
            pipeline,
            bind_group_layout,
            uniform_size,
            label,
            slots: Vec::new(),
        }
    }

    /// `quads` の各エントリに対し `encode` で uniform バイト列を生成して書き込む。
    /// 不足分の slot はプールへ追加する。
    pub(crate) fn prepare<T, F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[T],
        mut encode: F,
    ) where
        F: FnMut(&T) -> Vec<u8>,
    {
        while self.slots.len() < quads.len() {
            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("altpaint-{}-uniform", self.label)),
                size: self.uniform_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("altpaint-{}-bind-group", self.label)),
                layout: &self.bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });
            self.slots.push(QuadSlot {
                bind_group,
                uniform_buffer,
            });
        }
        for (slot, quad) in self.slots.iter().zip(quads.iter()) {
            queue.write_buffer(&slot.uniform_buffer, 0, &encode(quad));
        }
    }

    /// レンダーパスへ `range` の slot 区間の draw コールを積む。
    ///
    /// 連結 Vec を共有する複数レイヤーは、レイヤーごとの slot 区間
    /// (`QuadLayerRange`) を渡して描画する (旧 skip/take 演算の置換)。
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, range: QuadLayerRange) {
        if range.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        let end = (range.start + range.count).min(self.slots.len());
        for slot in &self.slots[range.start.min(self.slots.len())..end] {
            pass.set_bind_group(0, &slot.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

/// 連結 slot Vec 内で 1 レイヤーが占める区間を表す range トークン (BL-116)。
///
/// 旧コードの `slots.iter().skip(start).take(count)` の手書きインデックス演算を
/// 型で表現し、複数レイヤーが 1 パイプラインを共有する際のオフセット計算を
/// `QuadLayerRanges` に閉じ込める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuadLayerRange {
    pub(crate) start: usize,
    pub(crate) count: usize,
}

/// solid quad の連結 Vec を構成する 3 レイヤー (背景 / overlay / 前景) の区間を
/// 連続的に割り当てるビルダ。各レイヤーの先頭オフセットを順送りで計算する。
#[derive(Debug, Default)]
pub(crate) struct QuadLayerRanges {
    next_start: usize,
}

impl QuadLayerRanges {
    /// 次のレイヤーに `count` 個の slot 区間を割り当て、その range を返す。
    pub(crate) fn allocate(&mut self, count: usize) -> QuadLayerRange {
        let range = QuadLayerRange {
            start: self.next_start,
            count,
        };
        self.next_start += count;
        range
    }
}
