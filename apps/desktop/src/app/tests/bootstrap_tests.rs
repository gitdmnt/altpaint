//! 起動時 bootstrap の回帰テストをまとめる。

use crate::platform::default_project_path;
use panel_runtime::{ServiceRequest, services::names};
use project_store::load_project_from_path;

use super::{
    TestDialogs, test_app_with_dialogs, test_app_with_dialogs_and_session_path, unique_test_path,
};
use crate::app::{DesktopApp, DesktopAppOptions};

#[test]
fn startup_restores_last_project_from_session_path() {
    let session_path = unique_test_path("bootstrap-session");
    let project_path = unique_test_path("bootstrap-project");
    let mut source_app =
        test_app_with_dialogs_and_session_path(TestDialogs::default(), session_path.clone());
    source_app.document.work.title = "Recovered Project".to_string();
    assert!(source_app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_TO_PATH).with_value("path", project_path.to_string_lossy().to_string())));
    source_app.wait_for_pending_save_tasks();
    let app = DesktopApp::with_options(DesktopAppOptions {
        project_path: default_project_path(),
        dialogs: Box::new(TestDialogs::default()),
        session_path: session_path.clone(),
        workspace_preset_path: unique_test_path("bootstrap-workspace-presets"),
    });

    assert_eq!(app.paths.project_path, project_path);
    assert_eq!(app.document.work.title, "Recovered Project");

    let _ = std::fs::remove_file(session_path);
    let _ = std::fs::remove_file(project_path);
}

/// BL-095: meta 由来の default-floating カタログが、従来ハードコードされていた
/// 既定レイアウトと同一の anchor / position / size を再現することを担保するゴールデン
/// テスト (配置不変)。
#[test]
fn meta_derived_default_preset_matches_golden_layout() {
    use panel_workspace::{WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize};

    let app = test_app_with_dialogs(TestDialogs::default());
    let catalog = app.default_workspace_preset_catalog();
    assert_eq!(catalog.default_preset_id, "default-floating");
    let preset = catalog
        .presets
        .iter()
        .find(|preset| preset.id == "default-floating")
        .expect("default preset exists");
    let panels = &preset.ui_state.workspace_layout.panels;

    // 期待値: (id, anchor, x, y, width, height) — 旧 default_workspace_preset_catalog と同値。
    let golden: &[(&str, WorkspacePanelAnchor, usize, usize, usize, usize)] = &[
        ("builtin.workspace-layout", WorkspacePanelAnchor::TopLeft, 24, 72, 320, 280),
        ("builtin.tool-palette", WorkspacePanelAnchor::TopLeft, 24, 384, 300, 280),
        ("builtin.app-actions", WorkspacePanelAnchor::TopLeft, 356, 72, 320, 240),
        ("builtin.workspace-presets", WorkspacePanelAnchor::TopRight, 24, 616, 320, 180),
        ("builtin.layers", WorkspacePanelAnchor::TopRight, 24, 72, 320, 320),
        ("builtin.color-palette", WorkspacePanelAnchor::BottomLeft, 24, 24, 320, 320),
        ("builtin.tool-settings", WorkspacePanelAnchor::BottomRight, 24, 24, 320, 260),
        ("builtin.view-controls", WorkspacePanelAnchor::BottomRight, 376, 24, 320, 260),
        ("builtin.job-progress", WorkspacePanelAnchor::BottomLeft, 376, 24, 280, 180),
        ("builtin.snapshots", WorkspacePanelAnchor::TopRight, 24, 424, 280, 180),
    ];

    for (id, anchor, x, y, w, h) in golden {
        let panel = panels
            .iter()
            .find(|p| p.id == *id)
            .unwrap_or_else(|| panic!("{id} must be in default preset"));
        assert_eq!(panel.anchor, *anchor, "{id} anchor");
        assert_eq!(panel.position, Some(WorkspacePanelPosition { x: *x, y: *y }), "{id} position");
        assert_eq!(
            panel.size,
            Some(WorkspacePanelSize { width: *w, height: *h }),
            "{id} size"
        );
        assert!(panel.visible, "{id} visible");
    }

    // koma-list / text-flow は既定プリセットに含まれない (従来と同じ)。
    assert!(!panels.iter().any(|p| p.id == "builtin.koma-list"));
    assert!(!panels.iter().any(|p| p.id == "builtin.text-flow"));
}

#[test]
fn bootstrap_saved_project_can_be_loaded_again() {
    let project_path = unique_test_path("bootstrap-roundtrip-project");
    let mut source_app = test_app_with_dialogs(TestDialogs::default());
    source_app.document.work.title = "Roundtrip".to_string();
    assert!(source_app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_TO_PATH).with_value("path", project_path.to_string_lossy().to_string())));
    source_app.wait_for_pending_save_tasks();

    let loaded = load_project_from_path(&project_path).expect("saved project should load");
    assert_eq!(loaded.document.work.title, "Roundtrip");

    let _ = std::fs::remove_file(project_path);
}
