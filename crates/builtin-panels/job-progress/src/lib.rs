//! `builtin.job-progress` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    dom::{html_escape, query_selector, set_inner_html},
    host,
};

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    if let Some(node) = query_selector("#active") {
        set_inner_html(node, &host::jobs::active().to_string());
    }
    if let Some(node) = query_selector("#queued") {
        set_inner_html(node, &host::jobs::queued().to_string());
    }
    if let Some(node) = query_selector("#status") {
        set_inner_html(node, &html_escape(&host::jobs::status()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
    }
}
