//! `UiShell` のパネルレイヤー責務を簡略化したスタブ。
//!
//! Phase 9E-3 で CPU bitmap 経路 (`rebuild_panel_bitmaps` /
//! `compose_panel_surface[_incremental]`) は完全撤去された。すべての DSL/HTML
//! パネルは `PanelRuntime::render_panels` で GPU 直描画される。
//!
//! このモジュールに残るのは
//! - `render_panel_surface`: 1×1 dummy `PanelSurface` を返すだけのスタブ (L4 互換)
//! - `max_panel_scroll_offset`: 0 を返すだけのスタブ (スクロール経路は Engine 側へ移譲)
//!
//! `PanelSurface` 型自体は `interaction.rs` の旧テスト群が参照しているため、
//! 9E-5 のテスト基準値再設定までは残置。GPU 経路では中身が常に空 (1×1 透明) のため
//! 画面に何も寄与せず、L4 ui_panel_layer は実質 dummy。

use panel_runtime::PanelRuntime;

use super::*;

pub(super) const PANEL_SCROLL_PIXELS_PER_LINE: i32 = 48;

impl PanelPresentation {
    /// 9E-3: GPU 経路に統一されたため、L4 ui_panel_layer 用の dummy `PanelSurface` を返す。
    /// 呼び出し側 (apps/desktop/src/app/present.rs) は dummy をそのまま FrameLayer に積むが、
    /// 中身は空 (1×1 透明) なため画面に何も寄与しない。
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
