//! コマナビゲーション (`koma_nav.*`) service request のハンドラ。

use document_model::DocumentCommand;
use panel_runtime::{ServiceRequest, services::names};

use super::DesktopApp;

/// koma_nav service request を処理する。
pub(crate) fn handle_koma_navigation_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::KOMA_NAV_ADD => app.apply_document_command(&DocumentCommand::AddKoma),
        names::KOMA_NAV_REMOVE => app.apply_document_command(&DocumentCommand::RemoveActiveKoma),
        names::KOMA_NAV_SELECT => app.apply_document_command(&DocumentCommand::SelectKoma {
            index: request.u64("index")? as usize,
        }),
        names::KOMA_NAV_SELECT_NEXT => {
            app.apply_document_command(&DocumentCommand::SelectNextKoma)
        }
        names::KOMA_NAV_SELECT_PREVIOUS => {
            app.apply_document_command(&DocumentCommand::SelectPreviousKoma)
        }
        names::KOMA_NAV_FOCUS_ACTIVE => {
            app.apply_document_command(&DocumentCommand::FocusActiveKoma)
        }
        _ => return None,
    };
    Some(changed)
}
