//! `DesktopApp` の保存・読込とワークスペース復元に関するテストをまとめる。

use std::path::PathBuf;

use app_core::Command;
use app_core::{
    WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize, WorkspacePanelState,
};
use desktop_support::{
    DEFAULT_PROJECT_PATH, DesktopProfiler, WorkspacePreset, WorkspacePresetCatalog,
    save_workspace_preset_catalog,
};
use panel_api::{HostAction, PanelMoveDirection};
use serde_json::json;
use std::collections::BTreeMap;
use storage::{load_project_from_path, save_project_to_path};

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_session_path, unique_test_path,
};
use crate::app::DesktopApp;

/// execute コマンド 読込 プロジェクト uses native ダイアログ パス が期待どおりに動作することを検証する。
#[test]
fn execute_command_load_project_uses_native_dialog_path() {
    let path = std::env::temp_dir().join("altpaint-open-dialog-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    assert!(
        source_app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        })
    );
    save_project_to_path(
        &path,
        &source_app.document,
        &source_app.panel_presentation.workspace_layout(),
        &BTreeMap::new(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_command(Command::LoadProject));
    app.wait_for_pending_save_tasks();
    assert_eq!(app.io_state.project_path, path);
    assert!(
        !app.panel_presentation
            .panel_trees(&app.panel_runtime)
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );

    let _ = std::fs::remove_file(app.io_state.project_path.clone());
}

/// 保存 プロジェクト as updates プロジェクト パス and persists ワークスペース レイアウト が期待どおりに動作することを検証する。
#[test]
fn save_project_as_updates_project_path_and_persists_workspace_layout() {
    let path = std::env::temp_dir().join("altpaint-save-as-test.altp.json");
    let mut app = test_app_with_dialogs(TestDialogs::with_save_path(path.clone()));

    assert!(app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-palette".to_string(),
        visible: false,
    }));
    assert!(app.execute_command(Command::SaveProjectAs));
    assert_eq!(app.pending_save_task_count(), 1);
    app.wait_for_pending_save_tasks();

    let loaded = load_project_from_path(&path).expect("saved project should load");
    assert_eq!(app.io_state.project_path, path);
    assert!(
        loaded
            .ui_state
            .workspace_layout
            .panels
            .iter()
            .any(|entry| entry.id == "builtin.tool-palette" && !entry.visible)
    );

    let _ = std::fs::remove_file(app.io_state.project_path.clone());
}

