use app_core::{PageDirtyRect, CanvasViewTransform};

use crate::{
    CanvasScene, PixelRect, map_canvas_dirty_to_display_with_transform, prepare_canvas_scene,
};

/// `render` が扱うキャンバス表示計画を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasPlan {
    pub host_rect: PixelRect,
    pub source_width: usize,
    pub source_height: usize,
    pub transform: CanvasViewTransform,
}

impl CanvasPlan {
    pub fn scene(&self) -> Option<CanvasScene> {
        prepare_canvas_scene(
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

    pub fn map_dirty_rect(&self, dirty: PageDirtyRect) -> PixelRect {
        map_canvas_dirty_to_display_with_transform(
            dirty,
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

}
