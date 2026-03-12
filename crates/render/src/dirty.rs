use app_core::CanvasDirtyRect;

use crate::PixelRect;

/// 差分提示のために各レイヤーの更新領域を集約した結果を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DirtyFramePlan {
    pub base_dirty_rect: Option<PixelRect>,
    pub overlay_dirty_rect: Option<PixelRect>,
    pub canvas_dirty_rect: Option<CanvasDirtyRect>,
    pub canvas_transform_changed: bool,
}

impl DirtyFramePlan {
    /// ベースレイヤーの dirty rect を追加する。
    pub fn mark_base(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.base_dirty_rect, rect);
    }

    /// オーバーレイレイヤーの dirty rect を追加する。
    pub fn mark_overlay(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.overlay_dirty_rect, rect);
    }

    /// キャンバス dirty rect を設定する。
    pub fn set_canvas_dirty(&mut self, rect: Option<CanvasDirtyRect>) {
        self.canvas_dirty_rect = rect;
    }
}

/// dirty rect を既存値へ union して追加する。
pub fn union_dirty_rect(target: &mut Option<PixelRect>, rect: PixelRect) {
    *target = Some(target.map_or(rect, |existing| existing.union(rect)));
}

/// 2 つの optional dirty rect を union する。
pub fn union_optional_rect(left: Option<PixelRect>, right: Option<PixelRect>) -> Option<PixelRect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}
