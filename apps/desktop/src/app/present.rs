//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! レイアウト更新、UI シェル更新、パネル面再描画、キャンバス dirty rect 反映を
//! 一箇所で扱い、ランタイム側は出来上がったフレームだけを扱えるようにする。

use super::{DesktopApp, PresentFrameUpdate};
use crate::frame::{
    CanvasCompositeSource, CanvasOverlayState, DesktopLayout, blit_canvas_content,
    canvas_drawn_rect, clear_canvas_host_region, compose_desktop_frame, compose_panel_host_region,
    compose_status_region, draw_canvas_overlay, map_canvas_dirty_to_display_with_transform,
    scroll_canvas_region, status_text_rect,
};
use crate::profiler::DesktopProfiler;
use std::time::Instant;

impl DesktopApp {
    /// 現在状態から次に提示すべきフレームと差分更新情報を生成する。
    pub(crate) fn prepare_present_frame(
        &mut self,
        window_width: usize,
        window_height: usize,
        profiler: &mut DesktopProfiler,
    ) -> PresentFrameUpdate {
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

        if self.needs_full_present_rebuild || self.present_frame.is_none() {
            let layout = self.layout.clone().expect("layout exists");
            let panel_surface = self.panel_surface.clone().unwrap_or_else(|| {
                self.ui_shell.render_panel_surface(
                    layout.panel_surface_rect.width,
                    layout.panel_surface_rect.height,
                )
            });
            let status_text = self.status_text();
            let bitmap = self.document.active_bitmap();
            let present_frame = profiler.measure("compose_full_frame", || {
                compose_desktop_frame(
                    window_width,
                    window_height,
                    &layout,
                    &panel_surface,
                    CanvasCompositeSource {
                        width: bitmap.map_or(1, |bitmap| bitmap.width),
                        height: bitmap.map_or(1, |bitmap| bitmap.height),
                        pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
                    },
                    self.document.view_transform,
                    CanvasOverlayState {
                        brush_preview: self.hover_canvas_position,
                    },
                    &status_text,
                )
            });
            self.present_frame = Some(present_frame);
            self.pending_canvas_dirty_rect = None;
            self.pending_canvas_host_dirty_rect = None;
            self.needs_status_refresh = false;
            self.needs_full_present_rebuild = false;
            return PresentFrameUpdate {
                dirty_rect: None,
                canvas_updated: true,
            };
        }

        let layout = self.layout.clone().expect("layout exists");
        let status_text = self.needs_status_refresh.then(|| self.status_text());
        let Some(present_frame) = self.present_frame.as_mut() else {
            self.rebuild_present_frame();
            return PresentFrameUpdate {
                dirty_rect: None,
                canvas_updated: false,
            };
        };

        let mut dirty_rect = None;
        let mut canvas_updated = false;
        if panel_surface_refreshed && let Some(panel_surface) = self.panel_surface.as_ref() {
            profiler.measure("compose_dirty_panel", || {
                compose_panel_host_region(present_frame, &layout, panel_surface);
            });
            dirty_rect = Some(layout.panel_host_rect);
        }

        if let Some(status_text) = status_text.as_deref() {
            let status_rect = status_text_rect(window_width, window_height, &layout);
            profiler.measure("compose_dirty_status", || {
                compose_status_region(
                    present_frame,
                    window_width,
                    window_height,
                    &layout,
                    status_text,
                );
            });
            dirty_rect = Some(dirty_rect.map_or(status_rect, |existing| existing.union(status_rect)));
            self.needs_status_refresh = false;
        }

        let mut canvas_dirty_rect = self.pending_canvas_host_dirty_rect.take();
        let mut canvas_upload_rect = canvas_dirty_rect;
        if let Some((scroll_x, scroll_y)) = self.pending_canvas_scroll.take()
            && (scroll_x != 0 || scroll_y != 0)
        {
            let Some(bitmap) = self.document.active_bitmap() else {
                self.rebuild_present_frame();
                return PresentFrameUpdate {
                    dirty_rect: None,
                    canvas_updated: false,
                };
            };
            let previous_transform = app_core::CanvasViewTransform {
                pan_x: self.document.view_transform.pan_x - scroll_x as f32,
                pan_y: self.document.view_transform.pan_y - scroll_y as f32,
                ..self.document.view_transform
            };
            let old_drawn_rect = canvas_drawn_rect(
                layout.canvas_display_rect,
                bitmap.width,
                bitmap.height,
                previous_transform,
            );
            let new_drawn_rect = canvas_drawn_rect(
                layout.canvas_display_rect,
                bitmap.width,
                bitmap.height,
                self.document.view_transform,
            );

            if old_drawn_rect == Some(layout.canvas_display_rect)
                && new_drawn_rect == Some(layout.canvas_display_rect)
            {
                let exposed_rect = scroll_canvas_region(
                    present_frame,
                    layout.canvas_display_rect,
                    scroll_x,
                    scroll_y,
                );
                canvas_dirty_rect = Some(
                    canvas_dirty_rect.map_or(exposed_rect, |existing| existing.union(exposed_rect)),
                );
                canvas_upload_rect = Some(canvas_upload_rect.map_or(
                    layout.canvas_display_rect,
                    |existing| existing.union(layout.canvas_display_rect),
                ));
            } else {
                canvas_dirty_rect = Some(canvas_dirty_rect.map_or(
                    layout.canvas_display_rect,
                    |existing| existing.union(layout.canvas_display_rect),
                ));
                canvas_upload_rect = Some(canvas_upload_rect.map_or(
                    layout.canvas_display_rect,
                    |existing| existing.union(layout.canvas_display_rect),
                ));
            }
        }
        if let Some(dirty) = self.pending_canvas_dirty_rect.take() {
            let Some(bitmap) = self.document.active_bitmap() else {
                self.rebuild_present_frame();
                return PresentFrameUpdate {
                    dirty_rect: None,
                    canvas_updated: false,
                };
            };
            let mapped = map_canvas_dirty_to_display_with_transform(
                dirty,
                layout.canvas_display_rect,
                bitmap.width,
                bitmap.height,
                self.document.view_transform,
            );
            canvas_dirty_rect = Some(canvas_dirty_rect.map_or(mapped, |existing| existing.union(mapped)));
            canvas_upload_rect = Some(canvas_upload_rect.map_or(mapped, |existing| existing.union(mapped)));
        }

        if let Some(canvas_dirty_rect) = canvas_dirty_rect
            && canvas_dirty_rect.width > 0
            && canvas_dirty_rect.height > 0
        {
            profiler.record_value(
                "canvas_dirty_area_px",
                (canvas_dirty_rect.width * canvas_dirty_rect.height) as f64,
            );
            profiler.record_value("canvas_dirty_width_px", canvas_dirty_rect.width as f64);
            profiler.record_value("canvas_dirty_height_px", canvas_dirty_rect.height as f64);
            let Some(bitmap) = self.document.active_bitmap() else {
                self.rebuild_present_frame();
                return PresentFrameUpdate {
                    dirty_rect: None,
                    canvas_updated: false,
                };
            };
            let canvas_source = CanvasCompositeSource {
                width: bitmap.width,
                height: bitmap.height,
                pixels: bitmap.pixels.as_slice(),
            };
            let overlay = CanvasOverlayState {
                brush_preview: self.hover_canvas_position,
            };
            let compose_started = Instant::now();
            profiler.measure("compose_canvas_clear", || {
                clear_canvas_host_region(
                    present_frame,
                    &layout,
                    canvas_source,
                    self.document.view_transform,
                    Some(canvas_dirty_rect),
                );
            });
            profiler.measure("compose_canvas_blit", || {
                blit_canvas_content(
                    present_frame,
                    &layout,
                    canvas_source,
                    self.document.view_transform,
                    Some(canvas_dirty_rect),
                );
            });
            profiler.measure("compose_canvas_overlay", || {
                draw_canvas_overlay(
                    present_frame,
                    &layout,
                    canvas_source,
                    self.document.view_transform,
                    overlay,
                    Some(canvas_dirty_rect),
                );
            });
            profiler.record("compose_dirty_canvas", compose_started.elapsed());
            canvas_updated = true;
            let upload_rect = canvas_upload_rect.unwrap_or(canvas_dirty_rect);
            dirty_rect = Some(dirty_rect.map_or(upload_rect, |existing| existing.union(upload_rect)));
        }

        PresentFrameUpdate {
            dirty_rect,
            canvas_updated,
        }
    }
}
