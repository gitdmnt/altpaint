//! `DesktopApp` のキャンバスフレーム状態と overlay 補助を定義する。

use app_core::{CanvasDirtyRect, ClampToCanvasBounds};

use super::DesktopApp;
use render::{PanelNavigatorEntry, PanelNavigatorOverlay};

impl DesktopApp {
    pub(super) fn brush_preview_size(&self) -> Option<u32> {
        match self.document.active_tool {
            app_core::ToolKind::Pen | app_core::ToolKind::Eraser => {
                Some(self.document.active_pen_size.max(1))
            }
            app_core::ToolKind::Bucket
            | app_core::ToolKind::LassoBucket
            | app_core::ToolKind::PanelRect => None,
        }
    }

    pub(super) fn refresh_canvas_frame(&mut self) {
        self.canvas_frame = Some(render::RenderContext::new().render_frame(&self.document));
    }

    pub(super) fn refresh_canvas_frame_region(&mut self, dirty: CanvasDirtyRect) {
        let Some(frame) = self.canvas_frame.as_mut() else {
            self.refresh_canvas_frame();
            return;
        };
        let Some(page) = self.document.active_page() else {
            self.refresh_canvas_frame();
            return;
        };
        let Some(panel) = self.document.active_panel() else {
            self.refresh_canvas_frame();
            return;
        };

        if frame.width != page.width.max(1) || frame.height != page.height.max(1) {
            self.refresh_canvas_frame();
            return;
        }

        let dirty = dirty.clamp_to_canvas_bounds(frame.width, frame.height);
        if dirty.width == 0 || dirty.height == 0 {
            return;
        }

        let panel_bounds = panel.bounds;
        let panel_right = panel_bounds.x.saturating_add(panel.bitmap.width);
        let panel_bottom = panel_bounds.y.saturating_add(panel.bitmap.height);
        let dirty_right = dirty.x.saturating_add(dirty.width);
        let dirty_bottom = dirty.y.saturating_add(dirty.height);
        let copy_left = dirty.x.max(panel_bounds.x);
        let copy_top = dirty.y.max(panel_bounds.y);
        let copy_right = dirty_right.min(panel_right).min(frame.width);
        let copy_bottom = dirty_bottom.min(panel_bottom).min(frame.height);

        if copy_left >= copy_right || copy_top >= copy_bottom {
            return;
        }

        let copy_width = copy_right - copy_left;
        for row in copy_top..copy_bottom {
            let local_y = row.saturating_sub(panel_bounds.y);
            let local_x = copy_left.saturating_sub(panel_bounds.x);
            let src_row_start = (local_y * panel.bitmap.width + local_x) * 4;
            let src_row_end = src_row_start + copy_width * 4;
            let dst_row_start = (row * frame.width + copy_left) * 4;
            let dst_row_end = dst_row_start + copy_width * 4;
            frame.pixels[dst_row_start..dst_row_end]
                .copy_from_slice(&panel.bitmap.pixels[src_row_start..src_row_end]);
        }
    }

    pub(super) fn active_panel_mask_overlay(&self) -> Option<app_core::PanelBounds> {
        let page = self.document.active_page()?;
        let bounds = self.document.active_panel_bounds()?;
        (page.panels.len() > 1 || bounds != app_core::PanelBounds::full_page(page.width, page.height))
            .then_some(bounds)
    }

    pub(super) fn panel_creation_preview_bounds(&self) -> Option<app_core::PanelBounds> {
        let (page_width, page_height) = self.document.active_page_dimensions();
        canvas::panel_creation_preview_bounds(&self.canvas_input, page_width, page_height)
    }

    pub(super) fn panel_navigator_overlay(&self) -> Option<PanelNavigatorOverlay> {
        let page = self.document.active_page()?;
        (page.panels.len() > 1).then(|| PanelNavigatorOverlay {
            page_width: page.width,
            page_height: page.height,
            panels: page
                .panels
                .iter()
                .enumerate()
                .map(|(index, panel)| PanelNavigatorEntry {
                    bounds: panel.bounds,
                    active: index == self.document.active_panel_index(),
                })
                .collect(),
        })
    }

    /// アクティブビットマップ寸法を返す。
    pub(super) fn canvas_dimensions(&self) -> (usize, usize) {
        self.canvas_frame
            .as_ref()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    /// キャンバス描画中かどうかを返す。
    pub(crate) fn is_canvas_interacting(&self) -> bool {
        self.canvas_input.is_drawing
    }
}
