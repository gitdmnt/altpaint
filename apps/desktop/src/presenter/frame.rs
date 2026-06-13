//! presenter へ渡す 1 フレーム分の入力 DTO 群と `WgpuPresenter::render` 本体。
//!
//! `PresentFrame` は背景・キャンバス・オーバーレイ・パネル・前景・ステータスの quad を
//! レイヤー順にまとめた借用ビュー。CPU ピクセル参照 (`TextureSource`) と GPU テクスチャ
//! ハンドル (`GpuPanelQuad`) の両方を受け取り、`render` が GPU へアップロードして提示する。

use anyhow::{Context, Result};
use std::time::Instant;

use frame_profiler::PresentTimings;

use super::pipelines::{
    QuadLayerRanges, circle_quad_uniform_bytes, line_quad_uniform_bytes, solid_quad_uniform_bytes,
};
use super::shaders::LAYER_UNIFORM_SIZE;
use super::textures::{
    GpuBindGroupCache, GpuBindGroupKind, LayerUploadStats, PanelBindEntry, UploadedLayerTexture,
    quad_uniform_bytes,
};
use super::WgpuPresenter;
use crate::app::CpuCanvasSnapshot;
use crate::present_quads::{SolidQuad, TextureQuad};

/// CPU 側のピクセルデータへの参照を保持する軽量ビュー。
/// GPU へアップロードする直前にこの形で渡す。
#[derive(Debug, Clone, Copy)]
pub struct TextureSource<'a> {
    pub width: u32,
    pub height: u32,
    /// RGBA 各 1 バイト = 1 ピクセル 4 バイトのフラットなバイト列。
    pub pixels: &'a [u8],
}

impl<'a> From<&'a CpuCanvasSnapshot> for TextureSource<'a> {
    fn from(frame: &'a CpuCanvasSnapshot) -> Self {
        Self {
            width: frame.width as u32,
            height: frame.height as u32,
            pixels: frame.pixels.as_slice(),
        }
    }
}

/// GPU へ部分アップロードする矩形領域。
/// dirty rect 最適化のために使う（変化した領域だけを転送して帯域を節約する）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// キャンバスサーフェス (キャンバス表示面テクスチャ) のデータソース。CPU ビットマップか GPU テクスチャかを表す。
#[derive(Debug, Clone, Copy)]
pub enum CanvasSurfaceSource<'a> {
    /// CPU ビットマップから転送する通常パス。
    Cpu(TextureSource<'a>),
    /// 単一レイヤーの GPU テクスチャを直接 Present するパス。
    Gpu {
        panel_id: &'a str,
        layer_index: usize,
        width: u32,
        height: u32,
    },
    /// 多レイヤー合成済み GPU テクスチャ（composite texture）を Present するパス。
    GpuComposite {
        panel_id: &'a str,
        width: u32,
        height: u32,
    },
}

impl<'a> CanvasSurfaceSource<'a> {
    fn width(self) -> u32 {
        match self {
            Self::Cpu(src) => src.width,
            Self::Gpu { width, .. } => width,
            Self::GpuComposite { width, .. } => width,
        }
    }
    fn height(self) -> u32 {
        match self {
            Self::Cpu(src) => src.height,
            Self::Gpu { height, .. } => height,
            Self::GpuComposite { height, .. } => height,
        }
    }
    fn cpu_source(self) -> Option<TextureSource<'a>> {
        match self {
            Self::Cpu(src) => Some(src),
            Self::Gpu { .. } | Self::GpuComposite { .. } => None,
        }
    }
    fn is_gpu(self) -> bool {
        matches!(self, Self::Gpu { .. } | Self::GpuComposite { .. })
    }
}

/// キャンバスサーフェスの転送仕様。
/// `quad` でスクリーン上の描画位置・UV・回転を指定できる。
#[derive(Debug, Clone, Copy)]
pub struct CanvasSurface<'a> {
    pub source: CanvasSurfaceSource<'a>,
    pub upload_region: Option<UploadRegion>,
    /// 描画先矩形・UV 範囲・回転・反転などのジオメトリ情報。
    pub quad: TextureQuad,
}

