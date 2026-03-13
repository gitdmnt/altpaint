use plugin_sdk::{
    CommandDescriptor,
    commands::{self, Tool},
    host,
    runtime::{
        emit_command, emit_service, event_string, set_state_bool, set_state_i32, set_state_string,
        state_string, toggle_state,
    },
    services, state,
};
use serde_json::Value;
use std::collections::BTreeMap;

const ACTIVE_TOOL: state::StringKey = state::string("active_tool");
const ACTIVE_TOOL_ID: state::StringKey = state::string("active_tool_id");
const ACTIVE_TOOL_LABEL: state::StringKey = state::string("active_tool_label");
const TOOL_OPTIONS: state::StringKey = state::string("tool_options");
const PROVIDER_PLUGIN_ID: state::StringKey = state::string("provider_plugin_id");
const DRAWING_PLUGIN_ID: state::StringKey = state::string("drawing_plugin_id");
const PEN_NAME: state::StringKey = state::string("pen_name");
const PEN_SIZE: state::IntKey = state::int("pen_size");
const PEN_COUNT: state::IntKey = state::int("pen_count");
const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const PEN_SHORTCUT: state::StringKey = state::string("config.pen_shortcut");
const ERASER_SHORTCUT: state::StringKey = state::string("config.eraser_shortcut");
const BUCKET_SHORTCUT: state::StringKey = state::string("config.bucket_shortcut");
const LASSO_BUCKET_SHORTCUT: state::StringKey = state::string("config.lasso_bucket_shortcut");
const PANEL_RECT_SHORTCUT: state::StringKey = state::string("config.panel_rect_shortcut");
const SIZE_MEMORY: state::StringKey = state::string("config.size_memory");

/// ツール コマンド を構築する。
fn build_tool_command(tool: Tool) -> CommandDescriptor {
    commands::tool::set_active(tool)
}

/// ツール オプション を構築し、失敗時はエラーを返す。
fn build_tool_options(catalog_json: &str) -> String {
    serde_json::from_str::<Vec<Value>>(catalog_json)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?;
            let name = entry.get("name")?.as_str()?;
            Some(format!("{id}:{name}"))
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// Host snapshot を読み取り、表示用の状態へ同期する。
#[plugin_sdk::panel_sync_host]
fn sync_host() {
    set_state_string(ACTIVE_TOOL, host::tool::active_name());
    set_state_string(ACTIVE_TOOL_ID, host::tool::active_id());
    set_state_string(ACTIVE_TOOL_LABEL, host::tool::active_label());
    set_state_string(
        TOOL_OPTIONS,
        build_tool_options(&host::tool::catalog_json()),
    );
    set_state_string(PROVIDER_PLUGIN_ID, host::tool::active_provider_plugin_id());
    set_state_string(DRAWING_PLUGIN_ID, host::tool::active_drawing_plugin_id());
    set_state_string(PEN_NAME, host::tool::pen_name());
    set_state_i32(PEN_SIZE, host::tool::pen_size());
    set_state_i32(PEN_COUNT, host::tool::pen_count());
}

/// ツール を選択状態へ更新する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn select_tool() {
    let tool_id = event_string("value");
    if tool_id.trim().is_empty() {
        return;
    }
    emit_command(&commands::tool::select_tool(tool_id.trim()));
}

/// ショートカット 用のショートカット入力を受け付ける状態にする。
fn capture_shortcut(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
    set_state_bool(SHOW_SHORTCUTS, true);
}

