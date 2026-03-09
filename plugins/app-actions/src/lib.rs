use panel_sdk::{
    command,
    runtime::{emit_command_descriptor, error, set_state_bool, state_string},
};

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
    let width = state_string("new_width").trim().to_string();
    let height = state_string("new_height").trim().to_string();
    if width.is_empty() || height.is_empty() {
        error("width and height are required");
        return;
    }

    emit_command_descriptor(
        &command("project.new_sized")
            .string("size", format!("{width}x{height}"))
            .build(),
    );
    panel_handle_cancel_forms();
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_save_project() {
    emit_command_descriptor(&command("project.save").build());
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_save_project_as() {
    emit_command_descriptor(&command("project.save_as").build());
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_load_project() {
    emit_command_descriptor(&command("project.load").build());
}
