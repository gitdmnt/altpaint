//! command_router の経路分岐に関するテストをまとめる。

use document_model::DocumentCommand;
use editor_state::{ColorRgba8, SessionCommand, ToolKind};

use super::{TestDialogs, test_app_with_dialogs};

#[test]
fn session_command_route_updates_tool_state() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Eraser,
    }));

    assert_eq!(app.document.session.active_tool(), ToolKind::Eraser);
}

#[test]
fn session_command_route_updates_color_state() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.apply_session_command(&SessionCommand::SetActiveColor {
        color: ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff),
    }));

    assert_eq!(
        app.document.session.active_color,
        ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)
    );
}

#[test]
fn document_command_route_creates_koma() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let before = app.document.active_page_koma_count();

    assert!(app.apply_document_command(&DocumentCommand::AddKoma));

    assert_eq!(app.document.active_page_koma_count(), before + 1);
}

/// BL-063: 新規ドキュメントフォームは app-actions パネルの直接起動で開く。
#[test]
fn new_document_form_opens_via_panel_activation() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = frame_profiler::FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);

    assert!(app.activate_panel_control("builtin.app-actions", "app.new"));
}
