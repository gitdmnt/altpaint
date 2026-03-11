//! パネル persistent config への同期処理を扱う。

use serde_json::{Map, Value, json};

use desktop_support::{
    default_canvas_template_path, default_canvas_templates, load_canvas_templates,
    load_workspace_preset_catalog,
};

use super::{DesktopApp, WORKSPACE_PRESET_PANEL_ID};

impl DesktopApp {
    pub(crate) fn refresh_new_document_templates(&mut self) {
        let templates = load_canvas_templates(default_canvas_template_path());
        let default_template = templates
            .first()
            .cloned()
            .or_else(|| default_canvas_templates().into_iter().next());
        let options = templates
            .iter()
            .map(|template| template.dropdown_option())
            .collect::<Vec<_>>()
            .join("|");

        let mut configs = self.ui_shell.persistent_panel_configs();
        let entry = configs
            .entry("builtin.app-actions".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let object = entry.as_object_mut().expect("config object created");
        object.insert("template_options".to_string(), json!(options));
        object.insert(
            "default_template_size".to_string(),
            json!(
                default_template
                    .as_ref()
                    .map(|template| template.size_string())
                    .unwrap_or_else(|| "2894x4093".to_string())
            ),
        );
        self.ui_shell.set_persistent_panel_configs(configs);
    }

    pub(crate) fn refresh_workspace_presets(&mut self) {
        let options = self
            .workspace_presets
            .presets
            .iter()
            .map(|preset| format!("{}:{}", preset.id, preset.label))
            .collect::<Vec<_>>()
            .join("|");
        let selected_workspace = self.selected_workspace_preset_id();
        let selected_workspace_label = self
            .workspace_presets
            .presets
            .iter()
            .find(|preset| preset.id == selected_workspace)
            .map(|preset| preset.label.clone())
            .unwrap_or_else(|| selected_workspace.clone());

        let mut configs = self.ui_shell.persistent_panel_configs();
        let entry = configs
            .entry(WORKSPACE_PRESET_PANEL_ID.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let object = entry.as_object_mut().expect("config object created");
        object.insert("workspace_options".to_string(), json!(options));
        object.insert(
            "selected_workspace".to_string(),
            json!(selected_workspace.clone()),
        );
        object.insert(
            "selected_workspace_label".to_string(),
            json!(selected_workspace_label),
        );
        self.active_workspace_preset_id = selected_workspace;
        self.ui_shell.set_persistent_panel_configs(configs);
    }

    pub(crate) fn reload_workspace_presets(&mut self) -> bool {
        self.workspace_presets = load_workspace_preset_catalog(&self.io_state.workspace_preset_path);
        self.refresh_workspace_presets();
        self.mark_panel_surface_dirty();
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
        .get(WORKSPACE_PRESET_PANEL_ID)
        .and_then(|config| config.get("selected_workspace"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
