//! present 向け dirty 状態と更新指示を扱う。

use app_core::{BitmapEdit, CanvasDirtyRect, MergeInSpace};

use super::DesktopApp;
use crate::frame::Rect;

/// 差分提示のために更新領域を集約した結果を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentFrameUpdate {
    pub(crate) background_dirty_rect: Option<Rect>,
    pub(crate) temp_overlay_dirty_rect: Option<Rect>,
    pub(crate) ui_panel_dirty_rect: Option<Rect>,
    pub(crate) canvas_dirty_rect: Option<CanvasDirtyRect>,
    pub(crate) canvas_transform_changed: bool,
    pub(crate) canvas_updated: bool,
}

impl DesktopApp {
    /// パネル サーフェス 差分 を更新し、必要な dirty 状態も記録する。
    pub(super) fn mark_panel_surface_dirty(&mut self) {
        self.needs_panel_surface_refresh = true;
    }

    /// ステータス 差分 を更新し、必要な dirty 状態も記録する。
    pub(super) fn mark_status_dirty(&mut self) {
        self.needs_status_refresh = true;
    }

    /// ステータス refresh を後段の処理へ遅延させる。
    pub(super) fn defer_status_refresh(&mut self) {
        self.deferred_status_refresh = true;
    }

    /// 全パネルを dirty としてマークし、ドキュメント同期をスケジュールする。
    pub(super) fn sync_ui_from_document(&mut self) {
        self.panel_runtime.mark_all_dirty();
        self.mark_panel_surface_dirty();
    }

    /// 指定パネルを dirty としてマークし、ドキュメント同期をスケジュールする。
    pub(super) fn sync_ui_from_document_panels(&mut self, panel_ids: &[&str]) {
        if panel_ids.is_empty() {
            return;
        }
        for &id in panel_ids {
            self.panel_runtime.mark_dirty(id);
        }
        self.mark_panel_surface_dirty();
    }

    /// ビュー パネル 同期 を後段の処理へ遅延させる。
    pub(super) fn defer_view_panel_sync(&mut self) {
        self.deferred_view_panel_sync = true;
    }

    /// 保留中の deferred ビュー パネル 同期 を反映する。
    pub(crate) fn flush_deferred_view_panel_sync(&mut self) -> bool {
        if !self.deferred_view_panel_sync {
            return false;
        }
        self.deferred_view_panel_sync = false;
        self.sync_ui_from_document_panels(&["builtin.view-controls"]);
        true
    }

    /// 保留中の deferred ステータス refresh を反映する。
    pub(crate) fn flush_deferred_status_refresh(&mut self) -> bool {
        if !self.deferred_status_refresh {
            return false;
        }
        self.deferred_status_refresh = false;
        self.mark_status_dirty();
        true
    }

    /// 提示 フレーム を再構築する。
    pub(super) fn rebuild_present_frame(&mut self) {
        self.needs_full_present_rebuild = true;
    }

    /// 初期化 アクティブ interactions に必要な差分領域だけを描画または合成する。
    pub(super) fn reset_active_interactions(&mut self) {
        self.canvas_input.reset();
        self.pending_canvas_dirty_rect = None;
        self.pending_background_dirty_rect = None;
        self.pending_temp_overlay_dirty_rect = None;
        self.pending_ui_panel_dirty_rect = None;
        self.pending_canvas_transform_update = false;
        self.deferred_view_panel_sync = false;
        self.deferred_status_refresh = false;
        self.panel_interaction.active_panel_drag = None;
        self.panel_interaction.pending_panel_press = None;
        self.hover_canvas_position = None;
    }

    /// パネル サーフェス if changed を更新し、必要な dirty 状態も記録する。
    pub(super) fn refresh_panel_surface_if_changed(&mut self, changed: bool) -> bool {
        if changed {
            self.mark_panel_surface_dirty();
        }
        changed
    }

