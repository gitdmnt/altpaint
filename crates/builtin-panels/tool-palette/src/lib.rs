//! `builtin.tool-palette` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    RequestDescriptor,
    commands,
    dom::{query_selector, render_options, set_button_active, set_inner_html, set_text, set_visible},
    host_state::{ToolState, section},
    runtime::{
        StatePatchBuffer, emit_request, host_section, set_state_bool, set_state_string, state_bool,
        state_string,
    },
    serde::Deserialize,
    serde_json::Value,
    services,
    shortcut::{Outcome, ShortcutRegistry},
    state,
};

const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const PEN_SHORTCUT: state::StringKey = state::string("config.pen_shortcut");
const ERASER_SHORTCUT: state::StringKey = state::string("config.eraser_shortcut");
const BUCKET_SHORTCUT: state::StringKey = state::string("config.bucket_shortcut");
const LASSO_BUCKET_SHORTCUT: state::StringKey = state::string("config.lasso_bucket_shortcut");
const KOMA_RECT_SHORTCUT: state::StringKey = state::string("config.koma_rect_shortcut");
const LAST_IMPORT_SUMMARY: state::StringKey = state::string("config.last_import_summary");
const LAST_IMPORT_PREVIEW: state::StringKey = state::string("config.last_import_preview");
const LAST_IMPORT_ISSUES: state::StringKey = state::string("config.last_import_issues");

/// ショートカットスロット ID。host `ToolState.active` の値と一致する。
const SLOT_PEN: &str = "pen";
const SLOT_ERASER: &str = "eraser";
const SLOT_BUCKET: &str = "bucket";
const SLOT_LASSO_BUCKET: &str = "lasso_bucket";
const SLOT_KOMA_RECT: &str = "koma_rect";

/// ビルトインツールのカタログ id (P28: `tool.select` の `tool_id` payload に使う)。
fn slot_catalog_id(slot: &str) -> Option<&'static str> {
    match slot {
        SLOT_PEN => Some("builtin.pen"),
        SLOT_ERASER => Some("builtin.eraser"),
        SLOT_BUCKET => Some("builtin.bucket"),
        SLOT_LASSO_BUCKET => Some("builtin.lasso-bucket"),
        SLOT_KOMA_RECT => Some("builtin.koma-rect"),
        _ => None,
    }
}

/// セレクト入力 payload (`altp:select:*` は `event_payload.value` を文字列で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SelectValue {
    #[serde(default)]
    value: String,
}

/// keyboard イベント payload (`event_payload.shortcut`)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct KeyEvent {
    #[serde(default)]
    shortcut: String,
}

/// スロット ID からカタログ id ベースの `tool.select` request を作る (P28)。
fn build_tool_command(slot: &str) -> Option<RequestDescriptor> {
    slot_catalog_id(slot).map(commands::tool::select_tool)
}

fn build_tool_options(catalog_json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<Vec<Value>>(catalog_json)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let name = entry.get("name")?.as_str()?.to_string();
            Some((id, name))
        })
        .collect()
}

/// 現在の tool セクションを型付き DTO として取得する (BL-142)。
fn host_tool() -> Option<ToolState> {
    host_section::<ToolState>(section::TOOL)
}

fn render_dom() {
    let Some(tool) = host_tool() else {
        return;
    };
    let show_shortcuts = state_bool(SHOW_SHORTCUTS);
    let capture_target = state_string(CAPTURE_TARGET);

    set_text("#active-tool-label", &tool.active_label);
    set_text("#active-tool-id", &tool.active_id);
    set_text("#provider-plugin-id", &tool.active_provider_plugin_id);
    set_text("#drawing-plugin-id", &tool.active_drawing_plugin_id);
    set_text("#pen-name", &tool.pen_name);
    set_text("#pen-size", &tool.pen_size.to_string());
    set_text("#pen-count", &tool.pen_count.to_string());
    set_text("#active-child-tool-label", &tool.active_child_tool_label);

    if let Some(select) = query_selector("#tool\\.catalog") {
        let pairs = build_tool_options(&tool.catalog_json);
        let options = pairs.iter().map(|(id, name)| (id.as_str(), name.as_str()));
        set_inner_html(select, &render_options(options, &tool.active_id));
    }

    set_visible("#child-tools-section", tool.child_tools_json.trim() != "[]");

    set_button_active("#tool\\.pen", tool.active == SLOT_PEN);
    set_button_active("#tool\\.eraser", tool.active == SLOT_ERASER);
    set_button_active("#tool\\.bucket", tool.active == SLOT_BUCKET);
    set_button_active("#tool\\.lasso-bucket", tool.active == SLOT_LASSO_BUCKET);
    set_button_active("#tool\\.koma-rect", tool.active == SLOT_KOMA_RECT);

    set_visible("#shortcuts-section", show_shortcuts);
    set_button_active("#tool\\.shortcuts", show_shortcuts);
    set_button_active("#tool\\.shortcut\\.pen", capture_target == SLOT_PEN);
    set_button_active("#tool\\.shortcut\\.eraser", capture_target == SLOT_ERASER);
    set_visible("#capture-hint", !capture_target.is_empty());

    set_text("#pen-shortcut", &state_string(PEN_SHORTCUT));
    set_text("#eraser-shortcut", &state_string(ERASER_SHORTCUT));
    set_text("#bucket-shortcut", &state_string(BUCKET_SHORTCUT));
    set_text("#lasso-bucket-shortcut", &state_string(LASSO_BUCKET_SHORTCUT));
    set_text("#koma-rect-shortcut", &state_string(KOMA_RECT_SHORTCUT));

    let summary = state_string(LAST_IMPORT_SUMMARY);
    set_visible("#import-section", !summary.is_empty());
    set_text("#import-summary", &summary);
    let preview = state_string(LAST_IMPORT_PREVIEW);
    set_visible("#import-preview-row", !preview.is_empty());
    set_text("#import-preview", &preview);
    let issues = state_string(LAST_IMPORT_ISSUES);
    set_visible("#import-issues-row", !issues.is_empty());
    set_text("#import-issues", &issues);
}

