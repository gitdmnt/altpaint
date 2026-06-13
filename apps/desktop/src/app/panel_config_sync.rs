//! パネル persistent config への同期処理を扱う。

use serde_json::{Map, Value, json};

use desktop_support::{
    default_canvas_size_preset_path, default_canvas_size_presets, load_canvas_size_presets,
    load_workspace_preset_catalog,
};
use panel_runtime::services::names::{config_keys, panel_ids};

use super::DesktopApp;

impl DesktopApp {
    pub(crate) fn refresh_new_document_size_presets(&mut self) {
        let presets = load_canvas_size_presets(default_canvas_size_preset_path());
        let default_preset = presets
            .first()
            .cloned()
            .or_else(|| default_canvas_size_presets().into_iter().next());
        // BL-105: "WxH:Label" パイプ区切り文字列 → 構造化 JSON 配列。
        // config には JSON 文字列として格納し、パネル側は serde で配列をパースする。
        let options = presets
            .iter()
            .map(|preset| {
                json!({
                    "size": preset.size_string(),
                    "label": preset.label,
                })
            })
            .collect::<Vec<_>>();
        let options_json = serde_json::to_string(&options).unwrap_or_else(|_| "[]".to_string());
        let default_template_size = default_preset
            .as_ref()
            .map(|preset| preset.size_string())
            .unwrap_or_else(|| "2894x4093".to_string());

        self.update_panel_config(panel_ids::APP_ACTIONS, |object| {
            object.insert(config_keys::TEMPLATE_OPTIONS.to_string(), json!(options_json));
            object.insert(
                config_keys::DEFAULT_TEMPLATE_SIZE.to_string(),
                json!(default_template_size),
            );
        });
    }

    pub(crate) fn refresh_workspace_presets(&mut self) {
        // BL-105: "id:label" パイプ区切り文字列 → 構造化 JSON 配列 (JSON 文字列で格納)。
        let options = self
            .workspace_presets
            .presets
            .iter()
            .map(|preset| {
                json!({
                    "id": preset.id,
                    "label": preset.label,
                })
            })
            .collect::<Vec<_>>();
        let options_json = serde_json::to_string(&options).unwrap_or_else(|_| "[]".to_string());
        let selected_workspace = self.selected_workspace_preset_id();
        let selected_workspace_label = self
            .workspace_presets
            .presets
            .iter()
            .find(|preset| preset.id == selected_workspace)
            .map(|preset| preset.label.clone())
            .unwrap_or_else(|| selected_workspace.clone());

        self.update_panel_config(panel_ids::WORKSPACE_PRESETS, |object| {
            object.insert(config_keys::WORKSPACE_OPTIONS.to_string(), json!(options_json));
            object.insert(
                config_keys::SELECTED_WORKSPACE.to_string(),
                json!(selected_workspace.clone()),
            );
            object.insert(
                config_keys::SELECTED_WORKSPACE_LABEL.to_string(),
                json!(selected_workspace_label),
            );
        });
        self.active_workspace_preset_id = selected_workspace;
    }

    /// 指定パネルの persistent config オブジェクトを編集し、reconcile と永続化まで行う
    /// 共通処理 (BL-101)。
    ///
    /// BL-097: service 経由の config 変更はこの 1 箇所に集約されているため、
    /// 変更箇所自身が `persist_session_state` を担う。これにより呼び出し側 (desktop) の
    /// panel event 経路で config 変化を全パネル map 比較で再検出する必要がなくなる。
    pub(crate) fn update_panel_config(
        &mut self,
        panel_id: &str,
        edit: impl FnOnce(&mut Map<String, Value>),
    ) {
        let mut configs = self.panel_runtime.persistent_panel_configs();
        let entry = configs
            .entry(panel_id.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let object = entry.as_object_mut().expect("config object created");
        edit(object);
        self.panel_runtime.replace_persistent_panel_configs(configs);
        self.panel_workspace
            .reconcile_panels(self.panel_runtime.panel_ids());
        self.persist_session_state();
    }

    pub(crate) fn reload_workspace_presets(&mut self) -> bool {
        let default_catalog = self.default_workspace_preset_catalog();
        self.workspace_presets =
            load_workspace_preset_catalog(&self.io_state.workspace_preset_path, default_catalog);
        self.refresh_workspace_presets();
        self.request_panel_reconcile();
        self.mark_status_dirty();
        self.persist_session_state();
        true
    }

    fn selected_workspace_preset_id(&self) -> String {
        if self
            .workspace_presets
            .presets
            .iter()
            .any(|preset| preset.id == self.active_workspace_preset_id)
        {
            return self.active_workspace_preset_id.clone();
        }

        if self
            .workspace_presets
            .presets
            .iter()
            .any(|preset| preset.id == self.workspace_presets.default_preset_id)
        {
            return self.workspace_presets.default_preset_id.clone();
        }

        self.workspace_presets
            .presets
            .first()
            .map(|preset| preset.id.clone())
            .unwrap_or_default()
    }
}

pub(super) fn selected_workspace_preset_id_from_configs(
    configs: &std::collections::BTreeMap<String, Value>,
) -> Option<String> {
    configs
        .get(panel_ids::WORKSPACE_PRESETS)
        .and_then(|config| config.get(config_keys::SELECTED_WORKSPACE))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
