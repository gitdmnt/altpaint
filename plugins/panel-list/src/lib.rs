use plugin_sdk::{
    host,
    runtime::{emit_service, set_state_i32, set_state_string},
    services, state,
};

const TITLE: state::StringKey = state::string("title");
const ACTIVE_PAGE_NUMBER: state::IntKey = state::int("active_page_number");
const ACTIVE_PANEL_NUMBER: state::IntKey = state::int("active_panel_number");
const ACTIVE_PANEL_INDEX: state::IntKey = state::int("active_panel_index");
const ACTIVE_PAGE_PANEL_COUNT: state::IntKey = state::int("active_page_panel_count");
const ACTIVE_PANEL_BOUNDS: state::StringKey = state::string("active_panel_bounds");
const PANELS_JSON: state::StringKey = state::string("panels_json");

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    set_state_string(TITLE, host::document::title());
    set_state_i32(ACTIVE_PAGE_NUMBER, host::document::active_page_number());
    set_state_i32(ACTIVE_PANEL_NUMBER, host::document::active_panel_number());
    set_state_i32(ACTIVE_PANEL_INDEX, host::document::active_panel_index());
    set_state_i32(
        ACTIVE_PAGE_PANEL_COUNT,
        host::document::active_page_panel_count(),
    );
    set_state_string(ACTIVE_PANEL_BOUNDS, host::document::active_panel_bounds());
    set_state_string(PANELS_JSON, host::document::panels_json());
}

#[plugin_sdk::panel_handler]
fn add_panel() {
    emit_service(&services::panel_nav::add());
}

#[plugin_sdk::panel_handler]
fn remove_panel() {
    emit_service(&services::panel_nav::remove());
}

#[plugin_sdk::panel_handler]
fn select_previous_panel() {
    emit_service(&services::panel_nav::select_previous());
}

#[plugin_sdk::panel_handler]
fn select_next_panel() {
    emit_service(&services::panel_nav::select_next());
}

#[plugin_sdk::panel_handler]
fn focus_active_panel() {
    emit_service(&services::panel_nav::focus_active());
}

#[plugin_sdk::panel_handler]
fn handle_panel_list(value: i32) {
    emit_service(&services::panel_nav::select(value.max(0) as usize));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        add_panel();
        remove_panel();
        select_previous_panel();
        select_next_panel();
        focus_active_panel();
        handle_panel_list(0);
    }
}
