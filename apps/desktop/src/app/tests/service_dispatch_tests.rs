//! service request から desktop host service handler へ届く経路を検証する。

use desktop_support::{WorkspacePreset, WorkspacePresetCatalog, save_workspace_preset_catalog};
use panel_runtime::{HostAction, ServiceRequest, services::names};
use app_core::WorkspaceUiState;

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_workspace_preset_path,
    unique_test_path,
};

#[test]
fn request_service_new_document_sized_updates_bitmap() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(
        app.execute_host_action(HostAction::RequestService(
            ServiceRequest::new(names::PROJECT_NEW_DOCUMENT_SIZED)
                .with_value("width", 128)
                .with_value("height", 96),
        ))
    );

    let bitmap = app.document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (128, 96));
}

#[test]
fn request_service_save_project_enqueues_background_task() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(
        app.execute_host_action(HostAction::RequestService(ServiceRequest::new(
            names::PROJECT_SAVE_CURRENT,
        )))
    );

    assert_eq!(app.background_jobs.len(), 1);
}

#[test]
fn request_service_save_workspace_preset_persists_catalog() {
    let preset_path = unique_test_path("service-workspace-presets");
    let catalog = WorkspacePresetCatalog {
        format_version: 1,
        default_preset_id: "default".to_string(),
        presets: vec![WorkspacePreset {
            id: "default".to_string(),
            label: "Default".to_string(),
            ui_state: WorkspaceUiState::default(),
        }],
    };
    save_workspace_preset_catalog(&preset_path, &catalog).expect("save preset catalog");
    let mut app = test_app_with_dialogs_and_workspace_preset_path(
        TestDialogs::default(),
        preset_path.clone(),
    );

    assert!(
        app.execute_host_action(HostAction::RequestService(
            ServiceRequest::new(names::WORKSPACE_SAVE_PRESET)
                .with_value("preset_id", "review")
                .with_value("label", "Review"),
        ))
    );

    let reloaded = desktop_support::load_workspace_preset_catalog(&preset_path);
    assert!(reloaded.presets.iter().any(|preset| preset.id == "review"));
}

#[test]
fn snapshot_create_service_increases_snapshot_count() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert_eq!(app.snapshots.len(), 0);

    assert!(
        app.execute_host_action(HostAction::RequestService(
            ServiceRequest::new(names::SNAPSHOT_CREATE).with_value("label", "test-snap"),
        ))
    );

    assert_eq!(app.snapshots.len(), 1);
}

#[test]
fn snapshot_restore_service_restores_document() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let id = app.snapshots.push(app.document.clone());

    assert!(
        app.execute_host_action(HostAction::RequestService(
            ServiceRequest::new(names::SNAPSHOT_RESTORE).with_value("snapshot_id", id),
        ))
    );
}

/// `workspace_layout.set_panel_visibility` がパネル可視性を切り替える。
#[test]
fn request_service_workspace_layout_set_panel_visibility_toggles_visibility() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert!(
        app.panel_presentation
            .is_panel_visible("builtin.tool-palette"),
        "tool-palette defaults to visible"
    );

    assert!(
        app.execute_host_action(HostAction::RequestService(
            ServiceRequest::new(names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY)
                .with_value("panel_id", "builtin.tool-palette")
                .with_value("visible", false),
        ))
    );

    assert!(
        !app.panel_presentation
            .is_panel_visible("builtin.tool-palette"),
        "tool-palette should be hidden after service call"
    );
}

/// `koma_nav.*` service がコマ選択へ届く (K6 wire 改名の操作経路生存確認)。
#[test]
fn request_service_koma_nav_add_and_select_changes_active_koma() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert_eq!(app.document.active_panel_index(), 0);

    assert!(
        app.execute_host_action(HostAction::RequestService(ServiceRequest::new(
            names::KOMA_NAV_ADD,
        )))
    );
    assert_eq!(app.document.active_panel_index(), 1);

    assert!(
        app.execute_host_action(HostAction::RequestService(
            ServiceRequest::new(names::KOMA_NAV_SELECT).with_value("index", 0),
        ))
    );
    assert_eq!(app.document.active_panel_index(), 0);
}

#[test]
fn request_service_reload_pen_presets_refreshes_document_state() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.document.pen_presets.clear();

    assert!(
        app.execute_host_action(HostAction::RequestService(ServiceRequest::new(
            names::TOOL_CATALOG_RELOAD_PEN_PRESETS,
        )))
    );

    assert!(!app.document.pen_presets.is_empty());
}
