use panel_sdk::{CommandDescriptor, command, runtime::emit_command_descriptor};

fn build_tool_command(tool: &str) -> CommandDescriptor {
    command("tool.set_active").string("tool", tool).build()
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_activate_brush() {
    emit_command_descriptor(&build_tool_command("brush"));
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_activate_eraser() {
    emit_command_descriptor(&build_tool_command("eraser"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_command_embeds_requested_tool_name() {
        let command = build_tool_command("eraser");

        assert_eq!(command.name, "tool.set_active");
        assert_eq!(
            command.payload.get("tool").and_then(|value| value.as_str()),
            Some("eraser")
        );
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        panel_init();
        panel_handle_activate_brush();
        panel_handle_activate_eraser();
    }
}
