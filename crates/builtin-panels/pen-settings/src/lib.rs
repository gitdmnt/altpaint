//! `builtin.pen-settings` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    commands,
    dom::{clear_attribute, html_escape, query_selector, set_attribute, set_inner_html},
    host,
    runtime::{emit_command, error, event_string},
};

const LOG_SIZE_SLIDER_MAX: i32 = 1000;
const MAX_TOOL_SIZE: f32 = 10000.0;

fn size_to_slider(size: i32) -> i32 {
    if size <= 1 {
        return 0;
    }
    let normalized = (size as f32).ln() / MAX_TOOL_SIZE.ln();
    (normalized * LOG_SIZE_SLIDER_MAX as f32).round() as i32
}

fn slider_to_size(value: i32) -> u32 {
    let normalized = value.clamp(0, LOG_SIZE_SLIDER_MAX) as f32 / LOG_SIZE_SLIDER_MAX as f32;
    MAX_TOOL_SIZE
        .powf(normalized)
        .round()
        .clamp(1.0, MAX_TOOL_SIZE) as u32
}

fn parse_size_input(value: &str) -> Result<u32, &'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("width must not be empty");
    }
    let parsed = trimmed
        .parse::<u32>()
        .map_err(|_| "width must be a positive integer")?;
    Ok(parsed.clamp(1, MAX_TOOL_SIZE as u32))
}

fn set_visible(selector: &str, visible: bool) {
    if let Some(node) = query_selector(selector) {
        if visible {
            clear_attribute(node, "hidden");
        } else {
            set_attribute(node, "hidden", "");
        }
    }
}

fn set_text(selector: &str, text: &str) {
    if let Some(node) = query_selector(selector) {
        set_inner_html(node, &html_escape(text));
    }
}

fn set_button_active(selector: &str, active: bool) {
    if let Some(btn) = query_selector(selector) {
        let cls = if active { "btn active" } else { "btn" };
        set_attribute(btn, "class", cls);
    }
}

fn render_dom() {
    let snapshot = host::tool::snapshot();
    let capabilities = host::tool::capabilities();
    let active_tool = snapshot.active_name.clone();
    let size = snapshot.pen_size.max(1);

    set_text("#active-tool-label", &snapshot.active_label);
    set_text("#pen-name", &snapshot.pen_name);
    set_text("#size-display", &size.to_string());

    if let Some(slider) = query_selector("#pen\\.size") {
        set_attribute(slider, "value", &size_to_slider(size).to_string());
    }
    if let Some(input) = query_selector("#pen\\.size\\.input") {
        set_attribute(input, "value", &size.to_string());
    }
    if let Some(slider) = query_selector("#pen\\.stabilization") {
        set_attribute(slider, "value", &host::tool::pen_stabilization().to_string());
    }
    set_text("#stabilization-display", &host::tool::pen_stabilization().to_string());

    let supports_size = capabilities.supports_size;
    let supports_pressure = capabilities.supports_pressure_enabled;
    let supports_antialias = capabilities.supports_antialias;
    let supports_stabilization = capabilities.supports_stabilization;
    let has_settings =
        supports_size || supports_pressure || supports_antialias || supports_stabilization;

    set_visible("#size-section", supports_size);
    set_visible("#pen\\.pressure", supports_pressure);
    set_visible("#pen\\.antialias", supports_antialias);
    set_visible("#stabilization-row", supports_stabilization);
    set_visible("#characteristics-section",
        supports_pressure || supports_antialias || supports_stabilization);
    set_visible("#no-settings-section", !has_settings);

    set_button_active("#pen\\.pressure", host::tool::pen_pressure_enabled());
    set_button_active("#pen\\.antialias", host::tool::pen_antialias());

    let label = if active_tool.eq_ignore_ascii_case("eraser") {
        "Eraser Width"
    } else if active_tool.eq_ignore_ascii_case("pen") {
        "Pen Width"
    } else {
        "Tool Size"
    };
    let _ = label;
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
fn set_pen_size(value: i32) {
    let size = slider_to_size(value);
    emit_command(&commands::tool::set_size(size));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn set_pen_size_text() {
    let value = event_string("value");
    let Ok(size) = parse_size_input(&value) else {
        error("width must be a positive integer");
        return;
    };
    emit_command(&commands::tool::set_size(size));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn toggle_pressure() {
    emit_command(&commands::tool::set_pressure_enabled(
        !host::tool::pen_pressure_enabled(),
    ));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn toggle_antialias() {
    emit_command(&commands::tool::set_antialias(!host::tool::pen_antialias()));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn set_stabilization(value: i32) {
    emit_command(&commands::tool::set_stabilization(value.clamp(0, 100) as u8));
    render_dom();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_roundtrips_common_sizes() {
        for size in [1, 2, 4, 16, 128, 2048, 10000] {
            let s = size_to_slider(size);
            let restored = slider_to_size(s) as i32;
            assert!((restored - size).abs() <= 2.max(size / 20));
        }
    }

    #[test]
    fn parse_size_input_clamps() {
        assert_eq!(parse_size_input("24"), Ok(24));
        assert_eq!(parse_size_input("0"), Ok(1));
        assert_eq!(parse_size_input("999999"), Ok(10000));
        assert!(parse_size_input("abc").is_err());
    }

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        set_pen_size(400);
        set_pen_size_text();
        toggle_pressure();
        toggle_antialias();
        set_stabilization(24);
    }
}
