//! `DesktopApp` のコマンド適用と基本状態更新に関するテストをまとめる。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use app_core::{
    ColorRgba8, DocumentCommand, SessionCommand, ToolKind,
    WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
    WorkspacePanelState,
};
use desktop_support::{
    FrameProfiler, WorkspacePreset, WorkspacePresetCatalog,
    save_workspace_preset_catalog,
};
use panel_runtime::{HostAction, PanelEvent, ServiceRequest, services::names};
use serde_json::json;
use app_core::WorkspaceUiState;

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_workspace_preset_path,
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

#[test]
fn execute_command_updates_document_tool() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    let _ = app.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Eraser,
    });

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

#[test]
fn execute_command_select_tool_updates_document_tool_id() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    let _ = app.apply_session_command(&SessionCommand::SelectTool {
        tool_id: "builtin.eraser".to_string(),
    });

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
    assert_eq!(app.document.active_tool_id, "builtin.eraser");
}

#[test]
fn execute_command_select_child_tool_updates_active_child_tool_id() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    // Pen tool must be loaded so we can select one of its children
    let _ = app.apply_session_command(&SessionCommand::SelectTool {
        tool_id: "builtin.pen".to_string(),
    });
    // Inject a child tool definition into the tool catalog
    if let Some(pen_def) = app
        .document
        .tool_catalog
        .iter_mut()
        .find(|t| t.id == "builtin.pen")
    {
        pen_def.children.push(app_core::ToolDefinition {
            id: "builtin.pen.test".to_string(),
            name: "Test".to_string(),
            kind: app_core::ToolKind::Pen,
            provider_plugin_id: String::new(),
            drawing_plugin_id: String::new(),
            settings: vec![],
            children: vec![],
        });
    }

    let _ = app.apply_session_command(&SessionCommand::SelectChildTool {
        child_id: "builtin.pen.test".to_string(),
    });

    assert_eq!(app.document.active_child_tool_id, "builtin.pen.test");
}

#[test]
fn execute_command_updates_document_color() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    let _ = app.apply_session_command(&SessionCommand::SetActiveColor {
        color: ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff),
    });

    assert_eq!(
        app.document.active_color,
        ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)
    );
}

#[test]
fn execute_command_new_document_resets_tool_to_default() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.document.set_active_tool(ToolKind::Eraser);

    let _ = app.apply_document_command(&DocumentCommand::NewDocumentSized {
        width: 64,
        height: 64,
    });

    assert_eq!(app.document.active_tool, ToolKind::Pen);
}

#[test]
fn host_action_dispatches_tool_switch_command() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    let _ = app.execute_host_action(HostAction::DispatchSessionCommand(
        SessionCommand::SetActiveTool {
            tool: ToolKind::Eraser,
        },
    ));

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

#[test]
fn keyboard_panel_focus_can_activate_app_action() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(
        app.panel_workspace
            .focus_panel_node("builtin.app-actions", "app.save")
    );
    // app.save は emit_service 経由で保存サービスを発行するため、HostAction が
    // 生成され activate_focused_panel_control は true を返す。
    // pending_jobs でジョブがキューされていることを確認する。
    assert!(app.activate_focused_panel_control());
    assert_eq!(app.background_jobs.len(), 1);
}

#[test]
fn execute_command_new_document_opens_inline_form() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_service_request(ServiceRequest::new(names::PROJECT_NEW_DOCUMENT)));
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

#[test]
fn unmatched_keyboard_shortcut_is_not_consumed() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(!app.dispatch_keyboard_shortcut("Tab", "Tab", false));
}

#[test]
fn execute_command_new_document_sized_replaces_bitmap() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.apply_document_command(&DocumentCommand::NewDocumentSized {
        width: 320,
        height: 240,
    }));

    let bitmap = app.document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (320, 240));
}

/// Phase 10: builtin パネルが正しく登録されているか確認する。
/// (旧 phase6/phase7 DSL 系のアサーションは Phase 10 で PanelTree が空になったため撤去)
#[test]
fn builtin_panels_are_registered() {
    let app = test_app_with_dialogs(TestDialogs::default());
    let registered: Vec<&str> = app.panel_runtime.panel_static_ids();

    for panel_id in [
        "builtin.app-actions",
        "builtin.workspace-presets",
        "builtin.tool-palette",
        "builtin.layers",
        "builtin.tool-settings",
        "builtin.color-palette",
        "builtin.view-controls",
        "builtin.koma-list",
        "builtin.snapshots",
        "builtin.text-flow",
        "builtin.job-progress",
    ] {
        assert!(
            registered.contains(&panel_id),
            "expected panel {panel_id} to be registered, got {registered:?}"
        );
    }
}

#[test]
fn reload_pen_presets_reads_default_pen_directory() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_service_request(ServiceRequest::new(
        names::TOOL_CATALOG_RELOAD_PEN_PRESETS
    )));
    assert!(app.document.pen_presets.len() >= 3);
}

#[test]
fn startup_loads_tool_catalog_from_default_tool_directory() {
    let app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.document.tool_catalog.len() >= 5);
    assert!(
        app.document
            .tool_catalog
            .iter()
            .any(|tool| tool.id == "builtin.pen"
                && tool.provider_plugin_id == "plugins/default-pens-plugin")
    );
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
                                id: "builtin.layers".to_string(),
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
                                id: "builtin.layers".to_string(),
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

    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_APPLY_PRESET)
            .with_value("preset_id", "illustration"),
    ));

    let layout_entry = app
        .panel_workspace
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers")
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
                                id: "builtin.layers".to_string(),
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
                                id: "builtin.layers".to_string(),
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
        .panel_workspace
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers")
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

    assert!(app.execute_service_request(ServiceRequest::new(names::WORKSPACE_RELOAD_PRESETS)));

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

#[test]
fn execute_command_saves_current_workspace_preset_into_catalog() {
    let preset_path = unique_workspace_preset_path("workspace-preset-save");
    let mut app = test_app_with_dialogs_and_workspace_preset_path(
        TestDialogs::default(),
        preset_path.clone(),
    );

    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_SAVE_PRESET)
            .with_value("preset_id", "review")
            .with_value("label", "Review"),
    ));

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

#[test]
fn execute_command_exports_workspace_preset_to_dialog_path() {
    let export_path = unique_workspace_preset_path("workspace-preset-export");
    let mut app = test_app_with_dialogs(TestDialogs::with_workspace_save_path(export_path.clone()));

    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_EXPORT_PRESET)
            .with_value("preset_id", "exported")
            .with_value("label", "Exported"),
    ));

    let exported = desktop_support::load_workspace_preset_catalog(&export_path);
    assert_eq!(exported.default_preset_id, "exported");
    assert_eq!(exported.presets.len(), 1);
    assert_eq!(exported.presets[0].label, "Exported");

    let _ = std::fs::remove_file(export_path);
}

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

    assert!(app.execute_service_request(ServiceRequest::new(
        names::TOOL_CATALOG_IMPORT_PEN_PRESETS
    )));
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
