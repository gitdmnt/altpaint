//! `DesktopApp` の保存・読込とワークスペース復元に関するテストをまとめる。

use std::path::PathBuf;

use app_core::Command;
use plugin_api::HostAction;
use storage::{load_project_from_path, save_project_to_path};

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::DesktopApp;
use crate::frame::status_text_rect;
use crate::profiler::DesktopProfiler;

/// open ダイアログ経由の読込でワークスペース状態も復元されることを確認する。
#[test]
fn execute_command_load_project_uses_native_dialog_path() {
    let path = std::env::temp_dir().join("altpaint-open-dialog-test.altp.json");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    assert!(source_app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-palette".to_string(),
        visible: false,
    }));
    save_project_to_path(
        &path,
        &source_app.document,
        &source_app.ui_shell.workspace_layout(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
    assert!(app.execute_command(Command::LoadProject));
    assert_eq!(app.project_path, path);
    assert!(!app.ui_shell.panel_trees().iter().any(|panel| panel.id == "builtin.tool-palette"));

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
    assert!(source_app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-palette".to_string(),
        visible: false,
    }));
    save_project_to_path(
        &path,
        &source_app.document,
        &source_app.ui_shell.workspace_layout(),
    )
    .expect("project save should succeed");

    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert!(app.execute_command(Command::LoadProjectFromPath {
        path: path.to_string_lossy().to_string(),
    }));

    let panels = app.ui_shell.panel_trees();
    assert!(!panels.iter().any(|panel| panel.id == "builtin.tool-palette"));
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
        update.dirty_rect,
        Some(layout.panel_host_rect.union(status_text_rect(1280, 200, &layout)))
    );
}

/// パネル表示切替が全面再構成ではなく差分更新で反映されることを確認する。
#[test]
fn set_panel_visibility_updates_status_without_full_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
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
        update.dirty_rect,
        Some(layout.panel_host_rect.union(status_text_rect(1280, 200, &layout)))
    );
}
