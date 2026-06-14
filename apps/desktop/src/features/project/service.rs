//! プロジェクト I/O (`project.*`) service request のハンドラと save/load 実装。
//!
//! D9: 旧 `app/services/project_io.rs` のうち I/O 部 (save/load、~2 割) をここへ
//! 分離した。ペイント実行+履歴部 (~8 割) は features/paint へ。

use std::path::PathBuf;

use document_model::DocumentCommand;
use panel_runtime::{ServiceRequest, services::names};
use project_store::load_project_from_path;

use crate::app::DesktopApp;
use crate::platform::normalize_project_path;

/// project service request を処理する。
pub(crate) fn handle_project_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::PROJECT_NEW_DOCUMENT_SIZED => {
            app.apply_document_command(&DocumentCommand::NewDocumentSized {
                width: request.u64("width")? as usize,
                height: request.u64("height")? as usize,
            })
        }
        names::PROJECT_SAVE_CURRENT => app.save_project_to_current_path(),
        names::PROJECT_SAVE_AS => app.save_project_as(),
        names::PROJECT_SAVE_TO_PATH => {
            app.save_project_to_path(PathBuf::from(request.string("path")?))
        }
        names::PROJECT_LOAD_DIALOG => app.open_project(),
        names::PROJECT_LOAD_FROM_PATH => {
            app.load_project(PathBuf::from(request.string("path")?))
        }
        _ => return None,
    };
    Some(changed)
}

impl DesktopApp {
    pub(crate) fn save_project_to_current_path(&mut self) -> bool {
        self.enqueue_save_project(self.paths.project_path.clone())
    }

    pub(crate) fn save_project_as(&mut self) -> bool {
        let Some(path) = self
            .dialogs
            .pick_save_project_path(&self.paths.project_path)
        else {
            return false;
        };
        self.save_project_to_path(path)
    }

    pub(crate) fn save_project_to_path(&mut self, path: PathBuf) -> bool {
        self.paths.project_path = normalize_project_path(path);
        self.mark_status_dirty();
        self.persist_session_state();
        self.save_project_to_current_path()
    }

    pub(crate) fn open_project(&mut self) -> bool {
        let Some(path) = self
            .dialogs
            .pick_open_project_path(&self.paths.project_path)
        else {
            return false;
        };
        self.load_project(path)
    }

    pub(crate) fn load_project(&mut self, path: PathBuf) -> bool {
        let path = normalize_project_path(path);
        match load_project_from_path(&path) {
            Ok(project) => {
                self.paths.project_path = path;
                // BL-079: project ファイルは作品コンテンツのみを保持する。読込時は
                // 現在のエディタセッション (ツール/色/ペン/ビュー) を温存し、作品
                // データだけを差し替える。
                let mut loaded = project.document;
                loaded.session = std::mem::take(&mut self.document.session);
                self.document = loaded;
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = self.reload_pen_presets();
                self.panel_workspace
                    .replace_workspace_layout(project.ui_state.workspace_layout);
                self.panel_runtime
                    .replace_persistent_panel_configs(project.ui_state.panel_configs);
                self.panel_workspace
                    .reconcile_panels(self.panel_runtime.panel_ids());
                self.refresh_new_document_size_presets();
                self.refresh_workspace_presets();
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                self.persist_session_state();
                self.sync_all_layers_to_gpu();
                true
            }
            Err(error) => {
                let message = format!("failed to load project: {error}");
                eprintln!("{message}");
                self.dialogs.show_error("Open failed", &message);
                false
            }
        }
    }
}
