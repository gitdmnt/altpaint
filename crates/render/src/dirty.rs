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
    /// Base を更新し、必要な dirty 状態も記録する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn mark_base(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.base_dirty_rect, rect);
    }

    /// オーバーレイ を更新し、必要な dirty 状態も記録する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn mark_overlay(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.overlay_dirty_rect, rect);
    }

    /// キャンバス 差分 を設定する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn set_canvas_dirty(&mut self, rect: Option<CanvasDirtyRect>) {
        self.canvas_dirty_rect = rect;
    }
}

/// Union 差分 矩形 に必要な差分領域だけを描画または合成する。
///
/// 値を生成できない場合は `None` を返します。
pub fn union_dirty_rect(target: &mut Option<PixelRect>, rect: PixelRect) {
    *target = Some(target.map_or(rect, |existing| existing.union(rect)));
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
pub fn union_optional_rect(left: Option<PixelRect>, right: Option<PixelRect>) -> Option<PixelRect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}
