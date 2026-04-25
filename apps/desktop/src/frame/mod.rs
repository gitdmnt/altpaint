//! デスクトップ固有の固定レイアウト計算と presenter 入力変換をまとめる。

mod geometry;
use desktop_support::{FOOTER_HEIGHT, HEADER_HEIGHT, WINDOW_PADDING};

#[allow(unused_imports)]
pub(crate) use geometry::{
    fit_rect, map_window_to_panel_surface, map_window_to_panel_surface_clamped,
};

pub(crate) type Rect = render_types::PixelRect;
pub(crate) type TextureQuad = render_types::TextureQuad;

/// デスクトップ UI の固定レイアウト情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopLayout {
    pub(crate) window_rect: Rect,
    pub(crate) panel_host_rect: Rect,
    pub(crate) panel_surface_rect: Rect,
    pub(crate) canvas_host_rect: Rect,
    pub(crate) canvas_display_rect: Rect,
}

impl DesktopLayout {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub(crate) fn new(
        window_width: usize,
        window_height: usize,
        canvas_width: usize,
        canvas_height: usize,
    ) -> Self {
        let window_rect = Rect {
            x: 0,
            y: 0,
            width: window_width.max(1),
            height: window_height.max(1),
        };
        let panel_host_rect = window_rect;
        let panel_surface_rect = window_rect;

        let canvas_host_rect = Rect {
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
            panel_host_rect,
            panel_surface_rect,
            canvas_host_rect,
            canvas_display_rect,
        }
    }
}

#[cfg(test)]
mod tests;
