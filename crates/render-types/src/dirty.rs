use crate::PixelRect;

/// Union 差分 矩形 に必要な差分領域だけを描画または合成する。
pub fn union_dirty_rect(target: &mut Option<PixelRect>, rect: PixelRect) {
    *target = Some(target.map_or(rect, |existing| existing.union(rect)));
}

/// 入力や種別に応じて処理を振り分ける。
pub fn union_optional_rect(left: Option<PixelRect>, right: Option<PixelRect>) -> Option<PixelRect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}
