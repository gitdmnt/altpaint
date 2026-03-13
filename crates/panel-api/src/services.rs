//! host 側サービス要求の名前と搬送型を定義する。

use serde_json::{Map, Value};

pub mod names {
    pub const PROJECT_NEW_DOCUMENT: &str = "project_io.new_document";
    pub const PROJECT_NEW_DOCUMENT_SIZED: &str = "project_io.new_document_sized";
    pub const PROJECT_SAVE_CURRENT: &str = "project_io.save_current";
    pub const PROJECT_SAVE_AS: &str = "project_io.save_as";
    pub const PROJECT_SAVE_TO_PATH: &str = "project_io.save_to_path";
    pub const PROJECT_LOAD_DIALOG: &str = "project_io.load_dialog";
    pub const PROJECT_LOAD_FROM_PATH: &str = "project_io.load_from_path";

    pub const WORKSPACE_RELOAD_PRESETS: &str = "workspace_io.reload_presets";
    pub const WORKSPACE_APPLY_PRESET: &str = "workspace_io.apply_preset";
    pub const WORKSPACE_SAVE_PRESET: &str = "workspace_io.save_preset";
    pub const WORKSPACE_EXPORT_PRESET: &str = "workspace_io.export_preset";
    pub const WORKSPACE_EXPORT_PRESET_TO_PATH: &str = "workspace_io.export_preset_to_path";

    pub const TOOL_CATALOG_RELOAD_TOOLS: &str = "tool_catalog.reload_tools";
    pub const TOOL_CATALOG_RELOAD_PEN_PRESETS: &str = "tool_catalog.reload_pen_presets";
    pub const TOOL_CATALOG_IMPORT_PEN_PRESETS: &str = "tool_catalog.import_pen_presets";
    pub const TOOL_CATALOG_IMPORT_PEN_PATH: &str = "tool_catalog.import_pen_path";

    pub const VIEW_SET_ZOOM: &str = "view_service.set_zoom";
    pub const VIEW_SET_PAN: &str = "view_service.set_pan";
    pub const VIEW_SET_ROTATION: &str = "view_service.set_rotation";
    pub const VIEW_FLIP_HORIZONTAL: &str = "view_service.flip_horizontal";
    pub const VIEW_FLIP_VERTICAL: &str = "view_service.flip_vertical";
    pub const VIEW_RESET: &str = "view_service.reset";

    pub const PANEL_NAV_ADD: &str = "panel_nav.add";
    pub const PANEL_NAV_REMOVE: &str = "panel_nav.remove";
    pub const PANEL_NAV_SELECT: &str = "panel_nav.select";
    pub const PANEL_NAV_SELECT_NEXT: &str = "panel_nav.select_next";
    pub const PANEL_NAV_SELECT_PREVIOUS: &str = "panel_nav.select_previous";
    pub const PANEL_NAV_FOCUS_ACTIVE: &str = "panel_nav.focus_active";
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceRequest {
    pub name: String,
    pub payload: Map<String, Value>,
}

impl ServiceRequest {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            payload: Map::new(),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn with_value(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.payload.insert(key.into(), value.into());
        self
    }

    /// string を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn string(&self, key: &str) -> Option<&str> {
        self.payload.get(key).and_then(Value::as_str)
    }

    /// 入力を解析して 入力 に変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn u64(&self, key: &str) -> Option<u64> {
        self.payload.get(key).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
    }

    /// 入力を解析して 入力 に変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn f64(&self, key: &str) -> Option<f64> {
        self.payload.get(key).and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// サービス 要求 collects payload values が期待どおりに動作することを検証する。
    #[test]
    fn service_request_collects_payload_values() {
        let request = ServiceRequest::new(names::PROJECT_SAVE_TO_PATH)
            .with_value("path", json!("demo.altp"))
            .with_value("attempt", json!(1));

        assert_eq!(request.string("path"), Some("demo.altp"));
        assert_eq!(request.u64("attempt"), Some(1));
    }
}
