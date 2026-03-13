//! panel_dispatch の回帰テストをまとめる。

use std::path::PathBuf;

use app_core::Command;
use desktop_support::DesktopProfiler;

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::{DesktopApp, PanelDragState};

/// パネル 振り分け キーボード パス activates 保存 action が期待どおりに動作することを検証する。
#[test]
fn panel_dispatch_keyboard_path_activates_save_action() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(app.panel_presentation.focus_panel_node(
        &app.panel_runtime,
        "builtin.app-actions",
        "app.save"
    ));
    assert_eq!(
        app.activate_focused_panel_control(),
        Some(Command::SaveProject)
    );
    assert_eq!(app.io_state.pending_jobs.len(), 1);
}

/// パネル drag ソース advances for レイヤー 一覧 drag が期待どおりに動作することを検証する。
#[test]
fn panel_drag_source_advances_for_layer_list_drag() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    app.panel_interaction.active_panel_drag = Some(PanelDragState::Control {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        source_value: 2,
    });

    app.advance_panel_drag_source(&panel_api::PanelEvent::DragValue {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        from: 2,
        to: 1,
    });

    assert_eq!(
        app.panel_interaction
            .active_panel_drag
            .as_ref()
            .and_then(|drag| match drag {
                PanelDragState::Control { source_value, .. } => Some(*source_value),
                PanelDragState::Move { .. } => None,
            }),
        Some(1)
    );
}
