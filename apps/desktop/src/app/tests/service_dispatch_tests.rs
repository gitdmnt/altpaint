//! service request から desktop host service handler へ届く経路を検証する。

use desktop_support::{WorkspacePreset, WorkspacePresetCatalog, save_workspace_preset_catalog};
use panel_api::{HostAction, ServiceRequest, services::names};
use workspace_persistence::WorkspaceUiState;

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

    assert_eq!(app.io_state.pending_save_tasks.len(), 1);
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
