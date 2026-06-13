//! host service request と補助的な状態同期処理を扱う。

mod export;
mod gpu_sync;
mod project_io;
mod snapshot;
mod text_render;
mod tool_catalog;
mod workspace_io;
mod workspace_layout;

use app_core::HistoryEntry;
use document_model::{Document, DocumentCommand};
use editor_state::SessionCommand;
use desktop_support::DEFAULT_PROJECT_FILE_NAME;
use panel_runtime::{ServiceRequest, services::names};
use app_core::WorkspaceUiState;

use super::DesktopApp;

impl DesktopApp {
    pub(super) fn capture_workspace_ui_state(&self) -> WorkspaceUiState {
        WorkspaceUiState::new(
            self.panel_workspace.workspace_layout(),
            self.panel_runtime.persistent_panel_configs(),
        )
    }

    pub(crate) fn execute_service_request(&mut self, request: ServiceRequest) -> bool {
        if let Some(changed) = self.handle_project_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_workspace_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_workspace_layout_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_tool_catalog_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_view_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_koma_navigation_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_history_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_snapshot_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_export_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_text_render_service_request(&request) {
            return changed;
        }
        false
    }

    /// history service request を処理する。
    fn handle_history_service_request(&mut self, request: &ServiceRequest) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::HISTORY_UNDO => self.execute_undo(),
            names::HISTORY_REDO => self.execute_redo(),
            _ => return None,
        };
        Some(changed)
    }

    /// undo を実行する。
    ///
    /// `BitmapPatch` の before ビットマップ領域を復元する。
    pub(crate) fn execute_undo(&mut self) -> bool {
        match self.history.undo() {
            Some(HistoryEntry::BitmapPatch {
                koma_id,
                layer_index,
                dirty,
                before,
                ..
            }) => {
                if let Some(page_dirty) = self.document.restore_koma_layer_region(
                    koma_id,
                    layer_index,
                    dirty.x,
                    dirty.y,
                    &before,
                ) {
                    self.append_canvas_dirty_rect(page_dirty);
                    // GPU パス: dirty 領域だけを GPU へ同期（全レイヤー転送は不要）
                    if let Some(pool) = self.layer_texture_store()
                        && let Some(region) =
                            self.document
                                .capture_koma_layer_region(koma_id, layer_index, page_dirty)
                    {
                        pool.upload_region(
                            &koma_id.0.to_string(),
                            layer_index,
                            page_dirty,
                            &region.pixels,
                        );
                    }
                }
                self.sync_ui_from_document();
                true
            }
            Some(HistoryEntry::GpuBitmapPatch {
                koma_id,
                layer_index,
                dirty,
                gpu_data,
            }) => {
                if let (Some(pool), Some(snap)) = (
                    self.layer_texture_store(),
                    (*gpu_data.0).downcast_ref::<project_io::GpuPatchSnapshot>(),
                ) {
                    pool.restore_region(
                        &koma_id.0.to_string(),
                        layer_index,
                        geometry::KomaLocalPoint::new(dirty.x, dirty.y),
                        &snap.before,
                    );
                    self.append_canvas_dirty_rect(dirty);
                    self.recomposite_koma(koma_id, Some(dirty));
                }
                self.sync_ui_from_document();
                true
            }
            None => false,
        }
    }

    /// redo を実行する。
    ///
    /// `BitmapPatch` の after ビットマップ領域を復元する。
    pub(crate) fn execute_redo(&mut self) -> bool {
        match self.history.redo() {
            Some(HistoryEntry::BitmapPatch {
                koma_id,
                layer_index,
                dirty,
                after,
                ..
            }) => {
                if let Some(page_dirty) = self.document.restore_koma_layer_region(
                    koma_id,
                    layer_index,
                    dirty.x,
                    dirty.y,
                    &after,
                ) {
                    self.append_canvas_dirty_rect(page_dirty);
                    if let Some(pool) = self.layer_texture_store()
                        && let Some(region) =
                            self.document
                                .capture_koma_layer_region(koma_id, layer_index, page_dirty)
                    {
                        pool.upload_region(
                            &koma_id.0.to_string(),
                            layer_index,
                            page_dirty,
                            &region.pixels,
                        );
                    }
                }
                self.sync_ui_from_document();
                true
            }
            Some(HistoryEntry::GpuBitmapPatch {
                koma_id,
                layer_index,
                dirty,
                gpu_data,
            }) => {
                if let (Some(pool), Some(snap)) = (
                    self.layer_texture_store(),
                    (*gpu_data.0).downcast_ref::<project_io::GpuPatchSnapshot>(),
                ) {
                    pool.restore_region(
                        &koma_id.0.to_string(),
                        layer_index,
                        geometry::KomaLocalPoint::new(dirty.x, dirty.y),
                        &snap.after,
                    );
                    self.append_canvas_dirty_rect(dirty);
                    self.recomposite_koma(koma_id, Some(dirty));
                }
                self.sync_ui_from_document();
                true
            }
            None => false,
        }
    }

    fn handle_view_service_request(&mut self, request: &ServiceRequest) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::VIEW_SET_ZOOM => self.apply_session_command(&SessionCommand::SetViewZoom {
                zoom: request.f64("zoom")? as f32,
            }),
            names::VIEW_SET_PAN => self.apply_session_command(&SessionCommand::SetViewPan {
                pan_x: request.f64("pan_x")? as f32,
                pan_y: request.f64("pan_y")? as f32,
            }),
            names::VIEW_SET_ROTATION => {
                self.apply_session_command(&SessionCommand::SetViewRotation {
                    rotation_degrees: request.f64("rotation_degrees")? as f32,
                })
            }
            names::VIEW_FLIP_HORIZONTAL => {
                self.apply_session_command(&SessionCommand::FlipViewHorizontally)
            }
            names::VIEW_FLIP_VERTICAL => {
                self.apply_session_command(&SessionCommand::FlipViewVertically)
            }
            names::VIEW_RESET => self.apply_session_command(&SessionCommand::ResetView),
            _ => return None,
        };
        Some(changed)
    }

    fn handle_koma_navigation_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::KOMA_NAV_ADD => self.apply_document_command(&DocumentCommand::AddKoma),
            names::KOMA_NAV_REMOVE => {
                self.apply_document_command(&DocumentCommand::RemoveActiveKoma)
            }
            names::KOMA_NAV_SELECT => self.apply_document_command(&DocumentCommand::SelectKoma {
                index: request.u64("index")? as usize,
            }),
            names::KOMA_NAV_SELECT_NEXT => {
                self.apply_document_command(&DocumentCommand::SelectNextKoma)
            }
            names::KOMA_NAV_SELECT_PREVIOUS => {
                self.apply_document_command(&DocumentCommand::SelectPreviousKoma)
            }
            names::KOMA_NAV_FOCUS_ACTIVE => {
                self.apply_document_command(&DocumentCommand::FocusActiveKoma)
            }
            _ => return None,
        };
        Some(changed)
    }

    pub(crate) fn reload_tool_catalog_into_document(document: &mut Document) -> bool {
        let (tools, diagnostics) =
            storage::load_tool_directory(desktop_support::default_tool_dir());
        for diagnostic in diagnostics {
            eprintln!("tool catalog load warning: {diagnostic}");
        }
        // tools/ が無い・空の場合は desktop 既定カタログ (provider_plugin_id 付き) を
        // フォールバックとして注入する。editor-state の既定カタログは provider を持たない。
        let tools = if tools.is_empty() {
            crate::app::default_tool_catalog::desktop_default_tool_catalog()
        } else {
            tools
        };
        document.session.replace_tool_catalog(tools);
        true
    }

    /// 9E-4: HtmlPanelView ステータスバー用のスナップショットを組み立てる。
    /// ツール名・ズーム % ・status text を集約して返す。
    pub(crate) fn build_status_snapshot(&self) -> crate::present_quads::status_panel::StatusSnapshot {
        let tool_name = self.document.session.active_tool().display_label();
        let zoom_percent =
            (self.document.session.view_transform.zoom * 100.0).round().clamp(1.0, 100_000.0) as u32;
        let status_text = self.status_text();
        crate::present_quads::status_panel::StatusSnapshot::new(tool_name, zoom_percent, status_text)
    }

    pub(crate) fn status_text(&self) -> String {
        let file_name = self
            .io_state
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_FILE_NAME);
        let hidden_panels = self
            .panel_workspace
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / page={} / koma={}/{} / pages={} / komas={} / hidden={}",
            file_name,
            self.document.session.active_tool(),
            self.document
                .session
                .active_pen_preset()
                .map(|preset| preset.name.as_str())
                .unwrap_or("Round Pen"),
            self.document.session.active_pen_size,
            self.document.session.active_color.hex_rgb(),
            self.document.session.view_transform.zoom,
            self.document.active_page_index() + 1,
            self.document.active_koma_index() + 1,
            self.document.active_page_koma_count().max(1),
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.komas.len())
                .sum::<usize>(),
            hidden_panels,
        )
    }
}
