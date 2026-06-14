//! `builtin.app-actions` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    RequestDescriptor,
    dom::{
        parse_option_list, query_selector, render_options, set_attribute, set_button_active,
        set_inner_html, set_text, set_visible,
    },
    runtime::{
        StatePatchBuffer, emit_request, error, set_state_bool, set_state_string, state_bool,
        state_string,
    },
    serde::Deserialize,
    services,
    shortcut::{Outcome, ShortcutRegistry},
    state,
};

const SHOW_NEW: state::BoolKey = state::bool("show_new");
const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const NEW_WIDTH: state::StringKey = state::string("new_width");
const NEW_HEIGHT: state::StringKey = state::string("new_height");
const SELECTED_TEMPLATE: state::StringKey = state::string("selected_template");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const TEMPLATE_OPTIONS: state::StringKey = state::string("config.template_options");
const DEFAULT_TEMPLATE_SIZE: state::StringKey = state::string("config.default_template_size");
const NEW_SHORTCUT: state::StringKey = state::string("config.new_shortcut");
const SAVE_SHORTCUT: state::StringKey = state::string("config.save_shortcut");
const SAVE_AS_SHORTCUT: state::StringKey = state::string("config.save_as_shortcut");
const OPEN_SHORTCUT: state::StringKey = state::string("config.open_shortcut");

/// ショートカットスロット ID (capture_target / 設定 config キーと対応する)。
const SLOT_NEW: &str = "new";
const SLOT_SAVE: &str = "save";
const SLOT_SAVE_AS: &str = "save_as";
const SLOT_OPEN: &str = "open";

/// テキスト/セレクト入力 payload (`altp:input:*` / `altp:select:*` は
/// `event_payload.value` を文字列で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct TextValue {
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

fn parse_dimension(value: &str) -> Result<usize, &'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("width and height are required");
    }
    trimmed
        .parse::<usize>()
        .map_err(|_| "width and height must be positive integers")
}

fn build_new_project_command(width: &str, height: &str) -> Result<RequestDescriptor, &'static str> {
    let width = parse_dimension(width)?;
    let height = parse_dimension(height)?;
    Ok(services::project_io::new_document_sized(width, height))
}

fn apply_template_size(size: &str) -> Result<(), &'static str> {
    let normalized = size.trim();
    let (width, height) = normalized
        .split_once('x')
        .ok_or("template size must be WIDTHxHEIGHT")?;
    let width = parse_dimension(width)?;
    let height = parse_dimension(height)?;
    let mut batch = StatePatchBuffer::new();
    batch.set_string(NEW_WIDTH.as_ref(), width.to_string());
    batch.set_string(NEW_HEIGHT.as_ref(), height.to_string());
    batch.set_string(SELECTED_TEMPLATE.as_ref(), normalized);
    batch.apply();
    Ok(())
}

/// 永続 config から SDK ショートカットレジストリ (BL-144) を構築する。
///
/// slot のバインディングはパネルローカル config を唯一の真実とし、capture 中なら
/// その slot を capture モードへ復元する。
fn build_registry() -> ShortcutRegistry {
    let mut registry = ShortcutRegistry::new();
    registry.define(SLOT_NEW, state_string(NEW_SHORTCUT));
    registry.define(SLOT_SAVE, state_string(SAVE_SHORTCUT));
    registry.define(SLOT_SAVE_AS, state_string(SAVE_AS_SHORTCUT));
    registry.define(SLOT_OPEN, state_string(OPEN_SHORTCUT));
    let capture_target = state_string(CAPTURE_TARGET);
    if !capture_target.is_empty() {
        registry.begin_capture(capture_target);
    }
    registry
}

/// slot のバインディングを対応する config キーへ保存する (BL-144 Assigned 処理)。
fn save_slot_binding(slot: &str, shortcut: &str) {
    match slot {
        SLOT_NEW => set_state_string(NEW_SHORTCUT, shortcut),
        SLOT_SAVE => set_state_string(SAVE_SHORTCUT, shortcut),
        SLOT_SAVE_AS => set_state_string(SAVE_AS_SHORTCUT, shortcut),
        SLOT_OPEN => set_state_string(OPEN_SHORTCUT, shortcut),
        _ => {}
    }
}

/// 発火した slot のアクションを実行する (BL-144 Triggered 処理)。
fn run_slot_action(slot: &str) {
    match slot {
        SLOT_NEW => show_new_form(),
        SLOT_SAVE => save_project(),
        SLOT_SAVE_AS => save_project_as(),
        SLOT_OPEN => load_project(),
        _ => {}
    }
}

fn render_dom() {
    let show_new = state_bool(SHOW_NEW);
    let show_shortcuts = state_bool(SHOW_SHORTCUTS);
    let capture_target = state_string(CAPTURE_TARGET);

    set_visible("#new-section", show_new);
    set_visible("#shortcuts-section", show_shortcuts);
    set_visible("#capture-hint", !capture_target.is_empty());

    set_text("#new-shortcut", &state_string(NEW_SHORTCUT));
    set_text("#save-shortcut", &state_string(SAVE_SHORTCUT));
    set_text("#save-as-shortcut", &state_string(SAVE_AS_SHORTCUT));
    set_text("#open-shortcut", &state_string(OPEN_SHORTCUT));

    if let Some(input) = query_selector("#app\\.new\\.width") {
        set_attribute(input, "value", &state_string(NEW_WIDTH));
    }
    if let Some(input) = query_selector("#app\\.new\\.height") {
        set_attribute(input, "value", &state_string(NEW_HEIGHT));
    }

    if let Some(select) = query_selector("#app\\.new\\.template") {
        // BL-105: 構造化 JSON 配列 [{size,label}] を読み、option を構築する。
        let raw = state_string(TEMPLATE_OPTIONS);
        let selected = state_string(SELECTED_TEMPLATE);
        let pairs = parse_option_list(&raw, "size", "label");
        let options = pairs
            .iter()
            .map(|(size, label)| (size.as_str(), label.as_str()));
        set_inner_html(select, &render_options(options, &selected));
    }

    set_button_active("#app\\.shortcuts", show_shortcuts);
    set_button_active("#app\\.shortcut\\.new", capture_target == SLOT_NEW);
    set_button_active("#app\\.shortcut\\.save", capture_target == SLOT_SAVE);
    set_button_active("#app\\.shortcut\\.save_as", capture_target == SLOT_SAVE_AS);
    set_button_active("#app\\.shortcut\\.open", capture_target == SLOT_OPEN);
}