/// ショートカット matches を計算して返す。
fn shortcut_matches(configured: &str, incoming: &str) -> bool {
    !configured.is_empty() && configured.eq_ignore_ascii_case(incoming)
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn size_binding_key(tool_name: &str, pen_id: &str) -> Option<String> {
    match tool_name.to_ascii_lowercase().as_str() {
        "pen" | "eraser" => Some(format!("{}:{pen_id}", tool_name.to_ascii_lowercase())),
        _ => None,
    }
}

/// 入力を解析して サイズ メモリ に変換する。
fn parse_size_memory(serialized: &str) -> BTreeMap<String, u32> {
    serde_json::from_str(serialized).unwrap_or_default()
}

/// 現在の値を サイズ メモリ へ変換する。
fn serialize_size_memory(memory: &BTreeMap<String, u32>) -> String {
    serde_json::to_string(memory).unwrap_or_else(|_| "{}".to_string())
}

/// 入力を解析して ペン ids に変換する。
fn host_pen_ids() -> Vec<String> {
    serde_json::from_str::<Vec<Value>>(&host::tool::pen_presets_json())
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect()
}

/// 現在 サイズ を更新する。
fn remember_current_size() {
    let Some(key) = size_binding_key(&host::tool::active_name(), &host::tool::pen_id()) else {
        return;
    };
    let mut memory = parse_size_memory(&state_string(SIZE_MEMORY));
    memory.insert(key, host::tool::pen_size().max(1) as u32);
    set_state_string(SIZE_MEMORY, serialize_size_memory(&memory));
}

/// ツール 設定 サイズ に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
fn restore_size(tool_name: &str, pen_id: &str) {
    let Some(key) = size_binding_key(tool_name, pen_id) else {
        return;
    };
    let memory = parse_size_memory(&state_string(SIZE_MEMORY));
    if let Some(size) = memory.get(&key).copied() {
        emit_command(&commands::tool::set_size(size.max(1)));
    }
}

/// 構築 ツール コマンド に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
fn switch_tool_with_size_restore(tool: Tool) {
    remember_current_size();
    let pen_id = host::tool::pen_id();
    emit_command(&build_tool_command(tool));
    restore_size(tool.as_str(), &pen_id);
}

/// ツール 選択 前 ペン に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
fn switch_pen_with_size_restore(delta: isize) {
    remember_current_size();
    let pen_ids = host_pen_ids();
    let current_index = host::tool::pen_index().max(0) as usize;
    let Some(target_pen_id) = pen_ids
        .get((current_index as isize + delta).rem_euclid(pen_ids.len().max(1) as isize) as usize)
    else {
        if delta < 0 {
            emit_command(&commands::tool::select_previous_pen());
        } else {
            emit_command(&commands::tool::select_next_pen());
        }
        return;
    };

    if delta < 0 {
        emit_command(&commands::tool::select_previous_pen());
    } else {
        emit_command(&commands::tool::select_next_pen());
    }
    restore_size(&host::tool::active_name(), target_pen_id);
}

/// ペン をアクティブ化する。
#[plugin_sdk::panel_handler]
fn activate_pen() {
    switch_tool_with_size_restore(Tool::Pen);
}

/// 消しゴム をアクティブ化する。
#[plugin_sdk::panel_handler]
fn activate_eraser() {
    switch_tool_with_size_restore(Tool::Eraser);
}

/// Bucket をアクティブ化する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn activate_bucket() {
    emit_command(&build_tool_command(Tool::Bucket));
}

/// 投げ縄 bucket をアクティブ化する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn activate_lasso_bucket() {
    emit_command(&build_tool_command(Tool::LassoBucket));
}

/// パネル 矩形 をアクティブ化する。
///
/// 内部でコマンドを発行します。
#[plugin_sdk::panel_handler]
fn activate_panel_rect() {
    emit_command(&build_tool_command(Tool::PanelRect));
}

/// ペン をひとつ前へ切り替える。
#[plugin_sdk::panel_handler]
fn previous_pen() {
    switch_pen_with_size_restore(-1);
}

/// ペン をひとつ先へ切り替える。
#[plugin_sdk::panel_handler]
fn next_pen() {
    switch_pen_with_size_restore(1);
}

/// ツール カタログ 再読込 ペン presets に対応するサービス要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn reload_pens() {
    emit_service(&services::tool_catalog::reload_pen_presets());
}

/// ツール カタログ 読み込み ペン presets に対応するサービス要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn import_pens() {
    emit_service(&services::tool_catalog::import_pen_presets());
}

/// 状態上の shortcuts を切り替える。
#[plugin_sdk::panel_handler]
fn toggle_shortcuts() {
    toggle_state(SHOW_SHORTCUTS);
}

/// ペン ショートカット 用のショートカット入力を受け付ける状態にする。
#[plugin_sdk::panel_handler]
fn capture_pen_shortcut() {
    capture_shortcut("pen");
}

/// 消しゴム ショートカット 用のショートカット入力を受け付ける状態にする。
#[plugin_sdk::panel_handler]
fn capture_eraser_shortcut() {
    capture_shortcut("eraser");
}

