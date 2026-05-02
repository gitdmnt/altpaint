//! `UiShell` のパネルレイヤー責務を簡略化したスタブ。
//!
//! Phase 9E-3 で CPU bitmap 経路 (`rebuild_panel_bitmaps` /
//! `compose_panel_surface[_incremental]`) は完全撤去され、Phase 9F で旧 L4
//! `ui_panel_layer` `FrameLayer` も削除された。すべての DSL/HTML パネルは
//! `PanelRuntime::render_panels` で GPU 直描画され、`PresentScene::panel_quads`
//! として合成される。
//!
//! このモジュールに残るのは
//! - `render_panel_surface`: 1×1 dummy `PanelSurface` を返すだけのスタブ (旧テスト互換)
//! - `max_panel_scroll_offset`: 0 を返すだけのスタブ (スクロール経路は Engine 側へ移譲)

use panel_runtime::PanelRuntime;

use super::*;

pub(super) const PANEL_SCROLL_PIXELS_PER_LINE: i32 = 48;

impl PanelPresentation {
    /// GPU 経路に統一されたため、互換用に 1×1 dummy `PanelSurface` を返す。
    /// `PresentScene::panel_quads` 経路ではこの戻り値は描画に使われず、
    /// `panel_surface_*` 系プロファイラ集計のサイズ参照のみ。
    pub fn render_panel_surface(
        &mut self,
        runtime: &PanelRuntime,
        _width: usize,
        _height: usize,
    ) -> PanelSurface {
        self.reconcile_runtime_panels(runtime);
        PanelSurface::from_pixels(0, 0, 1, 1, vec![0; 4])
    }

    /// max パネル スクロール オフセット。9E-3 以降スクロールは Engine 内ハンドル。
    pub(super) fn max_panel_scroll_offset(&self, _viewport_height: usize) -> usize {
        0
    }
}
