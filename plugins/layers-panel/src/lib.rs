use panel_sdk::{
	commands, host,
	runtime::{emit_command, event_string, set_state_bool, set_state_i32, set_state_string},
	state,
};

const TITLE: state::StringKey = state::string("title");
const PAGE_COUNT: state::IntKey = state::int("page_count");
const PANEL_COUNT: state::IntKey = state::int("panel_count");
const LAYER_COUNT: state::IntKey = state::int("layer_count");
const ACTIVE_LAYER_NAME: state::StringKey = state::string("active_layer_name");
const ACTIVE_LAYER_INDEX: state::IntKey = state::int("active_layer_index");
const ACTIVE_LAYER_BLEND_MODE: state::StringKey = state::string("active_layer_blend_mode");
const ACTIVE_LAYER_VISIBLE: state::BoolKey = state::bool("active_layer_visible");
const ACTIVE_LAYER_MASKED: state::BoolKey = state::bool("active_layer_masked");
const LAYERS_JSON: state::StringKey = state::string("layers_json");

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
	set_state_string(TITLE, host::document::title());
	set_state_i32(PAGE_COUNT, host::document::page_count());
	set_state_i32(PANEL_COUNT, host::document::panel_count());
	set_state_i32(LAYER_COUNT, host::document::layer_count());
	set_state_string(ACTIVE_LAYER_NAME, host::document::active_layer_name());
	set_state_i32(ACTIVE_LAYER_INDEX, host::document::active_layer_index());
	set_state_string(
		ACTIVE_LAYER_BLEND_MODE,
		host::document::active_layer_blend_mode(),
	);
	set_state_bool(ACTIVE_LAYER_VISIBLE, host::document::active_layer_visible());
	set_state_bool(ACTIVE_LAYER_MASKED, host::document::active_layer_masked());
	set_state_string(LAYERS_JSON, host::document::layers_json());
}

#[panel_sdk::panel_handler]
fn add_layer() {
    emit_command(&commands::layer::add());
}

#[panel_sdk::panel_handler]
fn remove_layer() {
	emit_command(&commands::layer::remove());
}

#[panel_sdk::panel_handler]
fn handle_layer_list(value: i32) {
	let target_index = value.max(0) as usize;
	if let Ok(from_index) = event_string("from").parse::<usize>()
		&& from_index != target_index
	{
		emit_command(&commands::layer::move_to(from_index, target_index));
	}
	emit_command(&commands::layer::select(target_index));
}

#[panel_sdk::panel_handler]
fn rename_active_layer() {
	let name = event_string("value");
	emit_command(&commands::layer::rename_active(name));
}

#[panel_sdk::panel_handler]
fn set_blend_mode() {
	let mode = event_string("value");
	if mode.is_empty() {
		return;
	}
	emit_command(&commands::layer::set_blend_mode(mode));
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
		sync_host();
		add_layer();
		remove_layer();
		handle_layer_list(0);
		rename_active_layer();
		set_blend_mode();
		toggle_layer_visibility();
		toggle_layer_mask();
	}
}