/// キーボード入力やショートカットに応じて状態と処理を切り替える。
#[plugin_sdk::panel_handler]
fn keyboard() {
    let shortcut = event_string("shortcut");
    if shortcut.is_empty() {
        return;
    }

    match state_string(CAPTURE_TARGET).as_str() {
        "pen" => {
            set_state_string(PEN_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            return;
        }
        "eraser" => {
            set_state_string(ERASER_SHORTCUT, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            return;
        }
        _ => {}
    }

    if shortcut_matches(&state_string(PEN_SHORTCUT), &shortcut) {
        activate_pen();
        return;
    }
    if shortcut_matches(&state_string(ERASER_SHORTCUT), &shortcut) {
        activate_eraser();
        return;
    }
    if shortcut_matches(&state_string(BUCKET_SHORTCUT), &shortcut) {
        activate_bucket();
        return;
    }
    if shortcut_matches(&state_string(LASSO_BUCKET_SHORTCUT), &shortcut) {
        activate_lasso_bucket();
        return;
    }
    if shortcut_matches(&state_string(PANEL_RECT_SHORTCUT), &shortcut) {
        activate_panel_rect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ツール コマンド embeds requested ツール 名前 が期待どおりに動作することを検証する。
    #[test]
    fn tool_command_embeds_requested_tool_name() {
        let command = build_tool_command(Tool::Eraser);

        assert_eq!(command.name, "tool.set_active");
        assert_eq!(
            command.payload.get("tool").and_then(|value| value.as_str()),
            Some("eraser")
        );
    }

    /// パネル entrypoints are callable on native targets が期待どおりに動作することを検証する。
    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        activate_pen();
        activate_eraser();
        activate_bucket();
        activate_lasso_bucket();
        activate_panel_rect();
        select_tool();
        previous_pen();
        next_pen();
        reload_pens();
        import_pens();
        toggle_shortcuts();
        capture_pen_shortcut();
        capture_eraser_shortcut();
        keyboard();
    }

    /// ショートカット match is case insensitive が期待どおりに動作することを検証する。
    #[test]
    fn shortcut_match_is_case_insensitive() {
        assert!(shortcut_matches("B", "b"));
    }

    /// サイズ メモリ roundtrips JSON が期待どおりに動作することを検証する。
    #[test]
    fn size_memory_roundtrips_json() {
        let mut memory = BTreeMap::new();
        memory.insert("pen:builtin.round-pen".to_string(), 4);
        memory.insert("eraser:builtin.round-pen".to_string(), 12);

        let serialized = serialize_size_memory(&memory);
        assert_eq!(parse_size_memory(&serialized), memory);
    }

    /// サイズ binding key supports ペン and 消しゴム が期待どおりに動作することを検証する。
    #[test]
    fn size_binding_key_supports_pen_and_eraser() {
        assert_eq!(
            size_binding_key("pen", "builtin.round-pen"),
            Some("pen:builtin.round-pen".to_string())
        );
        assert_eq!(
            size_binding_key("eraser", "builtin.round-pen"),
            Some("eraser:builtin.round-pen".to_string())
        );
        assert_eq!(size_binding_key("bucket", "builtin.round-pen"), None);
    }

    /// ツール オプション are built from カタログ JSON が期待どおりに動作することを検証する。
    #[test]
    fn tool_options_are_built_from_catalog_json() {
        let options = build_tool_options(
            r#"[
  {"id":"builtin.pen","name":"Pen"},
  {"id":"builtin.eraser","name":"Eraser"}
]"#,
        );

        assert_eq!(options, "builtin.pen:Pen|builtin.eraser:Eraser");
    }

    /// 読み込み コマンド uses expected 名前 が期待どおりに動作することを検証する。
    #[test]
    fn import_command_uses_expected_name() {
        let command = services::tool_catalog::import_pen_presets();

        assert_eq!(command.name, "tool_catalog.import_pen_presets");
        assert!(command.payload.is_empty());
    }

    /// 選択 ツール コマンド uses expected 名前 が期待どおりに動作することを検証する。
    #[test]
    fn select_tool_command_uses_expected_name() {
        let command = commands::tool::select_tool("builtin.pen");

        assert_eq!(command.name, "tool.select");
        assert_eq!(
            command
                .payload
                .get("tool_id")
                .and_then(|value| value.as_str()),
            Some("builtin.pen")
        );
    }
}
