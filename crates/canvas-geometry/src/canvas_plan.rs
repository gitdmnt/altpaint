use editor_state::CanvasViewTransform;
use geometry::{PageDirtyRect, WindowRect};

use crate::CanvasViewGeometry;

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
