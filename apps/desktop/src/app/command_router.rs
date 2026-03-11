//! `Command` の分類と `DesktopApp` への適用経路を整理する。

use std::path::PathBuf;

use app_core::Command;

use super::DesktopApp;

const TOOL_PANEL_IDS: &[&str] = &["builtin.pen-settings", "builtin.tool-palette"];
const COLOR_PANEL_IDS: &[&str] = &["builtin.color-palette"];
impl DesktopApp {
    /// アプリケーション全体で扱うコマンドを解釈して適用する。
    pub(crate) fn execute_command(&mut self, command: Command) -> bool {
        self.poll_background_tasks();
        match command {
            Command::NewDocument => self.activate_panel_control("builtin.app-actions", "app.new"),
            Command::SaveProject => self.save_project_to_current_path(),
            Command::SaveProjectAs => self.save_project_as(),
            Command::SaveProjectToPath { path } => self.save_project_to_path(PathBuf::from(path)),
            Command::LoadProject => self.open_project(),
            Command::LoadProjectFromPath { path } => self.load_project(PathBuf::from(path)),
            Command::ReloadWorkspacePresets => self.reload_workspace_presets(),
            Command::ApplyWorkspacePreset { preset_id } => self.apply_workspace_preset(&preset_id),
            Command::SaveWorkspacePreset { preset_id, label } => {
                self.save_workspace_preset(&preset_id, &label)
            }
            Command::ExportWorkspacePreset { preset_id, label } => {
                self.export_workspace_preset(&preset_id, &label)
            }
            Command::ExportWorkspacePresetToPath {
                preset_id,
                label,
                path,
            } => self.export_workspace_preset_to_path(&preset_id, &label, PathBuf::from(path)),
            Command::ReloadPenPresets => self.reload_pen_presets(),
            Command::ImportPenPresets => self.import_pen_presets(),
            Command::ImportPenPresetsFromPath { path } => {
                self.import_pen_presets_from_path(PathBuf::from(path))
            }
            other => self.execute_document_command(other),
        }
    }

    /// ドキュメント変更系コマンドを適用し、dirty 状態を更新する。
    pub(super) fn execute_document_command(&mut self, command: Command) -> bool {
        let previous_transform = self.document.view_transform;
        let _dirty = self.document.apply_command(&command);
        match command {
            Command::SetActiveTool { .. }
            | Command::SelectTool { .. }
            | Command::SelectNextPenPreset
            | Command::SelectPreviousPenPreset => {
                self.sync_ui_from_document_panels(TOOL_PANEL_IDS);
                self.mark_status_dirty();
                true
            }
            Command::SetActivePenSize { .. }
            | Command::SetActivePenPressureEnabled { .. }
            | Command::SetActivePenAntialias { .. }
            | Command::SetActivePenStabilization { .. } => {
                self.sync_ui_from_document_panels(TOOL_PANEL_IDS);
                self.mark_status_dirty();
                true
            }
            Command::SetActiveColor { .. } => {
                self.sync_ui_from_document_panels(COLOR_PANEL_IDS);
                self.mark_status_dirty();
                true
            }
            Command::SetViewZoom { .. } | Command::ResetView => {
                self.defer_view_panel_sync();
                self.mark_canvas_transform_dirty(previous_transform);
                self.defer_status_refresh();
                true
            }
            Command::RotateView { .. }
            | Command::SetViewRotation { .. }
            | Command::FlipViewHorizontally
            | Command::FlipViewVertically => {
                self.defer_view_panel_sync();
                self.mark_canvas_transform_dirty(previous_transform);
                true
            }
            Command::PanView { .. } | Command::SetViewPan { .. } => {
                self.defer_view_panel_sync();
                self.mark_canvas_transform_dirty(previous_transform)
            }
            Command::AddRasterLayer
            | Command::RemoveActiveLayer
            | Command::SelectLayer { .. }
            | Command::RenameActiveLayer { .. }
            | Command::MoveLayer { .. }
            | Command::SelectNextLayer
            | Command::CycleActiveLayerBlendMode
            | Command::SetActiveLayerBlendMode { .. }
            | Command::ToggleActiveLayerVisibility
            | Command::ToggleActiveLayerMask => {
                self.refresh_canvas_frame();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::AddPanel
            | Command::CreatePanel { .. }
            | Command::RemoveActivePanel
            | Command::SelectPanel { .. }
            | Command::SelectNextPanel
            | Command::SelectPreviousPanel
            | Command::FocusActivePanel => {
                self.refresh_canvas_frame();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::NewDocumentSized { .. } => {
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = Self::reload_pen_presets_into_document(&mut self.document);
                self.reset_active_interactions();
                self.refresh_canvas_frame();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::Noop
            | Command::NewDocument
            | Command::SaveProject
            | Command::SaveProjectAs
            | Command::SaveProjectToPath { .. }
            | Command::LoadProject
            | Command::LoadProjectFromPath { .. }
            | Command::ReloadWorkspacePresets
            | Command::ApplyWorkspacePreset { .. }
            | Command::SaveWorkspacePreset { .. }
            | Command::ExportWorkspacePreset { .. }
            | Command::ExportWorkspacePresetToPath { .. }
            | Command::ReloadPenPresets
            | Command::ImportPenPresets
            | Command::ImportPenPresetsFromPath { .. } => false,
        }
    }
}
