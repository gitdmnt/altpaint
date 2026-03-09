use panel_sdk::{commands, runtime::emit_command};

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_handler]
fn add_layer() {
    emit_command(&commands::layer::add());
}

#[panel_sdk::panel_handler]
fn select_next_layer() {
    emit_command(&commands::layer::select_next());
}

#[panel_sdk::panel_handler]
fn cycle_blend_mode() {
    emit_command(&commands::layer::cycle_blend_mode());
}

#[panel_sdk::panel_handler]
fn toggle_layer_visibility() {
    emit_command(&commands::layer::toggle_visibility());
}

#[panel_sdk::panel_handler]
fn toggle_layer_mask() {
    emit_command(&commands::layer::toggle_mask());
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn panel_init_is_callable() {
		init();
		add_layer();
		select_next_layer();
		cycle_blend_mode();
		toggle_layer_visibility();
		toggle_layer_mask();
	}
}
