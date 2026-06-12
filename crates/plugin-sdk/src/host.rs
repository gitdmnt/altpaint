//! host snapshot を型付き getter で読む補助 API を提供する。

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

    pub fn title() -> String {
        host_string("document.title")
    }

    pub fn page_count() -> i32 {
        host_i32("document.page_count")
    }

    pub fn panel_count() -> i32 {
        host_i32("document.panel_count")
    }

    pub fn active_page_number() -> i32 {
        host_i32("document.active_page_number")
    }

    pub fn active_page_panel_count() -> i32 {
        host_i32("document.active_page_panel_count")
    }

    pub fn active_panel_number() -> i32 {
        host_i32("document.active_panel_number")
    }

    pub fn active_panel_index() -> i32 {
        host_i32("document.active_panel_index")
    }

    pub fn active_panel_bounds() -> String {
        host_string("document.active_panel_bounds")
    }

    pub fn layer_count() -> i32 {
        host_i32("document.layer_count")
    }

    pub fn active_layer_name() -> String {
        host_string("document.active_layer_name")
    }

    pub fn active_layer_index() -> i32 {
        host_i32("document.active_layer_index")
    }

    pub fn active_layer_blend_mode() -> String {
        host_string("document.active_layer_blend_mode")
    }

    pub fn active_layer_visible() -> bool {
        host_bool("document.active_layer_visible")
    }

    pub fn active_layer_masked() -> bool {
        host_bool("document.active_layer_masked")
    }

    pub fn layers_json() -> String {
        host_string("document.layers_json")
    }

    pub fn panels_json() -> String {
        host_string("document.panels_json")
    }
}

/// ツール関連 host 値を読む。
pub mod tool {
    use crate::runtime::{host_i32, host_string};

    use super::{ToolCapabilities, ToolSnapshot};

    pub fn active_name() -> String {
        host_string("tool.active")
    }

    pub fn active_id() -> String {
        host_string("tool.active_id")
    }

    pub fn active_label() -> String {
        host_string("tool.active_label")
    }

    pub fn pen_name() -> String {
        host_string("tool.pen_name")
    }

    pub fn catalog_json() -> String {
        host_string("tool.catalog_json")
    }

    pub fn active_provider_plugin_id() -> String {
        host_string("tool.active_provider_plugin_id")
    }

    pub fn active_drawing_plugin_id() -> String {
        host_string("tool.active_drawing_plugin_id")
    }

    pub fn active_child_tool_label() -> String {
        host_string("tool.active_child_tool_label")
    }

    /// アクティブツールの 子ツール 一覧 JSON を返す。
    pub fn child_tools_json() -> String {
        host_string("tool.child_tools_json")
    }

    pub fn pen_id() -> String {
        host_string("tool.pen_id")
    }

    pub fn pen_presets_json() -> String {
        host_string("tool.pen_presets_json")
    }

    pub fn pen_index() -> i32 {
        host_i32("tool.pen_index")
    }

    pub fn pen_count() -> i32 {
        host_i32("tool.pen_count")
    }

    pub fn pen_size() -> i32 {
        host_i32("tool.pen_size")
    }

    pub fn pen_pressure_enabled() -> bool {
        crate::runtime::host_bool("tool.pen_pressure_enabled")
    }

    pub fn pen_antialias() -> bool {
        crate::runtime::host_bool("tool.pen_antialias")
    }

    pub fn pen_stabilization() -> i32 {
        host_i32("tool.pen_stabilization")
    }

    pub fn supports_size() -> bool {
        crate::runtime::host_bool("tool.supports_size")
    }

    pub fn supports_pressure_enabled() -> bool {
        crate::runtime::host_bool("tool.supports_pressure_enabled")
    }

    pub fn supports_antialias() -> bool {
        crate::runtime::host_bool("tool.supports_antialias")
    }

    pub fn supports_stabilization() -> bool {
        crate::runtime::host_bool("tool.supports_stabilization")
    }

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

    pub fn active_hex() -> String {
        host_string("color.active")
    }

    pub fn red() -> i32 {
        host_i32("color.red")
    }

    pub fn green() -> i32 {
        host_i32("color.green")
    }

    pub fn blue() -> i32 {
        host_i32("color.blue")
    }
}

/// ビュー関連 host 値を読む。
pub mod view {
    use crate::runtime::{host_bool, host_i32};

    pub fn zoom_milli() -> i32 {
        host_i32("view.zoom_milli")
    }

    pub fn pan_x() -> i32 {
        host_i32("view.pan_x")
    }

    pub fn pan_y() -> i32 {
        host_i32("view.pan_y")
    }

    pub fn rotation_degrees() -> i32 {
        host_i32("view.rotation_degrees")
    }

    pub fn flipped_x() -> bool {
        host_bool("view.flip_x")
    }

    pub fn flipped_y() -> bool {
        host_bool("view.flip_y")
    }
}

/// ジョブ関連 host 値を読む。
pub mod jobs {
    use crate::runtime::{host_i32, host_string};

    pub fn active() -> i32 {
        host_i32("jobs.active")
    }

    pub fn queued() -> i32 {
        host_i32("jobs.queued")
    }

    pub fn status() -> String {
        host_string("jobs.status")
    }
}

/// ワークスペース (パネル一覧) 関連 host 値を読む。
pub mod workspace {
    use crate::runtime::host_string;

    /// ワークスペースに登録されたパネル一覧 (id / title / visible) を JSON 文字列として返す。
    ///
    /// 形式: `[{"id":"builtin.xxx","title":"...","visible":true}, ...]`。
    /// builtin.workspace-layout 自身も含めて返すので、利用側でフィルタする。
    pub fn panels_json() -> String {
        host_string("workspace.panels_json")
    }
}

/// スナップショット関連 host 値を読む。
pub mod snapshot {
    use crate::runtime::{host_i32, host_string};

    pub fn storage_status() -> String {
        host_string("snapshot.storage_status")
    }

    pub fn count() -> i32 {
        host_i32("snapshot.count")
    }
}
