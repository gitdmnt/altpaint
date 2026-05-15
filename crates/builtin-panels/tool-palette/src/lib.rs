//! `builtin.tool-palette` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    CommandDescriptor,
    commands::{self, Tool},
    dom::{clear_attribute, html_escape, query_selector, set_attribute, set_inner_html},
    host,
    runtime::{
        emit_command, emit_service, event_string, set_state_bool, set_state_string,
        state_bool, state_string, toggle_state,
    },
    services, state,
};
use serde_json::Value;
use std::collections::BTreeMap;

const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const PEN_SHORTCUT: state::StringKey = state::string("config.pen_shortcut");
const ERASER_SHORTCUT: state::StringKey = state::string("config.eraser_shortcut");
const BUCKET_SHORTCUT: state::StringKey = state::string("config.bucket_shortcut");
const LASSO_BUCKET_SHORTCUT: state::StringKey = state::string("config.lasso_bucket_shortcut");
const PANEL_RECT_SHORTCUT: state::StringKey = state::string("config.panel_rect_shortcut");
const SIZE_MEMORY: state::StringKey = state::string("config.size_memory");
const LAST_IMPORT_SUMMARY: state::StringKey = state::string("config.last_import_summary");
const LAST_IMPORT_PREVIEW: state::StringKey = state::string("config.last_import_preview");
const LAST_IMPORT_ISSUES: state::StringKey = state::string("config.last_import_issues");

fn build_tool_command(tool: Tool) -> CommandDescriptor {
    commands::tool::set_active(tool)
}

fn build_tool_options(catalog_json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<Vec<Value>>(catalog_json)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let name = entry.get("name")?.as_str()?.to_string();
            Some((id, name))
        })
        .collect()
}

fn render_dom() {
    let active_tool = host::tool::active_name();
    let active_tool_id = host::tool::active_id();
    let active_tool_label = host::tool::active_label();
    let provider_plugin_id = host::tool::active_provider_plugin_id();
    let drawing_plugin_id = host::tool::active_drawing_plugin_id();
    let pen_name = host::tool::pen_name();
    let pen_size = host::tool::pen_size();
    let pen_count = host::tool::pen_count();
    let active_child_tool_label = host::tool::active_child_tool_label();
    let child_tools_json = host::tool::child_tools_json();
    let show_shortcuts = state_bool(SHOW_SHORTCUTS);
    let capture_target = state_string(CAPTURE_TARGET);

    set_text("#active-tool-label", &active_tool_label);
    set_text("#active-tool-id", &active_tool_id);
    set_text("#provider-plugin-id", &provider_plugin_id);
    set_text("#drawing-plugin-id", &drawing_plugin_id);
    set_text("#pen-name", &pen_name);
    set_text("#pen-size", &pen_size.to_string());
    set_text("#pen-count", &pen_count.to_string());
    set_text("#active-child-tool-label", &active_child_tool_label);

    if let Some(select) = query_selector("#tool\\.catalog") {
        let mut html = String::new();
        for (id, name) in build_tool_options(&host::tool::catalog_json()) {
            let mark = if id == active_tool_id { " selected" } else { "" };
            html.push_str(&format!(
                r#"<option value="{}"{}>{}</option>"#,
                html_escape(&id),
                mark,
                html_escape(&name),
            ));
        }
        set_inner_html(select, &html);
    }

    set_visible("#child-tools-section", child_tools_json.trim() != "[]");

    set_button_active("#tool\\.pen", active_tool == "pen");
    set_button_active("#tool\\.eraser", active_tool == "eraser");
    set_button_active("#tool\\.bucket", active_tool == "bucket");
    set_button_active("#tool\\.lasso-bucket", active_tool == "lasso_bucket");
    set_button_active("#tool\\.panel-rect", active_tool == "panel_rect");

    set_visible("#shortcuts-section", show_shortcuts);
    set_button_active("#tool\\.shortcuts", show_shortcuts);
    set_button_active("#tool\\.shortcut\\.pen", capture_target == "pen");
    set_button_active("#tool\\.shortcut\\.eraser", capture_target == "eraser");
    set_visible("#capture-hint", !capture_target.is_empty());

    set_text("#pen-shortcut", &state_string(PEN_SHORTCUT));
    set_text("#eraser-shortcut", &state_string(ERASER_SHORTCUT));
    set_text("#bucket-shortcut", &state_string(BUCKET_SHORTCUT));
    set_text("#lasso-bucket-shortcut", &state_string(LASSO_BUCKET_SHORTCUT));
    set_text("#panel-rect-shortcut", &state_string(PANEL_RECT_SHORTCUT));

    let summary = state_string(LAST_IMPORT_SUMMARY);
    set_visible("#import-section", !summary.is_empty());
    set_text("#import-summary", &summary);
    let preview = state_string(LAST_IMPORT_PREVIEW);
    set_visible("#import-preview-row", !preview.is_empty());
    set_text("#import-preview", &preview);
    let issues = state_string(LAST_IMPORT_ISSUES);
    set_visible("#import-issues-row", !issues.is_empty());
    set_text("#import-issues", &issues);
}

