//! `builtin.workspace-presets` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    dom::{html_escape, query_selector, set_attribute, set_inner_html},
    runtime::{
        emit_service, error, event_string, set_state_string, state_string,
    },
    services, state,
};

const SELECTED_WORKSPACE: state::StringKey = state::string("config.selected_workspace");
const SELECTED_WORKSPACE_LABEL: state::StringKey =
    state::string("config.selected_workspace_label");
const WORKSPACE_OPTIONS: state::StringKey = state::string("config.workspace_options");

fn selected_workspace_id() -> String {
    state_string(SELECTED_WORKSPACE)
}

fn selected_workspace_label() -> String {
    state_string(SELECTED_WORKSPACE_LABEL)
}

fn parse_options(raw: &str) -> Vec<(String, String)> {
    raw.split('|')
        .filter_map(|entry| entry.split_once(':'))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn option_label_for_id(options: &[(String, String)], id: &str) -> Option<String> {
    options
        .iter()
        .find_map(|(candidate_id, label)| (candidate_id == id).then(|| label.clone()))
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

fn render_dom() {
    let preset_id = selected_workspace_id();
    let preset_label = selected_workspace_label();
    let options = parse_options(&state_string(WORKSPACE_OPTIONS));

    if let Some(select) = query_selector("#workspace\\.preset\\.selector") {
        let mut html = String::new();
        for (id, label) in &options {
            let selected = if id == &preset_id { " selected" } else { "" };
            html.push_str(&format!(
                r#"<option value="{}"{}>{}</option>"#,
                html_escape(id),
                selected,
                html_escape(label),
            ));
        }
        set_inner_html(select, &html);
    }
    if let Some(input) = query_selector("#workspace\\.preset\\.id") {
        set_attribute(input, "value", &preset_id);
    }
    if let Some(input) = query_selector("#workspace\\.preset\\.label") {
        set_attribute(input, "value", &preset_label);
    }
}

#[plugin_sdk::panel_init]
fn init() {
    render_dom();
}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    render_dom();
}

#[plugin_sdk::panel_handler]
fn select_workspace() {
    let value = event_string("value");
    if value.trim().is_empty() {
        return;
    }
    set_state_string(SELECTED_WORKSPACE, &value);
    let options = parse_options(&state_string(WORKSPACE_OPTIONS));
    if let Some(label) = option_label_for_id(&options, &value) {
        set_state_string(SELECTED_WORKSPACE_LABEL, &label);
    }
    render_dom();
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
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        select_workspace();
        edit_workspace_id();
        edit_workspace_label();
        load_workspace();
        save_workspace();
        export_workspace();
        reload_workspaces();
    }

    #[test]
    fn parse_options_handles_empty_and_pairs() {
        assert!(parse_options("").is_empty());
        let v = parse_options("a:A|b:B");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], ("a".to_string(), "A".to_string()));
    }
}
