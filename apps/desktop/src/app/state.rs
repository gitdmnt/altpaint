//! `DesktopApp` の状態集約と内部更新規約を定義する。
//!
//! ドキュメント更新、dirty rect 集約、セッション永続化のような
//! 状態管理責務をここへ寄せ、構築処理と入力処理から分離する。

use std::thread::JoinHandle;

use app_core::{Command, DirtyRect, Document};
use desktop_support::{DEFAULT_PROJECT_PATH, DesktopSessionState, default_pen_dir, save_session_state};
use render::RenderFrame;
use storage::load_pen_directory;

use super::DesktopApp;
use crate::canvas_bridge::CanvasInputState;
use crate::frame::{Rect, TextureQuad, brush_preview_rect};

fn from_render_rect(rect: render::PixelRect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn from_render_quad(quad: render::TextureQuad) -> TextureQuad {
    TextureQuad {
        destination: from_render_rect(quad.destination),
        uv_min: quad.uv_min,
        uv_max: quad.uv_max,
        rotation_turns: quad.rotation_turns,
        flip_x: quad.flip_x,
        flip_y: quad.flip_y,
    }
}

/// 差分提示のために更新領域を集約した結果を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentFrameUpdate {
    pub(crate) base_dirty_rect: Option<crate::frame::Rect>,
    pub(crate) overlay_dirty_rect: Option<crate::frame::Rect>,
    pub(crate) canvas_dirty_rect: Option<DirtyRect>,
    pub(crate) canvas_transform_changed: bool,
    pub(crate) canvas_updated: bool,
}

/// スライダードラッグ中のパネルノード情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelDragState {
    pub(crate) panel_id: String,
    pub(crate) node_id: String,
    pub(crate) source_value: usize,
}

/// 非同期保存タスクの join handle を保持する。
#[derive(Debug)]
pub(crate) struct PendingSaveTask {
    pub(crate) handle: JoinHandle<Result<(), String>>,
}

impl DesktopApp {
    /// 現在のデスクトップセッションとして保存すべき状態を組み立てる。
    pub(super) fn session_state(&self) -> DesktopSessionState {
        DesktopSessionState {
            last_project_path: Some(self.project_path.clone()),
            ui_state: workspace_persistence::WorkspaceUiState::new(
                self.ui_shell.workspace_layout(),
                self.ui_shell.persistent_panel_configs(),
            ),
        }
    }

    /// セッションファイルへ現在状態を書き戻す。
    pub(super) fn persist_session_state(&self) {
        if let Err(error) = save_session_state(&self.session_path, &self.session_state()) {
            eprintln!("failed to persist desktop session: {error}");
        }
    }

