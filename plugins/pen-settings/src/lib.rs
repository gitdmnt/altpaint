use panel_sdk::{
    commands,
    host,
    runtime::{emit_command, set_state_i32, set_state_string},
    state,
};

const PEN_NAME: state::StringKey = state::string("pen_name");
const PEN_SIZE: state::IntKey = state::int("size");

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_handler]
fn sync_host() {
    set_state_string(PEN_NAME, host::tool::pen_name());
    set_state_i32(PEN_SIZE, host::tool::pen_size());
}

#[panel_sdk::panel_handler]
fn set_pen_size(value: i32) {
    emit_command(&commands::tool::set_size(value.max(1) as u32));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        set_pen_size(8);
    }
}
