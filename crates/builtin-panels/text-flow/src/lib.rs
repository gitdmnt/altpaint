//! `builtin.text-flow` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    dom::set_slider,
    runtime::{emit_request, set_state_i32, set_state_string, state_i32, state_string},
    serde::Deserialize,
    services, state,
};

const INPUT_TEXT: state::StringKey = state::string("input_text");
const FONT_SIZE: state::IntKey = state::int("font_size");
const X: state::IntKey = state::int("x");
const Y: state::IntKey = state::int("y");

/// テキスト入力 payload (`altp:input:*` は `event_payload.value` を文字列で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct TextValue {
    #[serde(default)]
    value: String,
}

/// スライダー入力 payload (`altp:slider:*` は `event_payload.value` を整数で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SliderValue {
    #[serde(default)]
    value: i32,
}

fn render_dom() {
    set_slider("#font-size", state_i32(FONT_SIZE), "#font-size-label");
    set_slider("#x", state_i32(X), "#x-label");
    set_slider("#y", state_i32(Y), "#y-label");
}

#[panel_sdk::panel_init]
fn init() {
    render_dom();
}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    render_dom();
}

#[panel_sdk::panel_handler]
fn update_text(payload: TextValue) {
    set_state_string(INPUT_TEXT, &payload.value);
}

#[panel_sdk::panel_handler]
fn update_font_size(payload: SliderValue) {
    set_state_i32(FONT_SIZE, payload.value.clamp(8, 200));
    render_dom();
}

#[panel_sdk::panel_handler]
fn update_x(payload: SliderValue) {
    set_state_i32(X, payload.value.max(0));
    render_dom();
}

#[panel_sdk::panel_handler]
fn update_y(payload: SliderValue) {
    set_state_i32(Y, payload.value.max(0));
    render_dom();
}

#[panel_sdk::panel_handler]
fn render_text() {
    let text = state_string(INPUT_TEXT);
    if text.trim().is_empty() {
        return;
    }
    let font_size = state_i32(FONT_SIZE).max(8) as u32;
    let x = state_i32(X).max(0) as usize;
    let y = state_i32(Y).max(0) as usize;
    // 色指定 UI は未提供。空文字を渡すと host 側既定 (#000000) で描画される。
    emit_request(&services::text_render::render_to_layer(
        &text, font_size, "", x, y,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        update_text(TextValue { value: "hi".to_string() }),
        update_font_size(SliderValue { value: 64 }),
        update_x(SliderValue { value: 200 }),
        update_y(SliderValue { value: 300 }),
        render_text(),
    });
}
