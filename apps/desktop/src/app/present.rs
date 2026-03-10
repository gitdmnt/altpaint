//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! CPU 側ではパネル UI・背景・ステータス・オーバーレイを保持し、
//! キャンバス本体は GPU テクスチャとして別経路で提示する。

use desktop_support::DesktopProfiler;

use super::{DesktopApp, PresentFrameUpdate};
use crate::frame::{
    CanvasCompositeSource, CanvasOverlayState, DesktopLayout, Rect, clear_canvas_host_region,
    compose_base_frame, compose_overlay_frame, compose_overlay_region, compose_status_region,
    status_text_bounds,
};

impl DesktopApp {
    /// 現在状態から次に提示すべきフレームと差分更新情報を生成する。
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
                self.ui_shell.panel_count()
            } else {
                self.ui_sync_panel_ids.len()
            };
            profiler.record_value("ui_update_panels", synced_panels as f64);
            profiler.measure("ui_update", || {
                if self.ui_sync_panel_ids.is_empty() {
                    self.ui_shell.update(&self.document);
                } else {
                    self.ui_shell
                        .update_panels(&self.document, &self.ui_sync_panel_ids);
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
                self.ui_shell
                    .render_panel_surface(panel_surface_size.0, panel_surface_size.1)
            });
            let window_area = (window_width.max(1) * window_height.max(1)) as f64;
            profiler.record_value("panel_surface_buffer_area_px", (panel_surface.width * panel_surface.height) as f64);
            profiler.record_value("panel_surface_buffer_width_px", panel_surface.width as f64);
            profiler.record_value("panel_surface_buffer_height_px", panel_surface.height as f64);
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
                self.ui_shell.last_panel_rasterized_panels() as f64,
            );
            profiler.record_value(
                "panel_surface_composited_panels",
                self.ui_shell.last_panel_composited_panels() as f64,
            );
            profiler.record_value(
                "panel_surface_raster_ms",
                self.ui_shell.last_panel_raster_duration_ms(),
            );
            profiler.record_value(
                "panel_surface_compose_ms",
                self.ui_shell.last_panel_compose_duration_ms(),
            );
            self.panel_surface = Some(panel_surface);
            self.needs_panel_surface_refresh = false;
            panel_surface_refreshed = true;
        }

        if self.needs_full_present_rebuild || self.base_frame.is_none() || self.overlay_frame.is_none()
        {
            let layout = self.layout.clone().expect("layout exists");
            let panel_surface = self.panel_surface.clone().unwrap_or_else(|| {
                self.ui_shell.render_panel_surface(
                    layout.window_rect.width,
                    layout.window_rect.height,
                )
            });
            let status_text = self.status_text();
            let bitmap = self.document.active_bitmap();
            let canvas_source = CanvasCompositeSource {
                width: bitmap.map_or(1, |bitmap| bitmap.width),
                height: bitmap.map_or(1, |bitmap| bitmap.height),
                pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
            };
            let base_frame = profiler.measure("compose_base_frame", || {
                compose_base_frame(
                    window_width,
                    window_height,
                    &layout,
                    &panel_surface,
                    canvas_source,
                    self.document.view_transform,
                    &status_text,
                )
            });
            let overlay_frame = profiler.measure("compose_overlay_frame", || {
                compose_overlay_frame(
                    window_width,
                    window_height,
                    &layout,
                    &panel_surface,
                    canvas_source,
                    self.document.view_transform,
                    CanvasOverlayState {
                        brush_preview: self.hover_canvas_position,
                        lasso_points: self.canvas_input.lasso_points.clone(),
                    },
                )
            });
            self.base_frame = Some(base_frame);
            self.overlay_frame = Some(overlay_frame);
            self.pending_canvas_dirty_rect = None;
            self.pending_canvas_background_dirty_rect = None;
            self.pending_canvas_host_dirty_rect = None;
            self.pending_canvas_transform_update = false;
            self.needs_status_refresh = false;
            self.needs_full_present_rebuild = false;
            let window_rect = Rect {
                x: 0,
                y: 0,
                width: window_width,
                height: window_height,
            };
            return PresentFrameUpdate {
                base_dirty_rect: Some(window_rect),
                overlay_dirty_rect: Some(window_rect),
                canvas_dirty_rect: bitmap.map(|bitmap| app_core::DirtyRect {
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
        let Some(base_frame) = self.base_frame.as_mut() else {
            self.rebuild_present_frame();
            return PresentFrameUpdate::default();
        };
        let Some(overlay_frame) = self.overlay_frame.as_mut() else {
            self.rebuild_present_frame();
            return PresentFrameUpdate::default();
        };

        let mut base_dirty_rect = None;
        let mut overlay_dirty_rect = None;

        if panel_surface_refreshed && let Some(panel_surface) = self.panel_surface.as_ref() {
            let panel_dirty_rect = self.ui_shell.last_panel_surface_dirty_rect().map(|dirty| Rect {
                x: dirty.x,
                y: dirty.y,
                width: dirty.width,
                height: dirty.height,
            });
            let bitmap = self.document.active_bitmap();
            let canvas_source = CanvasCompositeSource {
                width: bitmap.map_or(1, |bitmap| bitmap.width),
                height: bitmap.map_or(1, |bitmap| bitmap.height),
                pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
            };
            profiler.measure("compose_dirty_panel", || {
                compose_overlay_region(
                    overlay_frame,
                    &layout,
                    panel_surface,
                    canvas_source,
                    self.document.view_transform,
                    CanvasOverlayState {
                        brush_preview: self.hover_canvas_position,
                        lasso_points: self.canvas_input.lasso_points.clone(),
                    },
                    panel_dirty_rect,
                );
            });
            if let Some(panel_dirty_rect) = panel_dirty_rect {
                overlay_dirty_rect = Some(
                    overlay_dirty_rect
                        .map_or(panel_dirty_rect, |existing: Rect| existing.union(panel_dirty_rect)),
                );
            }
        }

        if let Some(status_text) = status_text.as_deref() {
            let status_rect = status_text_bounds(window_width, window_height, &layout, status_text);
            profiler.measure("compose_dirty_status", || {
                compose_status_region(
                    base_frame,
                    window_width,
                    window_height,
                    &layout,
                    status_text,
                );
            });
            base_dirty_rect = Some(
                base_dirty_rect.map_or(status_rect, |existing: Rect| existing.union(status_rect)),
            );
            self.needs_status_refresh = false;
        }

        if let Some(dirty_rect) = self.pending_canvas_background_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            let bitmap = self.document.active_bitmap();
            let canvas_source = CanvasCompositeSource {
                width: bitmap.map_or(1, |bitmap| bitmap.width),
                height: bitmap.map_or(1, |bitmap| bitmap.height),
                pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
            };
            profiler.measure("compose_dirty_canvas_base", || {
                clear_canvas_host_region(
                    base_frame,
                    &layout,
                    canvas_source,
                    self.document.view_transform,
                    Some(dirty_rect),
                );
            });
            base_dirty_rect = Some(
                base_dirty_rect.map_or(dirty_rect, |existing: Rect| existing.union(dirty_rect)),
            );
        }

        if let Some(dirty_rect) = self.pending_canvas_host_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            let bitmap = self.document.active_bitmap();
            let canvas_source = CanvasCompositeSource {
                width: bitmap.map_or(1, |bitmap| bitmap.width),
                height: bitmap.map_or(1, |bitmap| bitmap.height),
                pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
            };
            profiler.measure("compose_dirty_overlay", || {
                compose_overlay_region(
                    overlay_frame,
                    &layout,
                    self.panel_surface.as_ref().expect("panel surface exists"),
                    canvas_source,
                    self.document.view_transform,
                    CanvasOverlayState {
                        brush_preview: self.hover_canvas_position,
                        lasso_points: self.canvas_input.lasso_points.clone(),
                    },
                    Some(dirty_rect),
                );
            });
            overlay_dirty_rect = Some(
                overlay_dirty_rect.map_or(dirty_rect, |existing: Rect| existing.union(dirty_rect)),
            );
        }

        let canvas_dirty_rect = self.pending_canvas_dirty_rect.take();
        let canvas_transform_changed = std::mem::take(&mut self.pending_canvas_transform_update);
        if let Some(canvas_dirty_rect) = canvas_dirty_rect {
            let dirty = canvas_dirty_rect.clamp_to_bitmap(canvas_width, canvas_height);
            let canvas_area = (canvas_width.max(1) * canvas_height.max(1)) as f64;
            profiler.record_value(
                "canvas_upload_area_px",
                (dirty.width * dirty.height) as f64,
            );
            profiler.record_value("canvas_upload_width_px", dirty.width as f64);
            profiler.record_value("canvas_upload_height_px", dirty.height as f64);
            profiler.record_value(
                "canvas_upload_coverage_pct",
                ((dirty.width * dirty.height) as f64 / canvas_area) * 100.0,
            );
        }
        if let Some(base_dirty_rect) = base_dirty_rect {
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
        if let Some(overlay_dirty_rect) = overlay_dirty_rect {
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
            base_dirty_rect,
            overlay_dirty_rect,
            canvas_dirty_rect,
            canvas_transform_changed,
            canvas_updated: canvas_dirty_rect.is_some() || canvas_transform_changed,
        }
    }
}
