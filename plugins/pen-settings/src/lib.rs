use panel_sdk::{
    commands,
    runtime::emit_command,
};

#[panel_sdk::panel_init]
fn init() {}

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
        set_pen_size(8);
    }
}