    /// 完了した非同期保存タスクを回収し、エラーを UI へ通知する。
    pub(super) fn poll_background_tasks(&mut self) {
        let mut remaining = Vec::new();
        let mut completed_any = false;

        for task in self.pending_save_tasks.drain(..) {
            if task.handle.is_finished() {
                completed_any = true;
                match task.handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        eprintln!("failed to save project: {error}");
                        self.dialogs.show_error("Save failed", &error);
                    }
                    Err(_) => {
                        self.dialogs
                            .show_error("Save failed", "background save task panicked");
                    }
                }
            } else {
                remaining.push(task);
            }
        }

        if completed_any {
            self.mark_status_dirty();
        }
        self.pending_save_tasks = remaining;
    }

    /// テスト用に全保存タスクの完了を待機する。
    #[cfg(test)]
    pub(crate) fn wait_for_pending_save_tasks(&mut self) {
        let mut remaining = Vec::new();
        std::mem::swap(&mut remaining, &mut self.pending_save_tasks);
        for task in remaining {
            match task.handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("background save failed: {error}"),
                Err(_) => panic!("background save task panicked"),
            }
        }
    }

    /// 未完了の保存タスク数を返す。
    #[cfg(test)]
    pub(crate) fn pending_save_task_count(&self) -> usize {
        self.pending_save_tasks.len()
    }

    /// パネル面の再描画が必要であることを記録する。
    pub(super) fn mark_panel_surface_dirty(&mut self) {
        self.needs_panel_surface_refresh = true;
    }

    /// ステータス表示の更新が必要であることを記録する。
    pub(super) fn mark_status_dirty(&mut self) {
        self.needs_status_refresh = true;
    }

    /// ドキュメント変更後に `UiShell` の再同期を要求する。
    pub(super) fn sync_ui_from_document(&mut self) {
        self.needs_ui_sync = true;
        self.mark_panel_surface_dirty();
    }

    /// ベースフレームの全面再構築を要求する。
    pub(super) fn rebuild_present_frame(&mut self) {
        self.needs_full_present_rebuild = true;
    }

    /// 読み込みや新規作成の前に対話中状態を初期化する。
    pub(super) fn reset_active_interactions(&mut self) {
        self.canvas_input = CanvasInputState::default();
        self.pending_canvas_dirty_rect = None;
        self.pending_canvas_background_dirty_rect = None;
        self.pending_canvas_host_dirty_rect = None;
        self.pending_canvas_transform_update = false;
        self.active_panel_drag = None;
        self.hover_canvas_position = None;
    }

    /// UI 変化に応じてパネル面 dirty を立てる。
    pub(super) fn refresh_panel_surface_if_changed(&mut self, changed: bool) -> bool {
        if changed {
            self.mark_panel_surface_dirty();
        }
        changed
    }

    /// キャンバス dirty rect を次回提示用に集約する。
    pub(super) fn append_canvas_dirty_rect(&mut self, dirty: DirtyRect) -> bool {
        self.pending_canvas_dirty_rect = Some(
            self.pending_canvas_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// キャンバス背景 dirty rect を次回提示用に集約する。
    pub(super) fn append_canvas_background_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.pending_canvas_background_dirty_rect = Some(
            self.pending_canvas_background_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// キャンバスホスト dirty rect を次回提示用に集約する。
    pub(super) fn append_canvas_host_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.pending_canvas_host_dirty_rect = Some(
            self.pending_canvas_host_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// 変換変更に伴う背景・プレビューの再描画範囲を計算する。
    pub(super) fn mark_canvas_transform_dirty(
        &mut self,
        previous_transform: app_core::CanvasViewTransform,
    ) -> bool {
        self.pending_canvas_transform_update = true;
        if let Some(canvas_viewport_rect) =
            self.layout.as_ref().map(|layout| layout.canvas_host_rect)
        {
            let (canvas_width, canvas_height) = self.canvas_dimensions();
            let background_dirty = render::exposed_canvas_background_rect(
                render::PixelRect {
                    x: canvas_viewport_rect.x,
                    y: canvas_viewport_rect.y,
                    width: canvas_viewport_rect.width,
                    height: canvas_viewport_rect.height,
                },
                canvas_width,
                canvas_height,
                previous_transform,
                self.document.view_transform,
            )
            .map(from_render_rect)
            .unwrap_or(canvas_viewport_rect);
            self.append_canvas_background_dirty_rect(background_dirty);
            if let Some(hover_position) = self.hover_canvas_position {
                let previous_preview = brush_preview_rect(
                    canvas_viewport_rect,
                    canvas_width,
                    canvas_height,
                    previous_transform,
                    hover_position,
                );
                let current_preview = brush_preview_rect(
                    canvas_viewport_rect,
                    canvas_width,
                    canvas_height,
                    self.document.view_transform,
                    hover_position,
                );

                match (previous_preview, current_preview) {
                    (Some(previous), Some(current)) => {
                        self.append_canvas_host_dirty_rect(previous.union(current));
                    }
                    (Some(previous), None) => {
                        self.append_canvas_host_dirty_rect(previous);
                    }
                    (None, Some(current)) => {
                        self.append_canvas_host_dirty_rect(current);
                    }
                    (None, None) => {}
                }
            }
        } else {
            self.rebuild_present_frame();
        }
        true
    }

    /// ドキュメント変更系コマンドを適用し、dirty 状態を更新する。
    pub(super) fn execute_document_command(&mut self, command: Command) -> bool {
        let previous_transform = self.document.view_transform;
        let dirty = self.document.apply_command(&command);
        match command {
            Command::DrawPoint { .. }
            | Command::DrawStroke { .. }
            | Command::ErasePoint { .. }
            | Command::EraseStroke { .. }
            | Command::FillRegion { .. }
            | Command::FillLasso { .. } => {
                dirty.is_some_and(|dirty| self.append_canvas_dirty_rect(dirty))
            }
            Command::SetActiveTool { .. }
            | Command::SetActivePenSize { .. }
            | Command::SetActivePenPressureEnabled { .. }
            | Command::SetActivePenAntialias { .. }
            | Command::SetActivePenStabilization { .. }
            | Command::SelectNextPenPreset
            | Command::SelectPreviousPenPreset
            | Command::SetActiveColor { .. } => {
                self.sync_ui_from_document();
                self.mark_status_dirty();
                true
            }
            Command::SetViewZoom { .. }
            | Command::RotateView { .. }
            | Command::FlipViewHorizontally
            | Command::FlipViewVertically
            | Command::ResetView => {
                self.mark_canvas_transform_dirty(previous_transform);
                self.mark_status_dirty();
                true
            }
            Command::PanView { .. } => self.mark_canvas_transform_dirty(previous_transform),
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
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::NewDocumentSized { .. } => {
                self.reset_active_interactions();
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
            | Command::ReloadPenPresets => false,
        }
    }

    /// 現在のベースフレームを返す。
    pub(crate) fn base_frame(&self) -> Option<&RenderFrame> {
        self.base_frame.as_ref()
    }

    /// 現在のオーバーレイフレームを返す。
    pub(crate) fn overlay_frame(&self) -> Option<&RenderFrame> {
        self.overlay_frame.as_ref()
    }

    /// 現在のキャンバスを描画する GPU 四角形を返す。
    pub(crate) fn canvas_texture_quad(&self) -> Option<TextureQuad> {
        self.canvas_scene()
            .and_then(|scene| scene.texture_quad())
            .map(from_render_quad)
    }

    /// 現在のキャンバス表示計画を返す。
    pub(crate) fn canvas_scene(&self) -> Option<render::CanvasScene> {
        let layout = self.layout.as_ref()?;
        let bitmap = self.document.active_bitmap()?;
        render::prepare_canvas_scene(
            render::PixelRect {
                x: layout.canvas_host_rect.x,
                y: layout.canvas_host_rect.y,
                width: layout.canvas_host_rect.width,
                height: layout.canvas_host_rect.height,
            },
            bitmap.width,
            bitmap.height,
            self.document.view_transform,
        )
    }

    /// アクティブビットマップ寸法を返す。
    pub(super) fn canvas_dimensions(&self) -> (usize, usize) {
        self.document
            .active_bitmap()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    /// 既定ペンディレクトリからプリセットを再読込する。
    pub(crate) fn reload_pen_presets(&mut self) -> bool {
        let changed = Self::reload_pen_presets_into_document(&mut self.document);
        if changed {
            self.sync_ui_from_document();
            self.mark_status_dirty();
            self.rebuild_present_frame();
        }
        changed
    }

    /// ドキュメントへ読み込んだペンプリセット群を適用する。
    pub(super) fn reload_pen_presets_into_document(document: &mut Document) -> bool {
        let (presets, diagnostics) = load_pen_directory(default_pen_dir());
        for diagnostic in diagnostics {
            eprintln!("pen preset load warning: {diagnostic}");
        }
        if presets.is_empty() {
            return false;
        }
        document.replace_pen_presets(presets);
        true
    }

    /// フッターへ表示する現在状態の概要文字列を生成する。
    pub(crate) fn status_text(&self) -> String {
        let file_name = self
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_PATH);
        let hidden_panels = self
            .ui_shell
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / pages={} / panels={} / hidden={}",
            file_name,
            self.document.active_tool,
            self.document
                .active_pen_preset()
                .map(|preset| preset.name.as_str())
                .unwrap_or("Round Pen"),
            self.document.active_pen_size,
            self.document.active_color.hex_rgb(),
            self.document.view_transform.zoom,
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

    /// キャンバス描画中かどうかを返す。
    pub(crate) fn is_canvas_interacting(&self) -> bool {
        self.canvas_input.is_drawing
    }
}

