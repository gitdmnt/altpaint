use app_core::{CanvasDirtyRect, CanvasViewTransform};

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
    /// シーン を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn scene(&self) -> Option<CanvasScene> {
        prepare_canvas_scene(
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

    /// 差分 矩形 を別座標系へ変換する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn map_dirty_rect(&self, dirty: CanvasDirtyRect) -> PixelRect {
        map_canvas_dirty_to_display_with_transform(
            dirty,
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

}
