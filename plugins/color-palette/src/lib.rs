use panel_sdk::{
    command,
    runtime::{emit_command_descriptor, state_i32},
};

#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_set_red(value: i32) {
    let green = state_i32("green");
    let blue = state_i32("blue");
    emit_color(value, green, blue);
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_set_green(value: i32) {
    let red = state_i32("red");
    let blue = state_i32("blue");
    emit_color(red, value, blue);
}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_set_blue(value: i32) {
    let red = state_i32("red");
    let green = state_i32("green");
    emit_color(red, green, value);
}

fn emit_color(red: i32, green: i32, blue: i32) {
    let color = format!(
        "#{:02X}{:02X}{:02X}",
        clamp_channel(red),
        clamp_channel(green),
        clamp_channel(blue)
    );
    emit_command_descriptor(&command("tool.set_color").string("color", color).build());
}

fn clamp_channel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}