/// HTML パネル 1 枚分の GPU 描画情報。
/// `texture` は panel-runtime 側が所有する `Rgba8Unorm + STORAGE_BINDING + view_formats=[Rgba8UnormSrgb]`
/// テクスチャの所有ハンドル (`Arc<wgpu::Texture>`)。refcount ハンドルなので複製は
/// 安価で、raw pointer + unsafe なしで present 経路へ受け渡せる (BL-092)。
/// `panel_id` は bind group キャッシュキー。所有 `String` のため `PresentFrame` を
/// 所有データから組み立てる (`app/frame.rs::ComposedFrame`) 際に借用が混ざらない (BL-115)。
/// `screen_rect` は画面ピクセル座標の配置矩形。
#[derive(Debug, Clone)]
pub struct GpuPanelQuad {
    pub panel_id: String,
    pub texture: std::sync::Arc<wgpu::Texture>,
    pub screen_rect: geometry::WindowRect,
}

/// 1 描画フレームに必要な全レイヤーをまとめた quad 集合。
/// レイヤーは以下の順番で上から合成される:
///   L0 background_quads     … 背景 solid quad 群（ウィンドウ背景・キャンバス枠 fill・ホスト枠線）
///   L1 canvas_surface         … キャンバス本体（None なら描画しない）
///   L2a overlay_solid_quads … 一時オーバーレイの AABB 単色矩形（マスク・コマプレビュー・navigator）
///   L2b overlay_circle_quads … ブラシプレビュー円リング（SDF）
///   L2c overlay_line_quads  … ラッソプレビュー線（カプセル SDF）
///   L3 panel_quads          … DSL/HTML 全パネル（vello で GPU 直描画されたテクスチャを quad 合成）
///   L4 foreground_quads     … 前景 solid quad 群（アクティブ UI パネル枠線）
///   L5 status_quad          … ステータスバー (HtmlPanelView GPU 描画) を最前面に配置
#[derive(Debug, Clone)]
pub struct PresentFrame<'a> {
    pub background_quads: &'a [SolidQuad],
    pub canvas_surface: Option<CanvasSurface<'a>>,
    pub overlay_solid_quads: &'a [SolidQuad],
    pub overlay_circle_quads: &'a [crate::present_quads::CircleQuad],
    pub overlay_line_quads: &'a [crate::present_quads::LineQuad],
    pub panel_quads: &'a [GpuPanelQuad],
    pub foreground_quads: &'a [SolidQuad],
    pub status_quad: Option<&'a GpuPanelQuad>,
}

impl WgpuPresenter {
    /// 1 フレーム分を GPU へアップロードして画面に表示する。
    ///
    /// # 内部処理の順序
    /// 1. 各レイヤーのテクスチャが適切なサイズで存在するか確認・再生成
    /// 2. 変更のあったレイヤーを GPU へアップロード
    /// 3. 各レイヤーのユニフォームバッファ（描画位置・UV）を更新
    /// 4. スワップチェーンから次フレーム用テクスチャを取得
    /// 5. レンダーパスを開始して全レイヤーを順番に描画
    /// 6. コマンドを submit して GPU へ投入、present で画面表示
    pub fn render(
        &mut self,
        frame: PresentFrame<'_>,
        layer_texture_store: Option<&gpu_paint::LayerTextureStore>,
    ) -> Result<PresentTimings> {
        // サーフェスが 0 サイズなら描画をスキップ（最小化時など）。
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(PresentTimings::default());
        }

        // ─── ステップ 1: テクスチャの確保 ────────────────────────────────────
        // canvas_surface は省略可能。GPU ソース時は ensure をスキップ（gpu-paint プール管理）。
        if let Some(canvas_surface) = frame
            .canvas_surface
            .filter(|c| c.source.cpu_source().is_some())
        {
            UploadedLayerTexture::ensure(
                &self.device,
                &self.present.sampler,
                &self.present.bind_group_layout,
                &mut self.canvas_surface,
                canvas_surface.source.width(),
                canvas_surface.source.height(),
                "canvas",
            );
        }

