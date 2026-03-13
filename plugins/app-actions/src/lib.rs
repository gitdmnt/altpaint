use plugin_sdk::{
    CommandDescriptor,
    runtime::{
        StatePatchBuffer, emit_service, error, event_string, set_state_bool, set_state_string,
        state_string, toggle_state,
    },
    services, state,
};

const SHOW_NEW: state::BoolKey = state::bool("show_new");
const SHOW_SHORTCUTS: state::BoolKey = state::bool("show_shortcuts");
const NEW_WIDTH: state::StringKey = state::string("new_width");
const NEW_HEIGHT: state::StringKey = state::string("new_height");
const SELECTED_TEMPLATE: state::StringKey = state::string("selected_template");
const CAPTURE_TARGET: state::StringKey = state::string("session.capture_target");
const DEFAULT_TEMPLATE_SIZE: state::StringKey = state::string("config.default_template_size");
const NEW_SHORTCUT: state::StringKey = state::string("config.new_shortcut");
const SAVE_SHORTCUT: state::StringKey = state::string("config.save_shortcut");
const SAVE_AS_SHORTCUT: state::StringKey = state::string("config.save_as_shortcut");
const OPEN_SHORTCUT: state::StringKey = state::string("config.open_shortcut");

/// 入力を解析して 寸法 に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
fn parse_dimension(value: &str) -> Result<usize, &'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("width and height are required");
    }

    trimmed
        .parse::<usize>()
        .map_err(|_| "width and height must be positive integers")
}

/// 新規 プロジェクト コマンド を構築し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
fn build_new_project_command(width: &str, height: &str) -> Result<CommandDescriptor, &'static str> {
    let width = parse_dimension(width)?;
    let height = parse_dimension(height)?;
    Ok(services::project_io::new_document_sized(width, height))
}

/// テンプレート サイズ を現在の状態へ適用する。
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

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// 状態上の 取得 target を更新する。
fn set_capture_target(target: &str) {
    set_state_string(CAPTURE_TARGET, target);
}

/// ショートカット 用のショートカット入力を受け付ける状態にする。
fn capture_shortcut(target: &str) {
    set_capture_target(target);
    set_state_bool(SHOW_SHORTCUTS, true);
}

/// 入力や種別に応じて処理を振り分ける。
fn assign_captured_shortcut(target: &str, shortcut: &str) {
    match target {
        "new" => set_state_string(NEW_SHORTCUT, shortcut),
        "save" => set_state_string(SAVE_SHORTCUT, shortcut),
        "save_as" => set_state_string(SAVE_AS_SHORTCUT, shortcut),
        "open" => set_state_string(OPEN_SHORTCUT, shortcut),
        _ => {}
    }
}

/// ショートカット matches を計算して返す。
fn shortcut_matches(configured: &str, incoming: &str) -> bool {
    !configured.is_empty() && configured.eq_ignore_ascii_case(incoming)
}

/// 新規 form を表示できるよう状態を更新する。
#[plugin_sdk::panel_handler]
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
}

/// Forms に関する表示や入力状態を閉じる。
#[plugin_sdk::panel_handler]
fn cancel_forms() {
    set_state_bool(SHOW_NEW, false);
}

/// 状態上の shortcuts を切り替える。
#[plugin_sdk::panel_handler]
fn toggle_shortcuts() {
    toggle_state(SHOW_SHORTCUTS);
}

/// 新規 ショートカット 用のショートカット入力を受け付ける状態にする。
#[plugin_sdk::panel_handler]
fn capture_new_shortcut() {
    capture_shortcut("new");
}

/// 保存 ショートカット 用のショートカット入力を受け付ける状態にする。
#[plugin_sdk::panel_handler]
fn capture_save_shortcut() {
    capture_shortcut("save");
}

/// 保存 as ショートカット 用のショートカット入力を受け付ける状態にする。
#[plugin_sdk::panel_handler]
fn capture_save_as_shortcut() {
    capture_shortcut("save_as");
}

/// 開く ショートカット 用のショートカット入力を受け付ける状態にする。
#[plugin_sdk::panel_handler]
fn capture_open_shortcut() {
    capture_shortcut("open");
}

/// 入力済みサイズから新規プロジェクト作成要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn new_project() {
    let width = state_string(NEW_WIDTH);
    let height = state_string(NEW_HEIGHT);
    let Ok(command) = build_new_project_command(&width, &height) else {
        error("width and height must be positive integers");
        return;
    };

    emit_service(&command);
    cancel_forms();
}

