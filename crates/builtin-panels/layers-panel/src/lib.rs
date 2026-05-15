//! `builtin.layers-panel` パネル (Phase 10 DOM mutation 版)。

use std::cell::RefCell;

use plugin_sdk::{
    commands,
    dom::{html_escape, query_selector, set_attribute, set_inner_html},
    host,
    runtime::{
        emit_command, event_string, set_state_string, state_string,
    },
    state,
};

const RENAME_TEXT: state::StringKey = state::string("rename_text");

thread_local! {
    static RENAME_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

#[derive(Default, serde::Deserialize)]
struct LayerEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    blend_mode: String,
    #[serde(default)]
    visible: bool,
    #[serde(default)]
    masked: bool,
}

fn render_layer_list(layers_json: &str, active_index: i32) -> String {
    let layers: Vec<LayerEntry> = serde_json::from_str(layers_json).unwrap_or_default();
    let mut out = String::new();
    for (idx, layer) in layers.iter().enumerate() {
        let class = if idx as i32 == active_index { "active" } else { "" };
        let detail = format!(
            "{} {}{}",
            html_escape(&layer.blend_mode),
            if layer.visible { "👁" } else { "·" },
            if layer.masked { " ◫" } else { "" },
        );
        out.push_str(&format!(
            r#"<li class="{class}" data-action="altp:activate:handle_layer_list" data-args='{{"value":{idx}}}'><span>{name}</span><span class="meta">{detail}</span></li>"#,
            class = class,
            idx = idx,
            name = html_escape(&layer.name),
            detail = detail,
        ));
    }
    out
}

fn render_dom() {
    if let Some(node) = query_selector("#title") {
        set_inner_html(node, &html_escape(&host::document::title()));
    }
    if let Some(node) = query_selector("#page-count") {
        set_inner_html(node, &host::document::page_count().to_string());
    }
    if let Some(node) = query_selector("#panel-count") {
        set_inner_html(node, &host::document::panel_count().to_string());
    }
    if let Some(node) = query_selector("#layer-count") {
        set_inner_html(node, &host::document::layer_count().to_string());
    }
    if let Some(node) = query_selector("#active-layer-index") {
        set_inner_html(node, &host::document::active_layer_index().to_string());
    }
    if let Some(node) = query_selector("#active-layer-visible") {
        set_inner_html(node, &host::document::active_layer_visible().to_string());
    }
    if let Some(node) = query_selector("#active-layer-masked") {
        set_inner_html(node, &host::document::active_layer_masked().to_string());
    }

    let layer_name = host::document::active_layer_name();
    RENAME_BUF.with(|buf| *buf.borrow_mut() = layer_name.clone());
    set_state_string(RENAME_TEXT, &layer_name);
    if let Some(input) = query_selector("#layers\\.name") {
        set_attribute(input, "value", &layer_name);
    }

    let blend_mode = host::document::active_layer_blend_mode();
    if let Some(select) = query_selector("#layers\\.blend_mode") {
        let modes = [
            ("normal", "通常"),
            ("multiply", "乗算"),
            ("screen", "スクリーン"),
            ("add", "加算"),
            ("max(src,dst)", "比較(明)"),
        ];
        let mut html = String::new();
        for (val, label) in modes {
            let mark = if val == blend_mode { " selected" } else { "" };
            html.push_str(&format!(
                r#"<option value="{}"{}>{}</option>"#,
                html_escape(val),
                mark,
                html_escape(label),
            ));
        }
        set_inner_html(select, &html);
    }

    if let Some(list) = query_selector("#layers-list") {
        let layers_json = host::document::layers_json();
        let active = host::document::active_layer_index();
        set_inner_html(list, &render_layer_list(&layers_json, active));
    }
}

#[plugin_sdk::panel_init]
fn init() {
    render_dom();
}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    render_dom();
}

#[plugin_sdk::panel_handler]
fn add_layer() {
    emit_command(&commands::layer::add());
}

#[plugin_sdk::panel_handler]
fn remove_layer() {
    emit_command(&commands::layer::remove());
}

#[plugin_sdk::panel_handler]
fn handle_layer_list(value: i32) {
    let layer_count = host::document::layer_count() as usize;
    let ui_target = value.max(0) as usize;
    let actual_target = layer_count.saturating_sub(1).saturating_sub(ui_target);
    if let Ok(ui_from) = event_string("from").parse::<usize>() {
        let actual_from = layer_count.saturating_sub(1).saturating_sub(ui_from);
        if actual_from != actual_target {
            emit_command(&commands::layer::move_to(actual_from, actual_target));
        }
    }
    emit_command(&commands::layer::select(actual_target));
}

#[plugin_sdk::panel_handler]
fn update_rename_text() {
    let name = event_string("value");
    RENAME_BUF.with(|buf| *buf.borrow_mut() = name.clone());
    set_state_string(RENAME_TEXT, &name);
}

#[plugin_sdk::panel_handler]
fn confirm_rename() {
    let name = RENAME_BUF.with(|buf| buf.borrow().clone());
    let name = if name.is_empty() {
        state_string(RENAME_TEXT)
    } else {
        name
    };
    if !name.is_empty() {
        emit_command(&commands::layer::rename_active(name));
    }
}

#[plugin_sdk::panel_handler]
fn set_blend_mode() {
    let mode = event_string("value");
    if mode.is_empty() {
        return;
    }
    emit_command(&commands::layer::set_blend_mode(mode));
}

#[plugin_sdk::panel_handler]
fn toggle_layer_visibility() {
    emit_command(&commands::layer::toggle_visibility());
}

#[plugin_sdk::panel_handler]
fn toggle_layer_mask() {
    emit_command(&commands::layer::toggle_mask());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        add_layer();
        remove_layer();
        handle_layer_list(0);
        update_rename_text();
        confirm_rename();
        set_blend_mode();
        toggle_layer_visibility();
        toggle_layer_mask();
    }

    #[test]
    fn render_layer_list_escapes_html() {
        let payload = r#"[{"name":"<script>","blend_mode":"normal","visible":true,"masked":false}]"#;
        let out = render_layer_list(payload, 0);
        assert!(!out.contains("<script>"));
        assert!(out.contains("&lt;script&gt;"));
    }
}
