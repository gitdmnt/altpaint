//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! CPU 側ではパネル UI・背景・ステータス・オーバーレイを保持し、
//! キャンバス本体は GPU テクスチャとして別経路で提示する。

use app_core::ClampToCanvasBounds;
use desktop_support::DesktopProfiler;

use super::{DesktopApp, PresentFrameUpdate};
use crate::frame::DesktopLayout;

impl DesktopApp {
    /// Prepare 提示 フレーム に必要な差分領域だけを描画または合成する。
    ///
    /// 必要に応じて dirty 状態も更新します。
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

        if self.needs_ui_sync {
            let synced_panels = if self.ui_sync_panel_ids.is_empty() {
                self.panel_runtime.panel_count()
            } else {
                self.ui_sync_panel_ids.len()
            };
            profiler.record_value("ui_update_panels", synced_panels as f64);
            profiler.measure("ui_update", || {
                let can_undo = self.history.can_undo();
                let can_redo = self.history.can_redo();
                let active_jobs = self.io_state.pending_jobs.len();
                if self.ui_sync_panel_ids.is_empty() {
                    let changed = self.panel_runtime.sync_document(
                        &self.document,
                        can_undo,
                        can_redo,
                        active_jobs,
                    );
                    self.panel_presentation
                        .reconcile_runtime_panels(&self.panel_runtime);
                    if !changed.is_empty() {
                        self.panel_presentation.mark_runtime_panels_dirty(&changed);
                        self.mark_panel_surface_dirty();
                    }
                } else {
                    let changed = self.panel_runtime.sync_document_panels(
                        &self.document,
                        &self.ui_sync_panel_ids,
                        can_undo,
                        can_redo,
                        active_jobs,
                    );
                    self.panel_presentation
                        .reconcile_runtime_panels(&self.panel_runtime);
                    if !changed.is_empty() {
                        self.panel_presentation.mark_runtime_panels_dirty(&changed);
                        self.mark_panel_surface_dirty();
                    }
                }
            });
            self.needs_ui_sync = false;
            self.ui_sync_panel_ids.clear();
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

        if self.needs_full_present_rebuild
            || self.base_frame.is_none()
            || self.overlay_frame.is_none()
        {
            let layout = self.layout.clone().expect("layout exists");
            let panel_surface = self.panel_surface.as_ref().expect("panel surface exists");
            let status_text = self.status_text();
            let bitmap = self.canvas_frame.as_ref();
            let canvas_source = render::CanvasCompositeSource {
                width: bitmap.map_or(1, |bitmap| bitmap.width),
                height: bitmap.map_or(1, |bitmap| bitmap.height),
                pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
            };
            let panel_surface_source = render::PanelSurfaceSource {
                x: panel_surface.x,
                y: panel_surface.y,
                width: panel_surface.width,
                height: panel_surface.height,
                pixels: panel_surface.pixels.as_slice(),
            };
            let frame_plan = render::FramePlan::new(
                window_width,
                window_height,
                layout.canvas_host_rect,
                panel_surface_source,
                canvas_source,
                self.document.view_transform,
                &status_text,
            );
            let overlay_state = render::CanvasOverlayState {
                brush_preview: self.hover_canvas_position,
                brush_size: self.brush_preview_size(),
                lasso_points: self.canvas_input.lasso_points.clone(),
                active_panel_bounds: self.active_panel_mask_overlay(),
                panel_navigator: self.panel_navigator_overlay(),
                panel_creation_preview: self.panel_creation_preview_bounds(),
            };
            let base_frame = profiler.measure("compose_base_frame", || {
                render::compose_base_frame(&frame_plan)
            });
            let overlay_frame = profiler.measure("compose_overlay_frame", || {
                render::compose_overlay_frame(&frame_plan, &overlay_state)
            });
            self.base_frame = Some(base_frame);
            self.overlay_frame = Some(overlay_frame);
            self.pending_canvas_dirty_rect = None;
            self.pending_canvas_background_dirty_rect = None;
            self.pending_canvas_host_dirty_rect = None;
            self.pending_canvas_transform_update = false;
            self.needs_status_refresh = false;
            self.needs_full_present_rebuild = false;
            let window_rect = frame_plan.window_rect();
            return PresentFrameUpdate {
                base_dirty_rect: Some(window_rect),
                overlay_dirty_rect: Some(window_rect),
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

        let layout = self.layout.clone().expect("layout exists");
        let status_text = self.needs_status_refresh.then(|| self.status_text());
        let brush_preview_size = self.brush_preview_size();
        let hover_canvas_position = self.hover_canvas_position;
        let lasso_points = self.canvas_input.lasso_points.clone();
        let active_panel_bounds = self.active_panel_mask_overlay();
        let panel_navigator = self.panel_navigator_overlay();
        let panel_creation_preview = self.panel_creation_preview_bounds();
        let Some(base_frame) = self.base_frame.as_mut() else {
            self.rebuild_present_frame();
            return PresentFrameUpdate::default();
        };
        let Some(overlay_frame) = self.overlay_frame.as_mut() else {
            self.rebuild_present_frame();
            return PresentFrameUpdate::default();
        };

        let mut dirty_plan = render::DirtyFramePlan::default();

        let canvas_source = render::CanvasCompositeSource {
            width: self.canvas_frame.as_ref().map_or(1, |bitmap| bitmap.width),
            height: self.canvas_frame.as_ref().map_or(1, |bitmap| bitmap.height),
            pixels: self
                .canvas_frame
                .as_ref()
                .map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
        };
        let panel_surface = self.panel_surface.as_ref().expect("panel surface exists");
        let panel_surface_source = render::PanelSurfaceSource {
            x: panel_surface.x,
            y: panel_surface.y,
            width: panel_surface.width,
            height: panel_surface.height,
            pixels: panel_surface.pixels.as_slice(),
        };
        let frame_status_text = status_text.as_deref().unwrap_or("");
        let frame_plan = render::FramePlan::new(
            window_width,
            window_height,
            layout.canvas_host_rect,
            panel_surface_source,
            canvas_source,
            self.document.view_transform,
            frame_status_text,
        );
        let overlay_state = render::CanvasOverlayState {
            brush_preview: hover_canvas_position,
            brush_size: brush_preview_size,
            lasso_points: lasso_points.clone(),
            active_panel_bounds,
            panel_navigator: panel_navigator.clone(),
            panel_creation_preview,
        };

        if panel_surface_refreshed && let Some(panel_surface) = self.panel_surface.as_ref() {
            let panel_dirty_rect = self.panel_presentation.last_panel_surface_dirty_rect();
            profiler.measure("compose_dirty_panel", || {
                let _ = panel_surface;
                render::compose_overlay_region(
                    overlay_frame,
                    &frame_plan,
                    &overlay_state,
                    panel_dirty_rect,
                );
            });
            if let Some(panel_dirty_rect) = panel_dirty_rect {
                dirty_plan.mark_overlay(panel_dirty_rect);
            }
        }

        if let Some(status_text) = status_text.as_deref() {
            let status_plan = render::FramePlan::new(
                window_width,
                window_height,
                layout.canvas_host_rect,
                panel_surface_source,
                canvas_source,
                self.document.view_transform,
                status_text,
            );
            let status_rect = render::status_text_bounds(
                window_width,
                window_height,
                layout.canvas_host_rect,
                status_text,
            );
            profiler.measure("compose_dirty_status", || {
                render::compose_status_region(base_frame, &status_plan);
            });
            dirty_plan.mark_base(status_rect);
            self.needs_status_refresh = false;
        }

        if let Some(dirty_rect) = self.pending_canvas_background_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            profiler.measure("compose_dirty_canvas_base", || {
                render::clear_canvas_host_region(base_frame, &frame_plan, Some(dirty_rect));
            });
            dirty_plan.mark_base(dirty_rect);
        }

        if let Some(dirty_rect) = self.pending_canvas_host_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            profiler.measure("compose_dirty_overlay", || {
                render::compose_overlay_region(
                    overlay_frame,
                    &frame_plan,
                    &overlay_state,
                    Some(dirty_rect),
                );
            });
            dirty_plan.mark_overlay(dirty_rect);
        }

        let canvas_dirty_rect = self.pending_canvas_dirty_rect.take();
        let canvas_transform_changed = std::mem::take(&mut self.pending_canvas_transform_update);
        if let Some(canvas_dirty_rect) = canvas_dirty_rect {
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
        if let Some(base_dirty_rect) = dirty_plan.base_dirty_rect {
            let window_area = (window_width.max(1) * window_height.max(1)) as f64;
            profiler.record_value(
                "base_upload_area_px",
                (base_dirty_rect.width * base_dirty_rect.height) as f64,
            );
            profiler.record_value("base_upload_width_px", base_dirty_rect.width as f64);
            profiler.record_value("base_upload_height_px", base_dirty_rect.height as f64);
            profiler.record_value(
                "base_upload_coverage_pct",
                ((base_dirty_rect.width * base_dirty_rect.height) as f64 / window_area) * 100.0,
            );
        }
        if let Some(overlay_dirty_rect) = dirty_plan.overlay_dirty_rect {
            let window_area = (window_width.max(1) * window_height.max(1)) as f64;
            profiler.record_value(
                "overlay_upload_area_px",
                (overlay_dirty_rect.width * overlay_dirty_rect.height) as f64,
            );
            profiler.record_value("overlay_upload_width_px", overlay_dirty_rect.width as f64);
            profiler.record_value("overlay_upload_height_px", overlay_dirty_rect.height as f64);
            profiler.record_value(
                "overlay_upload_coverage_pct",
                ((overlay_dirty_rect.width * overlay_dirty_rect.height) as f64 / window_area)
                    * 100.0,
            );
        }
        if canvas_dirty_rect.is_some() || canvas_transform_changed {
            profiler.measure("prepare_canvas_scene", || {
                let _ = self.canvas_scene();
            });
        }

        PresentFrameUpdate {
            base_dirty_rect: dirty_plan.base_dirty_rect,
            overlay_dirty_rect: dirty_plan.overlay_dirty_rect,
            canvas_dirty_rect,
            canvas_transform_changed,
            canvas_updated: canvas_dirty_rect.is_some() || canvas_transform_changed,
        }
    }
}