/// テンプレート を選択状態へ更新する。
#[plugin_sdk::panel_handler]
fn select_template() {
    let value = event_string("value");
    if value.is_empty() {
        return;
    }

    if let Err(message) = apply_template_size(&value) {
        error(message);
    }
}

/// 現在のプロジェクトを既存パスへ保存する要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn save_project() {
    emit_service(&services::project_io::save_current());
}

/// 保存先を選んでプロジェクトを書き出す要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn save_project_as() {
    emit_service(&services::project_io::save_as());
}

/// 読み込み対象を選んでプロジェクトを開く要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn load_project() {
    emit_service(&services::project_io::load_dialog());
}

/// 直前の描画操作を元に戻す要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn undo() {
    emit_service(&services::history::undo());
}

/// 元に戻した操作をやり直す要求を発行する。
///
/// 内部でサービス要求を発行します。
#[plugin_sdk::panel_handler]
fn redo() {
    emit_service(&services::history::redo());
}

/// キーボード入力やショートカットに応じて状態と処理を切り替える。
#[plugin_sdk::panel_handler]
fn keyboard() {
    let shortcut = event_string("shortcut");
    if shortcut.is_empty() {
        return;
    }

    let target = state_string(CAPTURE_TARGET);
    if !target.is_empty() {
        assign_captured_shortcut(&target, &shortcut);
        set_capture_target("");
        return;
    }

    if shortcut_matches(&state_string(NEW_SHORTCUT), &shortcut) {
        show_new_form();
        return;
    }
    if shortcut_matches(&state_string(SAVE_SHORTCUT), &shortcut) {
        save_project();
        return;
    }
    if shortcut_matches(&state_string(SAVE_AS_SHORTCUT), &shortcut) {
        save_project_as();
        return;
    }
    if shortcut_matches(&state_string(OPEN_SHORTCUT), &shortcut) {
        load_project();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 新規 プロジェクト コマンド trims dimensions が期待どおりに動作することを検証する。
    #[test]
    fn new_project_command_trims_dimensions() {
        let command = build_new_project_command(" 320 ", " 240 ").expect("command should build");

        assert_eq!(command.name, "project_io.new_document_sized");
        assert_eq!(
            command
                .payload
                .get("width")
                .and_then(|value| value.as_u64()),
            Some(320)
        );
        assert_eq!(
            command
                .payload
                .get("height")
                .and_then(|value| value.as_u64()),
            Some(240)
        );
    }

    /// 新規 プロジェクト コマンド rejects missing dimensions が期待どおりに動作することを検証する。
    #[test]
    fn new_project_command_rejects_missing_dimensions() {
        assert_eq!(
            build_new_project_command("", "240"),
            Err("width and height are required")
        );
        assert_eq!(
            build_new_project_command("320", "   "),
            Err("width and height are required")
        );
        assert_eq!(
            build_new_project_command("320px", "240"),
            Err("width and height must be positive integers")
        );
    }

    /// typed プロジェクト commands use expected names が期待どおりに動作することを検証する。
    #[test]
    fn typed_project_commands_use_expected_names() {
        let command = services::project_io::save_as();

        assert_eq!(command.name, "project_io.save_as");
        assert!(command.payload.is_empty());
    }

    /// パネル entrypoints are callable on native targets が期待どおりに動作することを検証する。
    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        show_new_form();
        cancel_forms();
        toggle_shortcuts();
        select_template();
        capture_new_shortcut();
        capture_save_shortcut();
        capture_save_as_shortcut();
        capture_open_shortcut();
        new_project();
        save_project();
        save_project_as();
        load_project();
        keyboard();
        undo();
        redo();
    }

    /// undo/redo service descriptor names は期待どおりに動作することを検証する。
    #[test]
    fn undo_redo_service_names_are_correct() {
        assert_eq!(services::history::undo().name, "history.undo");
        assert_eq!(services::history::redo().name, "history.redo");
    }

    /// ショートカット match is case insensitive が期待どおりに動作することを検証する。
    #[test]
    fn shortcut_match_is_case_insensitive() {
        assert!(shortcut_matches("Ctrl+S", "ctrl+s"));
        assert!(!shortcut_matches("", "Ctrl+S"));
    }

    /// テンプレート サイズ updates 幅 and 高さ が期待どおりに動作することを検証する。
    #[test]
    fn template_size_updates_width_and_height() {
        apply_template_size("2894x4093").expect("template size should parse");
    }
}
