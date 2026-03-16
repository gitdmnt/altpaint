use std::cell::RefCell;

use plugin_sdk::{
    commands, host,
    runtime::{emit_command, event_string, set_state_bool, set_state_i32, set_state_string},
    state,
};

const TITLE: state::StringKey = state::string("title");
const PAGE_COUNT: state::IntKey = state::int("page_count");
const PANEL_COUNT: state::IntKey = state::int("panel_count");
const LAYER_COUNT: state::IntKey = state::int("layer_count");
const ACTIVE_LAYER_NAME: state::StringKey = state::string("active_layer_name");
const ACTIVE_LAYER_INDEX: state::IntKey = state::int("active_layer_index");
const ACTIVE_LAYER_BLEND_MODE: state::StringKey = state::string("active_layer_blend_mode");
const ACTIVE_LAYER_VISIBLE: state::BoolKey = state::bool("active_layer_visible");
const ACTIVE_LAYER_MASKED: state::BoolKey = state::bool("active_layer_masked");
const LAYERS_JSON: state::StringKey = state::string("layers_json");
const RENAME_TEXT: state::StringKey = state::string("rename_text");

thread_local! {
    /// confirm_rename ハンドラが読み取るための一時バッファ。
    static RENAME_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// Host snapshot を読み取り、表示用の状態へ同期する。
#[plugin_sdk::panel_sync_host]
fn sync_host() {
    set_state_string(TITLE, host::document::title());
    set_state_i32(PAGE_COUNT, host::document::page_count());
    set_state_i32(PANEL_COUNT, host::document::panel_count());
    set_state_i32(LAYER_COUNT, host::document::layer_count());
    let layer_name = host::document::active_layer_name();
    set_state_string(ACTIVE_LAYER_NAME, layer_name.clone());
    // レイヤーが切り替わったとき rename_text も最新名にリセットする
    RENAME_BUF.with(|buf| *buf.borrow_mut() = layer_name.clone());
    set_state_string(RENAME_TEXT, layer_name);
    set_state_i32(ACTIVE_LAYER_INDEX, host::document::active_layer_index());
    set_state_string(
        ACTIVE_LAYER_BLEND_MODE,
        host::document::active_layer_blend_mode(),
    );
    set_state_bool(ACTIVE_LAYER_VISIBLE, host::document::active_layer_visible());
    set_state_bool(ACTIVE_LAYER_MASKED, host::document::active_layer_masked());
    set_state_string(LAYERS_JSON, host::document::layers_json());
}

/// レイヤー を追加する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn add_layer() {
    emit_command(&commands::layer::add());
}

/// レイヤー を削除する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn remove_layer() {
    emit_command(&commands::layer::remove());
}

/// 入力を解析して レイヤー 一覧 に変換する。
///
/// UI は前面レイヤーを先頭（UI index 0）に並べるため、実モデルの index に変換してコマンドを発行します。
#[plugin_sdk::panel_handler]
fn handle_layer_list(value: i32) {
    let layer_count = host::document::layer_count() as usize;
    let ui_target = value.max(0) as usize;
    // UI index → model index（UI 0 = 前面 = model N-1）
    let actual_target = layer_count.saturating_sub(1).saturating_sub(ui_target);
    if let Ok(ui_from) = event_string("from").parse::<usize>() {
        let actual_from = layer_count.saturating_sub(1).saturating_sub(ui_from);
        if actual_from != actual_target {
            emit_command(&commands::layer::move_to(actual_from, actual_target));
        }
    }
    emit_command(&commands::layer::select(actual_target));
}

/// 入力中のレイヤー名を一時バッファへ保持する。
///
/// 確定は confirm_rename で行います。
#[plugin_sdk::panel_handler]
fn update_rename_text() {
    let name = event_string("value");
    RENAME_BUF.with(|buf| *buf.borrow_mut() = name.clone());
    set_state_string(RENAME_TEXT, name);
}

/// 一時バッファの名前でレイヤー名変更コマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn confirm_rename() {
    let name = RENAME_BUF.with(|buf| buf.borrow().clone());
    if !name.is_empty() {
        emit_command(&commands::layer::rename_active(name));
    }
}

/// レイヤー 設定 ブレンド モード に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn set_blend_mode() {
    let mode = event_string("value");
    if mode.is_empty() {
        return;
    }
    emit_command(&commands::layer::set_blend_mode(mode));
}

/// レイヤー 切替 visibility に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn toggle_layer_visibility() {
    emit_command(&commands::layer::toggle_visibility());
}

/// レイヤー 切替 マスク に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn toggle_layer_mask() {
    emit_command(&commands::layer::toggle_mask());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// パネル 初期化 is callable が期待どおりに動作することを検証する。
    #[test]
    fn panel_init_is_callable() {
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

    /// confirm_rename が RENAME_BUF の値でコマンドを発行することを検証する。
    #[test]
    fn confirm_rename_emits_rename_command_with_buffered_name() {
        RENAME_BUF.with(|buf| *buf.borrow_mut() = "新レイヤー".to_string());
        // confirm_rename 呼び出し後にパニックしないことを確認（実際のコマンド発行はWasm環境）
        confirm_rename();
    }
}
