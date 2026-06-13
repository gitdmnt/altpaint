//! プロジェクト/セッション/ワークスペースプリセットの保存先パス状態 (D12)。
//!
//! 旧 `DesktopIoState` からパス状態を分離した型。ダイアログポート (`dialogs`) は
//! `DesktopApp` 直下のフィールドへ分離した (パス状態と依存ポートの混載解消)。

use std::path::PathBuf;

use crate::features::project::{DesktopSessionState, save_session_state};

use super::DesktopApp;

/// プロジェクト I/O とセッション永続化に関わる保存先パスを保持する。
pub(crate) struct ProjectPaths {
    pub(crate) project_path: PathBuf,
    pub(crate) session_path: PathBuf,
    pub(crate) workspace_preset_path: PathBuf,
}

impl ProjectPaths {
    pub(crate) fn new(
        project_path: PathBuf,
        session_path: PathBuf,
        workspace_preset_path: PathBuf,
    ) -> Self {
        Self {
            project_path,
            session_path,
            workspace_preset_path,
        }
    }
}

impl DesktopApp {
    pub(crate) fn session_state(&self) -> DesktopSessionState {
        DesktopSessionState {
            last_project_path: Some(self.paths.project_path.clone()),
            ui_state: panel_workspace::WorkspaceUiState::new(
                self.panel_workspace.workspace_layout(),
                self.panel_runtime.persistent_panel_configs(),
            ),
            // BL-079: エディタセッション (ツール/色/ペン/ビュー) は project ファイル
            // ではなく session 永続化で保持する。
            editor_session: self.document.session.clone(),
        }
    }

    pub(crate) fn persist_session_state(&self) {
        if let Err(error) = save_session_state(&self.paths.session_path, &self.session_state()) {
            eprintln!("failed to persist desktop session: {error}");
        }
    }
}
