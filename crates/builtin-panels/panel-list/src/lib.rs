//! `builtin.panel-list` パネル (Phase 10 DOM mutation 版)。

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
    if let Some(node) = query_selector("#active-page-number") {
        set_inner_html(node, &host::document::active_page_number().to_string());
    }
    if let Some(node) = query_selector("#active-panel-number") {
        set_inner_html(node, &host::document::active_panel_number().to_string());
    }
    if let Some(node) = query_selector("#active-page-panel-count") {
        set_inner_html(
            node,
            &host::document::active_page_panel_count().to_string(),
        );
    }
    if let Some(node) = query_selector("#active-panel-bounds") {
        set_inner_html(node, &html_escape(&host::document::active_panel_bounds()));
    }

    if let Some(list) = query_selector("#panel-list") {
        let panels_json = host::document::panels_json();
        let active_index = host::document::active_panel_index();
        set_inner_html(list, &render_panel_list(&panels_json, active_index));
    }
}

fn render_panel_list(panels_json: &str, active_index: i32) -> String {
    let parsed: Vec<PanelEntry> =
        serde_json::from_str(panels_json).unwrap_or_default();
    let mut out = String::new();
    for (idx, panel) in parsed.iter().enumerate() {
        let class = if idx as i32 == active_index {
            "active"
        } else {
            ""
        };
        out.push_str(&format!(
            r#"<li class="{class}" data-action="altp:activate:handle_panel_list" data-args='{{"value":{idx}}}'><span>{name}</span><span class="detail">{detail}</span></li>"#,
            class = class,
            idx = idx,
            name = html_escape(&panel.name),
            detail = html_escape(&panel.detail),
        ));
    }
    out
}

#[derive(Default, serde::Deserialize)]
struct PanelEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    detail: String,
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
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        add_panel();
        remove_panel();
        select_previous_panel();
        select_next_panel();
        focus_active_panel();
        handle_panel_list(0);
    }

    #[test]
    fn render_panel_list_escapes_html() {
        let payload = r#"[{"name":"<script>","detail":"a"}]"#;
        let html = render_panel_list(payload, 0);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
