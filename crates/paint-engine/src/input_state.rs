use app_core::KomaBounds;
use geometry::{PagePoint, PagePointF};

/// キャンバス入力中の最小状態を表す。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasInputState {
    pub is_drawing: bool,
    pub last_position: Option<PagePoint>,
    /// 手ぶれ補正で平滑化したサブピクセル位置 (キャンバス座標)。
    pub last_smoothed_position: Option<PagePointF>,
    pub lasso_points: Vec<PagePoint>,
    pub koma_rect_anchor: Option<PagePoint>,
}

impl CanvasInputState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub fn koma_creation_preview_bounds(
    state: &CanvasInputState,
    page_width: usize,
    page_height: usize,
) -> Option<KomaBounds> {
    let anchor = state.koma_rect_anchor?;
    let current = state.last_position?;
    let left = anchor.x.min(current.x).min(page_width.saturating_sub(1));
    let top = anchor.y.min(current.y).min(page_height.saturating_sub(1));
    let right = anchor.x.max(current.x).min(page_width.saturating_sub(1));
    let bottom = anchor.y.max(current.y).min(page_height.saturating_sub(1));
    let width = right.saturating_sub(left).saturating_add(1);
    let height = bottom.saturating_sub(top).saturating_add(1);
    (width > 0 && height > 0).then_some(KomaBounds {
        x: left,
        y: top,
        width,
        height,
    })
}
