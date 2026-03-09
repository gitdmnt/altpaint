use panel_sdk::{
    commands,
    host,
    runtime::{emit_command, set_state_bool, set_state_i32, set_state_string},
    state,
};

const PEN_NAME: state::StringKey = state::string("pen_name");
const PEN_SIZE: state::IntKey = state::int("size");
const PEN_PRESSURE: state::BoolKey = state::bool("pressure_enabled");
const PEN_ANTIALIAS: state::BoolKey = state::bool("antialias");
const PEN_STABILIZATION: state::IntKey = state::int("stabilization");

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
    set_state_string(PEN_NAME, host::tool::pen_name());
    set_state_i32(PEN_SIZE, host::tool::pen_size());
    set_state_bool(PEN_PRESSURE, host::tool::pen_pressure_enabled());
    set_state_bool(PEN_ANTIALIAS, host::tool::pen_antialias());
    set_state_i32(PEN_STABILIZATION, host::tool::pen_stabilization());
}

#[panel_sdk::panel_handler]
fn set_pen_size(value: i32) {
    emit_command(&commands::tool::set_size(value.max(1) as u32));
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
        set_pen_size(8);
        toggle_pressure();
        toggle_antialias();
        set_stabilization(24);
    }
}
