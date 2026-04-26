//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! Phase 9E-4 以降、L1 background_frame の CPU 合成は撤去済み。
//! ステータステキストは `StatusPanel` (HtmlPanelEngine + GPU) に置換され、
//! L1 base_layer は 1×1 dummy で送る。L4 ui_panel_layer も同様の dummy。

use std::time::Instant;

use desktop_support::DesktopProfiler;

use super::{DesktopApp, PresentFrameUpdate};
use crate::frame::DesktopLayout;

impl DesktopApp {
    /// Prepare 提示 フレーム に必要な差分領域だけを描画または合成する。
    pub(crate) fn prepare_present_frame(
        &mut self,
        window_width: usize,
        window_height: usize,
        profiler: &mut DesktopProfiler,
    ) -> PresentFrameUpdate {
        self.poll_background_tasks();
        let (canvas_width, canvas_height) = self.canvas_dimensions();
        let next_layout = profiler.measure("layout", || {
            DesktopLayout::new(window_width, window_height, canvas_width, canvas_height)
        });

        if self.layout.as_ref() != Some(&next_layout) {
            self.layout = Some(next_layout.clone());
            self.mark_panel_surface_dirty();
            self.rebuild_present_frame();
        }

        if self.panel_runtime.has_dirty_panels() {
            profiler.record_value("ui_update_panels", self.panel_runtime.dirty_panel_count() as f64);
            let can_undo = self.history.can_undo();
            let can_redo = self.history.can_redo();
            let active_jobs = self.io_state.pending_jobs.len();
            let snapshot_count = self.snapshots.len();
            let sync_t = Instant::now();
            let changed = self.panel_runtime.sync_dirty_panels(
                &self.document,
                can_undo,
                can_redo,
                active_jobs,
                snapshot_count,
            );
            profiler.record("ui_sync_panels", sync_t.elapsed());
            let reconcile_t = Instant::now();
            self.panel_presentation
                .reconcile_runtime_panels(&self.panel_runtime);
            profiler.record("ui_reconcile", reconcile_t.elapsed());
            if !changed.is_empty() {
                self.panel_presentation.mark_runtime_panels_dirty(&changed);
                self.mark_panel_surface_dirty();
            }
        }

        let mut panel_surface_refreshed = false;
        if self.needs_panel_surface_refresh {
            let panel_surface_size = self
                .layout
                .as_ref()
                .map(|layout| (layout.window_rect.width, layout.window_rect.height))
                .unwrap_or((1, 1));
            let panel_surface = profiler.measure("panel_surface", || {
                self.panel_presentation.render_panel_surface(
                    &self.panel_runtime,
                    panel_surface_size.0,
                    panel_surface_size.1,
                )
            });
            let window_area = (window_width.max(1) * window_height.max(1)) as f64;
            profiler.record_value(
                "panel_surface_buffer_area_px",
                (panel_surface.width * panel_surface.height) as f64,
            );
            profiler.record_value("panel_surface_buffer_width_px", panel_surface.width as f64);
            profiler.record_value(
                "panel_surface_buffer_height_px",
                panel_surface.height as f64,
            );
            profiler.record_value(
                "panel_surface_window_coverage_pct",
                ((panel_surface.width * panel_surface.height) as f64 / window_area) * 100.0,
            );
            profiler.record_value(
                "panel_surface_hit_regions",
                panel_surface.hit_region_count() as f64,
            );
            profiler.record_value(
                "panel_surface_rasterized_panels",
                self.panel_presentation.last_panel_rasterized_panels() as f64,
            );
            profiler.record_value(
                "panel_surface_composited_panels",
                self.panel_presentation.last_panel_composited_panels() as f64,
            );
            profiler.record_value(
                "panel_surface_raster_ms",
                self.panel_presentation.last_panel_raster_duration_ms(),
            );
            profiler.record_value(
                "panel_surface_compose_ms",
                self.panel_presentation.last_panel_compose_duration_ms(),
            );
            self.panel_surface = Some(panel_surface);
            self.needs_panel_surface_refresh = false;
            panel_surface_refreshed = true;
        }

        if self.needs_full_present_rebuild || self.ui_panel_frame.is_none() {
            // Phase 9E-4: L4 ui_panel_layer は GPU パネル化により全パネルが quad で描画される。
            // ただし PresentScene::ui_panel_layer 型は Phase 9F まで残存するため 1×1 透明 dummy を渡す。
            let dummy = render::RenderFrame {
                width: 1,
                height: 1,
                pixels: vec![0; 4],
            };
            self.ui_panel_frame = Some(dummy);
            self.pending_canvas_dirty_rect = None;
            self.pending_temp_overlay_dirty_rect = None;
            self.pending_ui_panel_dirty_rect = None;
            self.pending_canvas_transform_update = false;
            self.needs_status_refresh = false;
            self.needs_full_present_rebuild = false;
            let bitmap = self.canvas_frame.as_ref();
            let window_rect = render_types::PixelRect {
                x: 0,
                y: 0,
                width: window_width,
                height: window_height,
            };
            return PresentFrameUpdate {
                background_dirty_rect: Some(window_rect),
                temp_overlay_dirty_rect: Some(window_rect),
                ui_panel_dirty_rect: Some(window_rect),
                canvas_dirty_rect: bitmap.map(|bitmap| app_core::CanvasDirtyRect {
                    x: 0,
                    y: 0,
                    width: bitmap.width,
                    height: bitmap.height,
                }),
                canvas_transform_changed: true,
                canvas_updated: true,
            };
        }

        let mut layer_dirty = render_types::LayerGroupDirtyPlan::default();

        // L4: パネルサーフェス更新 — Phase 9E-3 で GPU 経路に移行済み。dummy ui_panel_frame
        // は 1×1 のまま再アップロードするだけ。
        if panel_surface_refreshed {
            let panel_dirty_rect = self.panel_presentation.last_panel_surface_dirty_rect();
            if let Some(panel_dirty_rect) = panel_dirty_rect {
                layer_dirty.mark_ui_panel(panel_dirty_rect);
            }
        }

        // L1: ステータス更新 — HtmlPanelEngine 化されたため、毎フレーム
        // status_panel.update() を呼んで snapshot を engine に流す（差分なら no-op）。
        // 実際の GPU 描画は runtime.rs の RedrawRequested で行う。
        if self.needs_status_refresh {
            self.needs_status_refresh = false;
        }

        // L3: 一時オーバーレイは GPU quad で毎フレーム描画されるため CPU 合成は不要。
        if let Some(dirty_rect) = self.pending_temp_overlay_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            layer_dirty.mark_temp_overlay(dirty_rect);
        }

