//! presenter へ渡すキャンバス表示計画 DTO。
//!
//! `CanvasViewGeometry` を構築するための入力 (host 矩形・ソース寸法・ビュー変換) を
//! まとめ、オーバーレイ quad ビルダがソース座標→表示座標の写像に使う。

use canvas_geometry::CanvasViewGeometry;
use editor_state::CanvasViewTransform;
use geometry::{PageDirtyRect, WindowRect};

/// presenter が扱うキャンバス表示計画を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CanvasPlan {
    pub(crate) host_rect: WindowRect,
    pub(crate) source_width: usize,
    pub(crate) source_height: usize,
    pub(crate) transform: CanvasViewTransform,
}

impl CanvasPlan {
    pub(crate) fn view_geometry(&self) -> Option<CanvasViewGeometry> {
        CanvasViewGeometry::compute(
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

    pub(crate) fn map_dirty_rect(&self, dirty: PageDirtyRect) -> WindowRect {
        self.view_geometry()
            .map(|geometry| geometry.map_canvas_dirty_rect(dirty))
            .unwrap_or(WindowRect {
                x: self.host_rect.x,
                y: self.host_rect.y,
                width: 0,
                height: 0,
            })
    }
}
