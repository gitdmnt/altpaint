use panel_sdk::{command, runtime::emit_command_descriptor};

#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_activate_brush() {
    emit_command_descriptor(&command("tool.set_active").string("tool", "brush").build());
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_activate_eraser() {
    emit_command_descriptor(&command("tool.set_active").string("tool", "eraser").build());
}
