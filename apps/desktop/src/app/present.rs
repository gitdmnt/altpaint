//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! レイアウト更新、UI シェル更新、パネル面再描画、キャンバス dirty rect 反映を
//! 一箇所で扱い、ランタイム側は出来上がったフレームだけを扱えるようにする。

use super::{DesktopApp, PresentFrameUpdate};
use crate::frame::{
    CanvasCompositeSource, DesktopLayout, blit_scaled_rgba_region, compose_desktop_frame,
    compose_panel_host_region, compose_status_region, map_canvas_dirty_to_display, status_text_rect,
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
                    &status_text,
                )
            });
            self.present_frame = Some(present_frame);
            self.pending_canvas_dirty_rect = None;
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

        if let Some(dirty) = self.pending_canvas_dirty_rect.take() {
            let Some(bitmap) = self.document.active_bitmap() else {
                self.rebuild_present_frame();
                return PresentFrameUpdate {
                    dirty_rect: None,
                    canvas_updated: false,
                };
            };
            let canvas_dirty_rect = map_canvas_dirty_to_display(
                dirty,
                layout.canvas_display_rect,
                bitmap.width,
                bitmap.height,
            );
            profiler.measure("compose_dirty_canvas", || {
                blit_scaled_rgba_region(
                    present_frame,
                    layout.canvas_display_rect,
                    bitmap.width,
                    bitmap.height,
                    bitmap.pixels.as_slice(),
                    Some(canvas_dirty_rect),
                );
            });
            canvas_updated = true;
            dirty_rect = Some(dirty_rect.map_or(canvas_dirty_rect, |existing| {
                existing.union(canvas_dirty_rect)
            }));
        }

        PresentFrameUpdate {
            dirty_rect,
            canvas_updated,
        }
    }
}