fn set_text(selector: &str, text: &str) {
    if let Some(node) = query_selector(selector) {
        set_inner_html(node, &html_escape(text));
    }
}

fn set_visible(selector: &str, visible: bool) {
    if let Some(node) = query_selector(selector) {
        if visible {
            clear_attribute(node, "hidden");
        } else {
            set_attribute(node, "hidden", "");
        }
    }
}

fn set_button_active(selector: &str, active: bool) {
    if let Some(btn) = query_selector(selector) {
        let cls = if active { "btn active" } else { "btn" };
        set_attribute(btn, "class", cls);
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
fn select_tool() {
    let tool_id = event_string("value");
    if tool_id.trim().is_empty() {
        return;
    }
    emit_command(&commands::tool::select_tool(tool_id.trim()));
}

fn capture_shortcut(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
    set_state_bool(SHOW_SHORTCUTS, true);
    render_dom();
}

fn shortcut_matches(configured: &str, incoming: &str) -> bool {
    !configured.is_empty() && configured.eq_ignore_ascii_case(incoming)
}

fn size_binding_key(tool_name: &str, pen_id: &str) -> Option<String> {
    match tool_name.to_ascii_lowercase().as_str() {
        "pen" | "eraser" => Some(format!("{}:{pen_id}", tool_name.to_ascii_lowercase())),
        _ => None,
    }
}

fn parse_size_memory(serialized: &str) -> BTreeMap<String, u32> {
    serde_json::from_str(serialized).unwrap_or_default()
}

fn serialize_size_memory(memory: &BTreeMap<String, u32>) -> String {
    serde_json::to_string(memory).unwrap_or_else(|_| "{}".to_string())
}

fn host_pen_ids() -> Vec<String> {
    serde_json::from_str::<Vec<Value>>(&host::tool::pen_presets_json())
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect()
}

fn remember_current_size() {
    let Some(key) = size_binding_key(&host::tool::active_name(), &host::tool::pen_id()) else {
        return;
    };
    let mut memory = parse_size_memory(&state_string(SIZE_MEMORY));
    memory.insert(key, host::tool::pen_size().max(1) as u32);
    set_state_string(SIZE_MEMORY, serialize_size_memory(&memory));
}

fn restore_size(tool_name: &str, pen_id: &str) {
    let Some(key) = size_binding_key(tool_name, pen_id) else {
        return;
    };
    let memory = parse_size_memory(&state_string(SIZE_MEMORY));
    if let Some(size) = memory.get(&key).copied() {
        emit_command(&commands::tool::set_size(size.max(1)));
    }
}

fn switch_tool_with_size_restore(tool: Tool) {
    remember_current_size();
    let pen_id = host::tool::pen_id();
    emit_command(&build_tool_command(tool));
    restore_size(tool.as_str(), &pen_id);
}

fn switch_pen_with_size_restore(delta: isize) {
    remember_current_size();
    let pen_ids = host_pen_ids();
    let current_index = host::tool::pen_index().max(0) as usize;
    let target_index =
        (current_index as isize + delta).rem_euclid(pen_ids.len().max(1) as isize) as usize;
    if delta < 0 {
        emit_command(&commands::tool::select_previous_pen());
    } else {
        emit_command(&commands::tool::select_next_pen());
    }
    if let Some(target_pen_id) = pen_ids.get(target_index) {
        restore_size(&host::tool::active_name(), target_pen_id);
    }
}

#[plugin_sdk::panel_handler]
fn activate_pen() {
    switch_tool_with_size_restore(Tool::Pen);
}

#[plugin_sdk::panel_handler]
fn activate_eraser() {
    switch_tool_with_size_restore(Tool::Eraser);
}

#[plugin_sdk::panel_handler]
fn activate_bucket() {
    emit_command(&build_tool_command(Tool::Bucket));
}

#[plugin_sdk::panel_handler]
fn activate_lasso_bucket() {
    emit_command(&build_tool_command(Tool::LassoBucket));
}

#[plugin_sdk::panel_handler]
fn activate_panel_rect() {
    emit_command(&build_tool_command(Tool::PanelRect));
}

#[plugin_sdk::panel_handler]
fn select_child_tool() {
    let child_id = event_string("value");
    if child_id.trim().is_empty() {
        return;
    }
    emit_command(&commands::tool::select_child_tool(child_id.trim()));
}

#[plugin_sdk::panel_handler]
fn previous_pen() {
    switch_pen_with_size_restore(-1);
}

#[plugin_sdk::panel_handler]
fn next_pen() {
    switch_pen_with_size_restore(1);
}

#[plugin_sdk::panel_handler]
fn reload_pens() {
    emit_service(&services::tool_catalog::reload_pen_presets());
}

#[plugin_sdk::panel_handler]
fn import_pens() {
    emit_service(&services::tool_catalog::import_pen_presets());
}

#[plugin_sdk::panel_handler]
fn toggle_shortcuts() {
    toggle_state(SHOW_SHORTCUTS);
    render_dom();
}

#[plugin_sdk::panel_handler]
fn capture_pen_shortcut() {
    capture_shortcut("pen");
}

#[plugin_sdk::panel_handler]
fn capture_eraser_shortcut() {
    capture_shortcut("eraser");
}

#[plugin_sdk::panel_handler]
fn keyboard() {
    let shortcut = event_string("shortcut");
    if shortcut.is_empty() {
        return;
    }
    match state_string(CAPTURE_TARGET).as_str() {
        "pen" => {
            set_state_string(PEN_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            render_dom();
            return;
        }
        "eraser" => {
            set_state_string(ERASER_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            render_dom();
            return;
        }
        _ => {}
    }
    if shortcut_matches(&state_string(PEN_SHORTCUT), &shortcut) {
        activate_pen();
        return;
    }
    if shortcut_matches(&state_string(ERASER_SHORTCUT), &shortcut) {
        activate_eraser();
        return;
    }
    if shortcut_matches(&state_string(BUCKET_SHORTCUT), &shortcut) {
        activate_bucket();
        return;
    }
    if shortcut_matches(&state_string(LASSO_BUCKET_SHORTCUT), &shortcut) {
        activate_lasso_bucket();
        return;
    }
    if shortcut_matches(&state_string(PANEL_RECT_SHORTCUT), &shortcut) {
        activate_panel_rect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_command_embeds_tool_name() {
        let c = build_tool_command(Tool::Eraser);
        assert_eq!(c.name, "tool.set_active");
    }

    #[test]
    fn build_tool_options_parses_catalog() {
        let opts = build_tool_options(r#"[{"id":"a","name":"A"}]"#);
        assert_eq!(opts, vec![("a".to_string(), "A".to_string())]);
    }

    #[test]
    fn size_memory_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert("pen:p".to_string(), 4);
        let s = serialize_size_memory(&m);
        assert_eq!(parse_size_memory(&s), m);
    }

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        select_tool();
        activate_pen();
        activate_eraser();
        activate_bucket();
        activate_lasso_bucket();
        activate_panel_rect();
        select_child_tool();
        previous_pen();
        next_pen();
        reload_pens();
        import_pens();
        toggle_shortcuts();
        capture_pen_shortcut();
        capture_eraser_shortcut();
        keyboard();
    }
}
