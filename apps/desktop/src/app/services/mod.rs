//! host service request と補助的な状態同期処理を扱う。

mod export;
mod project_io;
mod tool_catalog;
mod workspace_io;

use app_core::{CanvasDirtyRect, Command, Document, HistoryEntry};
use canvas::CanvasRuntime;
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
        if let Some(changed) = self.handle_export_service_request(&request) {
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
    /// 最後の描画操作を取り消し、それ以前の操作を replay してレイヤーを再構築する。
    pub(crate) fn execute_undo(&mut self) -> bool {
        let Some(HistoryEntry::BitmapOp(undone_record)) = self.history.undo() else {
            return false;
        };
        let Some((page_idx, panel_idx)) = self
            .document
            .find_panel_location(undone_record.panel_id)
        else {
            return false;
        };
        // 対象レイヤーを透明にリセットする
        self.document
            .reset_panel_layer_to_transparent(undone_record.panel_id, undone_record.layer_index);

        // past に残る全 BitmapOp を順番に replay する
        let past_records: Vec<_> = self
            .history
            .past_entries()
            .iter()
            .filter_map(|entry| match entry {
                HistoryEntry::BitmapOp(r)
                    if r.panel_id == undone_record.panel_id
                        && r.layer_index == undone_record.layer_index =>
                {
                    Some(r.clone())
                }
                _ => None,
            })
            .collect();

        let runtime = CanvasRuntime::default();
        for record in &past_records {
            let edits =
                runtime.replay_paint_record(&self.document, page_idx, panel_idx, record);
            self.document.apply_bitmap_edits_to_panel_layer(
                record.panel_id,
                record.layer_index,
                &edits,
            );
        }

        // 全面を dirty にして再描画させる
        let Some(panel) = self
            .document
            .work
            .pages
            .get(page_idx)
            .and_then(|p| p.panels.get(panel_idx))
        else {
            return true;
        };
        let dirty = CanvasDirtyRect {
            x: 0,
            y: 0,
            width: panel.bitmap.width,
            height: panel.bitmap.height,
        };
        self.refresh_canvas_frame_region(dirty);
        self.append_canvas_dirty_rect(dirty);
        true
    }

    /// redo を実行する。
    ///
    /// undo で取り消した操作を再適用する。
    pub(crate) fn execute_redo(&mut self) -> bool {
        let Some(HistoryEntry::BitmapOp(redo_record)) = self.history.redo() else {
            return false;
        };
        let Some((page_idx, panel_idx)) = self
            .document
            .find_panel_location(redo_record.panel_id)
        else {
            return false;
        };

        let runtime = CanvasRuntime::default();
        let edits =
            runtime.replay_paint_record(&self.document, page_idx, panel_idx, &redo_record);
        if let Some(dirty) = self.document.apply_bitmap_edits_to_panel_layer(
            redo_record.panel_id,
            redo_record.layer_index,
            &edits,
        ) {
            self.refresh_canvas_frame_region(dirty);
            self.append_canvas_dirty_rect(dirty);
        }
        true
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
