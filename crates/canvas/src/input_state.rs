use app_core::CanvasPoint;

/// キャンバス入力中の最小状態を表す。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasInputState {
    pub is_drawing: bool,
    pub last_position: Option<CanvasPoint>,
    pub last_smoothed_position: Option<(f32, f32)>,
    pub lasso_points: Vec<CanvasPoint>,
    pub panel_rect_anchor: Option<CanvasPoint>,
}

impl CanvasInputState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
