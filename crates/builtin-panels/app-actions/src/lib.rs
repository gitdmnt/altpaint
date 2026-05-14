//! `builtin.app-actions` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    CommandDescriptor,
    dom::{clear_attribute, html_escape, query_selector, set_attribute, set_inner_html},
    runtime::{
        StatePatchBuffer, emit_service, error, event_string, set_state_bool, set_state_string,
        state_bool, state_string, toggle_state,
    },
    services, state,
};

const SHOW_NEW: state::BoolKey = state::bool("show_new");
const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const NEW_WIDTH: state::StringKey = state::string("new_width");
const NEW_HEIGHT: state::StringKey = state::string("new_height");
const SELECTED_TEMPLATE: state::StringKey = state::string("selected_template");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const TEMPLATE_OPTIONS: state::StringKey = state::string("config.template_options");
const DEFAULT_TEMPLATE_SIZE: state::StringKey = state::string("config.default_template_size");
const NEW_SHORTCUT: state::StringKey = state::string("config.new_shortcut");
const SAVE_SHORTCUT: state::StringKey = state::string("config.save_shortcut");
const SAVE_AS_SHORTCUT: state::StringKey = state::string("config.save_as_shortcut");
const OPEN_SHORTCUT: state::StringKey = state::string("config.open_shortcut");

fn parse_dimension(value: &str) -> Result<usize, &'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("width and height are required");
    }
    trimmed
        .parse::<usize>()
        .map_err(|_| "width and height must be positive integers")
}

fn build_new_project_command(width: &str, height: &str) -> Result<CommandDescriptor, &'static str> {
    let width = parse_dimension(width)?;
    let height = parse_dimension(height)?;
    Ok(services::project_io::new_document_sized(width, height))
}

fn apply_template_size(size: &str) -> Result<(), &'static str> {
    let normalized = size.trim();
    let (width, height) = normalized
        .split_once('x')
        .ok_or("template size must be WIDTHxHEIGHT")?;
    let width = parse_dimension(width)?;
    let height = parse_dimension(height)?;
    let mut batch = StatePatchBuffer::new();
    batch.set_string(NEW_WIDTH.as_ref(), width.to_string());
    batch.set_string(NEW_HEIGHT.as_ref(), height.to_string());
    batch.set_string(SELECTED_TEMPLATE.as_ref(), normalized);
    batch.apply();
    Ok(())
}

