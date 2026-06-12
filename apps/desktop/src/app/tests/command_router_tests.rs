//! command_router の経路分岐に関するテストをまとめる。

use app_core::{ColorRgba8, DocumentCommand, SessionCommand, ToolKind};
use panel_runtime::{ServiceRequest, services::names};

use super::{TestDialogs, test_app_with_dialogs};

#[test]
fn session_command_route_updates_tool_state() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Eraser,
    }));

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

#[test]
fn session_command_route_updates_color_state() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.apply_session_command(&SessionCommand::SetActiveColor {
        color: ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff),
    }));

    assert_eq!(
        app.document.active_color,
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

#[test]
fn io_service_route_can_open_new_document_form() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_service_request(ServiceRequest::new(names::PROJECT_NEW_DOCUMENT)));
}
