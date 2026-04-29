use app_core::{CanvasPoint, PanelBounds};

use crate::PixelRect;

/// キャンバス上の一時オーバーレイ状態を保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanvasOverlayState {
    pub brush_preview: Option<CanvasPoint>,
    pub brush_size: Option<u32>,
    pub lasso_points: Vec<CanvasPoint>,
    pub active_panel_bounds: Option<PanelBounds>,
    pub panel_navigator: Option<PanelNavigatorOverlay>,
    pub panel_creation_preview: Option<PanelBounds>,
    /// アクティブ UI パネルの画面座標矩形。Some のとき枠線を描画する。
    pub active_ui_panel_rect: Option<PixelRect>,
}

/// コマ境界ナビゲータに表示する 1 件分の情報。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelNavigatorEntry {
    pub bounds: PanelBounds,
    pub active: bool,
}

/// ページ内コマを俯瞰表示する簡易ナビゲータ情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelNavigatorOverlay {
    pub page_width: usize,
    pub page_height: usize,
    pub panels: Vec<PanelNavigatorEntry>,
}
