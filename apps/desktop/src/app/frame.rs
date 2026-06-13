//! `event_loop` の RedrawRequested から提示フレームの組み立てを引き取る (BL-115)。
//!
//! `event_loop` は OS イベント正規化と `presenter.render()` の呼び出しだけを担い、
//! `PresentFrame` を構成する全データ (背景/前景/オーバーレイ quad、キャンバスサーフェス、
//! パネル/ステータステクスチャ) の取得・借用順序は本モジュールの `compose_frame` が
//! 内部へ閉じ込める。
//!
//! `PresentFrame<'_>` は借用スライスを保持するため直接返せない。代わりに全データを
//! 所有する `ComposedFrame` を返し、`event_loop` 側で `present_frame()` により
//! 借用ビューを構築して presenter へ渡す。

use frame_profiler::{FrameProfiler, FrameStage};

use super::{DesktopApp, GpuCanvasSourceKind, PANEL_CHROME_HEIGHT};
use crate::present_quads::{CircleQuad, LineQuad, SolidQuad};
use crate::presenter::{
    CanvasSurface, CanvasSurfaceSource, GpuPanelQuad, PresentFrame, TextureSource, UploadRegion,
};

/// ステータスバーの bind group キャッシュキー (panel_quads とキー衝突しない番兵)。
const STATUS_PANEL_ID: &str = "__status__";

/// `compose_frame` が組み立てた提示フレームの所有データ一式。
///
/// `PresentFrame<'_>` は借用スライスを保持するため、所有 Vec / オプション値を
/// 本構造体が保持し、`present_frame()` で借用ビューへ変換する。
pub(crate) struct ComposedFrame {
    background_quads: Vec<SolidQuad>,
    foreground_quads: Vec<SolidQuad>,
    overlay_solid_quads: Vec<SolidQuad>,
    overlay_circle_quads: Vec<CircleQuad>,
    overlay_line_quads: Vec<LineQuad>,
    /// キャンバスサーフェスのデータソース種別と寸法。`present_frame()` で
    /// `CanvasSurface` へ再構成する (borrowed source のため owned 形で保持する)。
    canvas: Option<CanvasSurfaceData>,
    panel_quads: Vec<GpuPanelQuad>,
    status_quad: Option<GpuPanelQuad>,
    /// このフレームでキャンバスが更新されたか (profiler 計測用)。
    pub(crate) canvas_updated: bool,
}

/// キャンバスサーフェスの所有データ。
enum CanvasSurfaceData {
    /// GPU テクスチャを直接 Present するパス。
    Gpu {
        koma_id: String,
        kind: GpuCanvasSourceKind,
        width: u32,
        height: u32,
        quad: canvas_geometry::TextureQuad,
    },
    /// CPU ビットマップから転送するパス。
    Cpu {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        upload_region: Option<UploadRegion>,
        quad: canvas_geometry::TextureQuad,
    },
}

impl ComposedFrame {
    /// 所有データから presenter へ渡す借用ビュー `PresentFrame<'_>` を構築する。
    pub(crate) fn present_frame(&self) -> PresentFrame<'_> {
        let canvas_surface = self.canvas.as_ref().map(|canvas| match canvas {
            CanvasSurfaceData::Gpu {
                koma_id,
                kind,
                width,
                height,
                quad,
            } => CanvasSurface {
                source: match kind {
                    GpuCanvasSourceKind::Single => CanvasSurfaceSource::Gpu {
                        panel_id: koma_id.as_str(),
                        layer_index: 0,
                        width: *width,
                        height: *height,
                    },
                    GpuCanvasSourceKind::Composite => CanvasSurfaceSource::GpuComposite {
                        panel_id: koma_id.as_str(),
                        width: *width,
                        height: *height,
                    },
                },
                upload_region: None,
                quad: *quad,
            },
            CanvasSurfaceData::Cpu {
                width,
                height,
                pixels,
                upload_region,
                quad,
            } => CanvasSurface {
                source: CanvasSurfaceSource::Cpu(TextureSource {
                    width: *width,
                    height: *height,
                    pixels: pixels.as_slice(),
                }),
                upload_region: *upload_region,
                quad: *quad,
            },
        });
        PresentFrame {
            background_quads: &self.background_quads,
            canvas_surface,
            overlay_solid_quads: &self.overlay_solid_quads,
            overlay_circle_quads: &self.overlay_circle_quads,
            overlay_line_quads: &self.overlay_line_quads,
            panel_quads: &self.panel_quads,
            foreground_quads: &self.foreground_quads,
            status_quad: self.status_quad.as_ref(),
        }
    }
}

