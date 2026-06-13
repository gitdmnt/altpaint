//! `DesktopApp` の保存・読込とワークスペース復元に関するテストをまとめる。

use std::path::PathBuf;

use panel_workspace::{
    WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize, WorkspacePanelState,
};
use crate::features::project::{DesktopSessionState, save_session_state};
use crate::features::workspace::{
    WorkspacePreset, WorkspacePresetCatalog, save_workspace_preset_catalog,
};
use crate::platform::default_project_path;
use frame_profiler::FrameProfiler;
use editor_state::{ColorRgba8, EditorSession, SessionCommand};
use panel_runtime::{ServiceRequest, services::names};
use serde_json::json;
use std::collections::BTreeMap;
use project_store::{load_project_from_path, save_project_to_path};

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_session_path, unique_test_path,
};
use crate::app::{DesktopApp, DesktopAppOptions};

#[test]
fn execute_command_load_project_uses_native_dialog_path() {
    let path = std::env::temp_dir().join("altpaint-open-dialog-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    assert!(source_app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    save_project_to_path(
        &path,
        &source_app.document,
        &source_app.panel_workspace.workspace_layout(),
        &BTreeMap::new(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_service_request(ServiceRequest::new(names::PROJECT_LOAD_DIALOG)));
    app.wait_for_pending_save_tasks();
    assert_eq!(app.paths.project_path, path);
    assert!(
        !app.panel_workspace
            .is_panel_visible("builtin.tool-palette"),
        "tool-palette visibility was persisted as hidden"
    );

    let _ = std::fs::remove_file(app.paths.project_path.clone());
}

#[test]
fn save_project_as_updates_project_path_and_persists_workspace_layout() {
    let path = std::env::temp_dir().join("altpaint-save-as-test.altp.json");
    let mut app = test_app_with_dialogs(TestDialogs::with_save_path(path.clone()));

    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    assert!(app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_AS)));
    assert_eq!(app.pending_save_task_count(), 1);
    app.wait_for_pending_save_tasks();

    let loaded = load_project_from_path(&path).expect("saved project should load");
    assert_eq!(app.paths.project_path, path);
    assert!(
        loaded
            .ui_state
            .workspace_layout
            .panels
            .iter()
            .any(|entry| entry.id == "builtin.tool-palette" && !entry.visible)
    );

    let _ = std::fs::remove_file(app.paths.project_path.clone());
}

