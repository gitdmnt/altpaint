use plugin_sdk::{
    runtime::{emit_service, event_string, set_state_i32, set_state_string, state_i32, state_string},
    services,
    state,
};

const INPUT_TEXT: state::StringKey = state::string("input_text");
const FONT_SIZE: state::IntKey = state::int("font_size");
const COLOR_HEX: state::StringKey = state::string("color_hex");
const X: state::IntKey = state::int("x");
const Y: state::IntKey = state::int("y");

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// Host snapshot を読み取り、表示用の状態へ同期する。
#[plugin_sdk::panel_sync_host]
fn sync_host() {}

/// テキスト入力を更新する。
#[plugin_sdk::panel_handler]
fn update_text() {
    let value = event_string("value");
    set_state_string(INPUT_TEXT, &value);
}

/// フォントサイズを更新する。
#[plugin_sdk::panel_handler]
fn update_font_size() {
    let value = event_string("value");
    if let Ok(n) = value.trim().parse::<i32>() {
        set_state_i32(FONT_SIZE, n.max(8).min(200));
    }
}

/// X 座標を更新する。
#[plugin_sdk::panel_handler]
fn update_x() {
    let value = event_string("value");
    if let Ok(n) = value.trim().parse::<i32>() {
        set_state_i32(X, n.max(0));
    }
}

/// Y 座標を更新する。
#[plugin_sdk::panel_handler]
fn update_y() {
    let value = event_string("value");
    if let Ok(n) = value.trim().parse::<i32>() {
        set_state_i32(Y, n.max(0));
    }
}

/// テキストをキャンバスへ描画する。
///
/// 内部でサービス要求を発行します。
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

    /// パネル エントリーポイント are callable が期待どおりに動作することを検証する。
    #[test]
    fn panel_entrypoints_are_callable() {
        init();
        sync_host();
        update_text();
        update_font_size();
        update_x();
        update_y();
        render_text();
    }
}
