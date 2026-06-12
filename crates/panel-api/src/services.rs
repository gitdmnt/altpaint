//! host 側サービス要求の名前と搬送型を定義する。

use serde_json::{Map, Value};

/// wire 名定数の互換表面。定義の正本は `panel_schema::names` (BL-036)。
/// panel-api 解体 (B6) までフラット名の再エクスポートとして維持する。
pub mod names {
    pub use panel_schema::names::project_io::{
        LOAD_DIALOG as PROJECT_LOAD_DIALOG, LOAD_FROM_PATH as PROJECT_LOAD_FROM_PATH,
        NEW_DOCUMENT as PROJECT_NEW_DOCUMENT, NEW_DOCUMENT_SIZED as PROJECT_NEW_DOCUMENT_SIZED,
        SAVE_AS as PROJECT_SAVE_AS, SAVE_CURRENT as PROJECT_SAVE_CURRENT,
        SAVE_TO_PATH as PROJECT_SAVE_TO_PATH,
    };

    pub use panel_schema::names::workspace::{
        APPLY_PRESET as WORKSPACE_APPLY_PRESET, EXPORT_PRESET as WORKSPACE_EXPORT_PRESET,
        EXPORT_PRESET_TO_PATH as WORKSPACE_EXPORT_PRESET_TO_PATH,
        RELOAD_PRESETS as WORKSPACE_RELOAD_PRESETS, SAVE_PRESET as WORKSPACE_SAVE_PRESET,
    };

    pub use panel_schema::names::tool::{
        CATALOG_IMPORT_PEN_PATH as TOOL_CATALOG_IMPORT_PEN_PATH,
        CATALOG_IMPORT_PEN_PRESETS as TOOL_CATALOG_IMPORT_PEN_PRESETS,
        CATALOG_RELOAD_PEN_PRESETS as TOOL_CATALOG_RELOAD_PEN_PRESETS,
        CATALOG_RELOAD_TOOLS as TOOL_CATALOG_RELOAD_TOOLS,
    };

    pub use panel_schema::names::view::{
        FLIP_HORIZONTAL as VIEW_FLIP_HORIZONTAL, FLIP_VERTICAL as VIEW_FLIP_VERTICAL,
        RESET as VIEW_RESET, SET_PAN as VIEW_SET_PAN, SET_ROTATION as VIEW_SET_ROTATION,
        SET_ZOOM as VIEW_SET_ZOOM,
    };

    pub use panel_schema::names::panel_nav::{
        ADD as PANEL_NAV_ADD, FOCUS_ACTIVE as PANEL_NAV_FOCUS_ACTIVE,
        REMOVE as PANEL_NAV_REMOVE, SELECT as PANEL_NAV_SELECT,
        SELECT_NEXT as PANEL_NAV_SELECT_NEXT, SELECT_PREVIOUS as PANEL_NAV_SELECT_PREVIOUS,
    };

    pub use panel_schema::names::history::{REDO as HISTORY_REDO, UNDO as HISTORY_UNDO};

    pub use panel_schema::names::snapshot::{
        CREATE as SNAPSHOT_CREATE, RESTORE as SNAPSHOT_RESTORE,
    };

    pub use panel_schema::names::export::IMAGE as EXPORT_IMAGE;

    pub use panel_schema::names::text_render::RENDER_TO_LAYER as TEXT_RENDER_TO_LAYER;

    pub use panel_schema::names::workspace_layout::SET_PANEL_VISIBILITY as WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceRequest {
    pub name: String,
    pub payload: Map<String, Value>,
}

impl ServiceRequest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            payload: Map::new(),
        }
    }

    pub fn with_value(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.payload.insert(key.into(), value.into());
        self
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.payload.get(key).and_then(Value::as_str)
    }

    pub fn u64(&self, key: &str) -> Option<u64> {
        self.payload.get(key).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
    }

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

    #[test]
    fn service_request_collects_payload_values() {
        let request = ServiceRequest::new(names::PROJECT_SAVE_TO_PATH)
            .with_value("path", json!("demo.altp"))
            .with_value("attempt", json!(1));

        assert_eq!(request.string("path"), Some("demo.altp"));
        assert_eq!(request.u64("attempt"), Some(1));
    }
}
