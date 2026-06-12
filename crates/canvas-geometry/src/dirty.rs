use crate::PixelRect;

/// `target` へ `rect` を in-place で統合 (union) して蓄積する。
pub fn accumulate_dirty_rect(target: &mut Option<PixelRect>, rect: PixelRect) {
    *target = Some(target.map_or(rect, |existing| existing.union(rect)));
}

pub fn union_optional_rect(left: Option<PixelRect>, right: Option<PixelRect>) -> Option<PixelRect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}
