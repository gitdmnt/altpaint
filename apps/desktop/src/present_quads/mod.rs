//! デスクトップ固有の固定レイアウト計算と presenter 入力変換をまとめる。

mod canvas_plan;
mod geometry;
mod layer_dirty;
mod overlay_quad;
mod overlay_state;
mod solid_quad;
pub(crate) mod status_panel;
use ::geometry::WindowRect;
use crate::theme::{FOOTER_HEIGHT, HEADER_HEIGHT, WINDOW_PADDING};

pub(crate) use canvas_plan::CanvasPlan;
pub(crate) use geometry::fit_rect;
pub(crate) use layer_dirty::LayerDirtyAccumulator;
pub(crate) use overlay_quad::{
    CircleQuad, LineQuad, build_overlay_circle_quads, build_overlay_line_quads,
    build_overlay_solid_quads,
};
pub(crate) use overlay_state::{CanvasOverlayState, KomaNavigatorEntry, KomaNavigatorOverlay};
pub(crate) use solid_quad::{
    SolidQuad, build_background_solid_quads, build_foreground_solid_quads, pixel_rect_to_ndc,
};

pub(crate) type TextureQuad = canvas_geometry::TextureQuad;

/// デスクトップ UI の固定レイアウト情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopLayout {
    pub(crate) window_rect: WindowRect,
    pub(crate) canvas_host_rect: WindowRect,
    pub(crate) canvas_display_rect: WindowRect,
}

impl DesktopLayout {
    pub(crate) fn new(
        window_width: usize,
        window_height: usize,
        canvas_width: usize,
        canvas_height: usize,
    ) -> Self {
        let window_rect = WindowRect {
            x: 0,
            y: 0,
            width: window_width.max(1),
            height: window_height.max(1),
        };
        let canvas_host_rect = WindowRect {
            x: WINDOW_PADDING,
            y: WINDOW_PADDING + HEADER_HEIGHT + WINDOW_PADDING,
            width: window_width.saturating_sub(WINDOW_PADDING * 2).max(1),
            height: window_height
                .saturating_sub(HEADER_HEIGHT)
                .saturating_sub(FOOTER_HEIGHT)
                .saturating_sub(WINDOW_PADDING * 3)
                .max(1),
        };
        let canvas_display_rect =
            fit_rect(canvas_width.max(1), canvas_height.max(1), canvas_host_rect);

        Self {
            window_rect,
            canvas_host_rect,
            canvas_display_rect,
        }
    }
}

#[cfg(test)]
mod tests;
