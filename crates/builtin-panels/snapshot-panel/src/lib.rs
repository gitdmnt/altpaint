//! `builtin.snapshot-panel` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    dom::{html_escape, query_selector, set_inner_html},
    host,
    runtime::emit_service,
    services,
};

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    if let Some(node) = query_selector("#title") {
        set_inner_html(node, &html_escape(&host::document::title()));
    }
    if let Some(node) = query_selector("#page-count") {
        set_inner_html(node, &host::document::page_count().to_string());
    }
    if let Some(node) = query_selector("#panel-count") {
        set_inner_html(node, &host::document::panel_count().to_string());
    }
    if let Some(node) = query_selector("#active-tool") {
        set_inner_html(node, &html_escape(&host::tool::active_name()));
    }
    if let Some(node) = query_selector("#storage-status") {
        set_inner_html(node, &html_escape(&host::snapshot::storage_status()));
    }
    if let Some(node) = query_selector("#snapshot-count") {
        set_inner_html(node, &host::snapshot::count().to_string());
    }
}

#[plugin_sdk::panel_handler]
fn create_snapshot() {
    emit_service(&services::snapshot::create("Snapshot"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        create_snapshot();
    }
}
