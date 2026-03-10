use panel_sdk::{
    CommandDescriptor, commands,
    runtime::{
        StatePatchBuffer, emit_command, error, event_string, set_state_bool, set_state_string,
        state_string, toggle_state,
    },
    state,
};

const SHOW_NEW: state::BoolKey = state::bool("show_new");
const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const NEW_WIDTH: state::StringKey = state::string("new_width");
const NEW_HEIGHT: state::StringKey = state::string("new_height");
const SELECTED_TEMPLATE: state::StringKey = state::string("selected_template");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const DEFAULT_TEMPLATE_SIZE: state::StringKey = state::string("config.default_template_size");
const NEW_SHORTCUT: state::StringKey = state::string("config.new_shortcut");
const SAVE_SHORTCUT: state::StringKey = state::string("config.save_shortcut");
const SAVE_AS_SHORTCUT: state::StringKey = state::string("config.save_as_shortcut");
const OPEN_SHORTCUT: state::StringKey = state::string("config.open_shortcut");
const SELECTED_WORKSPACE: state::StringKey = state::string("config.selected_workspace");

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
    Ok(commands::project::new_sized(width, height))
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

fn current_workspace_selection() -> String {
    state_string(SELECTED_WORKSPACE)
}

#[panel_sdk::panel_init]
fn init() {}

fn set_capture_target(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
}

fn capture_shortcut(target: &str) {
    set_capture_target(target);
    set_state_bool(SHOW_SHORTCUTS, true);
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

#[panel_sdk::panel_handler]
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
}

#[panel_sdk::panel_handler]
fn cancel_forms() {
    set_state_bool(SHOW_NEW, false);
}

#[panel_sdk::panel_handler]
fn toggle_shortcuts() {
    toggle_state(SHOW_SHORTCUTS);
}

#[panel_sdk::panel_handler]
fn capture_new_shortcut() {
    capture_shortcut("new");
}

#[panel_sdk::panel_handler]
fn capture_save_shortcut() {
    capture_shortcut("save");
}

#[panel_sdk::panel_handler]
fn capture_save_as_shortcut() {
    capture_shortcut("save_as");
}

#[panel_sdk::panel_handler]
fn capture_open_shortcut() {
    capture_shortcut("open");
}

#[panel_sdk::panel_handler]
fn new_project() {
    let width = state_string(NEW_WIDTH);
    let height = state_string(NEW_HEIGHT);
    let Ok(command) = build_new_project_command(&width, &height) else {
        error("width and height must be positive integers");
        return;
    };

    emit_command(&command);
    cancel_forms();
}

#[panel_sdk::panel_handler]
fn select_template() {
    let value = event_string("value");
    if value.is_empty() {
        return;
    }

    if let Err(message) = apply_template_size(&value) {
        error(message);
    }
}

#[panel_sdk::panel_handler]
fn select_workspace() {
    let value = event_string("value");
    if value.is_empty() {
        return;
    }

    set_state_string(SELECTED_WORKSPACE, &value);
}

#[panel_sdk::panel_handler]
fn apply_workspace() {
    let preset_id = current_workspace_selection();
    if preset_id.trim().is_empty() {
        error("workspace preset is required");
        return;
    }

    emit_command(&commands::workspace::apply_preset(preset_id));
}

#[panel_sdk::panel_handler]
fn reload_workspaces() {
    emit_command(&commands::workspace::reload_presets());
}

#[panel_sdk::panel_handler]
fn save_project() {
    emit_command(&commands::project::save());
}

#[panel_sdk::panel_handler]
fn save_project_as() {
    emit_command(&commands::project::save_as());
}

#[panel_sdk::panel_handler]
fn load_project() {
    emit_command(&commands::project::load());
}

#[panel_sdk::panel_handler]
fn keyboard() {
    let shortcut = event_string("shortcut");
    if shortcut.is_empty() {
        return;
    }

    let target = state_string(CAPTURE_TARGET);
    if !target.is_empty() {
        assign_captured_shortcut(&target, &shortcut);
        set_capture_target("");
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
        let command = build_new_project_command(" 320 ", " 240 ").expect("command should build");

        assert_eq!(command.name, "project.new_sized");
        assert_eq!(
            command.payload.get("size").and_then(|value| value.as_str()),
            Some("320x240")
        );
    }

    #[test]
    fn new_project_command_rejects_missing_dimensions() {
        assert_eq!(
            build_new_project_command("", "240"),
            Err("width and height are required")
        );
        assert_eq!(
            build_new_project_command("320", "   "),
            Err("width and height are required")
        );
        assert_eq!(
            build_new_project_command("320px", "240"),
            Err("width and height must be positive integers")
        );
    }

    #[test]
    fn typed_project_commands_use_expected_names() {
        let command = commands::project::save_as();

        assert_eq!(command.name, "project.save_as");
        assert!(command.payload.is_empty());
    }

    #[test]
    fn typed_workspace_commands_use_expected_names() {
        let reload = commands::workspace::reload_presets();
        let apply = commands::workspace::apply_preset("illustration");

        assert_eq!(reload.name, "workspace.reload_presets");
        assert!(reload.payload.is_empty());
        assert_eq!(apply.name, "workspace.apply_preset");
        assert_eq!(
            apply
                .payload
                .get("preset_id")
                .and_then(|value| value.as_str()),
            Some("illustration")
        );
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        show_new_form();
        cancel_forms();
        toggle_shortcuts();
        select_template();
        select_workspace();
        capture_new_shortcut();
        capture_save_shortcut();
        capture_save_as_shortcut();
        capture_open_shortcut();
        new_project();
        apply_workspace();
        reload_workspaces();
        save_project();
        save_project_as();
        load_project();
        keyboard();
    }

    #[test]
    fn shortcut_match_is_case_insensitive() {
        assert!(shortcut_matches("Ctrl+S", "ctrl+s"));
        assert!(!shortcut_matches("", "Ctrl+S"));
    }

    #[test]
    fn template_size_updates_width_and_height() {
        apply_template_size("2894x4093").expect("template size should parse");
    }
}
