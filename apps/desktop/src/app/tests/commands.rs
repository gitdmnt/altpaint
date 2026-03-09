//! `DesktopApp` のコマンド適用と基本状態更新に関するテストをまとめる。

use std::path::PathBuf;

use app_core::{ColorRgba8, Command, ToolKind};
use plugin_api::HostAction;
use serde_json::json;

use super::{TestDialogs, test_app_with_dialogs, tree_contains_text};
use crate::config::{default_panel_dir, parse_document_size};
use crate::profiler::DesktopProfiler;

/// ツール切替コマンドがドキュメントへ反映されることを確認する。
#[test]
fn execute_command_updates_document_tool() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    let _ = app.execute_command(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    });

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

/// 色変更コマンドがドキュメントへ反映されることを確認する。
#[test]
fn execute_command_updates_document_color() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    let _ = app.execute_command(Command::SetActiveColor {
        color: ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff),
    });

    assert_eq!(app.document.active_color, ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff));
}

/// 新規作成後にアクティブツールが既定値へ戻ることを確認する。
#[test]
fn execute_command_new_document_resets_tool_to_default() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.document.set_active_tool(ToolKind::Eraser);

    let _ = app.execute_command(Command::NewDocumentSized {
        width: 64,
        height: 64,
    });

    assert_eq!(app.document.active_tool, ToolKind::Brush);
}

/// ホストアクション経由でもツール切替が同じ経路で適用されることを確認する。
#[test]
fn host_action_dispatches_tool_switch_command() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    let _ = app.execute_host_action(HostAction::DispatchCommand(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    }));

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

/// フォーカス中のパネル操作対象をキーボードでアクティブ化できることを確認する。
#[test]
fn keyboard_panel_focus_can_activate_app_action() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(app.ui_shell.focus_panel_node("builtin.app-actions", "app.save"));
    assert_eq!(app.activate_focused_panel_control(), Some(Command::SaveProject));
}

/// 寸法文字列パーサが一般的なフォーマットを受け入れることを確認する。
#[test]
fn parse_document_size_accepts_common_formats() {
    assert_eq!(parse_document_size("64x64"), Some((64, 64)));
    assert_eq!(parse_document_size("320 240"), Some((320, 240)));
    assert_eq!(parse_document_size("800,600"), Some((800, 600)));
    assert_eq!(parse_document_size("0x600"), None);
}

/// `NewDocument` がパネル側のインラインフォーム表示へ繋がることを確認する。
#[test]
fn execute_command_new_document_opens_inline_form() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_command(Command::NewDocument));
}

#[test]
fn plugin_keyboard_shortcut_can_switch_tool() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.document.set_active_tool(ToolKind::Eraser);

    assert!(app.dispatch_keyboard_shortcut("B", "B", false));

    assert_eq!(app.document.active_tool, ToolKind::Brush);
}

#[test]
fn plugin_keyboard_shortcut_can_switch_to_pen() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.document.set_active_tool(ToolKind::Brush);

    assert!(app.dispatch_keyboard_shortcut("P", "P", false));

    assert_eq!(app.document.active_tool, ToolKind::Pen);
}

#[test]
fn plugin_keyboard_capture_updates_persistent_config() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.activate_panel_control("builtin.app-actions", "app.shortcuts"));
    assert!(app.activate_panel_control("builtin.app-actions", "app.shortcut.new"));
    assert!(app.dispatch_keyboard_shortcut("Ctrl+Alt+N", "N", false));

    let configs = app.ui_shell.persistent_panel_configs();
    assert_eq!(
        configs.get("builtin.app-actions"),
        Some(&json!({
            "new_shortcut": "Ctrl+Alt+N",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );
}

#[test]
fn unmatched_keyboard_shortcut_is_not_consumed() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(!app.dispatch_keyboard_shortcut("Tab", "Tab", false));
}

/// サイズ指定付き新規作成がビットマップ寸法を置き換えることを確認する。
#[test]
fn execute_command_new_document_sized_replaces_bitmap() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_command(Command::NewDocumentSized {
        width: 320,
        height: 240,
    }));

    let bitmap = app.document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (320, 240));
}

/// 既定 UI ディレクトリから DSL サンプルパネルが読み込まれることを確認する。
#[test]
fn desktop_app_loads_phase6_sample_panel_from_default_ui_directory() {
    let app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(default_panel_dir().join("phase6-sample").join("panel.altp-panel").exists());
    assert!(app.ui_shell.panel_trees().iter().any(|panel| panel.id == "builtin.dsl-sample"));
}

/// 組み込みパネルが DSL / Wasm 実装へ置換されていることを確認する。
#[test]
fn desktop_app_replaces_builtin_panels_with_phase7_dsl_variants() {
    let app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let panels = app.ui_shell.panel_trees();

    for panel_id in [
        "builtin.app-actions",
        "builtin.tool-palette",
        "builtin.layers-panel",
        "builtin.pen-settings",
    ] {
        assert_eq!(
            panels.iter().filter(|panel| panel.id == panel_id).count(),
            1,
            "expected a single panel for {panel_id}"
        );
    }

    let app_actions = panels
        .iter()
        .find(|panel| panel.id == "builtin.app-actions")
        .expect("app actions panel exists");
    let layers = panels
        .iter()
        .find(|panel| panel.id == "builtin.layers-panel")
        .expect("layers panel exists");

    assert!(tree_contains_text(&app_actions.children, "Hosted via Rust SDK + Wasm"));
    assert!(tree_contains_text(&layers.children, "Untitled"));
}

#[test]
fn reload_pen_presets_reads_default_pen_directory() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(app.execute_command(Command::ReloadPenPresets));
    assert!(app.document.pen_presets.len() >= 3);
}
