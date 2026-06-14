//! テクスチャ提示パイプライン (キャンバス / HTML パネル / ステータスバー共用)。
//!
//! `@group(0)` に texture(0) + sampler(1) + uniform(2) の 3 バインディングを持つ
//! レンダーパイプラインと、その bind group layout / sampler を構築する。

use super::super::shaders::PRESENT_SHADER;

/// ユニフォームバッファの可視ステージ（頂点・フラグメント両方から参照する）。
const LAYER_UNIFORM_VISIBILITY: wgpu::ShaderStages =
    wgpu::ShaderStages::VERTEX.union(wgpu::ShaderStages::FRAGMENT);

/// テクスチャ提示パイプラインと共用リソース一式。
pub(crate) struct PresentPipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
}

impl PresentPipeline {
    pub(crate) fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        // ─── バインドグループレイアウト ────────────────────────────────────────
        // シェーダの @group(0) @binding(0/1/2) に何をバインドするかを宣言する。
        // 実際のリソース（テクスチャ・サンプラー・バッファ）はまだ紐付けない。
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("altpaint-present-bind-group-layout"),
            entries: &[
                // binding(0): フラグメントシェーダ用の 2D テクスチャ。
                // filterable = true にしないとバイリニアサンプラーと組み合わせられない。
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false, // マルチサンプリング（MSAA）は使わない
                    },
                    count: None, // テクスチャ配列ではない
                },
                // binding(1): フラグメントシェーダ用のサンプラー（Filtering 対応）。
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding(2): 頂点・フラグメント両方から読む小さなユニフォームバッファ。
                // dynamic_offset = false なので常にバッファの先頭を使う。
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: LAYER_UNIFORM_VISIBILITY,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // ─── サンプラー ────────────────────────────────────────────────────────
        // テクスチャを拡大・縮小するときのフィルタリング方法を定義する。
        // Linear = バイリニアフィルタ（滑らかにぼかす）。
        // ClampToEdge = テクスチャ外の UV を端のピクセル色でクランプする。
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("altpaint-present-sampler"),
            mag_filter: wgpu::FilterMode::Linear, // 拡大時: バイリニア
            min_filter: wgpu::FilterMode::Linear, // 縮小時: バイリニア
            mipmap_filter: wgpu::MipmapFilterMode::Linear, // ミップマップ間: バイリニア
            address_mode_u: wgpu::AddressMode::ClampToEdge, // U 方向（横）: 端でクランプ
            address_mode_v: wgpu::AddressMode::ClampToEdge, // V 方向（縦）: 端でクランプ
            ..Default::default()
        });

        // ─── シェーダモジュール ────────────────────────────────────────────────
        // WGSL ソースをコンパイルして GPU 上で実行可能なシェーダオブジェクトを作る。
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("altpaint-present-shader"),
            source: wgpu::ShaderSource::Wgsl(PRESENT_SHADER.into()),
        });

        // ─── パイプラインレイアウト ────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("altpaint-present-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout], // @group(0) に対応
            immediate_size: 0,                          // プッシュ定数は使わない
        });

        // ─── レンダーパイプライン ──────────────────────────────────────────────
        // 頂点シェーダ → ラスタライズ → フラグメントシェーダ の処理チェーンを定義する。
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("altpaint-present-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"), // WGSL 内の @vertex fn 名
                buffers: &[],                 // 頂点バッファなし（vertex_index だけで座標生成）
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"), // WGSL 内の @fragment fn 名
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    // ALPHA_BLENDING: src.rgb * src.a + dst.rgb * (1 - src.a)
                    // 半透明レイヤーを正しく重ねるために必要。
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL, // RGBA すべて書き込む
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(), // 三角形リスト、カリングなし
            depth_stencil: None,                        // 深度バッファは使わない（2D なので不要）
            multisample: wgpu::MultisampleState::default(), // MSAA なし（1 サンプル）
            multiview_mask: None,                       // VR 多視点レンダリングは不使用
            cache: None,
        });

        Self {
            pipeline,
            sampler,
            bind_group_layout,
        }
    }
}
