use panel_sdk::{
    commands,
    host,
    runtime::{emit_command, set_state_bool, set_state_i32, set_state_string},
    state,
};

const PEN_NAME: state::StringKey = state::string("pen_name");
const PEN_SIZE: state::IntKey = state::int("size");
const PEN_SIZE_SLIDER: state::IntKey = state::int("size_slider");
const TOOL_LABEL: state::StringKey = state::string("tool_label");
const PEN_PRESSURE: state::BoolKey = state::bool("pressure_enabled");
const PEN_ANTIALIAS: state::BoolKey = state::bool("antialias");
const PEN_STABILIZATION: state::IntKey = state::int("stabilization");

const LOG_SIZE_SLIDER_MAX: i32 = 1000;
const MAX_TOOL_SIZE: f32 = 10000.0;

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
    let active_tool = host::tool::active_name();
    let size = host::tool::pen_size().max(1);
    set_state_string(PEN_NAME, host::tool::pen_name());
    set_state_i32(PEN_SIZE, size);
    set_state_i32(PEN_SIZE_SLIDER, size_to_slider(size));
    set_state_string(
        TOOL_LABEL,
        if active_tool.eq_ignore_ascii_case("eraser") {
            "Eraser Width"
        } else {
            "Pen Width"
        },
    );
    set_state_bool(PEN_PRESSURE, host::tool::pen_pressure_enabled());
    set_state_bool(PEN_ANTIALIAS, host::tool::pen_antialias());
    set_state_i32(PEN_STABILIZATION, host::tool::pen_stabilization());
}

fn size_to_slider(size: i32) -> i32 {
    if size <= 1 {
        return 0;
    }
    let normalized = (size as f32).ln() / MAX_TOOL_SIZE.ln();
    (normalized * LOG_SIZE_SLIDER_MAX as f32).round() as i32
}

fn slider_to_size(value: i32) -> u32 {
    let normalized = value.clamp(0, LOG_SIZE_SLIDER_MAX) as f32 / LOG_SIZE_SLIDER_MAX as f32;
    MAX_TOOL_SIZE.powf(normalized).round().clamp(1.0, MAX_TOOL_SIZE) as u32
}

#[panel_sdk::panel_handler]
fn set_pen_size(value: i32) {
    emit_command(&commands::tool::set_size(slider_to_size(value)));
}

#[panel_sdk::panel_handler]
fn toggle_pressure() {
    emit_command(&commands::tool::set_pressure_enabled(!host::tool::pen_pressure_enabled()));
}

#[panel_sdk::panel_handler]
fn toggle_antialias() {
    emit_command(&commands::tool::set_antialias(!host::tool::pen_antialias()));
}

#[panel_sdk::panel_handler]
fn set_stabilization(value: i32) {
    emit_command(&commands::tool::set_stabilization(value.clamp(0, 100) as u8));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        set_pen_size(400);
        toggle_pressure();
        toggle_antialias();
        set_stabilization(24);
    }

    #[test]
    fn logarithmic_slider_roundtrips_common_sizes() {
        for size in [1, 2, 4, 16, 128, 2048, 10000] {
            let slider = size_to_slider(size);
            let restored = slider_to_size(slider) as i32;
            assert!((restored - size).abs() <= 2.max(size / 20));
        }
    }
}
