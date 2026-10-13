//! コマンド種別ごとの宣言的副作用表 (D10)。
//!
//! `command_router` がコマンドをドキュメント/セッションへ適用したあと、
//! コマンド種別に応じた UI 同期・dirty マーク・GPU 再合成などの副作用を
//! ここで分類して実行する。ルーティング (apply 入口) と副作用 orchestration を
//! 分離する。

use document_model::DocumentCommand;
use editor_state::SessionCommand;
use geometry::PageDirtyRect;

use super::DesktopApp;
use super::invalidation::GpuSyncGranularity;

impl DesktopApp {
    /// ドキュメントコマンド適用後の副作用を、コマンド種別ごとに実行する。
    ///
    /// 戻り値はコマンドが何らかの変化を生じたか (再描画が必要か)。
    pub(super) fn document_command_effects(&mut self, command: &DocumentCommand) -> bool {
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
            // アクティブコマのレイヤー構成が変化する操作。当該コマだけ差分同期する。
            DocumentCommand::AddRasterLayer
            | DocumentCommand::RemoveActiveLayer
            | DocumentCommand::MoveLayer { .. } => {
                self.invalidate_document_structure(GpuSyncGranularity::ActiveKomaLayers);
                true
            }
            // blend mode 循環はアクティブコマの合成出力のみ変える。再合成のみ。
            DocumentCommand::CycleActiveLayerBlendMode => {
                self.invalidate_document_structure(GpuSyncGranularity::RecompositeActiveKoma);
                true
            }
            // レイヤー選択・リネームは GPU テクスチャ不変。同期不要。
            DocumentCommand::SelectLayer { .. }
            | DocumentCommand::RenameActiveLayer { .. }
            | DocumentCommand::SelectNextLayer => {
                self.invalidate_document_structure(GpuSyncGranularity::None);
                true
            }
            // コマ集合が変わる操作。全ページ全コマを同期する。
            DocumentCommand::AddKoma
            | DocumentCommand::CreateKoma { .. }
            | DocumentCommand::RemoveActiveKoma => {
                self.invalidate_document_structure(GpuSyncGranularity::Full);
                true
            }
            // コマ選択は GPU テクスチャ不変。同期不要。
            DocumentCommand::SelectKoma { .. }
            | DocumentCommand::SelectNextKoma
            | DocumentCommand::SelectPreviousKoma
            | DocumentCommand::FocusActiveKoma => {
                self.invalidate_document_structure(GpuSyncGranularity::None);
                true
            }
            DocumentCommand::NewDocumentSized { .. } => {
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = Self::reload_pen_presets_into_document(&mut self.document);
                self.reset_active_interactions();
                self.invalidate_document_structure(GpuSyncGranularity::Full);
                true
            }
            DocumentCommand::Noop => false,
        }
    }

    /// セッションコマンド適用後の副作用を、コマンド種別ごとに実行する。
    ///
    /// `previous_transform` は適用前のビュー変換 (ビュー系コマンドの dirty 計算に使う)。
    /// 戻り値はコマンドが何らかの変化を生じたか。
    pub(super) fn session_command_effects(
        &mut self,
        command: &SessionCommand,
        previous_transform: editor_state::CanvasViewTransform,
    ) -> bool {
        match command {
            SessionCommand::SetActiveTool { .. }
            | SessionCommand::SelectTool { .. }
            | SessionCommand::SelectChildTool { .. }
            | SessionCommand::SelectNextPenPreset { .. }
            | SessionCommand::SelectPreviousPenPreset { .. } => {
                self.sync_ui_from_section("tool");
                self.mark_status_dirty();
                true
            }
            SessionCommand::SetActivePenSize { .. }
            | SessionCommand::SetActivePenPressureEnabled { .. }
            | SessionCommand::SetActivePenAntialias { .. }
            | SessionCommand::SetActivePenStabilization { .. } => {
                self.sync_ui_from_section("tool");
                self.mark_status_dirty();
                true
            }
            SessionCommand::SetActiveColor { .. } => {
                self.sync_ui_from_section("color");
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
