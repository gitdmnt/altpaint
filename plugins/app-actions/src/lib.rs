use panel_sdk::{
    CommandDescriptor, command,
    runtime::{emit_command_descriptor, error, set_state_bool, state_string},
};

fn normalize_dimension(value: &str) -> String {
    value.trim().to_string()
}

fn build_new_project_command(width: &str, height: &str) -> Result<CommandDescriptor, &'static str> {
    let width = normalize_dimension(width);
    let height = normalize_dimension(height);
    if width.is_empty() || height.is_empty() {
        return Err("width and height are required");
    }

    Ok(command("project.new_sized")
        .string("size", format!("{width}x{height}"))
        .build())
}

fn build_project_command(name: &str) -> CommandDescriptor {
    command(name).build()
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_show_new_form() {
    set_state_bool("show_new", true);
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_cancel_forms() {
    set_state_bool("show_new", false);
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_new_project() {
    let width = state_string("new_width");
    let height = state_string("new_height");
    let Ok(command) = build_new_project_command(&width, &height) else {
        error("width and height are required");
        return;
    };

    emit_command_descriptor(&command);
    panel_handle_cancel_forms();
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_save_project() {
    emit_command_descriptor(&build_project_command("project.save"));
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_save_project_as() {
    emit_command_descriptor(&build_project_command("project.save_as"));
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_load_project() {
    emit_command_descriptor(&build_project_command("project.load"));
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
    }

    #[test]
    fn project_command_uses_requested_name() {
        let command = build_project_command("project.save_as");

        assert_eq!(command.name, "project.save_as");
        assert!(command.payload.is_empty());
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        panel_init();
        panel_handle_show_new_form();
        panel_handle_cancel_forms();
        panel_handle_new_project();
        panel_handle_save_project();
        panel_handle_save_project_as();
        panel_handle_load_project();
    }
}