        // L4: UIパネル dirty
        if let Some(dirty_rect) = self.pending_ui_panel_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            layer_dirty.mark_ui_panel(dirty_rect);
        }

        let canvas_dirty_rect = self.pending_canvas_dirty_rect.take();
        let canvas_transform_changed = std::mem::take(&mut self.pending_canvas_transform_update);
        if let Some(canvas_dirty_rect) = canvas_dirty_rect {
            use app_core::ClampToCanvasBounds;
            let dirty = canvas_dirty_rect.clamp_to_canvas_bounds(canvas_width, canvas_height);
            let canvas_area = (canvas_width.max(1) * canvas_height.max(1)) as f64;
            profiler.record_value("canvas_upload_area_px", (dirty.width * dirty.height) as f64);
            profiler.record_value("canvas_upload_width_px", dirty.width as f64);
            profiler.record_value("canvas_upload_height_px", dirty.height as f64);
            profiler.record_value(
                "canvas_upload_coverage_pct",
                ((dirty.width * dirty.height) as f64 / canvas_area) * 100.0,
            );
        }
        if let Some(temp_overlay_dirty_rect) = layer_dirty.temp_overlay {
            let window_area = (window_width.max(1) * window_height.max(1)) as f64;
            profiler.record_value(
                "overlay_upload_area_px",
                (temp_overlay_dirty_rect.width * temp_overlay_dirty_rect.height) as f64,
            );
            profiler.record_value(
                "overlay_upload_width_px",
                temp_overlay_dirty_rect.width as f64,
            );
            profiler.record_value(
                "overlay_upload_height_px",
                temp_overlay_dirty_rect.height as f64,
            );
            profiler.record_value(
                "overlay_upload_coverage_pct",
                ((temp_overlay_dirty_rect.width * temp_overlay_dirty_rect.height) as f64
                    / window_area)
                    * 100.0,
            );
        }
        if canvas_dirty_rect.is_some() || canvas_transform_changed {
            profiler.measure("prepare_canvas_scene", || {
                let _ = self.canvas_scene();
            });
        }

        PresentFrameUpdate {
            background_dirty_rect: layer_dirty.background,
            temp_overlay_dirty_rect: layer_dirty.temp_overlay,
            ui_panel_dirty_rect: layer_dirty.ui_panel,
            canvas_dirty_rect,
            canvas_transform_changed,
            canvas_updated: canvas_dirty_rect.is_some() || canvas_transform_changed,
        }
    }
}
