//! デスクトップ UI の固定レイアウト計算とソフトウェア合成の公開入口をまとめる。
//!
//! `apps/desktop` 内の描画補助を、幾何変換・合成 orchestration・低レベル raster 処理へ
//! 分割し、責務ごとの保守性を高める。

mod compositor;
mod geometry;
mod raster;
use desktop_support::{FOOTER_HEIGHT, HEADER_HEIGHT, SIDEBAR_WIDTH, WINDOW_PADDING};

#[allow(unused_imports)]
pub(crate) use compositor::{
    clear_canvas_host_region, compose_base_frame, compose_canvas_host_region,
    compose_desktop_frame, compose_overlay_frame, compose_overlay_region,
    compose_panel_host_region, compose_status_region, status_text_bounds, status_text_rect,
};
#[allow(unused_imports)]
pub(crate) use geometry::{
    brush_preview_rect, canvas_drawn_rect, canvas_texture_quad, exposed_canvas_background_rect,
    fit_rect, map_canvas_dirty_to_display, map_canvas_dirty_to_display_with_transform,
    map_view_to_surface, map_view_to_surface_clamped,
};
#[cfg(test)]
use raster::{SourceAxisRun, build_source_axis_runs, fill_rgba_block};
#[allow(unused_imports)]
pub(crate) use raster::{blit_scaled_rgba_region, scroll_canvas_region};

/// 合成対象の矩形を表す軽量な座標型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Rect {
    /// 指定座標が矩形内に入っているかを判定する。
    pub(crate) fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x as i32
            && y >= self.y as i32
            && x < (self.x + self.width) as i32
            && y < (self.y + self.height) as i32
    }

    /// 2 つの矩形を包む最小の矩形を返す。
    pub(crate) fn union(&self, other: Rect) -> Rect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        Rect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }

    /// 2 つの矩形の共通部分を返す。
    pub(crate) fn intersect(&self, other: Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        if left >= right || top >= bottom {
            return None;
        }

        Some(Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// デスクトップ UI の固定レイアウト情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopLayout {
    pub(crate) panel_host_rect: Rect,
    pub(crate) panel_surface_rect: Rect,
    pub(crate) canvas_host_rect: Rect,
    pub(crate) canvas_display_rect: Rect,
}

impl DesktopLayout {
    /// ウィンドウ寸法とキャンバス寸法から固定レイアウトを構築する。
    pub(crate) fn new(
        window_width: usize,
        window_height: usize,
        canvas_width: usize,
        canvas_height: usize,
    ) -> Self {
        let sidebar_width = SIDEBAR_WIDTH.min(window_width);
        let sidebar_inner_width = sidebar_width.saturating_sub(WINDOW_PADDING * 2).max(1);
        let panel_host_rect = Rect {
            x: WINDOW_PADDING,
            y: WINDOW_PADDING + HEADER_HEIGHT + WINDOW_PADDING,
            width: sidebar_inner_width,
            height: window_height
                .saturating_sub(HEADER_HEIGHT)
                .saturating_sub(FOOTER_HEIGHT)
                .saturating_sub(WINDOW_PADDING * 3)
                .max(1),
        };
        let panel_surface_rect = panel_host_rect;

        let canvas_host_rect = Rect {
            x: sidebar_width + WINDOW_PADDING,
            y: WINDOW_PADDING + HEADER_HEIGHT + WINDOW_PADDING,
            width: window_width
                .saturating_sub(sidebar_width)
                .saturating_sub(WINDOW_PADDING * 2)
                .max(1),
            height: window_height
                .saturating_sub(HEADER_HEIGHT)
                .saturating_sub(FOOTER_HEIGHT)
                .saturating_sub(WINDOW_PADDING * 3)
                .max(1),
        };
        let canvas_display_rect =
            fit_rect(canvas_width.max(1), canvas_height.max(1), canvas_host_rect);

        Self {
            panel_host_rect,
            panel_surface_rect,
            canvas_host_rect,
            canvas_display_rect,
        }
    }
}

/// キャンバス合成元を、`RenderFrame` に依存させずに渡すための軽量ビュー。
#[derive(Clone, Copy)]
pub(crate) struct CanvasCompositeSource<'a> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) pixels: &'a [u8],
}

/// キャンバス上の一時オーバーレイ状態を保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CanvasOverlayState {
    pub(crate) brush_preview: Option<(usize, usize)>,
    pub(crate) lasso_points: Vec<(usize, usize)>,
}

/// GPU 上で提示するテクスチャ付き矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextureQuad {
    pub(crate) destination: Rect,
    pub(crate) uv_min: [f32; 2],
    pub(crate) uv_max: [f32; 2],
    pub(crate) rotation_turns: u8,
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
}

#[cfg(test)]
mod tests;