#[panel_sdk::panel_init]
fn init() {
    // DSL 時代の初期 state 宣言に相当するデフォルトショートカット。
    // 永続化済み config がある場合は registry の restore_persistent_config が
    // init 後に config 全体を上書きするため、ここは初回起動時の既定値のみ担う。
    let mut defaults = StatePatchBuffer::new();
    defaults.set_string(PEN_SHORTCUT.as_ref(), "P");
    defaults.set_string(ERASER_SHORTCUT.as_ref(), "E");
    defaults.set_string(BUCKET_SHORTCUT.as_ref(), "G");
    defaults.set_string(LASSO_BUCKET_SHORTCUT.as_ref(), "Shift+G");
    defaults.set_string(KOMA_RECT_SHORTCUT.as_ref(), "K");
    defaults.apply();
    render_dom();
}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    render_dom();
}

#[panel_sdk::panel_handler]
fn select_tool(payload: SelectValue) {
    let tool_id = payload.value;
    if tool_id.trim().is_empty() {
        return;
    }
    emit_request(&commands::tool::select_tool(tool_id.trim()));
}

fn capture_shortcut(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
    set_state_bool(SHOW_SHORTCUTS, true);
    render_dom();
}

// === ツール別サイズ記憶 ===
//
// 記憶の保持・キー計算・退避/復元はすべてホスト `EditorSession` が担う (BL-149)。
// パネルは「サイズを覚える経路か」だけを remembering コマンドで表明する。ペン/消しゴム
// ボタンと前後ペン切替が記憶経路、ドロップダウン選択とバケツ系は非記憶経路。

/// カタログ id ベースでツールを切り替え、ホストにサイズ記憶を退避/復元させる。
fn activate_slot_remembering(slot: &str) {
    if let Some(catalog_id) = slot_catalog_id(slot) {
        emit_request(&commands::tool::select_tool_remembering(catalog_id));
    }
}

/// カタログ id ベースでツールを切り替える (size 記憶なし; P28)。
fn activate_slot(slot: &str) {
    if let Some(command) = build_tool_command(slot) {
        emit_request(&command);
    }
}

#[panel_sdk::panel_handler]
fn activate_pen() {
    activate_slot_remembering(SLOT_PEN);
}

#[panel_sdk::panel_handler]
fn activate_eraser() {
    activate_slot_remembering(SLOT_ERASER);
}

#[panel_sdk::panel_handler]
fn activate_bucket() {
    activate_slot(SLOT_BUCKET);
}

#[panel_sdk::panel_handler]
fn activate_lasso_bucket() {
    activate_slot(SLOT_LASSO_BUCKET);
}

#[panel_sdk::panel_handler]
fn activate_koma_rect() {
    activate_slot(SLOT_KOMA_RECT);
}

#[panel_sdk::panel_handler]
fn select_child_tool(payload: SelectValue) {
    let child_id = payload.value;
    if child_id.trim().is_empty() {
        return;
    }
    emit_request(&commands::tool::select_child_tool(child_id.trim()));
}

#[panel_sdk::panel_handler]
fn previous_pen() {
    emit_request(&commands::tool::select_previous_pen_remembering());
}

#[panel_sdk::panel_handler]
fn next_pen() {
    emit_request(&commands::tool::select_next_pen_remembering());
}

