use panel_sdk::{
	host,
	runtime::{set_state_i32, set_state_string},
	state,
};

const TITLE: state::StringKey = state::string("title");
const PAGE_COUNT: state::IntKey = state::int("page_count");
const PANEL_COUNT: state::IntKey = state::int("panel_count");
const ACTIVE_TOOL: state::StringKey = state::string("active_tool");
const STORAGE_STATUS: state::StringKey = state::string("storage_status");

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_handler]
fn sync_host() {
	set_state_string(TITLE, host::document::title());
	set_state_i32(PAGE_COUNT, host::document::page_count());
	set_state_i32(PANEL_COUNT, host::document::panel_count());
	set_state_string(ACTIVE_TOOL, host::tool::active_name());
	set_state_string(STORAGE_STATUS, host::snapshot::storage_status());
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn panel_init_is_callable() {
		init();
		sync_host();
	}
}
