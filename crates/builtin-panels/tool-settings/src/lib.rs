//! `builtin.tool-settings` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    commands,
    dom::{set_button_active, set_slider, set_text, set_visible},
    host_state::{ToolState, section},
    runtime::{emit_request, error, host_section},
    serde::Deserialize,
};

const LOG_SIZE_SLIDER_MAX: i32 = 1000;
const MAX_TOOL_SIZE: f32 = 10000.0;

/// スライダー入力 payload (`altp:slider:*` は `event_payload.value` を整数で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SliderValue {
    #[serde(default)]
    value: i32,
}

/// テキスト入力 payload (`altp:input:*` は `event_payload.value` を文字列で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct TextValue {
    #[serde(default)]
    value: String,
}

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

fn render_dom() {
    // BL-142: tool セクションを型付き DTO として 1 回取得する。
    let Some(tool) = host_section::<ToolState>(section::TOOL) else {
        return;
    };
    let size = tool.pen_size.max(1) as i32;

    set_text("#active-tool-label", &tool.active_label);
    set_text("#pen-name", &tool.pen_name);
    set_slider("#pen\\.size", size_to_slider(size), "#size-display");
    set_slider("#pen\\.size\\.input", size, "");
    set_slider(
        "#pen\\.stabilization",
        tool.pen_stabilization as i32,
        "#stabilization-display",
    );

    let supports_size = tool.supports_size;
    let supports_pressure = tool.supports_pressure_enabled;
    let supports_antialias = tool.supports_antialias;
    let supports_stabilization = tool.supports_stabilization;
    let has_settings =
        supports_size || supports_pressure || supports_antialias || supports_stabilization;

    set_visible("#size-section", supports_size);
    set_visible("#pen\\.pressure", supports_pressure);
    set_visible("#pen\\.antialias", supports_antialias);
    set_visible("#stabilization-row", supports_stabilization);
    set_visible(
        "#characteristics-section",
        supports_pressure || supports_antialias || supports_stabilization,
    );
    set_visible("#no-settings-section", !has_settings);

    set_button_active("#pen\\.pressure", tool.pen_pressure_enabled);
    set_button_active("#pen\\.antialias", tool.pen_antialias);
}

/// 現在の host tool 状態 (pen_pressure_enabled / pen_antialias) を読む補助。
fn host_tool() -> Option<ToolState> {
    host_section::<ToolState>(section::TOOL)
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
fn set_pen_size(payload: SliderValue) {
    let size = slider_to_size(payload.value);
    emit_request(&commands::tool::set_size(size));
    render_dom();
}

#[panel_sdk::panel_handler]
fn set_pen_size_text(payload: TextValue) {
    let Ok(size) = parse_size_input(&payload.value) else {
        error("width must be a positive integer");
        return;
    };
    emit_request(&commands::tool::set_size(size));
    render_dom();
}

#[panel_sdk::panel_handler]
fn toggle_pressure() {
    let enabled = host_tool().is_some_and(|tool| tool.pen_pressure_enabled);
    emit_request(&commands::tool::set_pressure_enabled(!enabled));
    render_dom();
}

#[panel_sdk::panel_handler]
fn toggle_antialias() {
    let enabled = host_tool().is_some_and(|tool| tool.pen_antialias);
    emit_request(&commands::tool::set_antialias(!enabled));
    render_dom();
}

#[panel_sdk::panel_handler]
fn set_stabilization(payload: SliderValue) {
    emit_request(&commands::tool::set_stabilization(
        payload.value.clamp(0, 100) as u8,
    ));
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

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        set_pen_size(SliderValue { value: 400 }),
        set_pen_size_text(TextValue { value: "24".to_string() }),
        toggle_pressure(),
        toggle_antialias(),
        set_stabilization(SliderValue { value: 24 }),
    });
}
