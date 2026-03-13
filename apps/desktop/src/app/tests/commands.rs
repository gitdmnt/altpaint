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
use panel_api::{HostAction, PanelEvent};
use serde_json::json;
use workspace_persistence::WorkspaceUiState;

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_workspace_preset_path,
    tree_contains_button_id, tree_contains_text,
};

/// 現在の unique ワークスペース preset パス を返す。
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

/// execute コマンド updates ドキュメント ツール が期待どおりに動作することを検証する。
#[test]
fn execute_command_updates_document_tool() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    let _ = app.execute_command(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    });

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

/// execute コマンド 選択 ツール updates ドキュメント ツール ID が期待どおりに動作することを検証する。
#[test]
fn execute_command_select_tool_updates_document_tool_id() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    let _ = app.execute_command(Command::SelectTool {
        tool_id: "builtin.eraser".to_string(),
    });

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
    assert_eq!(app.document.active_tool_id, "builtin.eraser");
}

/// execute コマンド updates ドキュメント 色 が期待どおりに動作することを検証する。
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

/// execute コマンド 新規 ドキュメント resets ツール to 既定 が期待どおりに動作することを検証する。
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

/// ホスト action dispatches ツール switch コマンド が期待どおりに動作することを検証する。
#[test]
fn host_action_dispatches_tool_switch_command() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    let _ = app.execute_host_action(HostAction::DispatchCommand(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    }));

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

/// キーボード パネル フォーカス can activate アプリ action が期待どおりに動作することを検証する。
#[test]
fn keyboard_panel_focus_can_activate_app_action() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(app.panel_presentation.focus_panel_node(
        &app.panel_runtime,
        "builtin.app-actions",
        "app.save"
    ));
    assert_eq!(
        app.activate_focused_panel_control(),
        Some(Command::SaveProject)
    );
    assert_eq!(app.io_state.pending_jobs.len(), 1);
}

/// 解析 ドキュメント サイズ accepts common formats が期待どおりに動作することを検証する。
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

/// execute コマンド 新規 ドキュメント opens inline form が期待どおりに動作することを検証する。
#[test]
fn execute_command_new_document_opens_inline_form() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_command(Command::NewDocument));
}

/// プラグイン キーボード ショートカット can switch ツール が期待どおりに動作することを検証する。
#[test]
fn plugin_keyboard_shortcut_can_switch_tool() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.document.set_active_tool(ToolKind::Eraser);

    assert!(app.dispatch_keyboard_shortcut("P", "P", false));

    assert_eq!(app.document.active_tool, ToolKind::Pen);
}

