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

    /// Host snapshot の document / title を 文字列として返す。
    pub fn title() -> String {
        host_string("document.title")
    }

    /// Host snapshot の document / page_count を 整数値として返す。
    pub fn page_count() -> i32 {
        host_i32("document.page_count")
    }

    /// Host snapshot の document / panel_count を 整数値として返す。
    pub fn panel_count() -> i32 {
        host_i32("document.panel_count")
    }

    /// アクティブな ページ number を返す。
    pub fn active_page_number() -> i32 {
        host_i32("document.active_page_number")
    }

    /// アクティブな ページ パネル 件数 を返す。
    pub fn active_page_panel_count() -> i32 {
        host_i32("document.active_page_panel_count")
    }

    /// アクティブな パネル number を返す。
    pub fn active_panel_number() -> i32 {
        host_i32("document.active_panel_number")
    }

    /// アクティブな パネル インデックス を返す。
    pub fn active_panel_index() -> i32 {
        host_i32("document.active_panel_index")
    }

    /// アクティブな パネル ラベル を返す。
    pub fn active_panel_label() -> String {
        host_string("document.active_panel_label")
    }

    /// アクティブな パネル 範囲 を返す。
    pub fn active_panel_bounds() -> String {
        host_string("document.active_panel_bounds")
    }

    /// Host snapshot の document / layer_count を 整数値として返す。
    pub fn layer_count() -> i32 {
        host_i32("document.layer_count")
    }

    /// アクティブな レイヤー 名前 を返す。
    pub fn active_layer_name() -> String {
        host_string("document.active_layer_name")
    }

    /// アクティブな レイヤー インデックス を返す。
    pub fn active_layer_index() -> i32 {
        host_i32("document.active_layer_index")
    }

    /// アクティブな レイヤー ブレンド モード を返す。
    pub fn active_layer_blend_mode() -> String {
        host_string("document.active_layer_blend_mode")
    }

    /// アクティブな レイヤー 表示状態 を返す。
    pub fn active_layer_visible() -> bool {
        host_bool("document.active_layer_visible")
    }

    /// アクティブな レイヤー masked を返す。
    pub fn active_layer_masked() -> bool {
        host_bool("document.active_layer_masked")
    }

    /// Host snapshot の document / layers_json を 文字列として返す。
    pub fn layers_json() -> String {
        host_string("document.layers_json")
    }

    /// Host snapshot の document / panels_json を 文字列として返す。
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

    /// アクティブな 名前 を返す。
    pub fn active_name() -> String {
        host_string("tool.active")
    }

    /// アクティブな ID を返す。
    pub fn active_id() -> String {
        host_string("tool.active_id")
    }

    /// アクティブな ラベル を返す。
    pub fn active_label() -> String {
        host_string("tool.active_label")
    }

    /// Is アクティブ かどうかを返す。
    pub fn is_active(tool: Tool) -> bool {
        active_name().eq_ignore_ascii_case(tool.as_str())
    }

    /// Host snapshot の tool / pen_name を 文字列として返す。
    pub fn pen_name() -> String {
        host_string("tool.pen_name")
    }

    /// Host snapshot の tool / catalog_json を 文字列として返す。
    pub fn catalog_json() -> String {
        host_string("tool.catalog_json")
    }

    /// アクティブな 設定 JSON を返す。
    pub fn active_settings_json() -> String {
        host_string("tool.active_settings_json")
    }

    /// アクティブな provider プラグイン ID を返す。
    pub fn active_provider_plugin_id() -> String {
        host_string("tool.active_provider_plugin_id")
    }

    /// アクティブな 描画 プラグイン ID を返す。
    pub fn active_drawing_plugin_id() -> String {
        host_string("tool.active_drawing_plugin_id")
    }

    /// Host snapshot の tool / pen_id を 文字列として返す。
    pub fn pen_id() -> String {
        host_string("tool.pen_id")
    }

    /// Host snapshot の tool / pen_presets_json を 文字列として返す。
    pub fn pen_presets_json() -> String {
        host_string("tool.pen_presets_json")
    }

    /// Host snapshot の tool / pen_index を 整数値として返す。
    pub fn pen_index() -> i32 {
        host_i32("tool.pen_index")
    }

    /// Host snapshot の tool / pen_count を 整数値として返す。
    pub fn pen_count() -> i32 {
        host_i32("tool.pen_count")
    }

    /// Host snapshot の tool / pen_size を 整数値として返す。
    pub fn pen_size() -> i32 {
        host_i32("tool.pen_size")
    }

    /// Host snapshot の tool / pen_pressure_enabled を 真偽値として返す。
    pub fn pen_pressure_enabled() -> bool {
        crate::runtime::host_bool("tool.pen_pressure_enabled")
    }

    /// Host snapshot の tool / pen_antialias を 真偽値として返す。
    pub fn pen_antialias() -> bool {
        crate::runtime::host_bool("tool.pen_antialias")
    }

    /// Host snapshot の tool / pen_stabilization を 整数値として返す。
    pub fn pen_stabilization() -> i32 {
        host_i32("tool.pen_stabilization")
    }

    /// Host snapshot の tool / supports_size を 真偽値として返す。
    pub fn supports_size() -> bool {
        crate::runtime::host_bool("tool.supports_size")
    }

    /// Host snapshot の tool / supports_pressure_enabled を 真偽値として返す。
    pub fn supports_pressure_enabled() -> bool {
        crate::runtime::host_bool("tool.supports_pressure_enabled")
    }

    /// Host snapshot の tool / supports_antialias を 真偽値として返す。
    pub fn supports_antialias() -> bool {
        crate::runtime::host_bool("tool.supports_antialias")
    }

    /// Host snapshot の tool / supports_stabilization を 真偽値として返す。
    pub fn supports_stabilization() -> bool {
        crate::runtime::host_bool("tool.supports_stabilization")
    }

    /// スナップショット を計算して返す。
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

    /// capabilities を計算して返す。
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

    /// アクティブな 16進文字列 を返す。
    pub fn active_hex() -> String {
        host_string("color.active")
    }

    /// Host snapshot の color / red を 整数値として返す。
    pub fn red() -> i32 {
        host_i32("color.red")
    }

    /// Host snapshot の color / green を 整数値として返す。
    pub fn green() -> i32 {
        host_i32("color.green")
    }

    /// Host snapshot の color / blue を 整数値として返す。
    pub fn blue() -> i32 {
        host_i32("color.blue")
    }

    /// アクティブな RGB を返す。
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

    /// Host snapshot の view / zoom_milli を 整数値として返す。
    pub fn zoom_milli() -> i32 {
        host_i32("view.zoom_milli")
    }

    /// Host snapshot の view / pan_x を 整数値として返す。
    pub fn pan_x() -> i32 {
        host_i32("view.pan_x")
    }

    /// Host snapshot の view / pan_y を 整数値として返す。
    pub fn pan_y() -> i32 {
        host_i32("view.pan_y")
    }

    /// Host snapshot の view / quarter_turns を 整数値として返す。
    pub fn quarter_turns() -> i32 {
        host_i32("view.quarter_turns")
    }

    /// Host snapshot の view / rotation_degrees を 整数値として返す。
    pub fn rotation_degrees() -> i32 {
        host_i32("view.rotation_degrees")
    }

    /// Host snapshot の view / flip_x を 真偽値として返す。
    pub fn flipped_x() -> bool {
        host_bool("view.flip_x")
    }

    /// Host snapshot の view / flip_y を 真偽値として返す。
    pub fn flipped_y() -> bool {
        host_bool("view.flip_y")
    }
}

/// ジョブ関連 host 値を読む。
pub mod jobs {
    use crate::runtime::{host_i32, host_string};

    /// Host snapshot の jobs / active を 整数値として返す。
    pub fn active() -> i32 {
        host_i32("jobs.active")
    }

    /// Host snapshot の jobs / queued を 整数値として返す。
    pub fn queued() -> i32 {
        host_i32("jobs.queued")
    }

    /// Host snapshot の jobs / status を 文字列として返す。
    pub fn status() -> String {
        host_string("jobs.status")
    }
}

/// スナップショット関連 host 値を読む。
pub mod snapshot {
    use crate::runtime::host_string;

    /// Host snapshot の snapshot / storage_status を 文字列として返す。
    pub fn storage_status() -> String {
        host_string("snapshot.storage_status")
    }
}
