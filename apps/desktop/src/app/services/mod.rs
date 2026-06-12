//! host service request と補助的な状態同期処理を扱う。

mod export;
mod gpu_sync;
mod project_io;
mod snapshot;
mod text_render;
mod tool_catalog;
mod workspace_io;
mod workspace_layout;

use app_core::{Command, Document, HistoryEntry};
use desktop_support::DEFAULT_PROJECT_PATH;
use panel_runtime::{ServiceRequest, services::names};
use app_core::WorkspaceUiState;

use super::DesktopApp;

impl DesktopApp {
    pub(super) fn capture_workspace_ui_state(&self) -> WorkspaceUiState {
        WorkspaceUiState::new(
            self.panel_presentation.workspace_layout(),
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
                    if let Some(pool) = self.gpu_canvas_pool()
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
                    self.gpu_canvas_pool(),
                    (*gpu_data.0).downcast_ref::<project_io::GpuPatchSnapshot>(),
                ) {
                    pool.restore_region(
                        &koma_id.0.to_string(),
                        layer_index,
                        app_core::KomaLocalPoint::new(dirty.x, dirty.y),
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
                    if let Some(pool) = self.gpu_canvas_pool()
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
                    self.gpu_canvas_pool(),
                    (*gpu_data.0).downcast_ref::<project_io::GpuPatchSnapshot>(),
                ) {
                    pool.restore_region(
                        &koma_id.0.to_string(),
                        layer_index,
                        app_core::KomaLocalPoint::new(dirty.x, dirty.y),
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
            names::VIEW_SET_ZOOM => self.execute_document_command(Command::SetViewZoom {
                zoom: request.f64("zoom")? as f32,
            }),
            names::VIEW_SET_PAN => self.execute_document_command(Command::SetViewPan {
                pan_x: request.f64("pan_x")? as f32,
                pan_y: request.f64("pan_y")? as f32,
            }),
            names::VIEW_SET_ROTATION => self.execute_document_command(Command::SetViewRotation {
                rotation_degrees: request.f64("rotation_degrees")? as f32,
            }),
            names::VIEW_FLIP_HORIZONTAL => {
                self.execute_document_command(Command::FlipViewHorizontally)
            }
            names::VIEW_FLIP_VERTICAL => self.execute_document_command(Command::FlipViewVertically),
            names::VIEW_RESET => self.execute_document_command(Command::ResetView),
            _ => return None,
        };
        Some(changed)
    }

    fn handle_koma_navigation_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::KOMA_NAV_ADD => self.execute_document_command(Command::AddKoma),
            names::KOMA_NAV_REMOVE => self.execute_document_command(Command::RemoveActiveKoma),
            names::KOMA_NAV_SELECT => self.execute_document_command(Command::SelectKoma {
                index: request.u64("index")? as usize,
            }),
            names::KOMA_NAV_SELECT_NEXT => self.execute_document_command(Command::SelectNextKoma),
            names::KOMA_NAV_SELECT_PREVIOUS => {
                self.execute_document_command(Command::SelectPreviousKoma)
            }
            names::KOMA_NAV_FOCUS_ACTIVE => {
                self.execute_document_command(Command::FocusActiveKoma)
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
        if tools.is_empty() {
            return false;
        }
        document.replace_tool_catalog(tools);
        true
    }

    /// 9E-4: HtmlPanelEngine ステータスバー用のスナップショットを組み立てる。
    /// ツール名・ズーム % ・status text を集約して返す。
    pub(crate) fn build_status_snapshot(&self) -> crate::frame::status_panel::StatusSnapshot {
        let tool_name = match self.document.active_tool {
            app_core::ToolKind::Pen => "Pen",
            app_core::ToolKind::Eraser => "Eraser",
            app_core::ToolKind::Bucket => "Bucket",
            app_core::ToolKind::LassoBucket => "LassoBucket",
            app_core::ToolKind::KomaRect => "KomaRect",
        };
        let zoom_percent =
            (self.document.view_transform.zoom * 100.0).round().clamp(1.0, 100_000.0) as u32;
        let status_text = self.status_text();
        crate::frame::status_panel::StatusSnapshot::new(tool_name, zoom_percent, status_text)
    }

    pub(crate) fn status_text(&self) -> String {
        let file_name = self
            .io_state
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_PATH);
        let hidden_panels = self
            .panel_presentation
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / page={} / koma={}/{} / pages={} / komas={} / hidden={}",
            file_name,
            self.document.active_tool,
            self.document
                .active_pen_preset()
                .map(|preset| preset.name.as_str())
                .unwrap_or("Round Pen"),
            self.document.active_pen_size,
            self.document.active_color.hex_rgb(),
            self.document.view_transform.zoom,
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
