use plugin_sdk::{
    host,
    runtime::{set_state_i32, set_state_string},
    state,
};

const ACTIVE: state::IntKey = state::int("active");
const QUEUED: state::IntKey = state::int("queued");
const STATUS: state::StringKey = state::string("status");

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    set_state_i32(ACTIVE, host::jobs::active());
    set_state_i32(QUEUED, host::jobs::queued());
    set_state_string(STATUS, host::jobs::status());
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
