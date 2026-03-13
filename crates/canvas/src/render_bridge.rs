use app_core::PanelBounds;

use crate::CanvasInputState;

/// 現在の パネル 生成 プレビュー 範囲 を返す。
pub fn panel_creation_preview_bounds(
    state: &CanvasInputState,
    page_width: usize,
    page_height: usize,
) -> Option<PanelBounds> {
    let anchor = state.panel_rect_anchor?;
    let current = state.last_position?;
    let left = anchor.x.min(current.x).min(page_width.saturating_sub(1));
    let top = anchor.y.min(current.y).min(page_height.saturating_sub(1));
    let right = anchor.x.max(current.x).min(page_width.saturating_sub(1));
    let bottom = anchor.y.max(current.y).min(page_height.saturating_sub(1));
    let width = right.saturating_sub(left).saturating_add(1);
    let height = bottom.saturating_sub(top).saturating_add(1);
    (width > 0 && height > 0).then_some(PanelBounds {
        x: left,
        y: top,
        width,
        height,
    })
}
