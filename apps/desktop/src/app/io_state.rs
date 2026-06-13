//! プロジェクトパス・ダイアログ・セッション保存先など I/O 系状態をまとめる。

use std::path::PathBuf;

use desktop_support::{DesktopDialogs, DesktopSessionState, save_session_state};

use super::DesktopApp;

/// プロジェクト I/O とセッション永続化に関わる状態を保持する。
pub(crate) struct DesktopIoState {
    pub(crate) project_path: PathBuf,
    pub(crate) session_path: PathBuf,
    pub(crate) workspace_preset_path: PathBuf,
    pub(crate) dialogs: Box<dyn DesktopDialogs>,
}

impl DesktopIoState {
    pub(crate) fn new(
        project_path: PathBuf,
        session_path: PathBuf,
        workspace_preset_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
    ) -> Self {
        Self {
            project_path,
            session_path,
            workspace_preset_path,
            dialogs,
        }
    }
}

impl DesktopApp {
    pub(super) fn session_state(&self) -> DesktopSessionState {
        DesktopSessionState {
            last_project_path: Some(self.io_state.project_path.clone()),
            ui_state: panel_workspace::WorkspaceUiState::new(
                self.panel_workspace.workspace_layout(),
                self.panel_runtime.persistent_panel_configs(),
            ),
        }
    }

    pub(super) fn persist_session_state(&self) {
        if let Err(error) = save_session_state(&self.io_state.session_path, &self.session_state()) {
            eprintln!("failed to persist desktop session: {error}");
        }
    }
}