/// 保存 and 読込 復元 プラグイン ショートカット configs が期待どおりに動作することを検証する。
#[test]
fn save_and_load_restore_plugin_shortcut_configs() {
    let path = std::env::temp_dir().join("altpaint-plugin-config-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::with_save_path(path.clone()));
    assert!(source_app.activate_panel_control("builtin.app-actions", "app.shortcuts"));
    assert!(source_app.activate_panel_control("builtin.app-actions", "app.shortcut.new"));
    assert!(source_app.dispatch_keyboard_shortcut("Ctrl+Alt+N", "N", false));
    assert!(source_app.execute_command(Command::SaveProjectAs));
    source_app.wait_for_pending_save_tasks();

    let loaded = load_project_from_path(&path).expect("saved project should load");
    assert_eq!(
        loaded.ui_state.plugin_configs.get("builtin.app-actions"),
        Some(&json!({
            "default_template_size": "2894x4093",
            "new_shortcut": "Ctrl+Alt+N",
            "template_options": "2894x4093:A4 350dpi (2894×4093)|2480x3508:A4 300dpi (2480×3508)|2048x2048:Square 2048 (2048×2048)|1920x1080:HD Landscape (1920×1080)",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );
    assert_eq!(
        loaded
            .ui_state
            .plugin_configs
            .get("builtin.workspace-presets"),
        Some(&json!({
            "workspace_options": "default-floating:Default floating workspace",
            "selected_workspace": "default-floating",
            "selected_workspace_label": "Default floating workspace"
        }))
    );

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_command(Command::LoadProject));
    assert_eq!(
        app.panel_runtime
            .persistent_panel_configs()
            .get("builtin.app-actions"),
        Some(&json!({
            "default_template_size": "2894x4093",
            "new_shortcut": "Ctrl+Alt+N",
            "template_options": "2894x4093:A4 350dpi (2894×4093)|2480x3508:A4 300dpi (2480×3508)|2048x2048:Square 2048 (2048×2048)|1920x1080:HD Landscape (1920×1080)",
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
            "workspace_options": "default-floating:Default floating workspace",
            "selected_workspace": "default-floating",
            "selected_workspace_label": "Default floating workspace"
        }))
    );

    let _ = std::fs::remove_file(path);
}

/// 読込 プロジェクト restores ワークスペース レイアウト が期待どおりに動作することを検証する。
#[test]
fn load_project_restores_workspace_layout() {
    let path = std::env::temp_dir().join("altpaint-load-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    let mut moved = false;
    for _ in 0..3 {
        moved |= source_app.execute_host_action(HostAction::MovePanel {
            panel_id: "builtin.layers-panel".to_string(),
            direction: PanelMoveDirection::Up,
        });
    }
    assert!(moved);
    assert!(
        source_app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        })
    );
    let expected_layout = source_app.panel_presentation.workspace_layout();
    save_project_to_path(
        &path,
        &source_app.document,
        &expected_layout,
        &BTreeMap::new(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert!(app.execute_command(Command::LoadProjectFromPath {
        path: path.to_string_lossy().to_string(),
    }));

    let panels = app.panel_presentation.panel_trees(&app.panel_runtime);
    assert!(
        !panels
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );
    assert_eq!(app.panel_presentation.workspace_layout(), expected_layout);

    let _ = std::fs::remove_file(path);
}

/// move パネル ホスト action updates ステータス without full recompose が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn move_panel_host_action_updates_status_without_full_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_host_action(HostAction::MovePanel {
        panel_id: "builtin.layers-panel".to_string(),
        direction: PanelMoveDirection::Up,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(profiler.stats.contains_key("compose_dirty_panel"));
    assert!(profiler.stats.contains_key("compose_dirty_status"));
    assert_eq!(
        update.base_dirty_rect,
        Some(render::status_text_bounds(
            1280,
            200,
            layout.canvas_host_rect,
            &app.status_text(),
        ))
    );
    assert_eq!(
        update.overlay_dirty_rect,
        app.panel_presentation
            .last_panel_surface_dirty_rect()
            .map(|dirty| crate::frame::Rect {
                x: dirty.x,
                y: dirty.y,
                width: dirty.width,
                height: dirty.height,
            })
    );
}

/// 設定 パネル visibility updates ステータス without full recompose が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn set_panel_visibility_updates_status_without_full_recompose() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-palette".to_string(),
        visible: false,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(profiler.stats.contains_key("compose_dirty_panel"));
    assert!(profiler.stats.contains_key("compose_dirty_status"));
    assert_eq!(
        update.base_dirty_rect,
        Some(render::status_text_bounds(
            1280,
            200,
            layout.canvas_host_rect,
            &app.status_text(),
        ))
    );
    assert_eq!(
        update.overlay_dirty_rect,
        app.panel_presentation
            .last_panel_surface_dirty_rect()
            .map(|dirty| crate::frame::Rect {
                x: dirty.x,
                y: dirty.y,
                width: dirty.width,
                height: dirty.height,
            })
    );
}

/// hiding パネル clears 前 オーバーレイ 範囲 when サーフェス shrinks が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn hiding_panel_clears_previous_overlay_bounds_when_surface_shrinks() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.panel_presentation.move_panel_to(
        "builtin.tool-palette",
        940,
        72,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let hidden_panel_rect = app
        .panel_presentation
        .panel_rect("builtin.tool-palette")
        .expect("hidden panel rect exists");

    profiler.stats.clear();
    assert!(app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-palette".to_string(),
        visible: false,
    }));
    let update = app.prepare_present_frame(1280, 800, &mut profiler);

    assert!(profiler.stats.contains_key("compose_dirty_panel"));
    assert_eq!(
        update.overlay_dirty_rect,
        Some(crate::frame::Rect {
            x: hidden_panel_rect.x,
            y: hidden_panel_rect.y,
            width: hidden_panel_rect.width,
            height: hidden_panel_rect.height,
        })
    );
}

