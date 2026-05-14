//! デスクトップ周辺で共有する定数と軽量ユーティリティを定義する。
//!
//! 起動設定、配色、既定パスのような設定値を一箇所へ集約し、
//! バイナリ側が実行配線だけへ集中できるようにする。

use std::path::PathBuf;
use std::time::Duration;

/// 既定のプロジェクト保存先ファイル名を返す。
pub const DEFAULT_PROJECT_PATH: &str = "altpaint-project.altp.json";
/// ウィンドウタイトルのベース文字列を表す。
pub const WINDOW_TITLE: &str = "altpaint";
/// 起動時の既定ウィンドウ幅を表す。
pub const WINDOW_WIDTH: u32 = 1280;
/// 起動時の既定ウィンドウ高さを表す。
pub const WINDOW_HEIGHT: u32 = 800;
/// サイドバーの基準幅を表す。
pub const SIDEBAR_WIDTH: usize = 280;
/// 各領域で共有する余白量を表す。
pub const WINDOW_PADDING: usize = 8;
/// ヘッダー領域の高さを表す。
pub const HEADER_HEIGHT: usize = 24;
/// フッター領域の高さを表す。
pub const FOOTER_HEIGHT: usize = 24;
/// アプリ全体の背景色を表す。
pub const APP_BACKGROUND: [u8; 4] = [0x18, 0x18, 0x18, 0xff];
/// サイドバー背景色を表す。
pub const SIDEBAR_BACKGROUND: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
/// パネル枠内の背景色を表す。
pub const PANEL_FRAME_BACKGROUND: [u8; 4] = [0x1f, 0x1f, 0x1f, 0xff];
/// パネル枠線色を表す。
pub const PANEL_FRAME_BORDER: [u8; 4] = [0x3f, 0x3f, 0x3f, 0xff];
/// キャンバス表示部の背景色を表す。
pub const CANVAS_BACKGROUND: [u8; 4] = [0x60, 0x60, 0x60, 0xff];
/// キャンバスホスト枠内の背景色を表す。
pub const CANVAS_FRAME_BACKGROUND: [u8; 4] = [0x40, 0x40, 0x40, 0xff];
/// キャンバス枠線色を表す。
pub const CANVAS_FRAME_BORDER: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
/// アクティブ UI パネル枠線の色（水色）。
pub const ACTIVE_UI_PANEL_BORDER: [u8; 4] = [0x42, 0xa5, 0xf5, 0xff];
/// アクティブパネル外側マスク（半透明黒）。
pub const ACTIVE_PANEL_MASK: [u8; 4] = [0x00, 0x00, 0x00, 0x90];
/// アクティブパネル内側 fill（薄い黄色）。
pub const ACTIVE_PANEL_FILL: [u8; 4] = [0xff, 0xc1, 0x07, 0x18];
/// アクティブパネル枠線（黄色）。
pub const ACTIVE_PANEL_BORDER: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// パネル作成プレビューの fill（薄シアン）。
pub const PANEL_PREVIEW_FILL: [u8; 4] = [0x80, 0xde, 0xea, 0x32];
/// パネル作成プレビューの枠線（シアン）。
pub const PANEL_PREVIEW_BORDER: [u8; 4] = [0x80, 0xde, 0xea, 0xff];
/// パネルナビゲータ背景。
pub const PANEL_NAVIGATOR_BACKGROUND: [u8; 4] = [0x10, 0x16, 0x21, 0xdd];
/// パネルナビゲータ枠線。
pub const PANEL_NAVIGATOR_BORDER: [u8; 4] = [0x90, 0xa4, 0xae, 0xff];
/// パネルナビゲータ内のパネル fill。
pub const PANEL_NAVIGATOR_PANEL: [u8; 4] = [0x4f, 0x5b, 0x6d, 0xd0];
/// パネルナビゲータ内のアクティブパネル色（黄色）。
pub const PANEL_NAVIGATOR_ACTIVE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// ブラシプレビュー円リング色。
pub const BRUSH_PREVIEW_RING: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
/// ラッソ選択プレビュー線の色（黄色）。
pub const LASSO_LINE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
/// 主要ラベル用テキスト色を表す。
pub const TEXT_PRIMARY: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
/// 補助情報用テキスト色を表す。
pub const TEXT_SECONDARY: [u8; 4] = [0xd8, 0xd8, 0xd8, 0xff];
/// パフォーマンス表示を集計する時間窓を表す。
pub const PERFORMANCE_SNAPSHOT_WINDOW: Duration = Duration::from_millis(1000);
/// 入力レイテンシの目標値を表す。
pub const INPUT_LATENCY_TARGET_MS: f64 = 10.0;
/// 入力サンプリング周波数の目標値を表す。
pub const INPUT_SAMPLING_TARGET_HZ: f64 = 120.0;
const MAX_DOCUMENT_DIMENSION: usize = 8192;
const MAX_DOCUMENT_PIXELS: usize = 16_777_216;

/// 既定の パネル dir (Phase 10 で `crates/builtin-panels/`) を返す。
pub fn default_panel_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("builtin-panels")
}

/// 既定の ペン dir を返す。
pub fn default_pen_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("pens")
}

/// 既定の ツール dir を返す。
pub fn default_tool_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
}

/// 入力を解析して ドキュメント サイズ に変換する。
///
/// 値を生成できない場合は `None` を返します。
pub fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized
        .split(|ch: char| ch == 'x' || ch.is_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }

    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0
        || height == 0
        || width > MAX_DOCUMENT_DIMENSION
        || height > MAX_DOCUMENT_DIMENSION
        || width.saturating_mul(height) > MAX_DOCUMENT_PIXELS
    {
        return None;
    }

    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 解析 ドキュメント サイズ accepts common formats が期待どおりに動作することを検証する。
    #[test]
    fn parse_document_size_accepts_common_formats() {
        assert_eq!(parse_document_size("64x64"), Some((64, 64)));
        assert_eq!(parse_document_size("2894x4093"), Some((2894, 4093)));
        assert_eq!(parse_document_size("320 240"), Some((320, 240)));
        assert_eq!(parse_document_size("800,600"), Some((800, 600)));
    }

    /// 解析 ドキュメント サイズ rejects invalid dimensions が期待どおりに動作することを検証する。
    #[test]
    fn parse_document_size_rejects_invalid_dimensions() {
        assert_eq!(parse_document_size("0x600"), None);
        assert_eq!(parse_document_size("99999x1"), None);
        assert_eq!(parse_document_size("foo"), None);
    }
}
