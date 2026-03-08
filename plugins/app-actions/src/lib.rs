use panel_sdk::{command, runtime::emit_command_descriptor};

#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_new_project() {
    emit_command_descriptor(&command("project.new").build());
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_save_project() {
    emit_command_descriptor(&command("project.save").build());
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_load_project() {
    emit_command_descriptor(&command("project.load").build());
}
