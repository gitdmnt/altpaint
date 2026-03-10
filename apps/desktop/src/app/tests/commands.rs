//! `DesktopApp` のコマンド適用と基本状態更新に関するテストをまとめる。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use app_core::{
    ColorRgba8, Command, DEFAULT_DOCUMENT_HEIGHT, DEFAULT_DOCUMENT_WIDTH, ToolKind,
    WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
    WorkspacePanelState,
};
use desktop_support::{
    DesktopProfiler, WorkspacePreset, WorkspacePresetCatalog, default_panel_dir,
    parse_document_size, save_workspace_preset_catalog,
};
use plugin_api::HostAction;
use serde_json::json;
use workspace_persistence::WorkspaceUiState;

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_workspace_preset_path,
    tree_contains_button_id, tree_contains_text,
};

fn unique_workspace_preset_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "altpaint-{name}-{}-{nanos}.json",
        std::process::id()
    ))
}

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

    assert_eq!(
        app.document.active_color,
        ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)
    );
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

    assert_eq!(app.document.active_tool, ToolKind::Pen);
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

    assert!(
        app.ui_shell
            .focus_panel_node("builtin.app-actions", "app.save")
    );
    assert_eq!(
        app.activate_focused_panel_control(),
        Some(Command::SaveProject)
    );
}

/// 寸法文字列パーサが一般的なフォーマットを受け入れることを確認する。
#[test]
fn parse_document_size_accepts_common_formats() {
    assert_eq!(parse_document_size("64x64"), Some((64, 64)));
    assert_eq!(
        parse_document_size("2894x4093"),
        Some((DEFAULT_DOCUMENT_WIDTH, DEFAULT_DOCUMENT_HEIGHT))
    );
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
            "default_template_size": "2894x4093",
            "new_shortcut": "Ctrl+Alt+N",
            "template_options": "2894x4093:A4 350dpi (2894×4093)|2480x3508:A4 300dpi (2480×3508)|2048x2048:Square 2048 (2048×2048)|1920x1080:HD Landscape (1920×1080)",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O",
            "workspace_options": "default-floating:Default floating workspace",
            "selected_workspace": "default-floating"
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
    let app = test_app_with_dialogs(TestDialogs::default());

    assert!(
        default_panel_dir()
            .join("phase6-sample")
            .join("panel.altp-panel")
            .exists()
    );
    assert!(
        app.ui_shell
            .panel_trees()
            .iter()
            .any(|panel| panel.id == "builtin.dsl-sample")
    );
}

/// 組み込みパネルが DSL / Wasm 実装へ置換されていることを確認する。
#[test]
fn desktop_app_replaces_builtin_panels_with_phase7_dsl_variants() {
    let app = test_app_with_dialogs(TestDialogs::default());
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

    assert!(tree_contains_button_id(&app_actions.children, "app.save"));
    assert!(tree_contains_text(&layers.children, "Untitled"));
}

#[test]
fn reload_pen_presets_reads_default_pen_directory() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(app.execute_command(Command::ReloadPenPresets));
    assert!(app.document.pen_presets.len() >= 3);
}

#[test]
fn execute_command_applies_selected_workspace_preset() {
    let preset_path = unique_workspace_preset_path("workspace-preset-apply");
    save_workspace_preset_catalog(
        &preset_path,
        &WorkspacePresetCatalog {
            format_version: 1,
            default_preset_id: "default".to_string(),
            presets: vec![
                WorkspacePreset {
                    id: "default".to_string(),
                    label: "Default".to_string(),
                    ui_state: WorkspaceUiState::new(
                        WorkspaceLayout {
                            panels: vec![WorkspacePanelState {
                                id: "builtin.layers-panel".to_string(),
                                visible: true,
                                anchor: WorkspacePanelAnchor::TopLeft,
                                position: Some(WorkspacePanelPosition { x: 24, y: 72 }),
                                size: Some(WorkspacePanelSize {
                                    width: 320,
                                    height: 260,
                                }),
                            }],
                        },
                        BTreeMap::new(),
                    ),
                },
                WorkspacePreset {
                    id: "illustration".to_string(),
                    label: "Illustration".to_string(),
                    ui_state: WorkspaceUiState::new(
                        WorkspaceLayout {
                            panels: vec![WorkspacePanelState {
                                id: "builtin.layers-panel".to_string(),
                                visible: true,
                                anchor: WorkspacePanelAnchor::BottomRight,
                                position: Some(WorkspacePanelPosition { x: 32, y: 40 }),
                                size: Some(WorkspacePanelSize {
                                    width: 360,
                                    height: 300,
                                }),
                            }],
                        },
                        BTreeMap::new(),
                    ),
                },
            ],
        },
    )
    .expect("workspace preset catalog should save");
    let mut app = test_app_with_dialogs_and_workspace_preset_path(
        TestDialogs::default(),
        preset_path.clone(),
    );

    assert!(app.execute_command(Command::ApplyWorkspacePreset {
        preset_id: "illustration".to_string(),
    }));

    let layout_entry = app
        .ui_shell
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers-panel")
        .expect("layers panel should exist");
    assert_eq!(layout_entry.anchor, WorkspacePanelAnchor::BottomRight);
    assert_eq!(
        layout_entry.position,
        Some(WorkspacePanelPosition { x: 32, y: 40 })
    );
    assert_eq!(
        app.ui_shell
            .persistent_panel_configs()
            .get("builtin.app-actions")
            .and_then(|config| config.get("selected_workspace"))
            .and_then(|value| value.as_str()),
        Some("illustration")
    );

    let _ = std::fs::remove_file(preset_path);
}

#[test]
fn execute_command_reloads_workspace_presets_into_app_actions_config() {
    let preset_path = unique_workspace_preset_path("workspace-preset-reload");
    save_workspace_preset_catalog(
        &preset_path,
        &WorkspacePresetCatalog {
            format_version: 1,
            default_preset_id: "default".to_string(),
            presets: vec![WorkspacePreset {
                id: "default".to_string(),
                label: "Default".to_string(),
                ui_state: WorkspaceUiState::default(),
            }],
        },
    )
    .expect("initial preset catalog should save");
    let mut app = test_app_with_dialogs_and_workspace_preset_path(
        TestDialogs::default(),
        preset_path.clone(),
    );

    save_workspace_preset_catalog(
        &preset_path,
        &WorkspacePresetCatalog {
            format_version: 1,
            default_preset_id: "review".to_string(),
            presets: vec![
                WorkspacePreset {
                    id: "review".to_string(),
                    label: "Review".to_string(),
                    ui_state: WorkspaceUiState::default(),
                },
                WorkspacePreset {
                    id: "compact".to_string(),
                    label: "Compact".to_string(),
                    ui_state: WorkspaceUiState::default(),
                },
            ],
        },
    )
    .expect("updated preset catalog should save");

    assert!(app.execute_command(Command::ReloadWorkspacePresets));

    let config = app
        .ui_shell
        .persistent_panel_configs()
        .get("builtin.app-actions")
        .cloned()
        .expect("app actions config should exist");
    assert_eq!(
        config
            .get("workspace_options")
            .and_then(|value| value.as_str()),
        Some("review:Review|compact:Compact")
    );
    assert_eq!(
        config
            .get("selected_workspace")
            .and_then(|value| value.as_str()),
        Some("review")
    );

    let _ = std::fs::remove_file(preset_path);
}