#[panel_sdk::panel_handler]
fn reload_pens() {
    emit_request(&services::tool_catalog::reload_pen_presets());
}

#[panel_sdk::panel_handler]
fn import_pens() {
    emit_request(&services::tool_catalog::import_pen_presets());
}

#[panel_sdk::panel_handler]
fn toggle_shortcuts() {
    set_state_bool(SHOW_SHORTCUTS, !state_bool(SHOW_SHORTCUTS));
    render_dom();
}

#[panel_sdk::panel_handler]
fn capture_pen_shortcut() {
    capture_shortcut(SLOT_PEN);
}

#[panel_sdk::panel_handler]
fn capture_eraser_shortcut() {
    capture_shortcut(SLOT_ERASER);
}

/// 永続 config から SDK ショートカットレジストリ (BL-144) を構築する。
fn build_registry() -> ShortcutRegistry {
    let mut registry = ShortcutRegistry::new();
    registry.define(SLOT_PEN, state_string(PEN_SHORTCUT));
    registry.define(SLOT_ERASER, state_string(ERASER_SHORTCUT));
    registry.define(SLOT_BUCKET, state_string(BUCKET_SHORTCUT));
    registry.define(SLOT_LASSO_BUCKET, state_string(LASSO_BUCKET_SHORTCUT));
    registry.define(SLOT_KOMA_RECT, state_string(KOMA_RECT_SHORTCUT));
    let capture_target = state_string(CAPTURE_TARGET);
    if !capture_target.is_empty() {
        registry.begin_capture(capture_target);
    }
    registry
}

/// capture 中の slot に割り当てられたバインディングを config へ保存する。
/// capture 対象は pen / eraser のみ (UI に capture ボタンがあるのはこの 2 つ)。
fn save_slot_binding(slot: &str, shortcut: &str) {
    match slot {
        SLOT_PEN => set_state_string(PEN_SHORTCUT, shortcut),
        SLOT_ERASER => set_state_string(ERASER_SHORTCUT, shortcut),
        _ => {}
    }
}

/// 発火した slot のツールをアクティブにする (BL-144 Triggered 処理)。
fn run_slot_action(slot: &str) {
    match slot {
        SLOT_PEN => activate_pen(),
        SLOT_ERASER => activate_eraser(),
        SLOT_BUCKET => activate_bucket(),
        SLOT_LASSO_BUCKET => activate_lasso_bucket(),
        SLOT_KOMA_RECT => activate_koma_rect(),
        _ => {}
    }
}

#[panel_sdk::panel_handler]
fn keyboard(payload: KeyEvent) {
    if payload.shortcut.is_empty() {
        return;
    }
    // BL-144: capture→割当→マッチを SDK ショートカットレジストリへ集約する。
    let mut registry = build_registry();
    match registry.handle_key(&payload.shortcut) {
        Outcome::Assigned { slot, shortcut } => {
            save_slot_binding(&slot, &shortcut);
            set_state_string(CAPTURE_TARGET, "");
            render_dom();
        }
        Outcome::Triggered { slot } => run_slot_action(&slot),
        Outcome::Ignored => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        select_tool(SelectValue::default()),
        activate_pen(),
        activate_eraser(),
        activate_bucket(),
        activate_lasso_bucket(),
        activate_koma_rect(),
        select_child_tool(SelectValue::default()),
        previous_pen(),
        next_pen(),
        reload_pens(),
        import_pens(),
        toggle_shortcuts(),
        capture_pen_shortcut(),
        capture_eraser_shortcut(),
        keyboard(KeyEvent::default()),
    });

    #[test]
    fn tool_command_uses_catalog_id_select() {
        let c = build_tool_command(SLOT_ERASER).expect("known slot");
        assert_eq!(c.name, panel_sdk::names::tool::SELECT);
        assert_eq!(
            c.payload.get("tool_id"),
            Some(&panel_sdk::serde_json::json!("builtin.eraser"))
        );
    }

    #[test]
    fn build_tool_options_parses_catalog() {
        let opts = build_tool_options(r#"[{"id":"a","name":"A"}]"#);
        assert_eq!(opts, vec![("a".to_string(), "A".to_string())]);
    }

    /// ペン/消しゴムボタンは記憶付き select_tool を発行する (BL-149)。
    #[test]
    fn activate_pen_emits_remembering_select() {
        let c = commands::tool::select_tool_remembering("builtin.pen");
        assert_eq!(c.name, panel_sdk::names::tool::SELECT);
        assert_eq!(
            c.payload.get("remember_size"),
            Some(&panel_sdk::serde_json::json!(true))
        );
    }
}
