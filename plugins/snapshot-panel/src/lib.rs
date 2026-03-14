use plugin_sdk::{
    host,
    runtime::{emit_service, set_state_i32, set_state_string},
    services,
    state,
};

const TITLE: state::StringKey = state::string("title");
const PAGE_COUNT: state::IntKey = state::int("page_count");
const PANEL_COUNT: state::IntKey = state::int("panel_count");
const ACTIVE_TOOL: state::StringKey = state::string("active_tool");
const STORAGE_STATUS: state::StringKey = state::string("storage_status");
const SNAPSHOT_COUNT: state::IntKey = state::int("snapshot_count");

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// Host snapshot を読み取り、表示用の状態へ同期する。
#[plugin_sdk::panel_sync_host]
fn sync_host() {
    set_state_string(TITLE, host::document::title());
    set_state_i32(PAGE_COUNT, host::document::page_count());
    set_state_i32(PANEL_COUNT, host::document::panel_count());
    set_state_string(ACTIVE_TOOL, host::tool::active_name());
    set_state_string(STORAGE_STATUS, host::snapshot::storage_status());
    set_state_i32(SNAPSHOT_COUNT, host::snapshot::count());
}

/// スナップショット作成ボタンの処理。
#[plugin_sdk::panel_handler]
fn create_snapshot() {
    emit_service(&services::snapshot::create("Snapshot"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// パネル 初期化 is callable が期待どおりに動作することを検証する。
    #[test]
    fn panel_init_is_callable() {
        init();
        sync_host();
    }

    /// create_snapshot is callable が期待どおりに動作することを検証する。
    #[test]
    fn create_snapshot_is_callable() {
        create_snapshot();
    }
}
