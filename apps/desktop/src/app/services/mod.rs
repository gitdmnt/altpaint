//! host service request と補助的な状態同期処理を扱う。

mod export;
#[cfg(feature = "gpu")]
mod gpu_sync;
mod project_io;
mod snapshot;
mod text_render;
mod tool_catalog;
mod workspace_io;

use app_core::{Command, Document, HistoryEntry};
use desktop_support::DEFAULT_PROJECT_PATH;
use panel_api::{ServiceRequest, services::names};
use workspace_persistence::WorkspaceUiState;

use super::DesktopApp;

impl DesktopApp {
    /// 取得 ワークスペース ui 状態 を計算して返す。
    pub(super) fn capture_workspace_ui_state(&self) -> WorkspaceUiState {
        WorkspaceUiState::new(
            self.panel_presentation.workspace_layout(),
            self.panel_runtime.persistent_panel_configs(),
        )
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(crate) fn execute_service_request(&mut self, request: ServiceRequest) -> bool {
        if let Some(changed) = self.handle_project_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_workspace_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_tool_catalog_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_view_service_request(&request) {
            return changed;
        }
        if let Some(changed) = self.handle_panel_navigation_service_request(&request) {
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
                panel_id,
                layer_index,
                dirty,
                before,
                ..
            }) => {
                if let Some(page_dirty) = self.document.restore_panel_layer_region(
                    panel_id,
                    layer_index,
                    dirty.x,
                    dirty.y,
                    &before,
                ) {
                    self.refresh_canvas_frame_region(page_dirty);
                    self.append_canvas_dirty_rect(page_dirty);
                    // GPU パス: dirty 領域だけを GPU へ同期（全レイヤー転送は不要）
                    #[cfg(feature = "gpu")]
                    if let Some(pool) = self.gpu_canvas_pool.as_ref()
                        && let Some(region) =
                            self.document
                                .capture_panel_layer_region(panel_id, layer_index, page_dirty)
                    {
                        pool.upload_region(
                            &panel_id.0.to_string(),
                            layer_index,
                            page_dirty.x as u32,
                            page_dirty.y as u32,
                            page_dirty.width as u32,
                            page_dirty.height as u32,
                            &region.pixels,
                        );
                    }
                }
                self.sync_ui_from_document();
                true
            }
            #[cfg(feature = "gpu")]
            Some(HistoryEntry::GpuBitmapPatch {
                panel_id,
                layer_index,
                dirty,
                gpu_data,
            }) => {
                if let (Some(pool), Some(snap)) = (
                    self.gpu_canvas_pool.as_ref(),
                    (*gpu_data.0).downcast_ref::<project_io::GpuPatchSnapshot>(),
                ) {
                    pool.restore_region(
                        &panel_id.0.to_string(),
                        layer_index,
                        dirty.x as u32,
                        dirty.y as u32,
                        &snap.before,
                    );
                    self.append_canvas_dirty_rect(dirty);
                    self.recomposite_panel(panel_id, Some(dirty));
                }
                self.sync_ui_from_document();
                true
            }
            Some(HistoryEntry::BitmapOp(_)) => {
                // レガシーエントリは何もしない
                self.sync_ui_from_document();
                true
            }
            #[cfg(not(feature = "gpu"))]
            Some(HistoryEntry::GpuBitmapPatch { .. }) => {
                // GPU feature 無効時は GpuBitmapPatch は生成されないため到達しない
                false
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
                panel_id,
                layer_index,
                dirty,
                after,
                ..
            }) => {
                if let Some(page_dirty) = self.document.restore_panel_layer_region(
                    panel_id,
                    layer_index,
                    dirty.x,
                    dirty.y,
                    &after,
                ) {
                    self.refresh_canvas_frame_region(page_dirty);
                    self.append_canvas_dirty_rect(page_dirty);
                    #[cfg(feature = "gpu")]
                    if let Some(pool) = self.gpu_canvas_pool.as_ref()
                        && let Some(region) =
                            self.document
                                .capture_panel_layer_region(panel_id, layer_index, page_dirty)
                    {
                        pool.upload_region(
                            &panel_id.0.to_string(),
                            layer_index,
                            page_dirty.x as u32,
                            page_dirty.y as u32,
                            page_dirty.width as u32,
                            page_dirty.height as u32,
                            &region.pixels,
                        );
                    }
                }
                self.sync_ui_from_document();
                true
            }
            #[cfg(feature = "gpu")]
            Some(HistoryEntry::GpuBitmapPatch {
                panel_id,
                layer_index,
                dirty,
                gpu_data,
            }) => {
                if let (Some(pool), Some(snap)) = (
                    self.gpu_canvas_pool.as_ref(),
                    (*gpu_data.0).downcast_ref::<project_io::GpuPatchSnapshot>(),
                ) {
                    pool.restore_region(
                        &panel_id.0.to_string(),
                        layer_index,
                        dirty.x as u32,
                        dirty.y as u32,
                        &snap.after,
                    );
                    self.append_canvas_dirty_rect(dirty);
                    self.recomposite_panel(panel_id, Some(dirty));
                }
                self.sync_ui_from_document();
                true
            }
            Some(HistoryEntry::BitmapOp(_)) => {
                // レガシーエントリは何もしない
                self.sync_ui_from_document();
                true
            }
            #[cfg(not(feature = "gpu"))]
            Some(HistoryEntry::GpuBitmapPatch { .. }) => false,
            None => false,
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// 入力や種別に応じて処理を振り分ける。
    fn handle_panel_navigation_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::PANEL_NAV_ADD => self.execute_document_command(Command::AddPanel),
            names::PANEL_NAV_REMOVE => self.execute_document_command(Command::RemoveActivePanel),
            names::PANEL_NAV_SELECT => self.execute_document_command(Command::SelectPanel {
                index: request.u64("index")? as usize,
            }),
            names::PANEL_NAV_SELECT_NEXT => self.execute_document_command(Command::SelectNextPanel),
            names::PANEL_NAV_SELECT_PREVIOUS => {
                self.execute_document_command(Command::SelectPreviousPanel)
            }
            names::PANEL_NAV_FOCUS_ACTIVE => {
                self.execute_document_command(Command::FocusActivePanel)
            }
            _ => return None,
        };
        Some(changed)
    }

    /// 再読込 ツール カタログ into ドキュメント を計算して返す。
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

    /// ステータス テキスト 用の表示文字列を組み立てる。
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
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / page={} / panel={}/{} / pages={} / panels={} / hidden={}",
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
            self.document.active_panel_index() + 1,
            self.document.active_page_panel_count().max(1),
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.panels.len())
                .sum::<usize>(),
            hidden_panels,
        )
    }
}
