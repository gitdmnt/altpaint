use panel_sdk::{
    CommandDescriptor,
    commands::{self, RgbColor},
    runtime::{emit_command, state_i32},
    state,
};

const RED: state::IntKey = state::int("red");
const GREEN: state::IntKey = state::int("green");
const BLUE: state::IntKey = state::int("blue");

fn format_color(red: i32, green: i32, blue: i32) -> String {
    rgb_color(red, green, blue).to_hex_string()
}

fn build_color_command(red: i32, green: i32, blue: i32) -> CommandDescriptor {
    commands::tool::set_color_hex(format_color(red, green, blue))
}

fn rgb_color(red: i32, green: i32, blue: i32) -> RgbColor {
    RgbColor::new(
        clamp_channel(red),
        clamp_channel(green),
        clamp_channel(blue),
    )
}

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_handler]
fn set_red(value: i32) {
    let green = state_i32(GREEN);
    let blue = state_i32(BLUE);
    emit_color(value, green, blue);
}

#[panel_sdk::panel_handler]
fn set_green(value: i32) {
    let red = state_i32(RED);
    let blue = state_i32(BLUE);
    emit_color(red, value, blue);
}

#[panel_sdk::panel_handler]
fn set_blue(value: i32) {
    let red = state_i32(RED);
    let green = state_i32(GREEN);
    emit_color(red, green, value);
}

fn emit_color(red: i32, green: i32, blue: i32) {
    emit_command(&build_color_command(red, green, blue));
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
        init();
        set_red(12);
        set_green(34);
        set_blue(56);
    }
}
