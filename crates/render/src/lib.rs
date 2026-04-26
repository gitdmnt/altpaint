//! `render` は CPU 経路で残存する描画ロジックを束ねるクレート。
//!
//! Phase 9E-4 までで text / status / panel ラスタライズ経路は完全削除。
//! 残るのは Phase 9F (`crates/render` 物理削除) までの暫定 CPU 合成 API:
//! - `compose_*` / `blit_*` / `fill_rgba_block` / `scroll_canvas_region`
//! - `RenderFrame` / `RenderContext` （L1/L4 dummy 経路と一部テスト）
//! - `PanelHitKind` / `PanelHitRegion` （ui-shell の hit-test 互換型）

mod compose;
mod panel;

use app_core::Document;

pub use compose::{
    SourceAxisRun, blit_scaled_rgba_region, build_source_axis_runs, compose_panel_host_region,
    compose_ui_panel_frame, compose_ui_panel_region, fill_rgba_block, scroll_canvas_region,
};
pub use panel::{PanelHitKind, PanelHitRegion};

/// 画面へ転送するための最小フレームデータ。
/// フレームバッファとしての役割を果たす。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFrame {
    /// フレームの横幅ピクセル数。
    pub width: usize,
    /// フレームの高さピクセル数。
    pub height: usize,
    /// RGBA8 のピクセル列。
    pub pixels: Vec<u8>,
}

/// キャンバス描画のための最小コンテキスト。
///
/// 将来的にはキャッシュ、カメラ、描画ターゲットなどを保持する。
#[derive(Debug, Default)]
pub struct RenderContext;

impl RenderContext {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn new() -> Self {
        Self
    }

    /// ドキュメント を計算して返す。
    pub fn document<'a>(&self, document: &'a Document) -> &'a Document {
        document
    }

    /// 描画 フレーム に必要な描画内容を組み立てる。
    pub fn render_frame(&self, document: &Document) -> RenderFrame {
        let page = document.active_page().unwrap_or(&document.work.pages[0]);
        let panel = document.active_panel().unwrap_or(&page.panels[0]);
        let mut frame = RenderFrame {
            width: page.width.max(1),
            height: page.height.max(1),
            pixels: vec![255; page.width.max(1) * page.height.max(1) * 4],
        };

        let copy_width = panel
            .bitmap
            .width
            .min(panel.bounds.width)
            .min(frame.width.saturating_sub(panel.bounds.x));
        let copy_height = panel
            .bitmap
            .height
            .min(panel.bounds.height)
            .min(frame.height.saturating_sub(panel.bounds.y));
        for row in 0..copy_height {
            let src_row_start = row * panel.bitmap.width * 4;
            let dst_row_start = ((panel.bounds.y + row) * frame.width + panel.bounds.x) * 4;
            let row_bytes = copy_width * 4;
            frame.pixels[dst_row_start..dst_row_start + row_bytes]
                .copy_from_slice(&panel.bitmap.pixels[src_row_start..src_row_start + row_bytes]);
        }

        frame
    }
}

#[cfg(test)]
mod tests;
