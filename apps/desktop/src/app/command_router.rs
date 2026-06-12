//! `DocumentCommand` / `SessionCommand` の `DesktopApp` への適用経路を整理する。
//!
//! I/O を伴う操作 (保存・読込・preset 入出力・undo/redo・新規作成) は
//! `ServiceRequest` 経路 (`execute_service_request`) に一本化されており、
//! ここではドキュメント変異とエディタセッション変更のみを扱う。

use app_core::{DocumentCommand, PageDirtyRect, SessionCommand};

use super::DesktopApp;

const TOOL_PANEL_IDS: &[&str] = &["builtin.tool-settings", "builtin.tool-palette"];
const COLOR_PANEL_IDS: &[&str] = &["builtin.color-palette"];

impl DesktopApp {
    /// 純粋なドキュメント変異コマンドを適用し、関連 UI を同期する。
    pub(crate) fn apply_document_command(&mut self, command: &DocumentCommand) -> bool {
        self.document.apply(command);
        match command {
            DocumentCommand::SetActiveLayerBlendMode { .. }
            | DocumentCommand::ToggleActiveLayerVisibility => {
                let koma_info = self.document.active_koma().map(|p| {
                    (
                        p.id,
                        PageDirtyRect::new(p.bounds.x, p.bounds.y, p.bounds.width, p.bounds.height),
                        PageDirtyRect::new(0, 0, p.composite_cache.width, p.composite_cache.height),
                    )
                });
                if let Some((_koma_id, page_dirty, _local_dirty)) = koma_info {
                    self.append_canvas_dirty_rect(page_dirty);
                } else {
                    self.refresh_cpu_canvas_snapshot();
                    self.rebuild_present_frame();
                }
                if let Some((koma_id, _page_dirty, local_dirty)) = koma_info {
                    self.recomposite_koma(koma_id, Some(local_dirty));
                }
                self.sync_ui_from_document();
                self.mark_status_dirty();
                true
            }
            DocumentCommand::AddRasterLayer
            | DocumentCommand::RemoveActiveLayer
            | DocumentCommand::SelectLayer { .. }
            | DocumentCommand::RenameActiveLayer { .. }
            | DocumentCommand::MoveLayer { .. }
            | DocumentCommand::SelectNextLayer
            | DocumentCommand::CycleActiveLayerBlendMode => {
                self.invalidate_document_structure();
                true
            }
            DocumentCommand::AddKoma
            | DocumentCommand::CreateKoma { .. }
            | DocumentCommand::RemoveActiveKoma
            | DocumentCommand::SelectKoma { .. }
            | DocumentCommand::SelectNextKoma
            | DocumentCommand::SelectPreviousKoma
            | DocumentCommand::FocusActiveKoma => {
                self.invalidate_document_structure();
                true
            }
            DocumentCommand::NewDocumentSized { .. } => {
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = Self::reload_pen_presets_into_document(&mut self.document);
                self.reset_active_interactions();
                self.invalidate_document_structure();
                true
            }
            DocumentCommand::Noop => false,
        }
    }

    /// エディタセッションコマンド (ツール/色/ペン/ビュー) を適用し、関連 UI を同期する。
    pub(crate) fn apply_session_command(&mut self, command: &SessionCommand) -> bool {
        let previous_transform = self.document.view_transform;
        self.document.apply_session_command(command);
        match command {
            SessionCommand::SetActiveTool { .. }
            | SessionCommand::SelectTool { .. }
            | SessionCommand::SelectChildTool { .. }
            | SessionCommand::SelectNextPenPreset
            | SessionCommand::SelectPreviousPenPreset => {
                self.sync_ui_from_document_panels(TOOL_PANEL_IDS);
                self.mark_status_dirty();
                true
            }
            SessionCommand::SetActivePenSize { .. }
            | SessionCommand::SetActivePenPressureEnabled { .. }
            | SessionCommand::SetActivePenAntialias { .. }
            | SessionCommand::SetActivePenStabilization { .. } => {
                self.sync_ui_from_document_panels(TOOL_PANEL_IDS);
                self.mark_status_dirty();
                true
            }
            SessionCommand::SetActiveColor { .. } => {
                self.sync_ui_from_document_panels(COLOR_PANEL_IDS);
                self.mark_status_dirty();
                true
            }
            SessionCommand::SetViewZoom { .. }
            | SessionCommand::ZoomViewBy { .. }
            | SessionCommand::ResetView => {
                self.defer_view_panel_sync();
                self.mark_canvas_transform_dirty(previous_transform);
                self.defer_status_refresh();
                true
            }
            SessionCommand::RotateView { .. }
            | SessionCommand::SetViewRotation { .. }
            | SessionCommand::FlipViewHorizontally
            | SessionCommand::FlipViewVertically => {
                self.defer_view_panel_sync();
                self.mark_canvas_transform_dirty(previous_transform);
                true
            }
            SessionCommand::PanView { .. }
            | SessionCommand::PanViewByLines { .. }
            | SessionCommand::SetViewPan { .. } => {
                self.defer_view_panel_sync();
                self.mark_canvas_transform_dirty(previous_transform)
            }
        }
    }
}