        // ─── ステップ 2: CPU→GPU テクスチャアップロード ──────────────────────
        let upload_started = Instant::now();
        let canvas_upload = if let Some(canvas_surface) = frame.canvas_surface {
            if let Some(cpu_src) = canvas_surface.source.cpu_source() {
                UploadedLayerTexture::upload(
                    &self.queue,
                    self.canvas_surface.as_mut(),
                    cpu_src,
                    canvas_surface.upload_region,
                )
            } else {
                LayerUploadStats::default()
            }
        } else {
            LayerUploadStats::default()
        };

        // ─── ステップ 3: ユニフォームバッファ更新 ────────────────────────────
        if let Some(canvas_surface) = frame.canvas_surface {
            if canvas_surface.source.is_gpu() {
                self.update_gpu_canvas_bind_group(
                    canvas_surface.source,
                    canvas_surface.quad,
                    layer_texture_store,
                    self.config.width,
                    self.config.height,
                );
            } else {
                UploadedLayerTexture::update_quad_uniform(
                    &self.queue,
                    self.canvas_surface.as_ref(),
                    canvas_surface.quad,
                    self.config.width,
                    self.config.height,
                );
            }
        }
        let upload = upload_started.elapsed();

        // ─── ステップ 4: スワップチェーンから次フレームテクスチャを取得 ──────
        let surface_texture = match self.surface.get_current_texture() {
            Ok(surface_texture) => surface_texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // サーフェスが無効化されたので再設定してから再取得する。
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .context("failed to acquire surface texture after reconfigure")?
            }
            Err(wgpu::SurfaceError::Timeout) => {
                // タイムアウト: このフレームは表示をスキップして次フレームへ。
                return Ok(PresentTimings {
                    upload,
                    canvas_upload: canvas_upload.duration,
                    canvas_upload_bytes: canvas_upload.bytes,
                    ..Default::default()
                });
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                anyhow::bail!("out of memory while acquiring surface texture")
            }
            Err(other) => return Err(other).context("failed to acquire surface texture"),
        };

        // ─── ステップ 5: コマンドのエンコード ────────────────────────────────
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("altpaint-present-encoder"),
            });

        // ─── HTML パネル: bind_group を準備（render pass 前に &mut self を済ませる） ───
        self.prepare_panel_bind_groups(&frame);

        // solid quad の uniform を準備（背景 + L3 overlay AABB + 前景 を 1 本の Vec に連結）。
        // 連結 Vec 内の各レイヤー区間は range トークン (QuadLayerRanges) で割り当てる。
        let mut combined_solid_quads: Vec<SolidQuad> = Vec::with_capacity(
            frame.background_quads.len()
                + frame.overlay_solid_quads.len()
                + frame.foreground_quads.len(),
        );
        combined_solid_quads.extend_from_slice(frame.background_quads);
        combined_solid_quads.extend_from_slice(frame.overlay_solid_quads);
        combined_solid_quads.extend_from_slice(frame.foreground_quads);
        let (surface_w, surface_h) = (self.config.width, self.config.height);
        self.solid_quad_pipeline
            .prepare(&self.device, &self.queue, &combined_solid_quads, |quad| {
                solid_quad_uniform_bytes(quad, surface_w, surface_h)
            });
        let mut solid_ranges = QuadLayerRanges::default();
        let background_range = solid_ranges.allocate(frame.background_quads.len());
        let overlay_solid_range = solid_ranges.allocate(frame.overlay_solid_quads.len());
        let foreground_range = solid_ranges.allocate(frame.foreground_quads.len());

        // L3 SDF パイプライン (円リング・ラッソ線分) の uniform を準備。
        self.overlay_circle_pipeline.prepare(
            &self.device,
            &self.queue,
            frame.overlay_circle_quads,
            |quad| circle_quad_uniform_bytes(quad, surface_w, surface_h),
        );
        let circle_range = QuadLayerRanges::default().allocate(frame.overlay_circle_quads.len());
        self.overlay_line_pipeline.prepare(
            &self.device,
            &self.queue,
            frame.overlay_line_quads,
            |quad| line_quad_uniform_bytes(quad, surface_w, surface_h),
        );
        let line_range = QuadLayerRanges::default().allocate(frame.overlay_line_quads.len());

        let encode_started = Instant::now();
        {
            // RenderPass を開始する。begin_render_pass の時点で「クリア」が指定される。
            // スコープを抜けると pass が drop されレンダーパスが終了する。
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("altpaint-present-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,          // 描画先テクスチャビュー
                    resolve_target: None, // MSAA 解決先なし
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // Clear: レンダーパス開始時にアプリ背景色でクリアする。
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store, // 描画結果を保持してサーフェスへ
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            // L0: 背景 solid quads（ウィンドウ背景・キャンバス枠 fill・ホスト枠線）
            self.solid_quad_pipeline.record(&mut pass, background_range);

            // レイヤーを下から順番に描画（後に描くほど手前に表示される）。
            pass.set_pipeline(&self.present.pipeline);
            if let Some(canvas_surface) = frame.canvas_surface {
                if canvas_surface.source.is_gpu() {
                    if let Some(cache) = &self.canvas_gpu_bind_group_cache {
                        pass.set_bind_group(0, &cache.bind_group, &[]);
                        pass.draw(0..6, 0..1);
                    }
                } else {
                    UploadedLayerTexture::draw(&mut pass, self.canvas_surface.as_ref());
                }
            }

            // L3a: 一時オーバーレイの AABB 単色矩形（マスク・コマプレビュー・navigator）
            self.solid_quad_pipeline.record(&mut pass, overlay_solid_range);
            // L3b: ブラシプレビュー円リング（SDF）
            self.overlay_circle_pipeline.record(&mut pass, circle_range);
            // L3c: ラッソ線分（カプセル SDF）
            self.overlay_line_pipeline.record(&mut pass, line_range);

            // L3: HTML パネル群（GPU 直描画）
            pass.set_pipeline(&self.present.pipeline);
            for quad in frame.panel_quads {
                if let Some(entry) = self.panel_bind_groups.get(&quad.panel_id) {
                    pass.set_bind_group(0, &entry.bind_group, &[]);
                    pass.draw(0..6, 0..1);
                }
            }

            // L4: 前景 solid quads（アクティブ UI パネル枠線）
            self.solid_quad_pipeline.record(&mut pass, foreground_range);

            // L5: ステータスバー (HtmlPanelView GPU 描画) を最前面に配置
            if let Some(status) = frame.status_quad
                && let Some(entry) = self.panel_bind_groups.get(&status.panel_id)
            {
                pass.set_pipeline(&self.present.pipeline);
                pass.set_bind_group(0, &entry.bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
        } // ← ここで pass が drop され、レンダーパス終了コマンドが記録される

        // ─── ステップ 6: submit → present ────────────────────────────────────
        self.queue.submit([encoder.finish()]);
        let encode_and_submit = encode_started.elapsed();

        let present_started = Instant::now();
        surface_texture.present();

        Ok(PresentTimings {
            upload,
            encode_and_submit,
            present: present_started.elapsed(),
            canvas_upload: canvas_upload.duration,
            canvas_upload_bytes: canvas_upload.bytes,
        })
    }

    /// HTML パネル / ステータスバー quad の bind_group を準備する。
    ///
    /// 各 quad について:
    ///  - 既存 bind_group があり texture サイズ一致 → uniform_buffer のみ更新
    ///  - 異なる or 未登録 → bind_group + uniform_buffer を新規生成
    ///
    /// 不要になった panel_id はキャッシュから drop する。
    fn prepare_panel_bind_groups(&mut self, frame: &PresentFrame<'_>) {
        let (surface_w, surface_h) = (self.config.width, self.config.height);
        // status_quad は panel_quads と同じ bind_group キャッシュ仕組みを共有する。
        let status_iter = frame.status_quad.into_iter();
        for quad in frame.panel_quads.iter().chain(status_iter) {
            let w = quad.texture.width();
            let h = quad.texture.height();
            let needs_rebuild = self
                .panel_bind_groups
                .get(&quad.panel_id)
                .map(|e| e.width != w || e.height != h)
                .unwrap_or(true);
            if needs_rebuild {
                let view = quad.texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("html-panel-quad-view"),
                    format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                    usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
                    ..Default::default()
                });
                let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("html-panel-quad-uniform"),
                    size: LAYER_UNIFORM_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("html-panel-quad-bind-group"),
                    layout: &self.present.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.present.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                    ],
                });
                self.panel_bind_groups.insert(
                    quad.panel_id.clone(),
                    PanelBindEntry {
                        bind_group,
                        uniform_buffer,
                        width: w,
                        height: h,
                    },
                );
            }
            // uniform 更新（位置 + サイズ）
            let entry = self
                .panel_bind_groups
                .get(&quad.panel_id)
                .expect("just inserted");
            let texture_quad = TextureQuad {
                destination: quad.screen_rect,
                uv_min: [0.0, 0.0],
                uv_max: [1.0, 1.0],
                rotation_degrees: 0.0,
                bbox_size: [
                    quad.screen_rect.width as f32,
                    quad.screen_rect.height as f32,
                ],
                flip_x: false,
                flip_y: false,
            };
            self.queue.write_buffer(
                &entry.uniform_buffer,
                0,
                &quad_uniform_bytes(texture_quad, surface_w, surface_h),
            );
        }
        // 既に消えた panel_id をキャッシュから drop（panel-runtime 側のテクスチャ解放と歩調を合わせる）
        let mut live_ids: std::collections::HashSet<&str> =
            frame.panel_quads.iter().map(|q| q.panel_id.as_str()).collect();
        if let Some(status) = frame.status_quad {
            live_ids.insert(status.panel_id.as_str());
        }
        self.panel_bind_groups
            .retain(|id, _| live_ids.contains(id.as_str()));
    }

    /// GPU キャンバステクスチャのバインドグループを作成/更新してキャッシュに保存する。
    ///
    /// `(panel_id, layer_index, width, height)` が前回と変わった場合のみ再生成する。
    fn update_gpu_canvas_bind_group(
        &mut self,
        source: CanvasSurfaceSource<'_>,
        quad: TextureQuad,
        pool: Option<&gpu_paint::LayerTextureStore>,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (panel_id, kind, layer_index, width, height) = match source {
            CanvasSurfaceSource::Gpu {
                panel_id,
                layer_index,
                width,
                height,
            } => (panel_id, GpuBindGroupKind::Single, layer_index, width, height),
            CanvasSurfaceSource::GpuComposite {
                panel_id,
                width,
                height,
            } => (panel_id, GpuBindGroupKind::Composite, usize::MAX, width, height),
            CanvasSurfaceSource::Cpu(_) => return,
        };
        let Some(pool) = pool else {
            return;
        };
        let needs_rebuild = self.canvas_gpu_bind_group_cache.as_ref().is_none_or(|c| {
            c.panel_id != panel_id
                || c.kind != kind
                || c.layer_index != layer_index
                || c.width != width
                || c.height != height
        });
        if needs_rebuild {
            let view = match kind {
                GpuBindGroupKind::Single => pool.get_view(panel_id, layer_index),
                GpuBindGroupKind::Composite => pool.get_composite_view(panel_id),
            };
            let Some(view) = view else {
                return;
            };
            let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("altpaint-canvas-gpu-uniform"),
                size: LAYER_UNIFORM_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("altpaint-canvas-gpu-bind-group"),
                layout: &self.present.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.present.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                ],
            });
            self.canvas_gpu_bind_group_cache = Some(GpuBindGroupCache {
                bind_group,
                uniform_buffer,
                panel_id: panel_id.to_string(),
                kind,
                layer_index,
                width,
                height,
            });
        }
        if let Some(cache) = &self.canvas_gpu_bind_group_cache {
            self.queue.write_buffer(
                &cache.uniform_buffer,
                0,
                &quad_uniform_bytes(quad, surface_width, surface_height),
            );
        }
    }
}
