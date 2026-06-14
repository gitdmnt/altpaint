//! `builtin.tool-palette` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    RequestDescriptor,
    commands::{self, Tool},
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
use std::collections::BTreeMap;

const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const PEN_SHORTCUT: state::StringKey = state::string("config.pen_shortcut");
const ERASER_SHORTCUT: state::StringKey = state::string("config.eraser_shortcut");
const BUCKET_SHORTCUT: state::StringKey = state::string("config.bucket_shortcut");
const LASSO_BUCKET_SHORTCUT: state::StringKey = state::string("config.lasso_bucket_shortcut");
const KOMA_RECT_SHORTCUT: state::StringKey = state::string("config.koma_rect_shortcut");
const SIZE_MEMORY: state::StringKey = state::string("config.size_memory");
const LAST_IMPORT_SUMMARY: state::StringKey = state::string("config.last_import_summary");
const LAST_IMPORT_PREVIEW: state::StringKey = state::string("config.last_import_preview");
const LAST_IMPORT_ISSUES: state::StringKey = state::string("config.last_import_issues");

/// ショートカットスロット ID。
const SLOT_PEN: &str = "pen";
const SLOT_ERASER: &str = "eraser";
const SLOT_BUCKET: &str = "bucket";
const SLOT_LASSO_BUCKET: &str = "lasso_bucket";
const SLOT_KOMA_RECT: &str = "koma_rect";

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

fn build_tool_command(tool: Tool) -> RequestDescriptor {
    commands::tool::set_active(tool)
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

// === サイズ記憶 (config.size_memory blob; BL-149 のホスト移管は tools feature の所管) ===

fn size_binding_key(tool_name: &str, pen_id: &str) -> Option<String> {
    match tool_name.to_ascii_lowercase().as_str() {
        SLOT_PEN | SLOT_ERASER => Some(format!("{}:{pen_id}", tool_name.to_ascii_lowercase())),
        _ => None,
    }
}

fn parse_size_memory(serialized: &str) -> BTreeMap<String, u32> {
    serde_json::from_str(serialized).unwrap_or_default()
}

fn serialize_size_memory(memory: &BTreeMap<String, u32>) -> String {
    serde_json::to_string(memory).unwrap_or_else(|_| "{}".to_string())
}

fn host_pen_ids() -> Vec<String> {
    let json = host_tool().map(|tool| tool.pen_presets_json).unwrap_or_default();
    serde_json::from_str::<Vec<Value>>(&json)
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

fn remember_current_size() {
    let Some(tool) = host_tool() else {
        return;
    };
    let Some(key) = size_binding_key(&tool.active, &tool.pen_id) else {
        return;
    };
    let mut memory = parse_size_memory(&state_string(SIZE_MEMORY));
    memory.insert(key, (tool.pen_size.max(1)) as u32);
    set_state_string(SIZE_MEMORY, serialize_size_memory(&memory));
}

fn restore_size(tool_name: &str, pen_id: &str) {
    let Some(key) = size_binding_key(tool_name, pen_id) else {
        return;
    };
    let memory = parse_size_memory(&state_string(SIZE_MEMORY));
    if let Some(size) = memory.get(&key).copied() {
        emit_request(&commands::tool::set_size(size.max(1)));
    }
}

fn switch_tool_with_size_restore(tool: Tool) {
    remember_current_size();
    let pen_id = host_tool().map(|tool| tool.pen_id).unwrap_or_default();
    emit_request(&build_tool_command(tool));
    restore_size(tool.as_str(), &pen_id);
}

fn switch_pen_with_size_restore(delta: isize) {
    remember_current_size();
    let pen_ids = host_pen_ids();
    let current_index = host_tool().map(|tool| tool.pen_index.max(0) as usize).unwrap_or(0);
    let target_index =
        (current_index as isize + delta).rem_euclid(pen_ids.len().max(1) as isize) as usize;
    if delta < 0 {
        emit_request(&commands::tool::select_previous_pen());
    } else {
        emit_request(&commands::tool::select_next_pen());
    }
    if let Some(target_pen_id) = pen_ids.get(target_index) {
        let active = host_tool().map(|tool| tool.active).unwrap_or_default();
        restore_size(&active, target_pen_id);
    }
}

#[panel_sdk::panel_handler]
fn activate_pen() {
    switch_tool_with_size_restore(Tool::Pen);
}

#[panel_sdk::panel_handler]
fn activate_eraser() {
    switch_tool_with_size_restore(Tool::Eraser);
}

#[panel_sdk::panel_handler]
fn activate_bucket() {
    emit_request(&build_tool_command(Tool::Bucket));
}

#[panel_sdk::panel_handler]
fn activate_lasso_bucket() {
    emit_request(&build_tool_command(Tool::LassoBucket));
}

#[panel_sdk::panel_handler]
fn activate_koma_rect() {
    emit_request(&build_tool_command(Tool::KomaRect));
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
    switch_pen_with_size_restore(-1);
}

#[panel_sdk::panel_handler]
fn next_pen() {
    switch_pen_with_size_restore(1);
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
    fn tool_command_embeds_tool_name() {
        let c = build_tool_command(Tool::Eraser);
        assert_eq!(c.name, panel_sdk::names::tool::SET_ACTIVE);
    }

    #[test]
    fn build_tool_options_parses_catalog() {
        let opts = build_tool_options(r#"[{"id":"a","name":"A"}]"#);
        assert_eq!(opts, vec![("a".to_string(), "A".to_string())]);
    }

    #[test]
    fn size_memory_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert("pen:p".to_string(), 4);
        let s = serialize_size_memory(&m);
        assert_eq!(parse_size_memory(&s), m);
    }
}