/// startup uses 既定 ワークスペース preset when プロジェクト and セッション are empty が期待どおりに動作することを検証する。
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
                ui_state: workspace_persistence::WorkspaceUiState::new(
                    app_core::WorkspaceLayout {
                        panels: vec![WorkspacePanelState {
                            id: "builtin.layers-panel".to_string(),
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
    let app = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        unique_test_path("preset-project"),
        Box::new(TestDialogs::default()),
        unique_test_path("preset-session"),
        preset_path.clone(),
    );
    let entry = app
        .panel_presentation
        .workspace_layout()
        .panels
        .into_iter()
        .find(|entry| entry.id == "builtin.layers-panel")
        .expect("layers panel layout exists");

    assert_eq!(entry.anchor, WorkspacePanelAnchor::TopRight);
    assert_eq!(
        entry.position,
        Some(WorkspacePanelPosition { x: 40, y: 88 })
    );

    let _ = std::fs::remove_file(&preset_path);
}

/// セッション レイアウト overrides 既定 ワークスペース preset が期待どおりに動作することを検証する。
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
                ui_state: workspace_persistence::WorkspaceUiState::new(
                    app_core::WorkspaceLayout {
                        panels: vec![WorkspacePanelState {
                            id: "builtin.layers-panel".to_string(),
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
    desktop_support::save_session_state(
        &session_path,
        &desktop_support::DesktopSessionState {
            last_project_path: None,
            ui_state: workspace_persistence::WorkspaceUiState::new(
                app_core::WorkspaceLayout {
                    panels: vec![WorkspacePanelState {
                        id: "builtin.layers-panel".to_string(),
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
        },
    )
    .expect("session save should succeed");

    let app = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(TestDialogs::default()),
        session_path.clone(),
        preset_path.clone(),
    );
    let entry = app
        .panel_presentation
        .workspace_layout()
        .panels
        .into_iter()
        .find(|entry| entry.id == "builtin.layers-panel")
        .expect("layers panel layout exists");

    assert_eq!(entry.anchor, WorkspacePanelAnchor::TopLeft);
    assert_eq!(
        entry.position,
        Some(WorkspacePanelPosition { x: 12, y: 24 })
    );

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(preset_path);
}

/// startup restores last opened プロジェクト from セッション が期待どおりに動作することを検証する。
#[test]
fn startup_restores_last_opened_project_from_session() {
    let session_path = unique_test_path("desktop-session");
    let project_path = unique_test_path("startup-project");
    let mut source_app = test_app_with_dialogs_and_session_path(
        TestDialogs::with_save_path(project_path.clone()),
        session_path.clone(),
    );
    source_app.document.work.title = "Recovered Project".to_string();

    assert!(source_app.execute_command(Command::SaveProjectAs));
    source_app.wait_for_pending_save_tasks();

    let app = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from(DEFAULT_PROJECT_PATH),
        Box::new(TestDialogs::default()),
        session_path.clone(),
        unique_test_path("workspace-presets"),
    );

    assert_eq!(app.io_state.project_path, project_path);
    assert_eq!(app.document.work.title, "Recovered Project");

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(app.io_state.project_path.clone());
}

/// パネル レイアウト persists across restart via セッション が期待どおりに動作することを検証する。
#[test]
fn panel_layout_persists_across_restart_via_session() {
    let session_path = unique_test_path("layout-session");
    let mut source_app =
        test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());

    assert!(source_app.execute_host_action(HostAction::MovePanel {
        panel_id: "builtin.layers-panel".to_string(),
        direction: PanelMoveDirection::Up,
    }));
    assert!(
        source_app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        })
    );
    let expected_layout = source_app.panel_presentation.workspace_layout();

    let app = test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());
    let panels = app.panel_presentation.panel_trees(&app.panel_runtime);
    assert!(
        !panels
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );
    assert_eq!(app.panel_presentation.workspace_layout(), expected_layout);

    let _ = std::fs::remove_file(session_path);
}

/// startup preserves last 選択中 ワークスペース preset ID が期待どおりに動作することを検証する。
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
                    ui_state: workspace_persistence::WorkspaceUiState::default(),
                },
                WorkspacePreset {
                    id: "review".to_string(),
                    label: "Review".to_string(),
                    ui_state: workspace_persistence::WorkspaceUiState::new(
                        app_core::WorkspaceLayout {
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
    .expect("preset save should succeed");

    let mut source_app = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(TestDialogs::default()),
        unique_test_path("selected-preset-session-source"),
        preset_path.clone(),
    );
    assert!(source_app.execute_command(Command::ApplyWorkspacePreset {
        preset_id: "review".to_string(),
    }));

    let restarted = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(TestDialogs::default()),
        source_app.io_state.session_path.clone(),
        preset_path.clone(),
    );

    assert_eq!(
        restarted
            .panel_runtime
            .persistent_panel_configs()
            .get("builtin.workspace-presets")
            .and_then(|config| config.get("selected_workspace"))
            .and_then(|value| value.as_str()),
        Some("review")
    );

    let _ = std::fs::remove_file(source_app.io_state.session_path.clone());
    let _ = std::fs::remove_file(preset_path);
}
