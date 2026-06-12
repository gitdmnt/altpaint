//! デスクトップ周辺で共有する定数と軽量ユーティリティを定義する。
//!
//! 起動設定、配色、既定パスのような設定値を一箇所へ集約し、
//! バイナリ側が実行配線だけへ集中できるようにする。

use std::path::PathBuf;
use std::time::Duration;

/// 既定のプロジェクト保存先ファイル名。
pub const DEFAULT_PROJECT_FILE_NAME: &str = "altpaint-project.altp.json";
/// ウィンドウタイトルのベース文字列を表す。
pub const WINDOW_TITLE: &str = "altpaint";
/// 起動時の既定ウィンドウ幅を表す。
pub const WINDOW_WIDTH: u32 = 1280;
/// 起動時の既定ウィンドウ高さを表す。
pub const WINDOW_HEIGHT: u32 = 800;
/// 各領域で共有する余白量を表す。
pub const WINDOW_PADDING: usize = 8;
/// ヘッダー領域の高さを表す。
pub const HEADER_HEIGHT: usize = 24;
/// フッター領域の高さを表す。
pub const FOOTER_HEIGHT: usize = 24;
/// アプリ全体の背景色を表す。
pub const APP_BACKGROUND: [u8; 4] = [0x18, 0x18, 0x18, 0xff];
/// キャンバス表示部の背景色を表す。
pub const CANVAS_BACKGROUND: [u8; 4] = [0x60, 0x60, 0x60, 0xff];
/// キャンバスホスト枠内の背景色を表す。
pub const CANVAS_FRAME_BACKGROUND: [u8; 4] = [0x40, 0x40, 0x40, 0xff];
/// キャンバス枠線色を表す。
pub const CANVAS_FRAME_BORDER: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
/// アクティブ UI パネル枠線の色（水色）。
pub const ACTIVE_PANEL_BORDER: [u8; 4] = [0x42, 0xa5, 0xf5, 0xff];
/// アクティブコマ外側マスク（半透明黒）。
pub const ACTIVE_KOMA_MASK: [u8; 4] = [0x00, 0x00, 0x00, 0x90];
/// アクティブコマ内側 fill（薄い黄色）。
pub const ACTIVE_KOMA_FILL: [u8; 4] = [0xff, 0xc1, 0x07, 0x18];
/// アクティブコマ枠線（黄色）。
pub const ACTIVE_KOMA_BORDER: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// コマ作成プレビューの fill（薄シアン）。
pub const KOMA_PREVIEW_FILL: [u8; 4] = [0x80, 0xde, 0xea, 0x32];
/// コマ作成プレビューの枠線（シアン）。
pub const KOMA_PREVIEW_BORDER: [u8; 4] = [0x80, 0xde, 0xea, 0xff];
/// コマナビゲータ背景。
pub const KOMA_NAVIGATOR_BACKGROUND: [u8; 4] = [0x10, 0x16, 0x21, 0xdd];
/// コマナビゲータ枠線。
pub const KOMA_NAVIGATOR_BORDER: [u8; 4] = [0x90, 0xa4, 0xae, 0xff];
/// コマナビゲータ内のコマ fill。
pub const KOMA_NAVIGATOR_KOMA: [u8; 4] = [0x4f, 0x5b, 0x6d, 0xd0];
/// コマナビゲータ内のアクティブコマ色（黄色）。
pub const KOMA_NAVIGATOR_ACTIVE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// ブラシプレビュー円リング色。
pub const BRUSH_PREVIEW_RING: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
/// ラッソ選択プレビュー線の色（黄色）。
pub const LASSO_LINE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// パフォーマンス表示を集計する時間窓を表す。
pub const PERFORMANCE_SNAPSHOT_WINDOW: Duration = Duration::from_millis(1000);
/// 入力レイテンシの目標値を表す。
pub const INPUT_LATENCY_TARGET_MS: f64 = 10.0;
/// 入力サンプリング周波数の目標値を表す。
pub const INPUT_SAMPLING_TARGET_HZ: f64 = 120.0;

pub fn builtin_panels_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("builtin-panels")
}

pub fn default_pen_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("pens")
}

pub fn default_tool_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
}

