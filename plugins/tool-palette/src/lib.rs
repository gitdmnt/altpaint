use panel_sdk::{
    CommandDescriptor,
    commands::{self, Tool},
    host,
    runtime::{
        emit_command, event_string, set_state_bool, set_state_i32, set_state_string,
        state_string, toggle_state,
    },
    state,
};

const ACTIVE_TOOL: state::StringKey = state::string("active_tool");
const PEN_NAME: state::StringKey = state::string("pen_name");
const PEN_SIZE: state::IntKey = state::int("pen_size");
const PEN_COUNT: state::IntKey = state::int("pen_count");
const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const BRUSH_SHORTCUT: state::StringKey = state::string("config.brush_shortcut");
const PEN_SHORTCUT: state::StringKey = state::string("config.pen_shortcut");
const ERASER_SHORTCUT: state::StringKey = state::string("config.eraser_shortcut");

fn build_tool_command(tool: Tool) -> CommandDescriptor {
    commands::tool::set_active(tool)
}

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
    set_state_string(ACTIVE_TOOL, host::tool::active_name());
    set_state_string(PEN_NAME, host::tool::pen_name());
    set_state_i32(PEN_SIZE, host::tool::pen_size());
    set_state_i32(PEN_COUNT, host::tool::pen_count());
}

fn capture_shortcut(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
    set_state_bool(SHOW_SHORTCUTS, true);
}

fn shortcut_matches(configured: &str, incoming: &str) -> bool {
    !configured.is_empty() && configured.eq_ignore_ascii_case(incoming)
}

#[panel_sdk::panel_handler]
fn activate_brush() {
    emit_command(&build_tool_command(Tool::Brush));
}

#[panel_sdk::panel_handler]
fn activate_pen() {
    emit_command(&build_tool_command(Tool::Pen));
}

#[panel_sdk::panel_handler]
fn activate_eraser() {
    emit_command(&build_tool_command(Tool::Eraser));
}

#[panel_sdk::panel_handler]
fn previous_pen() {
    emit_command(&commands::tool::select_previous_pen());
}

#[panel_sdk::panel_handler]
fn next_pen() {
    emit_command(&commands::tool::select_next_pen());
}

#[panel_sdk::panel_handler]
fn reload_pens() {
    emit_command(&commands::tool::reload_pen_presets());
}

#[panel_sdk::panel_handler]
fn toggle_shortcuts() {
    toggle_state(SHOW_SHORTCUTS);
}

#[panel_sdk::panel_handler]
fn capture_brush_shortcut() {
    capture_shortcut("brush");
}

#[panel_sdk::panel_handler]
fn capture_pen_shortcut() {
    capture_shortcut("pen");
}

#[panel_sdk::panel_handler]
fn capture_eraser_shortcut() {
    capture_shortcut("eraser");
}

#[panel_sdk::panel_handler]
fn keyboard() {
    let shortcut = event_string("shortcut");
    if shortcut.is_empty() {
        return;
    }

    match state_string(CAPTURE_TARGET).as_str() {
        "brush" => {
            set_state_string(BRUSH_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            return;
        }
        "pen" => {
            set_state_string(PEN_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            return;
        }
        "eraser" => {
            set_state_string(ERASER_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            return;
        }
        _ => {}
    }

    if shortcut_matches(&state_string(BRUSH_SHORTCUT), &shortcut) {
        activate_brush();
        return;
    }
    if shortcut_matches(&state_string(PEN_SHORTCUT), &shortcut) {
        activate_pen();
        return;
    }
    if shortcut_matches(&state_string(ERASER_SHORTCUT), &shortcut) {
        activate_eraser();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_command_embeds_requested_tool_name() {
        let command = build_tool_command(Tool::Eraser);

        assert_eq!(command.name, "tool.set_active");
        assert_eq!(
            command.payload.get("tool").and_then(|value| value.as_str()),
            Some("eraser")
        );
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        activate_brush();
        activate_pen();
        activate_eraser();
        previous_pen();
        next_pen();
        reload_pens();
        toggle_shortcuts();
        capture_brush_shortcut();
        capture_pen_shortcut();
        capture_eraser_shortcut();
        keyboard();
    }

    #[test]
    fn shortcut_match_is_case_insensitive() {
        assert!(shortcut_matches("B", "b"));
    }
}
