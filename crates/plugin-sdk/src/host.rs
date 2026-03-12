//! host snapshot を型付き getter で読む補助 API を提供する。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSnapshot {
    pub red: i32,
    pub green: i32,
    pub blue: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSnapshot {
    pub active_name: String,
    pub active_id: String,
    pub active_label: String,
    pub provider_plugin_id: String,
    pub drawing_plugin_id: String,
    pub pen_name: String,
    pub pen_id: String,
    pub pen_size: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolCapabilities {
    pub supports_size: bool,
    pub supports_pressure_enabled: bool,
    pub supports_antialias: bool,
    pub supports_stabilization: bool,
}

/// ドキュメント関連 host 値を読む。
pub mod document {
    use crate::runtime::{host_bool, host_i32, host_string};

    /// ドキュメントタイトルを返す。
    pub fn title() -> String {
        host_string("document.title")
    }

    /// ページ数を返す。
    pub fn page_count() -> i32 {
        host_i32("document.page_count")
    }

    /// パネル数を返す。
    pub fn panel_count() -> i32 {
        host_i32("document.panel_count")
    }

    /// 現在のページ番号を返す。
    pub fn active_page_number() -> i32 {
        host_i32("document.active_page_number")
    }

    /// 現在ページ内のコマ数を返す。
    pub fn active_page_panel_count() -> i32 {
        host_i32("document.active_page_panel_count")
    }

    /// 現在のコマ番号を返す。
    pub fn active_panel_number() -> i32 {
        host_i32("document.active_panel_number")
    }

    /// 現在のコマ index を返す。
    pub fn active_panel_index() -> i32 {
        host_i32("document.active_panel_index")
    }

    /// 現在のコマラベルを返す。
    pub fn active_panel_label() -> String {
        host_string("document.active_panel_label")
    }

    /// 現在のコマ矩形を返す。
    pub fn active_panel_bounds() -> String {
        host_string("document.active_panel_bounds")
    }

    /// レイヤー数を返す。
    pub fn layer_count() -> i32 {
        host_i32("document.layer_count")
    }

    /// アクティブレイヤー名を返す。
    pub fn active_layer_name() -> String {
        host_string("document.active_layer_name")
    }

    /// アクティブレイヤー index を返す。
    pub fn active_layer_index() -> i32 {
        host_i32("document.active_layer_index")
    }

    /// アクティブレイヤーのブレンドモード名を返す。
    pub fn active_layer_blend_mode() -> String {
        host_string("document.active_layer_blend_mode")
    }

    /// アクティブレイヤーの可視性を返す。
    pub fn active_layer_visible() -> bool {
        host_bool("document.active_layer_visible")
    }

    /// アクティブレイヤーのマスク状態を返す。
    pub fn active_layer_masked() -> bool {
        host_bool("document.active_layer_masked")
    }

    /// レイヤー JSON を返す。
    pub fn layers_json() -> String {
        host_string("document.layers_json")
    }

    /// コマ一覧 JSON を返す。
    pub fn panels_json() -> String {
        host_string("document.panels_json")
    }
}

/// ツール関連 host 値を読む。
pub mod tool {
    use crate::{
        commands::Tool,
        runtime::{host_i32, host_string},
    };

    use super::{ToolCapabilities, ToolSnapshot};

    /// アクティブツール名を返す。
    pub fn active_name() -> String {
        host_string("tool.active")
    }

    /// アクティブツール ID を返す。
    pub fn active_id() -> String {
        host_string("tool.active_id")
    }

    /// アクティブツールの表示名を返す。
    pub fn active_label() -> String {
        host_string("tool.active_label")
    }

    /// 指定ツールがアクティブなら true を返す。
    pub fn is_active(tool: Tool) -> bool {
        active_name().eq_ignore_ascii_case(tool.as_str())
    }

    /// ペン名を返す。
    pub fn pen_name() -> String {
        host_string("tool.pen_name")
    }

    /// ツールカタログ JSON を返す。
    pub fn catalog_json() -> String {
        host_string("tool.catalog_json")
    }

    /// アクティブツールの設定定義 JSON を返す。
    pub fn active_settings_json() -> String {
        host_string("tool.active_settings_json")
    }

    /// アクティブツールを管轄するプラグイン ID を返す。
    pub fn active_provider_plugin_id() -> String {
        host_string("tool.active_provider_plugin_id")
    }

    /// アクティブツールの描画計算プラグイン ID を返す。
    pub fn active_drawing_plugin_id() -> String {
        host_string("tool.active_drawing_plugin_id")
    }

    /// ペン ID を返す。
    pub fn pen_id() -> String {
        host_string("tool.pen_id")
    }

    /// ペンプリセット一覧 JSON を返す。
    pub fn pen_presets_json() -> String {
        host_string("tool.pen_presets_json")
    }

    /// ペン index を返す。
    pub fn pen_index() -> i32 {
        host_i32("tool.pen_index")
    }

    /// ペン総数を返す。
    pub fn pen_count() -> i32 {
        host_i32("tool.pen_count")
    }

    /// ペンサイズを返す。
    pub fn pen_size() -> i32 {
        host_i32("tool.pen_size")
    }

    /// 筆圧有効状態を返す。
    pub fn pen_pressure_enabled() -> bool {
        crate::runtime::host_bool("tool.pen_pressure_enabled")
    }

    /// アンチエイリアス有効状態を返す。
    pub fn pen_antialias() -> bool {
        crate::runtime::host_bool("tool.pen_antialias")
    }

    /// 手ぶれ補正強さを返す。
    pub fn pen_stabilization() -> i32 {
        host_i32("tool.pen_stabilization")
    }

    /// 現在ツールが `size` 設定を公開するなら true を返す。
    pub fn supports_size() -> bool {
        crate::runtime::host_bool("tool.supports_size")
    }

    /// 現在ツールが `pressure_enabled` 設定を公開するなら true を返す。
    pub fn supports_pressure_enabled() -> bool {
        crate::runtime::host_bool("tool.supports_pressure_enabled")
    }

    /// 現在ツールが `antialias` 設定を公開するなら true を返す。
    pub fn supports_antialias() -> bool {
        crate::runtime::host_bool("tool.supports_antialias")
    }

    /// 現在ツールが `stabilization` 設定を公開するなら true を返す。
    pub fn supports_stabilization() -> bool {
        crate::runtime::host_bool("tool.supports_stabilization")
    }

    /// ツールの主要スナップショットをまとめて返す。
    pub fn snapshot() -> ToolSnapshot {
        ToolSnapshot {
            active_name: active_name(),
            active_id: active_id(),
            active_label: active_label(),
            provider_plugin_id: active_provider_plugin_id(),
            drawing_plugin_id: active_drawing_plugin_id(),
            pen_name: pen_name(),
            pen_id: pen_id(),
            pen_size: pen_size(),
        }
    }

    /// 現在ツールの capability をまとめて返す。
    pub fn capabilities() -> ToolCapabilities {
        ToolCapabilities {
            supports_size: supports_size(),
            supports_pressure_enabled: supports_pressure_enabled(),
            supports_antialias: supports_antialias(),
            supports_stabilization: supports_stabilization(),
        }
    }
}

/// 色関連 host 値を読む。
pub mod color {
    use crate::runtime::{host_i32, host_string};

    use super::ColorSnapshot;

    /// 現在色の 16 進文字列を返す。
    pub fn active_hex() -> String {
        host_string("color.active")
    }

    /// 赤成分を返す。
    pub fn red() -> i32 {
        host_i32("color.red")
    }

    /// 緑成分を返す。
    pub fn green() -> i32 {
        host_i32("color.green")
    }

    /// 青成分を返す。
    pub fn blue() -> i32 {
        host_i32("color.blue")
    }

    /// 現在色の RGB 成分をまとめて返す。
    pub fn active_rgb() -> ColorSnapshot {
        ColorSnapshot {
            red: red(),
            green: green(),
            blue: blue(),
        }
    }
}

/// ビュー関連 host 値を読む。
pub mod view {
    use crate::runtime::{host_bool, host_i32};

    /// 現在ズーム倍率を 1/1000 単位で返す。
    pub fn zoom_milli() -> i32 {
        host_i32("view.zoom_milli")
    }

    /// 現在パン X を返す。
    pub fn pan_x() -> i32 {
        host_i32("view.pan_x")
    }

    /// 現在パン Y を返す。
    pub fn pan_y() -> i32 {
        host_i32("view.pan_y")
    }

    /// 現在の 90 度回転数を返す。
    pub fn quarter_turns() -> i32 {
        host_i32("view.quarter_turns")
    }

    /// 現在の回転角を度単位で返す。
    pub fn rotation_degrees() -> i32 {
        host_i32("view.rotation_degrees")
    }

    /// 左右反転中なら true を返す。
    pub fn flipped_x() -> bool {
        host_bool("view.flip_x")
    }

    /// 上下反転中なら true を返す。
    pub fn flipped_y() -> bool {
        host_bool("view.flip_y")
    }
}

/// ジョブ関連 host 値を読む。
pub mod jobs {
    use crate::runtime::{host_i32, host_string};

    /// 稼働中ジョブ数を返す。
    pub fn active() -> i32 {
        host_i32("jobs.active")
    }

    /// キュー中ジョブ数を返す。
    pub fn queued() -> i32 {
        host_i32("jobs.queued")
    }

    /// ジョブ状態文字列を返す。
    pub fn status() -> String {
        host_string("jobs.status")
    }
}

/// スナップショット関連 host 値を読む。
pub mod snapshot {
    use crate::runtime::host_string;

    /// 保存状態文字列を返す。
    pub fn storage_status() -> String {
        host_string("snapshot.storage_status")
    }
}