impl DesktopApp {
    /// 1 フレーム分の `PresentFrame` 構成データを所有形 (`ComposedFrame`) で組み立てる
    /// (BL-115)。
    ///
    /// `prepare_present_frame` による無効化消化、キャンバスサーフェス (GPU/CPU)・
    /// 背景/前景/オーバーレイ quad の生成、HTML パネル/ステータスバーの GPU 描画を
    /// すべて本メソッドへ閉じ込める。各取得の借用順序 (とりわけ `&mut panel_runtime` を
    /// 要する描画と `&self` 参照の交錯) は内部で完結し、`event_loop` は
    /// `present_frame()` を presenter へ渡すだけになる。
    pub(crate) fn compose_frame(
        &mut self,
        window_width: u32,
        window_height: u32,
        footer_height: u32,
        profiler: &mut FrameProfiler,
    ) -> ComposedFrame {
        let prepare_started = std::time::Instant::now();
        let update = self.prepare_present_frame(
            window_width as usize,
            window_height as usize,
            profiler,
        );
        profiler.record_stage(FrameStage::PrepareFrame, prepare_started.elapsed());

        // canvas_texture_quad は &mut self を必要とするため frame 参照の取得より先に呼ぶ。
        let canvas_quad =
            profiler.measure("canvas_texture_quad", || self.canvas_texture_quad());

        // HTML パネル描画も先に処理（&mut panel_runtime を必要とするため、
        // 続く &self 借用と衝突しない順序で実施）。
        let html_quad_entries =
            self.render_visible_panels(window_width, window_height, PANEL_CHROME_HEIGHT);

        let canvas = self.compose_canvas_surface(canvas_quad, update.canvas_dirty_rect);

        let panel_quads: Vec<GpuPanelQuad> = html_quad_entries
            .into_iter()
            .map(|entry| GpuPanelQuad {
                panel_id: entry.panel_id,
                texture: entry.texture,
                screen_rect: entry.screen_rect,
            })
            .collect();

        let background_quads = self.background_solid_quads();
        let foreground_quads = self.foreground_solid_quads();
        let (overlay_solid_quads, overlay_circle_quads, overlay_line_quads) = self.overlay_quads();

        // ステータスバーを HtmlPanelView で GPU 描画する (画面下端、幅 = window 幅)。
        let status_quad = self
            .render_status_bar(window_width, footer_height, window_height)
            .map(|entry| GpuPanelQuad {
                panel_id: STATUS_PANEL_ID.to_string(),
                texture: entry.texture,
                screen_rect: entry.screen_rect,
            });

        ComposedFrame {
            background_quads,
            foreground_quads,
            overlay_solid_quads,
            overlay_circle_quads,
            overlay_line_quads,
            canvas,
            panel_quads,
            status_quad,
            canvas_updated: update.canvas_updated,
        }
    }

    /// キャンバスサーフェスの所有データを組み立てる。
    ///
    /// GPU ソースが使える場合は GPU テクスチャ参照、そうでなければ CPU スナップショットの
    /// ピクセルを複製して保持する (`render_status_bar` の `&mut` 借用と交錯しないよう
    /// owned 形で確定させる)。
    fn compose_canvas_surface(
        &self,
        canvas_quad: Option<canvas_geometry::TextureQuad>,
        canvas_dirty_rect: Option<geometry::PageDirtyRect>,
    ) -> Option<CanvasSurfaceData> {
        let quad = canvas_quad?;
        if let Some(spec) = self.canvas_gpu_source_spec() {
            return Some(CanvasSurfaceData::Gpu {
                koma_id: spec.koma_id,
                kind: spec.kind,
                width: spec.width,
                height: spec.height,
                quad,
            });
        }
        let bitmap = self.cpu_canvas_snapshot()?;
        Some(CanvasSurfaceData::Cpu {
            width: bitmap.width as u32,
            height: bitmap.height as u32,
            pixels: bitmap.pixels.clone(),
            upload_region: canvas_dirty_rect.map(|rect| UploadRegion {
                x: rect.x as u32,
                y: rect.y as u32,
                width: rect.width as u32,
                height: rect.height as u32,
            }),
            quad,
        })
    }
}