fn render_dom() {
    let show_new = state_bool(SHOW_NEW);
    let show_shortcuts = state_bool(SHOW_SHORTCUTS);
    let capture_target = state_string(CAPTURE_TARGET);

    set_visible("#new-section", show_new);
    set_visible("#shortcuts-section", show_shortcuts);
    set_visible("#capture-hint", !capture_target.is_empty());

    set_text_node("#new-shortcut", &state_string(NEW_SHORTCUT));
    set_text_node("#save-shortcut", &state_string(SAVE_SHORTCUT));
    set_text_node("#save-as-shortcut", &state_string(SAVE_AS_SHORTCUT));
    set_text_node("#open-shortcut", &state_string(OPEN_SHORTCUT));

    if let Some(input) = query_selector("#app\\.new\\.width") {
        set_attribute(input, "value", &state_string(NEW_WIDTH));
    }
    if let Some(input) = query_selector("#app\\.new\\.height") {
        set_attribute(input, "value", &state_string(NEW_HEIGHT));
    }

    if let Some(select) = query_selector("#app\\.new\\.template") {
        let raw = state_string(TEMPLATE_OPTIONS);
        let selected = state_string(SELECTED_TEMPLATE);
        let mut html = String::new();
        for entry in raw.split('|') {
            if let Some((id, label)) = entry.split_once(':') {
                let mark = if id == selected { " selected" } else { "" };
                html.push_str(&format!(
                    r#"<option value="{}"{}>{}</option>"#,
                    html_escape(id),
                    mark,
                    html_escape(label),
                ));
            }
        }
        set_inner_html(select, &html);
    }

    set_button_active("#app\\.shortcuts", show_shortcuts);
    set_button_active("#app\\.shortcut\\.new", capture_target == "new");
    set_button_active("#app\\.shortcut\\.save", capture_target == "save");
    set_button_active("#app\\.shortcut\\.save_as", capture_target == "save_as");
    set_button_active("#app\\.shortcut\\.open", capture_target == "open");
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

fn set_text_node(selector: &str, text: &str) {
    if let Some(node) = query_selector(selector) {
        set_inner_html(node, &html_escape(text));
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

fn set_capture_target(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
}

fn capture_shortcut(target: &str) {
    set_capture_target(target);
    set_state_bool(SHOW_SHORTCUTS, true);
    render_dom();
}

fn assign_captured_shortcut(target: &str, shortcut: &str) {
    match target {
        "new" => set_state_string(NEW_SHORTCUT, shortcut),
        "save" => set_state_string(SAVE_SHORTCUT, shortcut),
        "save_as" => set_state_string(SAVE_AS_SHORTCUT, shortcut),
        "open" => set_state_string(OPEN_SHORTCUT, shortcut),
        _ => {}
    }
}

fn shortcut_matches(configured: &str, incoming: &str) -> bool {
    !configured.is_empty() && configured.eq_ignore_ascii_case(incoming)
}

#[plugin_sdk::panel_handler]
fn show_new_form() {
    let selected = state_string(SELECTED_TEMPLATE);
    let fallback = state_string(DEFAULT_TEMPLATE_SIZE);
    let template_size = if selected.trim().is_empty() {
        fallback
    } else {
        selected
    };
    let _ = apply_template_size(&template_size);
    set_state_bool(SHOW_NEW, true);
    render_dom();
}

#[plugin_sdk::panel_handler]
fn cancel_forms() {
    set_state_bool(SHOW_NEW, false);
    render_dom();
}

#[plugin_sdk::panel_handler]
fn toggle_shortcuts() {
    toggle_state(SHOW_SHORTCUTS);
    render_dom();
}

#[plugin_sdk::panel_handler]
fn capture_new_shortcut() {
    capture_shortcut("new");
}

#[plugin_sdk::panel_handler]
fn capture_save_shortcut() {
    capture_shortcut("save");
}

#[plugin_sdk::panel_handler]
fn capture_save_as_shortcut() {
    capture_shortcut("save_as");
}

#[plugin_sdk::panel_handler]
fn capture_open_shortcut() {
    capture_shortcut("open");
}

#[plugin_sdk::panel_handler]
fn edit_new_width() {
    let value = event_string("value");
    if !value.is_empty() {
        set_state_string(NEW_WIDTH, &value);
    }
}

#[plugin_sdk::panel_handler]
fn edit_new_height() {
    let value = event_string("value");
    if !value.is_empty() {
        set_state_string(NEW_HEIGHT, &value);
    }
}

#[plugin_sdk::panel_handler]
fn new_project() {
    let width = state_string(NEW_WIDTH);
    let height = state_string(NEW_HEIGHT);
    let Ok(command) = build_new_project_command(&width, &height) else {
        error("width and height must be positive integers");
        return;
    };
    emit_service(&command);
    cancel_forms();
}

#[plugin_sdk::panel_handler]
fn select_template() {
    let value = event_string("value");
    if value.is_empty() {
        return;
    }
    if let Err(message) = apply_template_size(&value) {
        error(message);
    }
    render_dom();
}

#[plugin_sdk::panel_handler]
fn save_project() {
    emit_service(&services::project_io::save_current());
}

#[plugin_sdk::panel_handler]
fn save_project_as() {
    emit_service(&services::project_io::save_as());
}

#[plugin_sdk::panel_handler]
fn load_project() {
    emit_service(&services::project_io::load_dialog());
}

#[plugin_sdk::panel_handler]
fn undo() {
    emit_service(&services::history::undo());
}

#[plugin_sdk::panel_handler]
fn redo() {
    emit_service(&services::history::redo());
}

#[plugin_sdk::panel_handler]
fn keyboard() {
    let shortcut = event_string("shortcut");
    if shortcut.is_empty() {
        return;
    }
    let target = state_string(CAPTURE_TARGET);
    if !target.is_empty() {
        assign_captured_shortcut(&target, &shortcut);
        set_capture_target("");
        render_dom();
        return;
    }
    if shortcut_matches(&state_string(NEW_SHORTCUT), &shortcut) {
        show_new_form();
        return;
    }
    if shortcut_matches(&state_string(SAVE_SHORTCUT), &shortcut) {
        save_project();
        return;
    }
    if shortcut_matches(&state_string(SAVE_AS_SHORTCUT), &shortcut) {
        save_project_as();
        return;
    }
    if shortcut_matches(&state_string(OPEN_SHORTCUT), &shortcut) {
        load_project();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_command_trims_dimensions() {
        let cmd = build_new_project_command(" 320 ", " 240 ").expect("ok");
        assert_eq!(cmd.name, "project_io.new_document_sized");
    }

    #[test]
    fn new_project_command_rejects_missing_dimensions() {
        assert!(build_new_project_command("", "240").is_err());
        assert!(build_new_project_command("320px", "240").is_err());
    }

    #[test]
    fn shortcut_match_is_case_insensitive() {
        assert!(shortcut_matches("Ctrl+S", "ctrl+s"));
        assert!(!shortcut_matches("", "Ctrl+S"));
    }

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        show_new_form();
        cancel_forms();
        toggle_shortcuts();
        select_template();
        edit_new_width();
        edit_new_height();
        capture_new_shortcut();
        capture_save_shortcut();
        capture_save_as_shortcut();
        capture_open_shortcut();
        new_project();
        save_project();
        save_project_as();
        load_project();
        keyboard();
        undo();
        redo();
    }
}