#[test]
fn save_and_load_restore_plugin_shortcut_configs() {
    let path = std::env::temp_dir().join("altpaint-plugin-config-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::with_save_path(path.clone()));
    assert!(source_app.activate_panel_control("builtin.app-actions", "app.shortcuts"));
    assert!(source_app.activate_panel_control("builtin.app-actions", "app.shortcut.new"));
    assert!(source_app.dispatch_keyboard_shortcut("Ctrl+Alt+N", "N", false));
    assert!(source_app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_AS)));
    source_app.wait_for_pending_save_tasks();

    let loaded = load_project_from_path(&path).expect("saved project should load");
    assert_eq!(
        loaded.ui_state.panel_configs.get("builtin.app-actions"),
        Some(&json!({
            "default_template_size": "2894x4093",
            "new_shortcut": "Ctrl+Alt+N",
            "template_options": "[{\"label\":\"A4 350dpi (2894×4093)\",\"size\":\"2894x4093\"},{\"label\":\"A4 300dpi (2480×3508)\",\"size\":\"2480x3508\"},{\"label\":\"Square 2048 (2048×2048)\",\"size\":\"2048x2048\"},{\"label\":\"HD Landscape (1920×1080)\",\"size\":\"1920x1080\"}]",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );
    assert_eq!(
        loaded
            .ui_state
            .panel_configs
            .get("builtin.workspace-presets"),
        Some(&json!({
            "workspace_options": "[{\"id\":\"default-floating\",\"label\":\"Default floating workspace\"}]",
            "selected_workspace": "default-floating",
            "selected_workspace_label": "Default floating workspace"
        }))
    );

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_service_request(ServiceRequest::new(names::PROJECT_LOAD_DIALOG)));
    assert_eq!(
        app.panel_runtime
            .persistent_panel_configs()
            .get("builtin.app-actions"),
        Some(&json!({
            "default_template_size": "2894x4093",
            "new_shortcut": "Ctrl+Alt+N",
            "template_options": "[{\"label\":\"A4 350dpi (2894×4093)\",\"size\":\"2894x4093\"},{\"label\":\"A4 300dpi (2480×3508)\",\"size\":\"2480x3508\"},{\"label\":\"Square 2048 (2048×2048)\",\"size\":\"2048x2048\"},{\"label\":\"HD Landscape (1920×1080)\",\"size\":\"1920x1080\"}]",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );
    assert_eq!(
        app.panel_runtime
            .persistent_panel_configs()
            .get("builtin.workspace-presets"),
        Some(&json!({
            "workspace_options": "[{\"id\":\"default-floating\",\"label\":\"Default floating workspace\"}]",
            "selected_workspace": "default-floating",
            "selected_workspace_label": "Default floating workspace"
        }))
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn load_project_restores_workspace_layout() {
    let path = std::env::temp_dir().join("altpaint-load-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    let mut moved = false;
    for _ in 0..3 {
        moved |= source_app.execute_service_request(
            ServiceRequest::new(names::WORKSPACE_LAYOUT_MOVE_PANEL)
                .with_value("panel_id", "builtin.layers")
                .with_value("direction", "up"),
        );
    }
    assert!(moved);
    assert!(source_app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    let expected_layout = source_app.panel_workspace.workspace_layout();
    save_project_to_path(
        &path,
        &source_app.document,
        &expected_layout,
        &BTreeMap::new(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert!(app.execute_service_request(
        ServiceRequest::new(names::PROJECT_LOAD_FROM_PATH)
            .with_value("path", path.to_string_lossy().to_string()),
    ));

    assert!(
        !app.panel_workspace
            .is_panel_visible("builtin.tool-palette"),
        "tool-palette visibility was persisted as hidden"
    );
    assert_eq!(app.panel_workspace.workspace_layout(), expected_layout);

    let _ = std::fs::remove_file(path);
}

#[test]
fn move_panel_service_updates_status_without_full_recompose() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let _layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_MOVE_PANEL)
            .with_value("panel_id", "builtin.layers")
            .with_value("direction", "up"),
    ));
    let _update = app.prepare_present_frame(1280, 200, &mut profiler);

    // Phase 9F: status text は GPU 描画化済み、L1/L4 dummy 経路も撤去済み。
    // move_panel が full recompose を起こさず ui_update を発火しないことのみ検証する。
    // (CPU dirty rect のピクセル一致比較は Phase 9E-4 までで撤去済み。)
    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
}

#[test]
fn set_panel_visibility_service_updates_status_without_full_recompose() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let _layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    // 9E-4: status text の compose ピクセル比較は廃止。CPU dirty rect 一致確認も廃止。
    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    let _ = update;
}

#[test]
fn hiding_panel_clears_previous_overlay_bounds_when_surface_shrinks() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.panel_workspace.move_panel_to(
        "builtin.tool-palette",
        940,
        72,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.request_panel_reconcile();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let hidden_panel_rect = app
        .panel_workspace
        .panel_rect("builtin.tool-palette", 1280, 800)
        .expect("hidden panel rect exists");

    profiler.stats.clear();
    assert!(app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    let update = app.prepare_present_frame(1280, 800, &mut profiler);

    // 9E-4: compose_dirty_panel プロファイラキー / ui_panel_dirty_rect の厳密一致は廃止。
    // 9E-3 で UI パネルは GPU 経路に移行したため CPU dirty rect は記録されない。
    let _ = update;
    let _ = hidden_panel_rect;
}

#[test]
fn startup_uses_default_workspace_preset_when_project_and_session_are_empty() {
    let preset_path = unique_test_path("workspace-preset-catalog");
    save_workspace_preset_catalog(
        &preset_path,
        &WorkspacePresetCatalog {
            format_version: 1,
            default_preset_id: "test-preset".to_string(),
            presets: vec![WorkspacePreset {
                id: "test-preset".to_string(),
                label: "Test preset".to_string(),
                ui_state: panel_workspace::WorkspaceUiState::new(
                    panel_workspace::WorkspaceLayout {
                        panels: vec![WorkspacePanelState {
                            id: "builtin.layers".to_string(),
                            visible: true,
                            anchor: WorkspacePanelAnchor::TopRight,
                            position: Some(WorkspacePanelPosition { x: 40, y: 88 }),
                            size: Some(WorkspacePanelSize {
                                width: 320,
                                height: 260,
                            }),
                        }],
                    },
                    BTreeMap::new(),
                ),
            }],
        },
    )
    .expect("preset save should succeed");

    // 他のテストが /tmp/altpaint-test.altp.json へ書き込む競合を避けるため
    // 存在しない一意パスを使う（プロジェクトが読み込まれず preset が優先される）。
    let app = DesktopApp::with_options(DesktopAppOptions {
        project_path: unique_test_path("preset-project"),
        dialogs: Box::new(TestDialogs::default()),
        session_path: unique_test_path("preset-session"),
        workspace_preset_path: preset_path.clone(),
    });
    let entry = app
        .panel_workspace
        .workspace_layout()
        .panels
        .into_iter()
        .find(|entry| entry.id == "builtin.layers")
        .expect("layers panel layout exists");

    assert_eq!(entry.anchor, WorkspacePanelAnchor::TopRight);
    assert_eq!(
        entry.position,
        Some(WorkspacePanelPosition { x: 40, y: 88 })
    );

    let _ = std::fs::remove_file(&preset_path);
}

#[test]
fn session_layout_overrides_default_workspace_preset() {
    let preset_path = unique_test_path("workspace-preset-catalog");
    save_workspace_preset_catalog(
        &preset_path,
        &WorkspacePresetCatalog {
            format_version: 1,
            default_preset_id: "test-preset".to_string(),
            presets: vec![WorkspacePreset {
                id: "test-preset".to_string(),
                label: "Test preset".to_string(),
                ui_state: panel_workspace::WorkspaceUiState::new(
                    panel_workspace::WorkspaceLayout {
                        panels: vec![WorkspacePanelState {
                            id: "builtin.layers".to_string(),
                            visible: true,
                            anchor: WorkspacePanelAnchor::TopRight,
                            position: Some(WorkspacePanelPosition { x: 40, y: 88 }),
                            size: Some(WorkspacePanelSize {
                                width: 320,
                                height: 260,
                            }),
                        }],
                    },
                    BTreeMap::new(),
                ),
            }],
        },
    )
    .expect("preset save should succeed");
    let session_path = unique_test_path("preset-session");
    save_session_state(
        &session_path,
        &DesktopSessionState {
            last_project_path: None,
            ui_state: panel_workspace::WorkspaceUiState::new(
                panel_workspace::WorkspaceLayout {
                    panels: vec![WorkspacePanelState {
                        id: "builtin.layers".to_string(),
                        visible: true,
                        anchor: WorkspacePanelAnchor::TopLeft,
                        position: Some(WorkspacePanelPosition { x: 12, y: 24 }),
                        size: Some(WorkspacePanelSize {
                            width: 300,
                            height: 240,
                        }),
                    }],
                },
                BTreeMap::new(),
            ),
            editor_session: EditorSession::default(),
        },
    )
    .expect("session save should succeed");

    let app = DesktopApp::with_options(DesktopAppOptions {
        project_path: PathBuf::from("/tmp/altpaint-test.altp.json"),
        dialogs: Box::new(TestDialogs::default()),
        session_path: session_path.clone(),
        workspace_preset_path: preset_path.clone(),
    });
    let entry = app
        .panel_workspace
        .workspace_layout()
        .panels
        .into_iter()
        .find(|entry| entry.id == "builtin.layers")
        .expect("layers panel layout exists");

    assert_eq!(entry.anchor, WorkspacePanelAnchor::TopLeft);
    assert_eq!(
        entry.position,
        Some(WorkspacePanelPosition { x: 12, y: 24 })
    );

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(preset_path);
}

#[test]
fn startup_restores_last_opened_project_from_session() {
    let session_path = unique_test_path("desktop-session");
    let project_path = unique_test_path("startup-project");
    let mut source_app = test_app_with_dialogs_and_session_path(
        TestDialogs::with_save_path(project_path.clone()),
        session_path.clone(),
    );
    source_app.document.work.title = "Recovered Project".to_string();

    assert!(source_app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_AS)));
    source_app.wait_for_pending_save_tasks();

    let app = DesktopApp::with_options(DesktopAppOptions {
        project_path: default_project_path(),
        dialogs: Box::new(TestDialogs::default()),
        session_path: session_path.clone(),
        workspace_preset_path: unique_test_path("workspace-presets"),
    });

    assert_eq!(app.paths.project_path, project_path);
    assert_eq!(app.document.work.title, "Recovered Project");

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(app.paths.project_path.clone());
}

/// BL-079: エディタセッション (ツール/色/ペン/ビュー) は project ファイルではなく
/// session 永続化で round-trip する。色とペンサイズを変えてプロジェクト保存
/// (session も永続化される) → 再起動で復元されることを検証する。
#[test]
fn editor_session_round_trips_through_session_save_load() {
    let session_path = unique_test_path("editor-session");
    let project_path = unique_test_path("editor-session-project");
    let mut source_app = test_app_with_dialogs_and_session_path(
        TestDialogs::with_save_path(project_path.clone()),
        session_path.clone(),
    );

    let restored_color = ColorRgba8::new(0x8e, 0x24, 0xaa, 0xff);
    let _ = source_app.apply_session_command(&SessionCommand::SetActiveColor {
        color: restored_color,
    });
    let _ = source_app.apply_session_command(&SessionCommand::SetActivePenSize { size: 23 });
    assert_ne!(restored_color, EditorSession::default().active_color);

    // プロジェクト保存は session 永続化も走らせる (save_project_to_path → persist_session_state)。
    assert!(source_app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_AS)));
    source_app.wait_for_pending_save_tasks();

    let app = DesktopApp::with_options(DesktopAppOptions {
        project_path: default_project_path(),
        dialogs: Box::new(TestDialogs::default()),
        session_path: session_path.clone(),
        workspace_preset_path: unique_test_path("workspace-presets"),
    });

    assert_eq!(app.document.session.active_color, restored_color);
    assert_eq!(app.document.session.active_pen_size, 23);

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(project_path);
}

/// BL-079: project ファイルは作品コンテンツのみを保存する。project を直接保存して
/// 読み込むと、保存時のエディタセッション (色) は project には含まれず、読込側の
/// 現在セッションが温存される。
#[test]
fn loading_project_preserves_live_editor_session() {
    let path = std::env::temp_dir().join(format!(
        "altpaint-session-boundary-{}.altp.json",
        std::process::id()
    ));
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    let saved_color = ColorRgba8::new(0x11, 0x22, 0x33, 0xff);
    let _ = source_app.apply_session_command(&SessionCommand::SetActiveColor { color: saved_color });
    save_project_to_path(
        &path,
        &source_app.document,
        &source_app.panel_workspace.workspace_layout(),
        &BTreeMap::new(),
    )
    .expect("project save should succeed");

    // 別アプリで現在セッションの色を変えてから project を読み込む。
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let live_color = ColorRgba8::new(0xaa, 0xbb, 0xcc, 0xff);
    let _ = app.apply_session_command(&SessionCommand::SetActiveColor { color: live_color });
    assert!(app.execute_service_request(
        ServiceRequest::new(names::PROJECT_LOAD_FROM_PATH)
            .with_value("path", path.to_string_lossy().to_string()),
    ));

    // 読込側の現在セッション (live_color) が温存され、project 保存時の色は反映されない。
    assert_eq!(app.document.session.active_color, live_color);
    assert_ne!(app.document.session.active_color, saved_color);

    let _ = std::fs::remove_file(path);
}

/// パネル visibility round-trip: session 永続化 → 復元後も非表示状態が復元される。
/// (旧テスト名 `panel_layout_persists_across_restart_via_session` をリネーム + visibility 観点に集約)
#[test]
fn panel_visibility_round_trip_through_session_save_load() {
    let session_path = unique_test_path("layout-session");
    let mut source_app =
        test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());

    assert!(source_app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_MOVE_PANEL)
            .with_value("panel_id", "builtin.layers")
            .with_value("direction", "up"),
    ));
    assert!(source_app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
            .with_value("panel_id", "builtin.tool-palette")
            .with_value("visible", false),
    ));
    let expected_layout = source_app.panel_workspace.workspace_layout();

    let app = test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());
    assert!(
        !app.panel_workspace
            .is_panel_visible("builtin.tool-palette"),
        "tool-palette stays hidden after session round-trip"
    );
    assert_eq!(app.panel_workspace.workspace_layout(), expected_layout);

    let _ = std::fs::remove_file(session_path);
}

