use app_core::{KomaBounds, PagePoint, WindowRect};

/// キャンバス上の一時オーバーレイ状態を保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanvasOverlayState {
    pub brush_preview: Option<PagePoint>,
    pub brush_size: Option<u32>,
    pub lasso_points: Vec<PagePoint>,
    pub active_koma_bounds: Option<KomaBounds>,
    pub koma_navigator: Option<KomaNavigatorOverlay>,
    pub panel_creation_preview: Option<KomaBounds>,
    /// アクティブ UI パネルの画面座標矩形。Some のとき枠線を描画する。
    pub active_ui_panel_rect: Option<WindowRect>,
}

/// コマ境界ナビゲータに表示する 1 件分の情報。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KomaNavigatorEntry {
    pub bounds: KomaBounds,
    pub active: bool,
}

/// ページ内コマを俯瞰表示する簡易ナビゲータ情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KomaNavigatorOverlay {
    pub page_width: usize,
    pub page_height: usize,
    pub panels: Vec<KomaNavigatorEntry>,
}