#[panel_sdk::panel_init]
fn init() {
    // DSL 時代の初期 state 宣言に相当するデフォルトショートカット。
    // 永続化済み config がある場合は registry の restore_persistent_config が
    // init 後に config 全体を上書きするため、ここは初回起動時の既定値のみ担う。
    let mut defaults = StatePatchBuffer::new();
    defaults.set_string(NEW_SHORTCUT.as_ref(), "Ctrl+N");
    defaults.set_string(SAVE_SHORTCUT.as_ref(), "Ctrl+S");
    defaults.set_string(SAVE_AS_SHORTCUT.as_ref(), "Ctrl+Shift+S");
    defaults.set_string(OPEN_SHORTCUT.as_ref(), "Ctrl+O");
    defaults.apply();
    render_dom();
}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    render_dom();
}

fn capture_shortcut(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
    set_state_bool(SHOW_SHORTCUTS, true);
    render_dom();
}

#[panel_sdk::panel_handler]
fn show_new_form() {
    let selected = state_string(SELECTED_TEMPLATE);
    let fallback = state_string(DEFAULT_TEMPLATE_SIZE);
    let template_size = if selected.trim().is_empty() {
        fallback
    } else {
        selected
    };
    let _ = apply_template_size(&template_size);
    set_state_bool(SHOW_NEW, true);
    render_dom();
}

#[panel_sdk::panel_handler]
fn cancel_forms() {
    set_state_bool(SHOW_NEW, false);
    render_dom();
}

#[panel_sdk::panel_handler]
fn toggle_shortcuts() {
    set_state_bool(SHOW_SHORTCUTS, !state_bool(SHOW_SHORTCUTS));
    render_dom();
}

#[panel_sdk::panel_handler]
fn capture_new_shortcut() {
    capture_shortcut(SLOT_NEW);
}

#[panel_sdk::panel_handler]
fn capture_save_shortcut() {
    capture_shortcut(SLOT_SAVE);
}

#[panel_sdk::panel_handler]
fn capture_save_as_shortcut() {
    capture_shortcut(SLOT_SAVE_AS);
}

#[panel_sdk::panel_handler]
fn capture_open_shortcut() {
    capture_shortcut(SLOT_OPEN);
}

#[panel_sdk::panel_handler]
fn edit_new_width(payload: TextValue) {
    if !payload.value.is_empty() {
        set_state_string(NEW_WIDTH, &payload.value);
    }
}

#[panel_sdk::panel_handler]
fn edit_new_height(payload: TextValue) {
    if !payload.value.is_empty() {
        set_state_string(NEW_HEIGHT, &payload.value);
    }
}

#[panel_sdk::panel_handler]
fn new_project() {
    let width = state_string(NEW_WIDTH);
    let height = state_string(NEW_HEIGHT);
    let Ok(command) = build_new_project_command(&width, &height) else {
        error("width and height must be positive integers");
        return;
    };
    emit_request(&command);
    cancel_forms();
}

#[panel_sdk::panel_handler]
fn select_template(payload: TextValue) {
    if payload.value.is_empty() {
        return;
    }
    if let Err(message) = apply_template_size(&payload.value) {
        error(message);
    }
    render_dom();
}

#[panel_sdk::panel_handler]
fn save_project() {
    emit_request(&services::project_io::save_current());
}

#[panel_sdk::panel_handler]
fn save_project_as() {
    emit_request(&services::project_io::save_as());
}

#[panel_sdk::panel_handler]
fn load_project() {
    emit_request(&services::project_io::load_dialog());
}

#[panel_sdk::panel_handler]
fn undo() {
    emit_request(&services::history::undo());
}

#[panel_sdk::panel_handler]
fn redo() {
    emit_request(&services::history::redo());
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
        show_new_form(),
        cancel_forms(),
        toggle_shortcuts(),
        select_template(TextValue::default()),
        edit_new_width(TextValue { value: "320".to_string() }),
        edit_new_height(TextValue { value: "240".to_string() }),
        capture_new_shortcut(),
        capture_save_shortcut(),
        capture_save_as_shortcut(),
        capture_open_shortcut(),
        new_project(),
        save_project(),
        save_project_as(),
        load_project(),
        keyboard(KeyEvent::default()),
        undo(),
        redo(),
    });

    #[test]
    fn new_project_command_trims_dimensions() {
        let cmd = build_new_project_command(" 320 ", " 240 ").expect("ok");
        assert_eq!(cmd.name, panel_sdk::names::project_io::NEW_DOCUMENT_SIZED);
    }

    #[test]
    fn new_project_command_rejects_missing_dimensions() {
        assert!(build_new_project_command("", "240").is_err());
        assert!(build_new_project_command("320px", "240").is_err());
    }
}
