//! `builtin.text-flow` パネル (Phase 10 DOM mutation 版)。

use plugin_sdk::{
    dom::{query_selector, set_attribute, set_inner_html},
    runtime::{
        emit_service, event_string, set_state_i32, set_state_string, state_i32, state_string,
    },
    services, state,
};

const INPUT_TEXT: state::StringKey = state::string("input_text");
const FONT_SIZE: state::IntKey = state::int("font_size");
const COLOR_HEX: state::StringKey = state::string("color_hex");
const X: state::IntKey = state::int("x");
const Y: state::IntKey = state::int("y");

fn render_dom() {
    if let Some(node) = query_selector("#font-size-label") {
        set_inner_html(node, &state_i32(FONT_SIZE).to_string());
    }
    if let Some(node) = query_selector("#x-label") {
        set_inner_html(node, &state_i32(X).to_string());
    }
    if let Some(node) = query_selector("#y-label") {
        set_inner_html(node, &state_i32(Y).to_string());
    }
    if let Some(node) = query_selector("#font-size") {
        set_attribute(node, "value", &state_i32(FONT_SIZE).to_string());
    }
    if let Some(node) = query_selector("#x") {
        set_attribute(node, "value", &state_i32(X).to_string());
    }
    if let Some(node) = query_selector("#y") {
        set_attribute(node, "value", &state_i32(Y).to_string());
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
fn update_text() {
    let value = event_string("value");
    set_state_string(INPUT_TEXT, &value);
}

#[plugin_sdk::panel_handler]
fn update_font_size(value: i32) {
    set_state_i32(FONT_SIZE, value.clamp(8, 200));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn update_x(value: i32) {
    set_state_i32(X, value.max(0));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn update_y(value: i32) {
    set_state_i32(Y, value.max(0));
    render_dom();
}

#[plugin_sdk::panel_handler]
fn render_text() {
    let text = state_string(INPUT_TEXT);
    if text.trim().is_empty() {
        return;
    }
    let font_size = state_i32(FONT_SIZE).max(8) as u32;
    let color_hex = state_string(COLOR_HEX);
    let x = state_i32(X).max(0) as usize;
    let y = state_i32(Y).max(0) as usize;
    emit_service(&services::text_render::render_to_layer(
        &text, font_size, &color_hex, x, y,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        update_text();
        update_font_size(64);
        update_x(200);
        update_y(300);
        render_text();
    }
}
