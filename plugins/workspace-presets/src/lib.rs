use plugin_sdk::{
    runtime::{emit_service, error, event_string, set_state_string, state_string},
    services, state,
};

const SELECTED_WORKSPACE: state::StringKey = state::string("config.selected_workspace");
const SELECTED_WORKSPACE_LABEL: state::StringKey = state::string("config.selected_workspace_label");
const WORKSPACE_OPTIONS: state::StringKey = state::string("config.workspace_options");

fn selected_workspace_id() -> String {
    state_string(SELECTED_WORKSPACE)
}

fn selected_workspace_label() -> String {
    state_string(SELECTED_WORKSPACE_LABEL)
}

fn option_label_for_id(id: &str) -> Option<String> {
    state_string(WORKSPACE_OPTIONS)
        .split('|')
        .filter_map(|entry| entry.split_once(':'))
        .find_map(|(candidate_id, label)| (candidate_id == id).then(|| label.to_string()))
}

fn validate_selection() -> Result<(String, String), &'static str> {
    let preset_id = selected_workspace_id();
    let label = selected_workspace_label();
    if preset_id.trim().is_empty() {
        return Err("workspace preset id is required");
    }
    if label.trim().is_empty() {
        return Err("workspace preset label is required");
    }
    Ok((preset_id, label))
}

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_handler]
fn select_workspace() {
    let value = event_string("value");
    if value.trim().is_empty() {
        return;
    }

    set_state_string(SELECTED_WORKSPACE, &value);
    if let Some(label) = option_label_for_id(&value) {
        set_state_string(SELECTED_WORKSPACE_LABEL, &label);
    }
    emit_service(&services::workspace_io::apply_preset(value.trim()));
}

#[plugin_sdk::panel_handler]
fn edit_workspace_id() {
    let value = event_string("value");
    if value.trim().is_empty() {
        return;
    }

    set_state_string(SELECTED_WORKSPACE, value.trim());
}

#[plugin_sdk::panel_handler]
fn edit_workspace_label() {
    let value = event_string("value");
    if value.trim().is_empty() {
        return;
    }

    set_state_string(SELECTED_WORKSPACE_LABEL, value.trim());
}

#[plugin_sdk::panel_handler]
fn load_workspace() {
    let Ok((preset_id, _)) = validate_selection() else {
        error("workspace preset id is required");
        return;
    };

    emit_service(&services::workspace_io::apply_preset(preset_id));
}

#[plugin_sdk::panel_handler]
fn save_workspace() {
    let Ok((preset_id, label)) = validate_selection() else {
        error("workspace preset id and label are required");
        return;
    };

    emit_service(&services::workspace_io::save_preset(preset_id, label));
}

#[plugin_sdk::panel_handler]
fn export_workspace() {
    let Ok((preset_id, label)) = validate_selection() else {
        error("workspace preset id and label are required");
        return;
    };

    emit_service(&services::workspace_io::export_preset(preset_id, label));
}

#[plugin_sdk::panel_handler]
fn reload_workspaces() {
    emit_service(&services::workspace_io::reload_presets());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_commands_use_expected_names() {
        let save = services::workspace_io::save_preset("review", "Review");
        let export = services::workspace_io::export_preset("review", "Review");

        assert_eq!(save.name, "workspace_io.save_preset");
        assert_eq!(
            save.payload
                .get("preset_id")
                .and_then(|value| value.as_str()),
            Some("review")
        );
        assert_eq!(
            save.payload.get("label").and_then(|value| value.as_str()),
            Some("Review")
        );
        assert_eq!(export.name, "workspace_io.export_preset");
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        select_workspace();
        edit_workspace_id();
        edit_workspace_label();
        load_workspace();
        save_workspace();
        export_workspace();
        reload_workspaces();
    }
}
