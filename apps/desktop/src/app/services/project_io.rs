use std::path::PathBuf;

use app_core::{Command, PaintInput};
use desktop_support::normalize_project_path;
use panel_api::{ServiceRequest, services::names};
use storage::load_project_from_path;

use super::DesktopApp;

impl DesktopApp {
    pub(super) fn handle_project_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::PROJECT_NEW_DOCUMENT => self.execute_command(Command::NewDocument),
            names::PROJECT_NEW_DOCUMENT_SIZED => {
                self.execute_document_command(Command::NewDocumentSized {
                    width: request.u64("width")? as usize,
                    height: request.u64("height")? as usize,
                })
            }
            names::PROJECT_SAVE_CURRENT => self.save_project_to_current_path(),
            names::PROJECT_SAVE_AS => self.save_project_as(),
            names::PROJECT_SAVE_TO_PATH => {
                self.save_project_to_path(PathBuf::from(request.string("path")?))
            }
            names::PROJECT_LOAD_DIALOG => self.open_project(),
            names::PROJECT_LOAD_FROM_PATH => {
                self.load_project(PathBuf::from(request.string("path")?))
            }
            _ => return None,
        };
        Some(changed)
    }

    /// キャンバス入力を描画プラグインへ渡してビットマップ差分として適用する。
    pub(crate) fn execute_paint_input(&mut self, input: PaintInput) -> bool {
        let edits = self
            .paint_runtime
            .execute_paint_input(&self.document, &input);
        self.apply_bitmap_edits(edits)
    }

    /// 現在のプロジェクトパスへ保存を行う。
    pub(super) fn save_project_to_current_path(&mut self) -> bool {
        self.enqueue_save_project(self.io_state.project_path.clone())
    }

    /// 保存先を選んでプロジェクトを保存する。
    pub(super) fn save_project_as(&mut self) -> bool {
        let Some(path) = self
            .io_state
            .dialogs
            .pick_save_project_path(&self.io_state.project_path)
        else {
            return false;
        };
        self.save_project_to_path(path)
    }

    /// 指定パスへプロジェクトを保存し、状態上の現在パスも更新する。
    pub(super) fn save_project_to_path(&mut self, path: PathBuf) -> bool {
        self.io_state.project_path = normalize_project_path(path);
        self.mark_status_dirty();
        self.persist_session_state();
        self.save_project_to_current_path()
    }

    /// 開く対象を選んでプロジェクトを読み込む。
    pub(super) fn open_project(&mut self) -> bool {
        let Some(path) = self
            .io_state
            .dialogs
            .pick_open_project_path(&self.io_state.project_path)
        else {
            return false;
        };
        self.load_project(path)
    }

    /// 指定パスのプロジェクトを読み込み、UI 状態も復元する。
    pub(super) fn load_project(&mut self, path: PathBuf) -> bool {
        let path = normalize_project_path(path);
        match load_project_from_path(&path) {
            Ok(project) => {
                self.io_state.project_path = path;
                self.document = project.document;
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = self.reload_pen_presets();
                self.panel_presentation
                    .replace_workspace_layout(project.ui_state.workspace_layout);
                self.panel_runtime
                    .replace_persistent_panel_configs(project.ui_state.plugin_configs);
                self.panel_presentation
                    .reconcile_runtime_panels(&self.panel_runtime);
                self.refresh_new_document_templates();
                self.refresh_workspace_presets();
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                self.persist_session_state();
                true
            }
            Err(error) => {
                let message = format!("failed to load project: {error}");
                eprintln!("{message}");
                self.io_state.dialogs.show_error("Open failed", &message);
                false
            }
        }
    }
}