/// プラグイン キーボード 取得 updates persistent 設定 が期待どおりに動作することを検証する。
#[test]
fn plugin_keyboard_capture_updates_persistent_config() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.activate_panel_control("builtin.app-actions", "app.shortcuts"));
    assert!(app.activate_panel_control("builtin.app-actions", "app.shortcut.new"));
    assert!(app.dispatch_keyboard_shortcut("Ctrl+Alt+N", "N", false));

    let configs = app.panel_runtime.persistent_panel_configs();
    assert_eq!(
        configs.get("builtin.app-actions"),
        Some(&json!({
            "default_template_size": "2894x4093",
            "new_shortcut": "Ctrl+Alt+N",
            "template_options": "2894x4093:A4 350dpi (2894×4093)|2480x3508:A4 300dpi (2480×3508)|2048x2048:Square 2048 (2048×2048)|1920x1080:HD Landscape (1920×1080)",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );
}

/// unmatched キーボード ショートカット is not consumed が期待どおりに動作することを検証する。
#[test]
fn unmatched_keyboard_shortcut_is_not_consumed() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(!app.dispatch_keyboard_shortcut("Tab", "Tab", false));
}

/// execute コマンド 新規 ドキュメント sized replaces ビットマップ が期待どおりに動作することを検証する。
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

/// phase6 sample assets live under tools experimental が期待どおりに動作することを検証する。
#[test]
fn phase6_sample_assets_live_under_tools_experimental() {
    let app = test_app_with_dialogs(TestDialogs::default());

    assert!(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("apps dir")
            .parent()
            .expect("workspace root")
            .join("tools")
            .join("experimental")
            .join("phase6-sample")
            .join("panel.altp-panel")
            .exists()
    );
    assert!(
        !default_panel_dir()
            .join("phase6-sample")
            .join("panel.altp-panel")
            .exists()
    );
    assert!(
        app.panel_presentation
            .panel_trees(&app.panel_runtime)
            .iter()
            .all(|panel| panel.id != "builtin.dsl-sample")
    );
}

/// desktop アプリ replaces builtin panels with phase7 dsl variants が期待どおりに動作することを検証する。
#[test]
fn desktop_app_replaces_builtin_panels_with_phase7_dsl_variants() {
    let app = test_app_with_dialogs(TestDialogs::default());
    let panels = app.panel_presentation.panel_trees(&app.panel_runtime);

    for panel_id in [
        "builtin.app-actions",
        "builtin.workspace-presets",
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

/// 再読込 ペン presets reads 既定 ペン directory が期待どおりに動作することを検証する。
#[test]
fn reload_pen_presets_reads_default_pen_directory() {
    let mut app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(app.execute_command(Command::ReloadPenPresets));
    assert!(app.document.pen_presets.len() >= 3);
}

/// startup loads ツール カタログ from 既定 ツール directory が期待どおりに動作することを検証する。
#[test]
fn startup_loads_tool_catalog_from_default_tool_directory() {
    let app = super::DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(app.document.tool_catalog.len() >= 5);
    assert!(
        app.document
            .tool_catalog
            .iter()
            .any(|tool| tool.id == "builtin.pen"
                && tool.provider_plugin_id == "plugins/default-pens-plugin")
    );
}

/// execute コマンド applies 選択中 ワークスペース preset が期待どおりに動作することを検証する。
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
        .panel_presentation
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
        app.panel_runtime
            .persistent_panel_configs()
            .get("builtin.workspace-presets")
            .and_then(|config| config.get("selected_workspace"))
            .and_then(|value| value.as_str()),
        Some("illustration")
    );

    let _ = std::fs::remove_file(preset_path);
}

/// ワークスペース preset dropdown selection auto applies and persists 既定 が期待どおりに動作することを検証する。
#[test]
fn workspace_preset_dropdown_selection_auto_applies_and_persists_default() {
    let preset_path = unique_workspace_preset_path("workspace-preset-dropdown-apply");
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

    assert!(app.dispatch_panel_event(PanelEvent::SetText {
        panel_id: "builtin.workspace-presets".to_string(),
        node_id: "workspace.preset.selector".to_string(),
        value: "illustration".to_string(),
    }));

    let layout_entry = app
        .panel_presentation
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers-panel")
        .expect("layers panel should exist");
    assert_eq!(layout_entry.anchor, WorkspacePanelAnchor::BottomRight);
    assert_eq!(
        app.panel_runtime
            .persistent_panel_configs()
            .get("builtin.workspace-presets")
            .and_then(|config| config.get("selected_workspace"))
            .and_then(|value| value.as_str()),
        Some("illustration")
    );

    let saved = desktop_support::load_workspace_preset_catalog(&preset_path);
    assert_eq!(saved.default_preset_id, "illustration");

    let _ = std::fs::remove_file(preset_path);
}

/// execute コマンド reloads ワークスペース presets into ワークスペース パネル 設定 が期待どおりに動作することを検証する。
#[test]
fn execute_command_reloads_workspace_presets_into_workspace_panel_config() {
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
        .panel_runtime
        .persistent_panel_configs()
        .get("builtin.workspace-presets")
        .cloned()
        .expect("workspace config should exist");
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
    assert_eq!(
        config
            .get("selected_workspace_label")
            .and_then(|value| value.as_str()),
        Some("Review")
    );

    let _ = std::fs::remove_file(preset_path);
}

/// execute コマンド saves 現在 ワークスペース preset into カタログ が期待どおりに動作することを検証する。
#[test]
fn execute_command_saves_current_workspace_preset_into_catalog() {
    let preset_path = unique_workspace_preset_path("workspace-preset-save");
    let mut app = test_app_with_dialogs_and_workspace_preset_path(
        TestDialogs::default(),
        preset_path.clone(),
    );

    assert!(app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-palette".to_string(),
        visible: false,
    }));
    assert!(app.execute_command(Command::SaveWorkspacePreset {
        preset_id: "review".to_string(),
        label: "Review".to_string(),
    }));

    let saved = desktop_support::load_workspace_preset_catalog(&preset_path);
    let preset = saved
        .presets
        .iter()
        .find(|preset| preset.id == "review")
        .expect("saved preset exists");
    assert!(
        preset
            .ui_state
            .workspace_layout
            .panels
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette" && !panel.visible)
    );

    let _ = std::fs::remove_file(preset_path);
}

/// execute コマンド exports ワークスペース preset to ダイアログ パス が期待どおりに動作することを検証する。
#[test]
fn execute_command_exports_workspace_preset_to_dialog_path() {
    let export_path = unique_workspace_preset_path("workspace-preset-export");
    let mut app = test_app_with_dialogs(TestDialogs::with_workspace_save_path(export_path.clone()));

    assert!(app.execute_command(Command::ExportWorkspacePreset {
        preset_id: "exported".to_string(),
        label: "Exported".to_string(),
    }));

    let exported = desktop_support::load_workspace_preset_catalog(&export_path);
    assert_eq!(exported.default_preset_id, "exported");
    assert_eq!(exported.presets.len(), 1);
    assert_eq!(exported.presets[0].label, "Exported");

    let _ = std::fs::remove_file(export_path);
}

/// execute コマンド imports ペン file and records report が期待どおりに動作することを検証する。
#[test]
fn execute_command_imports_pen_file_and_records_report() {
    let path = unique_workspace_preset_path("import-pen").with_extension("altp-pen.json");
    std::fs::write(
        &path,
        r#"{
  "format_version": 2,
  "id": "imported.pen",
  "name": "Imported Pen",
  "base_size": 11,
  "min_size": 1,
  "max_size": 32,
  "spacing_percent": 25,
  "opacity": 1.0,
  "flow": 1.0,
  "pressure_enabled": true,
  "antialias": true,
  "stabilization": 0,
  "engine": "stamp"
}"#,
    )
    .expect("pen file written");
    let mut app = test_app_with_dialogs(TestDialogs::with_pen_open_path(path.clone()));

    assert!(app.execute_command(Command::ImportPenPresets));
    assert!(
        app.document
            .pen_presets
            .iter()
            .any(|preset| preset.id == "imported.pen")
    );
    assert_eq!(
        app.panel_runtime
            .persistent_panel_configs()
            .get("builtin.tool-palette")
            .and_then(|config| config.get("last_import_summary"))
            .and_then(|value| value.as_str())
            .map(|value| value.contains("imported=1")),
        Some(true)
    );

    let _ = std::fs::remove_file(path);
}
