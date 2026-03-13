use plugin_sdk::{
    commands, host,
    runtime::{emit_command, error, event_string, set_state_bool, set_state_i32, set_state_string},
    state,
};

const PEN_NAME: state::StringKey = state::string("pen_name");
const ACTIVE_TOOL_ID: state::StringKey = state::string("active_tool_id");
const ACTIVE_TOOL_LABEL: state::StringKey = state::string("active_tool_label");
const PROVIDER_PLUGIN_ID: state::StringKey = state::string("provider_plugin_id");
const DRAWING_PLUGIN_ID: state::StringKey = state::string("drawing_plugin_id");
const PEN_SIZE: state::IntKey = state::int("size");
const PEN_SIZE_SLIDER: state::IntKey = state::int("size_slider");
const PEN_SIZE_INPUT: state::StringKey = state::string("size_input");
const TOOL_LABEL: state::StringKey = state::string("tool_label");
const PEN_PRESSURE: state::BoolKey = state::bool("pressure_enabled");
const PEN_ANTIALIAS: state::BoolKey = state::bool("antialias");
const PEN_STABILIZATION: state::IntKey = state::int("stabilization");
const SUPPORTS_SIZE: state::BoolKey = state::bool("supports_size");
const SUPPORTS_PRESSURE: state::BoolKey = state::bool("supports_pressure");
const SUPPORTS_ANTIALIAS: state::BoolKey = state::bool("supports_antialias");
const SUPPORTS_STABILIZATION: state::BoolKey = state::bool("supports_stabilization");
const HAS_SETTINGS: state::BoolKey = state::bool("has_settings");

const LOG_SIZE_SLIDER_MAX: i32 = 1000;
const MAX_TOOL_SIZE: f32 = 10000.0;

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// Host snapshot を読み取り、表示用の状態へ同期する。
#[plugin_sdk::panel_sync_host]
fn sync_host() {
    let snapshot = host::tool::snapshot();
    let capabilities = host::tool::capabilities();
    let active_tool = snapshot.active_name.clone();
    let size = snapshot.pen_size.max(1);
    set_state_string(PEN_NAME, snapshot.pen_name);
    set_state_string(ACTIVE_TOOL_ID, snapshot.active_id);
    set_state_string(ACTIVE_TOOL_LABEL, snapshot.active_label);
    set_state_string(PROVIDER_PLUGIN_ID, snapshot.provider_plugin_id);
    set_state_string(DRAWING_PLUGIN_ID, snapshot.drawing_plugin_id);
    set_state_i32(PEN_SIZE, size);
    set_state_i32(PEN_SIZE_SLIDER, size_to_slider(size));
    set_state_string(PEN_SIZE_INPUT, size.to_string());
    set_state_string(
        TOOL_LABEL,
        if active_tool.eq_ignore_ascii_case("eraser") {
            "Eraser Width"
        } else if active_tool.eq_ignore_ascii_case("pen") {
            "Pen Width"
        } else {
            "Tool Size"
        },
    );
    set_state_bool(PEN_PRESSURE, host::tool::pen_pressure_enabled());
    set_state_bool(PEN_ANTIALIAS, host::tool::pen_antialias());
    set_state_i32(PEN_STABILIZATION, host::tool::pen_stabilization());
    let supports_size = capabilities.supports_size;
    let supports_pressure = capabilities.supports_pressure_enabled;
    let supports_antialias = capabilities.supports_antialias;
    let supports_stabilization = capabilities.supports_stabilization;
    set_state_bool(SUPPORTS_SIZE, supports_size);
    set_state_bool(SUPPORTS_PRESSURE, supports_pressure);
    set_state_bool(SUPPORTS_ANTIALIAS, supports_antialias);
    set_state_bool(SUPPORTS_STABILIZATION, supports_stabilization);
    set_state_bool(
        HAS_SETTINGS,
        supports_size || supports_pressure || supports_antialias || supports_stabilization,
    );
}

/// サイズ to slider を計算して返す。
fn size_to_slider(size: i32) -> i32 {
    if size <= 1 {
        return 0;
    }
    let normalized = (size as f32).ln() / MAX_TOOL_SIZE.ln();
    (normalized * LOG_SIZE_SLIDER_MAX as f32).round() as i32
}

/// 現在の slider to サイズ を返す。
fn slider_to_size(value: i32) -> u32 {
    let normalized = value.clamp(0, LOG_SIZE_SLIDER_MAX) as f32 / LOG_SIZE_SLIDER_MAX as f32;
    MAX_TOOL_SIZE
        .powf(normalized)
        .round()
        .clamp(1.0, MAX_TOOL_SIZE) as u32
}

/// 入力を解析して サイズ 入力 に変換する。
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

/// 現在の値を サイズ 状態 へ変換する。
fn sync_size_state(size: u32) {
    let clamped = size.clamp(1, MAX_TOOL_SIZE as u32);
    set_state_i32(PEN_SIZE, clamped as i32);
    set_state_i32(PEN_SIZE_SLIDER, size_to_slider(clamped as i32));
    set_state_string(PEN_SIZE_INPUT, clamped.to_string());
}

/// ツール 設定 サイズ に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn set_pen_size(value: i32) {
    let size = slider_to_size(value);
    sync_size_state(size);
    emit_command(&commands::tool::set_size(size));
}

/// ツール 設定 サイズ に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn set_pen_size_text() {
    let value = event_string("value");
    let Ok(size) = parse_size_input(&value) else {
        error("width must be a positive integer");
        return;
    };
    sync_size_state(size);
    emit_command(&commands::tool::set_size(size));
}

/// Tool set_pressure_enabled(
        !host tool pen_pressure_enabled( に対応するコマンドを発行する。
/// ツール 設定 pressure enabled に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn toggle_pressure() {
    emit_command(&commands::tool::set_pressure_enabled(
        !host::tool::pen_pressure_enabled(),
    ));
}

/// ツール 設定 アンチエイリアス に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn toggle_antialias() {
    emit_command(&commands::tool::set_antialias(!host::tool::pen_antialias()));
}

/// ツール 設定 stabilization に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn set_stabilization(value: i32) {
    emit_command(&commands::tool::set_stabilization(value.clamp(0, 100) as u8));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// パネル entrypoints are callable on native targets が期待どおりに動作することを検証する。
    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        set_pen_size(400);
        set_pen_size_text();
        toggle_pressure();
        toggle_antialias();
        set_stabilization(24);
    }

    /// logarithmic slider roundtrips common sizes が期待どおりに動作することを検証する。
    #[test]
    fn logarithmic_slider_roundtrips_common_sizes() {
        for size in [1, 2, 4, 16, 128, 2048, 10000] {
            let slider = size_to_slider(size);
            let restored = slider_to_size(slider) as i32;
            assert!((restored - size).abs() <= 2.max(size / 20));
        }
    }

    /// サイズ 入力 parses and clamps が期待どおりに動作することを検証する。
    #[test]
    fn size_input_parses_and_clamps() {
        assert_eq!(parse_size_input("24"), Ok(24));
        assert_eq!(parse_size_input("0"), Ok(1));
        assert_eq!(parse_size_input("200000"), Ok(10000));
        assert!(parse_size_input("abc").is_err());
    }
}
