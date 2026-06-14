//! `builtin.koma-list` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    dom::{html_escape, query_selector, set_inner_html},
    host,
    runtime::emit_service,
    services,
};

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    if let Some(node) = query_selector("#title") {
        set_inner_html(node, &html_escape(&host::document::title()));
    }
    if let Some(node) = query_selector("#active-page-number") {
        set_inner_html(node, &host::document::active_page_number().to_string());
    }
    if let Some(node) = query_selector("#active-panel-number") {
        set_inner_html(node, &host::document::active_koma_number().to_string());
    }
    if let Some(node) = query_selector("#active-page-panel-count") {
        set_inner_html(
            node,
            &host::document::active_page_koma_count().to_string(),
        );
    }
    if let Some(node) = query_selector("#active-panel-bounds") {
        // BL-094: host state は bounds 生データのみ。ラベル整形はパネル側で行う。
        let bounds = format_bounds(
            host::document::active_koma_x(),
            host::document::active_koma_y(),
            host::document::active_koma_width(),
            host::document::active_koma_height(),
        );
        set_inner_html(node, &html_escape(&bounds));
    }

    if let Some(list) = query_selector("#koma-list") {
        let komas_json = host::document::komas_json();
        let active_index = host::document::active_koma_index();
        set_inner_html(list, &render_panel_list(&komas_json, active_index));
    }
}

/// `(x, y) w×h` 形式へ整形する (BL-094: 旧 host state の文字列契約をパネル側へ移管)。
fn format_bounds(x: i32, y: i32, width: i32, height: i32) -> String {
    format!("({x}, {y}) {width}×{height}")
}

fn render_panel_list(komas_json: &str, active_index: i32) -> String {
    let parsed: Vec<PanelEntry> =
        serde_json::from_str(komas_json).unwrap_or_default();
    let mut out = String::new();
    for (idx, panel) in parsed.iter().enumerate() {
        let class = if idx as i32 == active_index {
            "active"
        } else {
            ""
        };
        // BL-094: name / detail のラベル整形はパネル側で行う (生データから組み立て)。
        let name = format!("コマ {}", idx + 1);
        let detail = format!(
            "{}×{} / ({}, {})",
            panel.width, panel.height, panel.x, panel.y
        );
        out.push_str(&format!(
            r#"<li class="{class}" data-action="altp:activate:handle_panel_list" data-args='{{"value":{idx}}}'><span>{name}</span><span class="detail">{detail}</span></li>"#,
            class = class,
            idx = idx,
            name = html_escape(&name),
            detail = html_escape(&detail),
        ));
    }
    out
}

#[derive(Default, serde::Deserialize)]
struct PanelEntry {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    width: i32,
    #[serde(default)]
    height: i32,
}

#[panel_sdk::panel_handler]
fn add_panel() {
    emit_service(&services::koma_nav::add());
}

#[panel_sdk::panel_handler]
fn remove_panel() {
    emit_service(&services::koma_nav::remove());
}

#[panel_sdk::panel_handler]
fn select_previous_panel() {
    emit_service(&services::koma_nav::select_previous());
}

#[panel_sdk::panel_handler]
fn select_next_panel() {
    emit_service(&services::koma_nav::select_next());
}

#[panel_sdk::panel_handler]
fn focus_active_panel() {
    emit_service(&services::koma_nav::focus_active());
}

#[panel_sdk::panel_handler]
fn handle_panel_list(value: i32) {
    emit_service(&services::koma_nav::select(value.max(0) as usize));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        on_host_change();
        add_panel();
        remove_panel();
        select_previous_panel();
        select_next_panel();
        focus_active_panel();
        handle_panel_list(0);
    }

    #[test]
    fn render_panel_list_formats_labels_from_raw_bounds() {
        // BL-094: 生データ (x/y/width/height) からラベルを組み立てる。
        let payload = r#"[{"x":7,"y":9,"width":123,"height":45}]"#;
        let html = render_panel_list(payload, 0);
        assert!(html.contains("コマ 1"), "html: {html}");
        assert!(html.contains("123×45 / (7, 9)"), "html: {html}");
    }

    #[test]
    fn format_bounds_renders_parenthesized_size() {
        assert_eq!(format_bounds(7, 9, 123, 45), "(7, 9) 123×45");
    }
}
