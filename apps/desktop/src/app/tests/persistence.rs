//! `DesktopApp` の保存・読込とワークスペース復元に関するテストをまとめる。

use std::path::PathBuf;

use app_core::Command;
use desktop_support::{DEFAULT_PROJECT_PATH, DesktopProfiler};
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
    assert_eq!(app.project_path, path);
    assert!(
        !app.ui_shell
            .panel_trees()
            .iter()
            .any(|panel| panel.id == "builtin.tool-palette")
    );

    let _ = std::fs::remove_file(app.project_path.clone());
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
    assert_eq!(app.project_path, path);
    assert!(
        loaded
            .workspace_layout
            .panels
            .iter()
            .any(|entry| entry.id == "builtin.tool-palette" && !entry.visible)
    );

    let _ = std::fs::remove_file(app.project_path.clone());
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
        loaded.plugin_configs.get("builtin.app-actions"),
        Some(&json!({
            "new_shortcut": "Ctrl+Alt+N",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_command(Command::LoadProject));
    assert_eq!(
        app.ui_shell
            .persistent_panel_configs()
            .get("builtin.app-actions"),
        Some(&json!({
            "new_shortcut": "Ctrl+Alt+N",
            "save_shortcut": "Ctrl+S",
            "save_as_shortcut": "Ctrl+Shift+S",
            "open_shortcut": "Ctrl+O"
        }))
    );

    let _ = std::fs::remove_file(path);
}

/// 読込でパネル順序と表示状態がワークスペースへ復元されることを確認する。
#[test]
fn load_project_restores_workspace_layout() {
    let path = std::env::temp_dir().join("altpaint-load-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    let before_ids = source_app
        .ui_shell
        .panel_trees()
        .iter()
        .map(|panel| panel.id)
        .collect::<Vec<_>>();
    let before_index = before_ids
        .iter()
        .position(|panel_id| *panel_id == "builtin.layers-panel")
        .expect("layers panel visible");
    assert!(source_app.execute_host_action(HostAction::MovePanel {
        panel_id: "builtin.layers-panel".to_string(),
        direction: plugin_api::PanelMoveDirection::Up,
    }));
    assert!(source_app.execute_host_action(HostAction::MovePanel {
        panel_id: "builtin.layers-panel".to_string(),
        direction: plugin_api::PanelMoveDirection::Up,
    }));
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
    save_project_to_path(
        &path,
        &source_app.document,
        &source_app.ui_shell.workspace_layout(),
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
    let visible_ids = panels.iter().map(|panel| panel.id).collect::<Vec<_>>();
    let layers_index = visible_ids
        .iter()
        .position(|panel_id| *panel_id == "builtin.layers-panel")
        .expect("layers panel visible");
    assert!(layers_index < before_index);

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
        Some(layout.panel_host_rect.union(status_text_bounds(
            1280,
            200,
            &layout,
            &app.status_text()
        )))
    );
    assert_eq!(update.overlay_dirty_rect, None);
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
        Some(layout.panel_host_rect.union(status_text_bounds(
            1280,
            200,
            &layout,
            &app.status_text()
        )))
    );
    assert_eq!(update.overlay_dirty_rect, None);
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

    let app = DesktopApp::new_with_dialogs_and_session_path(
        PathBuf::from(DEFAULT_PROJECT_PATH),
        Box::new(TestDialogs::default()),
        session_path.clone(),
    );

    assert_eq!(app.project_path, project_path);
    assert_eq!(app.document.work.title, "Recovered Project");

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(app.project_path.clone());
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
