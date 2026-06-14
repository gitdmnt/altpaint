//! ビュー操作 (`view.*`) service request のハンドラ。

use editor_state::SessionCommand;
use panel_runtime::{ServiceRequest, services::names};

use crate::app::DesktopApp;

/// view service request を処理する。
pub(crate) fn handle_view_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::VIEW_SET_ZOOM => app.apply_session_command(&SessionCommand::SetViewZoom {
            zoom: request.f64("zoom")? as f32,
        }),
        names::VIEW_SET_PAN => app.apply_session_command(&SessionCommand::SetViewPan {
            pan_x: request.f64("pan_x")? as f32,
            pan_y: request.f64("pan_y")? as f32,
        }),
        names::VIEW_SET_ROTATION => app.apply_session_command(&SessionCommand::SetViewRotation {
            rotation_degrees: request.f64("rotation_degrees")? as f32,
        }),
        names::VIEW_FLIP_HORIZONTAL => {
            app.apply_session_command(&SessionCommand::FlipViewHorizontally)
        }
        names::VIEW_FLIP_VERTICAL => app.apply_session_command(&SessionCommand::FlipViewVertically),
        names::VIEW_RESET => app.apply_session_command(&SessionCommand::ResetView),
        _ => return None,
    };
    Some(changed)
}
