//! `Command` の分類と `DesktopApp` への適用経路を整理する。

use app_core::{CanvasDirtyRect, Command};
use panel_api::{ServiceRequest, services::names};

use super::DesktopApp;

impl DesktopApp {
    /// アクティブペンのペン先テクスチャを GPU キャッシュへアップロードする。
    ///
    /// `gpu_pen_tip_cache` が None の場合（GPU 未初期化時）は何もしない。
    pub(super) fn upload_active_pen_tip_to_gpu_cache(&mut self) {
        let Some(cache) = &mut self.gpu_pen_tip_cache else {
            return;
        };
        if let Some(pen) = self.document.active_pen_preset() {
            let preset_id = pen.id.clone();
            cache.upload_from_preset(&preset_id, pen);
        }
    }
}

const TOOL_PANEL_IDS: &[&str] = &["builtin.pen-settings", "builtin.tool-palette"];
const COLOR_PANEL_IDS: &[&str] = &["builtin.color-palette"];
impl DesktopApp {
    /// 入力や種別に応じて処理を振り分ける。
    pub(crate) fn execute_command(&mut self, command: Command) -> bool {
        self.poll_background_tasks();
        match command {
            Command::NewDocument => self.activate_panel_control("builtin.app-actions", "app.new"),
            Command::SaveProject => {
                self.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_CURRENT))
            }
            Command::SaveProjectAs => {
                self.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_AS))
            }
            Command::SaveProjectToPath { path } => self.execute_service_request(
                ServiceRequest::new(names::PROJECT_SAVE_TO_PATH).with_value("path", path),
            ),
            Command::LoadProject => {
                self.execute_service_request(ServiceRequest::new(names::PROJECT_LOAD_DIALOG))
            }
            Command::LoadProjectFromPath { path } => self.execute_service_request(
                ServiceRequest::new(names::PROJECT_LOAD_FROM_PATH).with_value("path", path),
            ),
            Command::ReloadWorkspacePresets => {
                self.execute_service_request(ServiceRequest::new(names::WORKSPACE_RELOAD_PRESETS))
            }
            Command::ApplyWorkspacePreset { preset_id } => self.execute_service_request(
                ServiceRequest::new(names::WORKSPACE_APPLY_PRESET)
                    .with_value("preset_id", preset_id),
            ),
            Command::SaveWorkspacePreset { preset_id, label } => self.execute_service_request(
                ServiceRequest::new(names::WORKSPACE_SAVE_PRESET)
                    .with_value("preset_id", preset_id)
                    .with_value("label", label),
            ),
            Command::ExportWorkspacePreset { preset_id, label } => self.execute_service_request(
                ServiceRequest::new(names::WORKSPACE_EXPORT_PRESET)
                    .with_value("preset_id", preset_id)
                    .with_value("label", label),
            ),
            Command::ExportWorkspacePresetToPath {
                preset_id,
                label,
                path,
            } => self.execute_service_request(
                ServiceRequest::new(names::WORKSPACE_EXPORT_PRESET_TO_PATH)
                    .with_value("preset_id", preset_id)
                    .with_value("label", label)
                    .with_value("path", path),
            ),
            Command::ReloadPenPresets => self.execute_service_request(ServiceRequest::new(
                names::TOOL_CATALOG_RELOAD_PEN_PRESETS,
            )),
            Command::ImportPenPresets => self.execute_service_request(ServiceRequest::new(
                names::TOOL_CATALOG_IMPORT_PEN_PRESETS,
            )),
            Command::ImportPenPresetsFromPath { path } => self.execute_service_request(
                ServiceRequest::new(names::TOOL_CATALOG_IMPORT_PEN_PATH).with_value("path", path),
            ),
            Command::Undo => self.execute_undo(),
            Command::Redo => self.execute_redo(),
            other => self.execute_document_command(other),
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(super) fn execute_document_command(&mut self, command: Command) -> bool {
        let previous_transform = self.document.view_transform;
        let _dirty = self.document.apply_command(&command);
        match command {
            Command::SetActiveTool { .. }
            | Command::SelectTool { .. }
            | Command::SelectChildTool { .. }
            | Command::SelectNextPenPreset
            | Command::SelectPreviousPenPreset => {
                self.sync_ui_from_document_panels(TOOL_PANEL_IDS);
                self.mark_status_dirty();
                self.upload_active_pen_tip_to_gpu_cache();
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
            Command::SetActiveLayerBlendMode { .. }
            | Command::ToggleActiveLayerVisibility => {
                let panel_info = self.document.active_panel().map(|p| {
                    (
                        p.id,
                        CanvasDirtyRect::new(p.bounds.x, p.bounds.y, p.bounds.width, p.bounds.height),
                        CanvasDirtyRect::new(0, 0, p.bitmap.width, p.bitmap.height),
                    )
                });
                if let Some((_panel_id, page_dirty, _local_dirty)) = panel_info {
                    self.append_canvas_dirty_rect(page_dirty);
                } else {
                    self.refresh_canvas_frame();
                    self.rebuild_present_frame();
                }
                if let Some((panel_id, _page_dirty, local_dirty)) = panel_info {
                    self.recomposite_panel(panel_id, Some(local_dirty));
                }
                self.sync_ui_from_document();
                self.mark_status_dirty();
                true
            }
            Command::AddRasterLayer
            | Command::RemoveActiveLayer
            | Command::SelectLayer { .. }
            | Command::RenameActiveLayer { .. }
            | Command::MoveLayer { .. }
            | Command::SelectNextLayer
            | Command::CycleActiveLayerBlendMode
            | Command::ToggleActiveLayerMask => {
                self.refresh_canvas_frame();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                self.sync_all_layers_to_gpu();
                self.recomposite_all_panels();
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
                self.sync_all_layers_to_gpu();
                self.recomposite_all_panels();
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
                self.sync_all_layers_to_gpu();
                self.recomposite_all_panels();
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
            | Command::ImportPenPresetsFromPath { .. }
            | Command::Undo
            | Command::Redo => false,
        }
    }
}
