//! L3 一時オーバーレイの presenter 入力状態 DTO。
//!
//! ブラシプレビュー・ラッソ・アクティブコママスク・コマ作成プレビュー・
//! コマナビゲータなど、毎フレーム再構築される一時描画情報をまとめる。

use document_model::KomaBounds;
use geometry::{PagePoint, WindowRect};

/// キャンバス上の一時オーバーレイ状態を保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CanvasOverlayState {
    pub(crate) brush_preview: Option<PagePoint>,
    pub(crate) brush_size: Option<u32>,
    pub(crate) lasso_points: Vec<PagePoint>,
    pub(crate) active_koma_bounds: Option<KomaBounds>,
    pub(crate) koma_navigator: Option<KomaNavigatorOverlay>,
    pub(crate) panel_creation_preview: Option<KomaBounds>,
    /// アクティブ UI パネルの画面座標矩形。Some のとき枠線を描画する。
    pub(crate) active_ui_panel_rect: Option<WindowRect>,
}

/// コマ境界ナビゲータに表示する 1 件分の情報。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KomaNavigatorEntry {
    pub(crate) bounds: KomaBounds,
    pub(crate) active: bool,
}

/// ページ内コマを俯瞰表示する簡易ナビゲータ情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KomaNavigatorOverlay {
    pub(crate) page_width: usize,
    pub(crate) page_height: usize,
    pub(crate) panels: Vec<KomaNavigatorEntry>,
}
