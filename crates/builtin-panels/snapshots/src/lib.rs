//! `builtin.snapshots` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    dom::set_text,
    host_state::{DocumentState, SnapshotState, ToolState, section},
    runtime::{emit_request, host_section},
    services,
};

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    // BL-142: 購読セクションを 1 回ずつ取得し型付き DTO へ落とす。
    if let Some(document) = host_section::<DocumentState>(section::DOCUMENT) {
        set_text("#title", &document.title);
        set_text("#page-count", &document.page_count.to_string());
        set_text("#panel-count", &document.koma_count.to_string());
    }
    if let Some(tool) = host_section::<ToolState>(section::TOOL) {
        set_text("#active-tool", &tool.active);
    }
    if let Some(snapshot) = host_section::<SnapshotState>(section::SNAPSHOT) {
        set_text("#storage-status", &snapshot.storage_status);
        set_text("#snapshot-count", &snapshot.count.to_string());
    }
}

#[panel_sdk::panel_handler]
fn create_snapshot() {
    emit_request(&services::snapshot::create("Snapshot"));
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        create_snapshot(),
    });
}
