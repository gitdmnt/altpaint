//! present 向け dirty 状態と更新指示を扱う。

use app_core::{BitmapEdit, PageDirtyRect, MergeInSpace};

use super::DesktopApp;
use crate::frame::Rect;

/// 差分提示のために更新領域を集約した結果を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentFrameUpdate {
    pub(crate) background_dirty_rect: Option<Rect>,
    pub(crate) temp_overlay_dirty_rect: Option<Rect>,
    pub(crate) ui_panel_dirty_rect: Option<Rect>,
    pub(crate) canvas_dirty_rect: Option<PageDirtyRect>,
    pub(crate) canvas_transform_changed: bool,
    pub(crate) canvas_updated: bool,
}

/// 次フレームの提示で消化される無効化状態 (保留 dirty rect・再構築/再同期フラグ) を集約する。
///
/// `prepare_present_frame` が消化するまで蓄積され、消化後にクリアされる。
#[derive(Debug, Default)]
pub(crate) struct PresentInvalidation {
    /// L2 キャンバス層の保留 dirty rect (キャンバス座標)。
    pub(crate) canvas_dirty_rect: Option<PageDirtyRect>,
    /// L3 一時オーバーレイ層の保留 dirty rect (window 座標)。
    pub(crate) temp_overlay_dirty_rect: Option<Rect>,
    /// L4 UI パネル層の保留 dirty rect (window 座標)。
    pub(crate) ui_panel_dirty_rect: Option<Rect>,
    /// ビュー変換 (pan/zoom/rotation) が変化したか。
    pub(crate) canvas_transform_update: bool,
    /// view-controls パネルの再同期をフレーム後段へ遅延しているか。
    pub(crate) deferred_view_panel_sync: bool,
    /// ステータスバー更新をフレーム後段へ遅延しているか。
    pub(crate) deferred_status_refresh: bool,
    /// presentation の workspace layout を runtime のパネル一覧と再整合させる必要があるか。
    pub(crate) needs_panel_reconcile: bool,
    /// ステータスバーの再構築が必要か。
    pub(crate) needs_status_refresh: bool,
    /// フレーム全体の再構築が必要か。
    pub(crate) needs_full_present_rebuild: bool,
}

impl PresentInvalidation {
    /// 起動直後の状態。初回フレームで全構築とパネル再整合を要求する。
    pub(crate) fn at_startup() -> Self {
        Self {
            needs_panel_reconcile: true,
            needs_full_present_rebuild: true,
            ..Self::default()
        }
    }

    /// アクティブな操作に紐づく保留中の差分・遅延フラグを破棄する (needs_* は維持)。
    pub(crate) fn clear_pending(&mut self) {
        self.canvas_dirty_rect = None;
        self.temp_overlay_dirty_rect = None;
        self.ui_panel_dirty_rect = None;
        self.canvas_transform_update = false;
        self.deferred_view_panel_sync = false;
        self.deferred_status_refresh = false;
    }
}

impl DesktopApp {
    /// presentation と runtime のパネル一覧の再整合を予約する。
    pub(super) fn request_panel_reconcile(&mut self) {
        self.invalidation.needs_panel_reconcile = true;
    }

    pub(super) fn mark_status_dirty(&mut self) {
        self.invalidation.needs_status_refresh = true;
    }

    pub(super) fn defer_status_refresh(&mut self) {
        self.invalidation.deferred_status_refresh = true;
    }

    /// 全パネルを dirty としてマークし、ドキュメント同期をスケジュールする。
    pub(super) fn sync_ui_from_document(&mut self) {
        self.panel_runtime.mark_all_dirty();
        self.request_panel_reconcile();
    }

    /// 指定パネルを dirty としてマークし、ドキュメント同期をスケジュールする。
    pub(super) fn sync_ui_from_document_panels(&mut self, panel_ids: &[&str]) {
        if panel_ids.is_empty() {
            return;
        }
        for &id in panel_ids {
            self.panel_runtime.mark_dirty(id);
        }
        self.request_panel_reconcile();
    }

