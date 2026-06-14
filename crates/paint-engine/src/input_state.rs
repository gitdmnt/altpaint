use geometry::{PagePoint, PagePointF};

/// キャンバスのペイント入力中の最小状態を表す。
///
/// ペン/消しゴム/バケツ/投げ縄バケツのペイント系ジェスチャのみを保持する。
/// コマ作成 (KomaRect) のジェスチャ状態は desktop feature が所有する (BL-081)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasInputState {
    pub is_drawing: bool,
    pub last_position: Option<PagePoint>,
    /// 手ぶれ補正で平滑化したサブピクセル位置 (キャンバス座標)。
    pub last_smoothed_position: Option<PagePointF>,
    pub lasso_points: Vec<PagePoint>,
}

impl CanvasInputState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