    /// Append キャンバス 差分 矩形 に必要な差分領域だけを描画または合成する。
    pub(super) fn append_canvas_dirty_rect(&mut self, dirty: CanvasDirtyRect) -> bool {
        self.pending_canvas_dirty_rect = Some(
            self.pending_canvas_dirty_rect
                .map_or(dirty, |existing| existing.merge(dirty)),
        );
        true
    }

    /// ビットマップ edits を更新し、必要な dirty 状態も記録する。
    pub(super) fn apply_bitmap_edits(&mut self, edits: Vec<BitmapEdit>) -> bool {
        self.document
            .apply_bitmap_edits_to_active_layer(&edits)
            .is_some_and(|dirty| {
                self.refresh_canvas_frame_region(dirty);
                self.append_canvas_dirty_rect(dirty)
            })
    }

    /// Append temp オーバーレイ 差分 矩形（L3）に必要な差分領域だけを描画または合成する。
    pub(super) fn append_temp_overlay_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.pending_temp_overlay_dirty_rect = Some(
            self.pending_temp_overlay_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// Append UI パネル 差分 矩形（L4）に必要な差分領域だけを描画または合成する。
    pub(super) fn append_ui_panel_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.pending_ui_panel_dirty_rect = Some(
            self.pending_ui_panel_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// キャンバス 変換 差分 を更新し、必要な dirty 状態も記録する。
    pub(super) fn mark_canvas_transform_dirty(
        &mut self,
        previous_transform: app_core::CanvasViewTransform,
    ) -> bool {
        self.pending_canvas_transform_update = true;
        if let Some(canvas_viewport_rect) =
            self.layout.as_ref().map(|layout| layout.canvas_host_rect)
        {
            let (canvas_width, canvas_height) = self.canvas_dimensions();
            let viewport = render::PixelRect {
                x: canvas_viewport_rect.x,
                y: canvas_viewport_rect.y,
                width: canvas_viewport_rect.width,
                height: canvas_viewport_rect.height,
            };
            let previous_scene = render::prepare_canvas_scene(
                viewport,
                canvas_width,
                canvas_height,
                previous_transform,
            );
            let current_scene = render::prepare_canvas_scene(
                viewport,
                canvas_width,
                canvas_height,
                self.document.view_transform,
            );
            if let Some(exposed) =
                render::exposed_canvas_background_rect_from_scenes(previous_scene, current_scene)
            {
                self.pending_background_dirty_rect = Some(
                    self.pending_background_dirty_rect
                        .map_or(exposed, |existing| existing.union(exposed)),
                );
            }
            if let Some(dirty) = self.hover_canvas_position.and_then(|hover_position| {
                render::brush_preview_dirty_rect(
                    previous_scene,
                    current_scene,
                    hover_position,
                    self.brush_preview_size().unwrap_or(1) as f32,
                )
            }) {
                self.append_temp_overlay_dirty_rect(dirty);
            }
        } else {
            self.rebuild_present_frame();
        }
        true
    }

    /// Background フレーム を返す。
    pub(crate) fn background_frame(&self) -> Option<&render::RenderFrame> {
        self.background_frame.as_ref()
    }

    /// TempOverlay フレーム を返す。
    pub(crate) fn temp_overlay_frame(&self) -> Option<&render::RenderFrame> {
        self.temp_overlay_frame.as_ref()
    }

    /// UiPanel フレーム を返す。
    pub(crate) fn ui_panel_frame(&self) -> Option<&render::RenderFrame> {
        self.ui_panel_frame.as_ref()
    }

    /// キャンバス texture quad を計算して返す。
    pub(crate) fn canvas_texture_quad(&self) -> Option<render::TextureQuad> {
        self.canvas_scene().and_then(|scene| scene.texture_quad())
    }

    /// キャンバス シーン を計算して返す。
    pub(crate) fn canvas_scene(&self) -> Option<render::CanvasScene> {
        let layout = self.layout.as_ref()?;
        let bitmap = self.canvas_frame()?;
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

    /// キャンバス フレーム を返す。
    pub(crate) fn canvas_frame(&self) -> Option<&render::RenderFrame> {
        self.canvas_frame.as_ref()
    }
}
