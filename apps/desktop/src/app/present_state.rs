//! present 向け dirty 状態と更新指示を扱う。

use std::collections::BTreeSet;

use app_core::{BitmapEdit, CanvasDirtyRect, MergeInSpace};

use super::DesktopApp;
use crate::frame::Rect;

/// 差分提示のために更新領域を集約した結果を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentFrameUpdate {
    pub(crate) base_dirty_rect: Option<Rect>,
    pub(crate) overlay_dirty_rect: Option<Rect>,
    pub(crate) canvas_dirty_rect: Option<CanvasDirtyRect>,
    pub(crate) canvas_transform_changed: bool,
    pub(crate) canvas_updated: bool,
}

impl DesktopApp {
    /// パネル面の再描画が必要であることを記録する。
    pub(super) fn mark_panel_surface_dirty(&mut self) {
        self.needs_panel_surface_refresh = true;
    }

    /// ステータス表示の更新が必要であることを記録する。
    pub(super) fn mark_status_dirty(&mut self) {
        self.needs_status_refresh = true;
    }

    /// 高頻度ビュー操作中のステータス更新を後段へ遅延する。
    pub(super) fn defer_status_refresh(&mut self) {
        self.deferred_status_refresh = true;
    }

    /// ドキュメント変更後に `UiShell` の再同期を要求する。
    pub(super) fn sync_ui_from_document(&mut self) {
        self.needs_ui_sync = true;
        self.ui_sync_panel_ids = BTreeSet::new();
        self.mark_panel_surface_dirty();
    }

    /// 指定 panel 群だけを document から再同期する。
    pub(super) fn sync_ui_from_document_panels(&mut self, panel_ids: &[&str]) {
        if panel_ids.is_empty() {
            return;
        }
        if !self.needs_ui_sync {
            self.ui_sync_panel_ids.clear();
            self.ui_sync_panel_ids
                .extend(panel_ids.iter().map(|panel_id| (*panel_id).to_string()));
        } else if !self.ui_sync_panel_ids.is_empty() {
            self.ui_sync_panel_ids
                .extend(panel_ids.iter().map(|panel_id| (*panel_id).to_string()));
        }
        self.needs_ui_sync = true;
        self.mark_panel_surface_dirty();
    }

    /// 高頻度更新で重い panel 同期を後段へ遅延する。
    pub(super) fn defer_view_panel_sync(&mut self) {
        self.deferred_view_panel_sync = true;
    }

    /// 遅延していた view panel 同期を 1 回だけ反映する。
    pub(crate) fn flush_deferred_view_panel_sync(&mut self) -> bool {
        if !self.deferred_view_panel_sync {
            return false;
        }
        self.deferred_view_panel_sync = false;
        self.sync_ui_from_document_panels(&["builtin.view-controls"]);
        true
    }

    /// 遅延していたステータス更新を 1 回だけ反映する。
    pub(crate) fn flush_deferred_status_refresh(&mut self) -> bool {
        if !self.deferred_status_refresh {
            return false;
        }
        self.deferred_status_refresh = false;
        self.mark_status_dirty();
        true
    }

    /// ベースフレームの全面再構築を要求する。
    pub(super) fn rebuild_present_frame(&mut self) {
        self.needs_full_present_rebuild = true;
    }

    /// 読み込みや新規作成の前に対話中状態を初期化する。
    pub(super) fn reset_active_interactions(&mut self) {
        self.canvas_input.reset();
        self.pending_canvas_dirty_rect = None;
        self.pending_canvas_background_dirty_rect = None;
        self.pending_canvas_host_dirty_rect = None;
        self.pending_canvas_transform_update = false;
        self.deferred_view_panel_sync = false;
        self.deferred_status_refresh = false;
        self.panel_interaction.active_panel_drag = None;
        self.panel_interaction.pending_panel_press = None;
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
    pub(super) fn append_canvas_dirty_rect(&mut self, dirty: CanvasDirtyRect) -> bool {
        self.pending_canvas_dirty_rect = Some(
            self.pending_canvas_dirty_rect
                .map_or(dirty, |existing| existing.merge(dirty)),
        );
        true
    }

    /// 描画プラグインが返したビットマップ差分をドキュメントへ反映する。
    pub(super) fn apply_bitmap_edits(&mut self, edits: Vec<BitmapEdit>) -> bool {
        self.document
            .apply_bitmap_edits_to_active_layer(&edits)
            .is_some_and(|dirty| {
                self.refresh_canvas_frame_region(dirty);
                self.append_canvas_dirty_rect(dirty)
            })
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
            if let Some(hover_position) = self.hover_canvas_position {
                let brush_size = self.brush_preview_size().unwrap_or(1) as f32;
                let previous_preview = previous_scene
                    .and_then(|scene| scene.brush_preview_rect_for_diameter(hover_position, brush_size))
                    .map(|rect| Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    });
                let current_preview = current_scene
                    .and_then(|scene| scene.brush_preview_rect_for_diameter(hover_position, brush_size))
                    .map(|rect| Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    });

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

    /// 現在のベースフレームを返す。
    pub(crate) fn base_frame(&self) -> Option<&render::RenderFrame> {
        self.base_frame.as_ref()
    }

    /// 現在のオーバーレイフレームを返す。
    pub(crate) fn overlay_frame(&self) -> Option<&render::RenderFrame> {
        self.overlay_frame.as_ref()
    }

    /// 現在のキャンバスを描画する GPU 四角形を返す。
    pub(crate) fn canvas_texture_quad(&self) -> Option<crate::frame::TextureQuad> {
        self.canvas_scene().and_then(|scene| scene.texture_quad()).map(|quad| crate::frame::TextureQuad {
            destination: Rect {
                x: quad.destination.x,
                y: quad.destination.y,
                width: quad.destination.width,
                height: quad.destination.height,
            },
            uv_min: quad.uv_min,
            uv_max: quad.uv_max,
            rotation_degrees: quad.rotation_degrees,
            bbox_size: quad.bbox_size,
            flip_x: quad.flip_x,
            flip_y: quad.flip_y,
        })
    }

    /// 現在のキャンバス表示計画を返す。
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

    /// 現在のキャンバスフレームを返す。
    pub(crate) fn canvas_frame(&self) -> Option<&render::RenderFrame> {
        self.canvas_frame.as_ref()
    }
}
