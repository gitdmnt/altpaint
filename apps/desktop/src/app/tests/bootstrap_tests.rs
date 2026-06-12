//! 起動時 bootstrap の回帰テストをまとめる。

use std::path::PathBuf;

use app_core::Command;
use desktop_support::DEFAULT_PROJECT_PATH;
use storage::load_project_from_path;

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_session_path, unique_test_path,
};
use crate::app::DesktopApp;

#[test]
fn startup_restores_last_project_from_session_path() {
    let session_path = unique_test_path("bootstrap-session");
    let project_path = unique_test_path("bootstrap-project");
    let mut source_app =
        test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());
    source_app.document.work.title = "Recovered Project".to_string();
    assert!(source_app.execute_command(Command::SaveProjectToPath {
        path: project_path.to_string_lossy().to_string(),
    }));
    source_app.wait_for_pending_save_tasks();
    let app = DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from(DEFAULT_PROJECT_PATH),
        Box::new(TestDialogs::default()),
        session_path.clone(),
        unique_test_path("bootstrap-workspace-presets"),
    );

    assert_eq!(app.io_state.project_path, project_path);
    assert_eq!(app.document.work.title, "Recovered Project");

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn bootstrap_saved_project_can_be_loaded_again() {
    let project_path = unique_test_path("bootstrap-roundtrip-project");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    source_app.document.work.title = "Roundtrip".to_string();
    assert!(source_app.execute_command(Command::SaveProjectToPath {
        path: project_path.to_string_lossy().to_string(),
    }));
    source_app.wait_for_pending_save_tasks();

    let loaded = load_project_from_path(&project_path).expect("saved project should load");
    assert_eq!(loaded.document.work.title, "Roundtrip");

    let _ = std::fs::remove_file(project_path);
}
