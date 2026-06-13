//! `DesktopApp` が `event_loop` の提示経路へ公開する読み取り中心のメソッド境界 (BL-110)。
//!
//! `event_loop` は `DesktopApp` の private フィールドへ直接触らず、本モジュールの
//! メソッドだけを通じて提示に必要なデータを取得する。各メソッドは複数フィールドに
//! またがる借用順序を内部へ閉じ込め、composition root のフィールドを private に保つ。

use std::sync::Arc;

use geometry::WindowRect;
use panel_runtime::PanelPointerInput;

use super::{DesktopApp, GpuCanvasSourceKind};
use crate::present_quads::DesktopLayout;

/// HTML パネル 1 枚の GPU テクスチャと画面上の配置矩形。
pub(crate) struct PanelRenderEntry {
    pub(crate) panel_id: String,
    pub(crate) texture: Arc<wgpu::Texture>,
    pub(crate) screen_rect: WindowRect,
}

/// キャンバスを GPU テクスチャから提示する際のソース指定。
pub(crate) struct CanvasGpuSourceSpec {
    pub(crate) koma_id: String,
    pub(crate) kind: GpuCanvasSourceKind,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// ステータスバー描画結果 (GPU テクスチャと画面下端への配置矩形)。
pub(crate) struct StatusBarRender {
    pub(crate) texture: Arc<wgpu::Texture>,
    pub(crate) screen_rect: WindowRect,
}

impl DesktopApp {
    /// 現在のビュー倍率 (zoom)。
    pub(crate) fn current_zoom(&self) -> f32 {
        self.document.session.view_transform.zoom
    }

    /// 現在のキャンバスレイアウト (確定済みの場合)。
    pub(crate) fn layout(&self) -> Option<&DesktopLayout> {
        self.layout.as_ref()
    }

    /// 現在のドキュメントへの参照 (event_loop テストのビュー状態検証用)。
    #[cfg(test)]
    pub(crate) fn document(&self) -> &document_model::Document {
        &self.document
    }

    /// プロジェクト保存先パスを差し替える (event_loop テストの保存ショートカット検証用)。
    #[cfg(test)]
    pub(crate) fn set_project_path(&mut self, path: std::path::PathBuf) {
        self.paths.project_path = path;
    }

    /// パネルジオメトリを直接注入する (event_loop テストの focus 検証用)。
    #[cfg(test)]
    pub(crate) fn inject_panel_geometry(
        &mut self,
        panel_id: &str,
        full_rect: WindowRect,
        chrome_height: usize,
        hits: Vec<(String, WindowRect)>,
    ) {
        self.panel_workspace
            .update_panel_geometry(panel_id, full_rect, chrome_height, hits);
    }

    /// HTML パネルへポインタ入力を転送する。
    pub(crate) fn forward_panel_input(
        &mut self,
        panel_id: &str,
        input: PanelPointerInput,
    ) -> bool {
        self.panel_runtime.forward_panel_input(panel_id, input)
    }

    /// 全パネルを dirty 化し、次フレームで再描画させる。
    pub(crate) fn mark_all_panels_dirty(&mut self) {
        self.panel_runtime.mark_all_dirty();
    }

    /// パネルランタイムへ共有 GPU コンテキスト (device/queue) を注入する。
    pub(crate) fn install_panel_gpu_context(
        &mut self,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) {
        self.panel_runtime.install_gpu_context(device, queue);
    }

    /// 可視 HTML パネルを GPU 描画し、テクスチャと配置矩形を返す (BL-092)。
    ///
    /// hit / move handle / full rect テーブルは `prepare_present_frame` が CPU 側で
    /// 更新済み。本メソッドは GPU テクスチャの描画と quad 配置のみを担う。
    pub(crate) fn render_visible_panels(
        &mut self,
        viewport_width: u32,
        viewport_height: u32,
        chrome_height: u32,
    ) -> Vec<PanelRenderEntry> {
        let panel_ids: Vec<String> = self
            .panel_runtime
            .panel_ids_with_gpu()
            .into_iter()
            .filter(|id| self.panel_workspace.is_panel_visible(id))
            .collect();
        if panel_ids.is_empty() {
            return Vec::new();
        }
        // viewport は GPU テクスチャの上限としてそのまま渡し、View 側でクランプさせる。
        let sized: Vec<(String, u32, u32)> = panel_ids
            .iter()
            .map(|id| (id.clone(), viewport_width, viewport_height))
            .collect();
        // render_panels は所有テクスチャハンドル (`Arc<wgpu::Texture>`) を返すため、
        // panel_runtime の借用とは独立して保持できる (BL-092: raw pointer + unsafe 撤去)。
        let textures = self.panel_runtime.render_panels(&sized, 1.0, chrome_height);
        // quad の screen rect は hit テーブルと同じ full rect を共有する。
        textures
            .into_iter()
            .map(|rendered| {
                let screen_rect = self
                    .panel_workspace
                    .panel_full_rect(&rendered.panel_id)
                    .unwrap_or(WindowRect {
                        x: 0,
                        y: 0,
                        width: rendered.width as usize,
                        height: rendered.height as usize,
                    });
                PanelRenderEntry {
                    panel_id: rendered.panel_id,
                    texture: rendered.texture,
                    screen_rect,
                }
            })
            .collect()
    }

    /// アクティブコマを GPU テクスチャから提示する際のソース指定を返す。
    ///
    /// GPU ソースが使えない場合 (GPU 非対応・アクティブコマ無し) は `None`。
    pub(crate) fn canvas_gpu_source_spec(&self) -> Option<CanvasGpuSourceSpec> {
        let kind = self.canvas_surface_source_kind()?;
        let koma = self.document.active_koma()?;
        let (width, height) = match kind {
            GpuCanvasSourceKind::Single => koma
                .layers
                .first()
                .map(|l| (l.bitmap.width as u32, l.bitmap.height as u32))?,
            GpuCanvasSourceKind::Composite => (
                koma.composite_cache.width as u32,
                koma.composite_cache.height as u32,
            ),
        };
        Some(CanvasGpuSourceSpec {
            koma_id: koma.id.0.to_string(),
            kind,
            width,
            height,
        })
    }

    /// ステータスバーを HTML パネルとして GPU 描画する (BL-092)。
    ///
    /// スナップショット構築 → `StatusBar::update` → `render_gpu` を 1 メソッドへ
    /// 閉じ込め、`status_bar` / `panel_runtime` の `&mut` 借用の交錯を内部に隠す。
    /// 共有 GPU コンテキスト未初期化なら `None`。
    pub(crate) fn render_status_bar(
        &mut self,
        viewport_width: u32,
        footer_height: u32,
        window_height: u32,
    ) -> Option<StatusBarRender> {
        let snapshot = self.build_status_snapshot();
        self.status_bar.update(&snapshot);
        let surface = self.panel_runtime.html_surface_renderer()?;
        let viewport_w = viewport_width.max(1);
        let outcome = self.status_bar.render_gpu(
            surface.device,
            surface.queue,
            surface.renderer,
            surface.scene_scratch,
            (viewport_w, footer_height),
        );
        let target = outcome.target();
        // 所有ハンドルを複製して保持する (unsafe 不要)。
        let texture = target.texture_handle();
        let target_h = target.height;
        let screen_rect = WindowRect {
            x: 0,
            y: window_height.saturating_sub(target_h) as usize,
            width: target.width as usize,
            height: target_h as usize,
        };
        Some(StatusBarRender {
            texture,
            screen_rect,
        })
    }
}