    pub(super) fn defer_view_panel_sync(&mut self) {
        self.invalidation.deferred_view_panel_sync = true;
    }

    pub(crate) fn flush_deferred_view_panel_sync(&mut self) -> bool {
        if !self.invalidation.deferred_view_panel_sync {
            return false;
        }
        self.invalidation.deferred_view_panel_sync = false;
        self.sync_ui_from_document_panels(&["builtin.view-controls"]);
        true
    }

    pub(crate) fn flush_deferred_status_refresh(&mut self) -> bool {
        if !self.invalidation.deferred_status_refresh {
            return false;
        }
        self.invalidation.deferred_status_refresh = false;
        self.mark_status_dirty();
        true
    }

    pub(super) fn rebuild_present_frame(&mut self) {
        self.invalidation.needs_full_present_rebuild = true;
    }

    pub(super) fn reset_active_interactions(&mut self) {
        self.canvas_input.reset();
        self.invalidation.clear_pending();
        self.panel_interaction = super::panel_dispatch::PanelInteractionState::default();
        self.hover_canvas_position = None;
    }

    /// 変更があった場合のみパネル再整合を予約する。
    pub(super) fn request_panel_reconcile_if_changed(&mut self, changed: bool) -> bool {
        if changed {
            self.request_panel_reconcile();
        }
        changed
    }

    pub(super) fn append_canvas_dirty_rect(&mut self, dirty: PageDirtyRect) -> bool {
        self.invalidation.canvas_dirty_rect = Some(
            self.invalidation.canvas_dirty_rect
                .map_or(dirty, |existing| existing.merge(dirty)),
        );
        true
    }

    pub(super) fn apply_bitmap_edits(&mut self, edits: Vec<BitmapEdit>) -> bool {
        self.document
            .apply_bitmap_edits_to_active_layer(&edits)
            .is_some_and(|dirty| self.append_canvas_dirty_rect(dirty))
    }

