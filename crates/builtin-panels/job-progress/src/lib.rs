//! `builtin.job-progress` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    dom::{html_escape, query_selector, set_inner_html},
    host,
};

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
    let active = host::jobs::active();
    if let Some(node) = query_selector("#active") {
        set_inner_html(node, &active.to_string());
    }
    if let Some(node) = query_selector("#queued") {
        set_inner_html(node, &host::jobs::queued().to_string());
    }
    if let Some(node) = query_selector("#status") {
        // BL-094: host state は生データのみ。status 文字列の整形はパネル側で行う。
        let status = format_status(active, &host::document::title());
        set_inner_html(node, &html_escape(&status));
    }
}

/// アクティブジョブ数と作品タイトルから status 文字列を整形する (BL-094)。
fn format_status(active: i32, work_title: &str) -> String {
    if active <= 0 {
        format!("idle / work={work_title}")
    } else {
        format!("{active} job(s) running")
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

    #[test]
    fn format_status_idle_includes_work_title() {
        assert_eq!(format_status(0, "untitled"), "idle / work=untitled");
    }

    #[test]
    fn format_status_running_counts_jobs() {
        assert_eq!(format_status(3, "untitled"), "3 job(s) running");
    }
}
