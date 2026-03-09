//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! CPU 側ではパネル UI・背景・ステータス・オーバーレイを保持し、
//! キャンバス本体は GPU テクスチャとして別経路で提示する。

use super::{DesktopApp, PresentFrameUpdate};
use crate::frame::{
    CanvasCompositeSource, CanvasOverlayState, DesktopLayout, Rect, clear_canvas_host_region,
    compose_base_frame, compose_overlay_frame, compose_overlay_region,
    compose_panel_host_region, compose_status_region, status_text_bounds,
};
use crate::profiler::DesktopProfiler;

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
            profiler.measure("ui_update", || self.ui_shell.update(&self.document));
            self.needs_ui_sync = false;
        }

        let mut panel_surface_refreshed = false;
        if self.needs_panel_surface_refresh {
            let panel_surface_size = self
                .layout
                .as_ref()
                .map(|layout| {
                    (
                        layout.panel_surface_rect.width,
                        layout.panel_surface_rect.height,
                    )
                })
                .unwrap_or((1, 1));
            let panel_surface = profiler.measure("panel_surface", || {
                self.ui_shell
                    .render_panel_surface(panel_surface_size.0, panel_surface_size.1)
            });
            self.panel_surface = Some(panel_surface);
            self.needs_panel_surface_refresh = false;
            panel_surface_refreshed = true;
        }

        if self.needs_full_present_rebuild || self.base_frame.is_none() || self.overlay_frame.is_none()
        {
            let layout = self.layout.clone().expect("layout exists");
            let panel_surface = self.panel_surface.clone().unwrap_or_else(|| {
                self.ui_shell.render_panel_surface(
                    layout.panel_surface_rect.width,
                    layout.panel_surface_rect.height,
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
                    canvas_source,
                    self.document.view_transform,
                    CanvasOverlayState {
                        brush_preview: self.hover_canvas_position,
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
            profiler.measure("compose_dirty_panel", || {
                compose_panel_host_region(base_frame, &layout, panel_surface);
            });
            base_dirty_rect = Some(layout.panel_host_rect);
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
                base_dirty_rect.map_or(status_rect, |existing| existing.union(status_rect)),
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
                base_dirty_rect.map_or(dirty_rect, |existing| existing.union(dirty_rect)),
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
                    canvas_source,
                    self.document.view_transform,
                    CanvasOverlayState {
                        brush_preview: self.hover_canvas_position,
                    },
                    Some(dirty_rect),
                );
            });
            overlay_dirty_rect = Some(dirty_rect);
        }

        let canvas_dirty_rect = self.pending_canvas_dirty_rect.take();
        let canvas_transform_changed = std::mem::take(&mut self.pending_canvas_transform_update);
        if let Some(canvas_dirty_rect) = canvas_dirty_rect {
            let dirty = canvas_dirty_rect.clamp_to_bitmap(canvas_width, canvas_height);
            profiler.record_value(
                "canvas_upload_area_px",
                (dirty.width * dirty.height) as f64,
            );
            profiler.record_value("canvas_upload_width_px", dirty.width as f64);
            profiler.record_value("canvas_upload_height_px", dirty.height as f64);
        }
        if let Some(base_dirty_rect) = base_dirty_rect {
            profiler.record_value(
                "base_upload_area_px",
                (base_dirty_rect.width * base_dirty_rect.height) as f64,
            );
            profiler.record_value("base_upload_width_px", base_dirty_rect.width as f64);
            profiler.record_value("base_upload_height_px", base_dirty_rect.height as f64);
        }
        if let Some(overlay_dirty_rect) = overlay_dirty_rect {
            profiler.record_value(
                "overlay_upload_area_px",
                (overlay_dirty_rect.width * overlay_dirty_rect.height) as f64,
            );
            profiler.record_value("overlay_upload_width_px", overlay_dirty_rect.width as f64);
            profiler.record_value("overlay_upload_height_px", overlay_dirty_rect.height as f64);
        }
        if canvas_dirty_rect.is_some() || canvas_transform_changed {
            profiler.measure("prepare_canvas_scene", || {});
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
