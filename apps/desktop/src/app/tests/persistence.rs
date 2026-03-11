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
use plugin_api::HostAction;
use serde_json::json;
use std::collections::BTreeMap;
use storage::{load_project_from_path, save_project_to_path};

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_session_path, unique_test_path,
};
use crate::app::DesktopApp;
use crate::frame::status_text_bounds;

/// open ダイアログ経由の読込でワークスペース状態も復元されることを確認する。
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
        &source_app.ui_shell.workspace_layout(),
        &BTreeMap::new(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_command(Command::LoadProject));
    app.wait_for_pending_save_tasks();
    assert_eq!(app.io_state.project_path, path);
    assert!(
        !app.ui_shell
            .panel_trees()
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );

    let _ = std::fs::remove_file(app.io_state.project_path.clone());
}

/// 保存先選択付き保存で現在パスとワークスペース状態が永続化されることを確認する。
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
        loaded.ui_state.plugin_configs.get("builtin.workspace-presets"),
        Some(&json!({
            "workspace_options": "default-floating:Default floating workspace",
            "selected_workspace": "default-floating",
            "selected_workspace_label": "Default floating workspace"
        }))
    );

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_command(Command::LoadProject));
    assert_eq!(
        app.ui_shell
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
        app.ui_shell
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

/// 読込でパネル順序と表示状態がワークスペースへ復元されることを確認する。
#[test]
fn load_project_restores_workspace_layout() {
    let path = std::env::temp_dir().join("altpaint-load-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    let mut moved = false;
    for _ in 0..3 {
        moved |= source_app.execute_host_action(HostAction::MovePanel {
            panel_id: "builtin.layers-panel".to_string(),
            direction: plugin_api::PanelMoveDirection::Up,
        });
    }
    assert!(moved);
    assert!(
        source_app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        })
    );
    let expected_layout = source_app.ui_shell.workspace_layout();
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

    let panels = app.ui_shell.panel_trees();
    assert!(
        !panels
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );
    assert_eq!(app.ui_shell.workspace_layout(), expected_layout);

    let _ = std::fs::remove_file(path);
}

/// パネル移動が全面再構成ではなく差分更新で反映されることを確認する。
#[test]
fn move_panel_host_action_updates_status_without_full_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_host_action(HostAction::MovePanel {
        panel_id: "builtin.layers-panel".to_string(),
        direction: plugin_api::PanelMoveDirection::Up,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(profiler.stats.contains_key("compose_dirty_panel"));
    assert!(profiler.stats.contains_key("compose_dirty_status"));
    assert_eq!(
        update.base_dirty_rect,
        Some(status_text_bounds(1280, 200, &layout, &app.status_text()))
    );
    assert_eq!(
        update.overlay_dirty_rect,
        app.ui_shell
            .last_panel_surface_dirty_rect()
            .map(|dirty| crate::frame::Rect {
                x: dirty.x,
                y: dirty.y,
                width: dirty.width,
                height: dirty.height,
            })
    );
}

/// パネル表示切替が全面再構成ではなく差分更新で反映されることを確認する。
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
        Some(status_text_bounds(1280, 200, &layout, &app.status_text()))
    );
    assert_eq!(
        update.overlay_dirty_rect,
        app.ui_shell
            .last_panel_surface_dirty_rect()
            .map(|dirty| crate::frame::Rect {
                x: dirty.x,
                y: dirty.y,
                width: dirty.width,
                height: dirty.height,
            })
    );
}

#[test]
fn hiding_panel_clears_previous_overlay_bounds_when_surface_shrinks() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.ui_shell.move_panel_to(
        "builtin.tool-palette",
        940,
        72,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let hidden_panel_rect = app
        .ui_shell
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

    let app = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(TestDialogs::default()),
        unique_test_path("preset-session"),
        preset_path.clone(),
    );
    let entry = app
        .ui_shell
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
        .ui_shell
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

#[test]
fn panel_layout_persists_across_restart_via_session() {
    let session_path = unique_test_path("layout-session");
    let mut source_app =
        test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());

    assert!(source_app.execute_host_action(HostAction::MovePanel {
        panel_id: "builtin.layers-panel".to_string(),
        direction: plugin_api::PanelMoveDirection::Up,
    }));
    assert!(
        source_app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        })
    );
    let expected_layout = source_app.ui_shell.workspace_layout();

    let app = test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());
    let panels = app.ui_shell.panel_trees();
    assert!(
        !panels
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );
    assert_eq!(app.ui_shell.workspace_layout(), expected_layout);

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
            .ui_shell
            .persistent_panel_configs()
            .get("builtin.workspace-presets")
            .and_then(|config| config.get("selected_workspace"))
            .and_then(|value| value.as_str()),
        Some("review")
    );

    let _ = std::fs::remove_file(source_app.io_state.session_path.clone());
    let _ = std::fs::remove_file(preset_path);
}
