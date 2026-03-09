use panel_sdk::{
    CommandDescriptor,
    command,
    runtime::{emit_command_descriptor, state_i32},
};

fn format_color(red: i32, green: i32, blue: i32) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        clamp_channel(red),
        clamp_channel(green),
        clamp_channel(blue)
    )
}

fn build_color_command(red: i32, green: i32, blue: i32) -> CommandDescriptor {
    command("tool.set_color")
        .string("color", format_color(red, green, blue))
        .build()
}

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
    emit_command_descriptor(&build_color_command(red, green, blue));
}

fn clamp_channel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_channel_limits_values_into_byte_range() {
        assert_eq!(clamp_channel(-10), 0);
        assert_eq!(clamp_channel(127), 127);
        assert_eq!(clamp_channel(300), 255);
    }

    #[test]
    fn format_color_clamps_each_channel_and_uses_uppercase_hex() {
        assert_eq!(format_color(-1, 16, 999), "#0010FF");
    }

    #[test]
    fn build_color_command_writes_color_payload() {
        let command = build_color_command(12, 34, 56);

        assert_eq!(command.name, "tool.set_color");
        assert_eq!(
            command.payload.get("color").and_then(|value| value.as_str()),
            Some("#0C2238")
        );
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        panel_init();
        panel_handle_set_red(12);
        panel_handle_set_green(34);
        panel_handle_set_blue(56);
    }
}