    /// temp オーバーレイ (L3) の dirty rect を蓄積する。
    pub(super) fn append_temp_overlay_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.invalidation.temp_overlay_dirty_rect = Some(
            self.invalidation.temp_overlay_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// UI パネル (L4) の dirty rect を蓄積する。
    pub(super) fn append_ui_panel_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.invalidation.ui_panel_dirty_rect = Some(
            self.invalidation.ui_panel_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    pub(super) fn mark_canvas_transform_dirty(
        &mut self,
        previous_transform: app_core::CanvasViewTransform,
    ) -> bool {
        self.invalidation.canvas_transform_update = true;
        if let Some(canvas_viewport_rect) =
            self.layout.as_ref().map(|layout| layout.canvas_host_rect)
        {
            let (canvas_width, canvas_height) = self.canvas_dimensions();
            let viewport = canvas_geometry::PixelRect {
                x: canvas_viewport_rect.x,
                y: canvas_viewport_rect.y,
                width: canvas_viewport_rect.width,
                height: canvas_viewport_rect.height,
            };
            // previous_geometry は変更前の transform で計算するためキャッシュは使えない
            let previous_geometry = canvas_geometry::CanvasViewGeometry::compute(
                viewport,
                canvas_width,
                canvas_height,
                previous_transform,
            );
            // current_geometry はキャッシュを使う（キャッシュが古ければ再計算して更新）
            self.cached_canvas_view_geometry = None;
            let current_geometry = self.canvas_view_geometry();
            if let Some(dirty) = self.hover_canvas_position.and_then(|hover_position| {
                canvas_geometry::brush_preview_dirty_rect(
                    previous_geometry,
                    current_geometry,
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

    /// L0 背景 solid quads (ウィンドウ背景・キャンバス枠 fill・ホスト枠線) を組み立てる。
    pub(crate) fn background_solid_quads(&self) -> Vec<crate::frame::SolidQuad> {
        let Some(layout) = self.layout.as_ref() else {
            return Vec::new();
        };
        crate::frame::build_background_solid_quads(
            layout.window_rect,
            layout.canvas_host_rect,
            layout.canvas_display_rect,
        )
    }

    /// L6 前景 solid quads (アクティブ UI パネル枠線) を組み立てる。
    pub(crate) fn foreground_solid_quads(&self) -> Vec<crate::frame::SolidQuad> {
        let active_rect = self
            .panel_presentation
            .focused_target()
            .and_then(|(panel_id, _)| self.panel_presentation.panel_rect(panel_id));
        crate::frame::build_foreground_solid_quads(active_rect)
    }

    /// L3 一時オーバーレイ用 quad を組み立てる。毎フレーム呼ぶ前提の純関数経路。
    /// 戻り値: (AABB 単色, 円リング, 線分カプセル)
    pub(crate) fn overlay_quads(
        &self,
    ) -> (
        Vec<crate::frame::SolidQuad>,
        Vec<crate::frame::CircleQuad>,
        Vec<crate::frame::LineQuad>,
    ) {
        let Some(layout) = self.layout.as_ref() else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let bitmap = self.canvas_frame.as_ref();
        let canvas_plan = canvas_geometry::CanvasPlan {
            host_rect: layout.canvas_host_rect,
            source_width: bitmap.map_or(1, |b| b.width),
            source_height: bitmap.map_or(1, |b| b.height),
            transform: self.document.view_transform,
        };
        let overlay_state = canvas_geometry::CanvasOverlayState {
            brush_preview: self.hover_canvas_position,
            brush_size: self.brush_preview_size(),
            lasso_points: self.canvas_input.lasso_points.clone(),
            active_koma_bounds: self.active_koma_mask_overlay(),
            koma_navigator: self.koma_navigator_overlay(),
            panel_creation_preview: self.koma_creation_preview_bounds(),
            active_ui_panel_rect: self
                .panel_presentation
                .focused_target()
                .and_then(|(panel_id, _)| self.panel_presentation.panel_rect(panel_id)),
        };
        (
            crate::frame::build_overlay_solid_quads(&canvas_plan, &overlay_state),
            crate::frame::build_overlay_circle_quads(&canvas_plan, &overlay_state),
            crate::frame::build_overlay_line_quads(&canvas_plan, &overlay_state),
        )
    }

    pub(crate) fn canvas_texture_quad(&mut self) -> Option<canvas_geometry::TextureQuad> {
        self.canvas_view_geometry().and_then(|geometry| geometry.texture_quad())
    }

    /// 入力が変わらない限りキャッシュした結果を再利用する。
    pub(crate) fn canvas_view_geometry(&mut self) -> Option<canvas_geometry::CanvasViewGeometry> {
        let layout = self.layout.as_ref()?;
        let bitmap = self.canvas_frame()?;
        let viewport = canvas_geometry::PixelRect {
            x: layout.canvas_host_rect.x,
            y: layout.canvas_host_rect.y,
            width: layout.canvas_host_rect.width,
            height: layout.canvas_host_rect.height,
        };
        let canvas_width = bitmap.width;
        let canvas_height = bitmap.height;
        let transform = self.document.view_transform;

        if let Some(ref cache) = self.cached_canvas_view_geometry
            && cache.viewport == viewport
            && cache.canvas_width == canvas_width
            && cache.canvas_height == canvas_height
            && cache.transform == transform
        {
            return cache.geometry;
        }
        let geometry = canvas_geometry::CanvasViewGeometry::compute(viewport, canvas_width, canvas_height, transform);
        self.cached_canvas_view_geometry = Some(super::CachedCanvasViewGeometry {
            viewport,
            canvas_width,
            canvas_height,
            transform,
            geometry,
        });
        geometry
    }

    pub(crate) fn canvas_frame(&self) -> Option<&super::canvas_frame::CanvasFrame> {
        self.canvas_frame.as_ref()
    }
}
