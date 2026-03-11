//! command_router の経路分岐に関するテストをまとめる。

use std::path::PathBuf;

use app_core::{ColorRgba8, Command, ToolKind};

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::DesktopApp;

#[test]
fn document_command_route_updates_tool_state() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(app.execute_command(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    }));

    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

#[test]
fn document_command_route_updates_color_state() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(app.execute_command(Command::SetActiveColor {
        color: ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff),
    }));

    assert_eq!(
        app.document.active_color,
        ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)
    );
}

#[test]
fn io_command_route_can_open_new_document_form() {
    let mut app = test_app_with_dialogs(TestDialogs::default());

    assert!(app.execute_command(Command::NewDocument));
}
