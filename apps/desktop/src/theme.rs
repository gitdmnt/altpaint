//! デスクトップの配色・ウィンドウ寸法・表示用閾値を集約する。
//!
//! BL-112 暫定配置。`desktop-support::config` の解体に伴い移設した。最終的な
//! presenter/theme への配置は B7-part2 で行う。

/// ウィンドウタイトルのベース文字列を表す。
pub(crate) const WINDOW_TITLE: &str = "altpaint";
/// 起動時の既定ウィンドウ幅を表す。
pub(crate) const WINDOW_WIDTH: u32 = 1280;
/// 起動時の既定ウィンドウ高さを表す。
pub(crate) const WINDOW_HEIGHT: u32 = 800;
/// 各領域で共有する余白量を表す。
pub(crate) const WINDOW_PADDING: usize = 8;
/// ヘッダー領域の高さを表す。
pub(crate) const HEADER_HEIGHT: usize = 24;
/// フッター領域の高さを表す。
pub(crate) const FOOTER_HEIGHT: usize = 24;

/// アプリ全体の背景色を表す。
pub(crate) const APP_BACKGROUND: [u8; 4] = [0x18, 0x18, 0x18, 0xff];
/// キャンバス表示部の背景色を表す。
pub(crate) const CANVAS_BACKGROUND: [u8; 4] = [0x60, 0x60, 0x60, 0xff];
/// キャンバスホスト枠内の背景色を表す。
pub(crate) const CANVAS_FRAME_BACKGROUND: [u8; 4] = [0x40, 0x40, 0x40, 0xff];
/// キャンバス枠線色を表す。
pub(crate) const CANVAS_FRAME_BORDER: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
/// アクティブ UI パネル枠線の色（水色）。
pub(crate) const ACTIVE_PANEL_BORDER: [u8; 4] = [0x42, 0xa5, 0xf5, 0xff];
/// アクティブコマ外側マスク（半透明黒）。
pub(crate) const ACTIVE_KOMA_MASK: [u8; 4] = [0x00, 0x00, 0x00, 0x90];
/// アクティブコマ内側 fill（薄い黄色）。
pub(crate) const ACTIVE_KOMA_FILL: [u8; 4] = [0xff, 0xc1, 0x07, 0x18];
/// アクティブコマ枠線（黄色）。
pub(crate) const ACTIVE_KOMA_BORDER: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// コマ作成プレビューの fill（薄シアン）。
pub(crate) const KOMA_PREVIEW_FILL: [u8; 4] = [0x80, 0xde, 0xea, 0x32];
/// コマ作成プレビューの枠線（シアン）。
pub(crate) const KOMA_PREVIEW_BORDER: [u8; 4] = [0x80, 0xde, 0xea, 0xff];
/// コマナビゲータ背景。
pub(crate) const KOMA_NAVIGATOR_BACKGROUND: [u8; 4] = [0x10, 0x16, 0x21, 0xdd];
/// コマナビゲータ枠線。
pub(crate) const KOMA_NAVIGATOR_BORDER: [u8; 4] = [0x90, 0xa4, 0xae, 0xff];
/// コマナビゲータ内のコマ fill。
pub(crate) const KOMA_NAVIGATOR_KOMA: [u8; 4] = [0x4f, 0x5b, 0x6d, 0xd0];
/// コマナビゲータ内のアクティブコマ色（黄色）。
pub(crate) const KOMA_NAVIGATOR_ACTIVE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// ブラシプレビュー円リング色。
pub(crate) const BRUSH_PREVIEW_RING: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
/// ラッソ選択プレビュー線の色（黄色）。
pub(crate) const LASSO_LINE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];

/// 入力レイテンシの目標値を表す (プロファイル表示の閾値)。
pub(crate) const INPUT_LATENCY_TARGET_MS: f64 = 10.0;
/// 入力サンプリング周波数の目標値を表す (プロファイル表示の閾値)。
pub(crate) const INPUT_SAMPLING_TARGET_HZ: f64 = 120.0;
