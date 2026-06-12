use app_core::{CanvasViewTransform, PageDirtyRect, WindowRect};

use crate::{CanvasViewGeometry, map_canvas_dirty_to_display_with_transform};

/// `render` が扱うキャンバス表示計画を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasPlan {
    pub host_rect: WindowRect,
    pub source_width: usize,
    pub source_height: usize,
    pub transform: CanvasViewTransform,
}

impl CanvasPlan {
    pub fn view_geometry(&self) -> Option<CanvasViewGeometry> {
        CanvasViewGeometry::compute(
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

    pub fn map_dirty_rect(&self, dirty: PageDirtyRect) -> WindowRect {
        map_canvas_dirty_to_display_with_transform(
            dirty,
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

}