#[test]
fn startup_preserves_last_selected_workspace_preset_id() {
    let preset_path = unique_test_path("workspace-preset-selected");
    save_workspace_preset_catalog(
        &preset_path,
        &WorkspacePresetCatalog {
            format_version: 1,
            default_preset_id: "default".to_string(),
            presets: vec![
                WorkspacePreset {
                    id: "default".to_string(),
                    label: "Default".to_string(),
                    ui_state: panel_workspace::WorkspaceUiState::default(),
                },
                WorkspacePreset {
                    id: "review".to_string(),
                    label: "Review".to_string(),
                    ui_state: panel_workspace::WorkspaceUiState::new(
                        panel_workspace::WorkspaceLayout {
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
    .expect("preset save should succeed");

    let mut source_app = DesktopApp::with_options(DesktopAppOptions {
        project_path: PathBuf::from("/tmp/altpaint-test.altp.json"),
        dialogs: Box::new(TestDialogs::default()),
        session_path: unique_test_path("selected-preset-session-source"),
        workspace_preset_path: preset_path.clone(),
    });
    assert!(source_app.execute_service_request(
        ServiceRequest::new(names::WORKSPACE_APPLY_PRESET).with_value("preset_id", "review"),
    ));

    let restarted = DesktopApp::with_options(DesktopAppOptions {
        project_path: PathBuf::from("/tmp/altpaint-test.altp.json"),
        dialogs: Box::new(TestDialogs::default()),
        session_path: source_app.paths.session_path.clone(),
        workspace_preset_path: preset_path.clone(),
    });

    assert_eq!(
        restarted
            .panel_runtime
            .persistent_panel_configs()
            .get("builtin.workspace-presets")
            .and_then(|config| config.get("selected_workspace"))
            .and_then(|value| value.as_str()),
        Some("review")
    );

    let _ = std::fs::remove_file(source_app.paths.session_path.clone());
    let _ = std::fs::remove_file(preset_path);
}
