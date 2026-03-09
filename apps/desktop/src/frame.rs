use app_core::DirtyRect;
use ui_shell::{PanelSurface, draw_text_rgba};

use crate::{
    APP_BACKGROUND, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND, CANVAS_FRAME_BORDER,
    FOOTER_HEIGHT, HEADER_HEIGHT, PANEL_FRAME_BACKGROUND, PANEL_FRAME_BORDER, SIDEBAR_BACKGROUND,
    SIDEBAR_WIDTH, TEXT_PRIMARY, TEXT_SECONDARY, WINDOW_PADDING,
};

/// 合成対象の矩形を表す軽量な座標型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Rect {
    pub(crate) fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x as i32
            && y >= self.y as i32
            && x < (self.x + self.width) as i32
            && y < (self.y + self.height) as i32
    }

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

    fn intersect(&self, other: Rect) -> Option<Rect> {
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

/// キャンバス合成元を、`RenderFrame` へ依存させずに渡すための軽量ビュー。
pub(crate) struct CanvasCompositeSource<'a> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: &'a [u8],
}

pub(crate) fn fit_rect(source_width: usize, source_height: usize, target: Rect) -> Rect {
    if source_width == 0 || source_height == 0 || target.width == 0 || target.height == 0 {
        return Rect {
            x: target.x,
            y: target.y,
            width: 0,
            height: 0,
        };
    }

    let scale_x = target.width as f32 / source_width as f32;
    let scale_y = target.height as f32 / source_height as f32;
    let scale = scale_x.min(scale_y);
    let fitted_width = ((source_width as f32 * scale).floor() as usize).max(1);
    let fitted_height = ((source_height as f32 * scale).floor() as usize).max(1);

    Rect {
        x: target.x + (target.width.saturating_sub(fitted_width)) / 2,
        y: target.y + (target.height.saturating_sub(fitted_height)) / 2,
        width: fitted_width,
        height: fitted_height,
    }
}

pub(crate) fn map_canvas_dirty_to_display(
    dirty: DirtyRect,
    destination: Rect,
    source_width: usize,
    source_height: usize,
) -> Rect {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return destination;
    }

    let clamped = dirty.clamp_to_bitmap(source_width, source_height);
    let start_x = destination.x + (clamped.x * destination.width) / source_width;
    let start_y = destination.y + (clamped.y * destination.height) / source_height;
    let end_x =
        destination.x + ((clamped.x + clamped.width) * destination.width).div_ceil(source_width);
    let end_y =
        destination.y + ((clamped.y + clamped.height) * destination.height).div_ceil(source_height);

    Rect {
        x: start_x.min(destination.x + destination.width.saturating_sub(1)),
        y: start_y.min(destination.y + destination.height.saturating_sub(1)),
        width: end_x.saturating_sub(start_x).max(1),
        height: end_y.saturating_sub(start_y).max(1),
    }
}

/// パネル面・キャンバス面・ステータス表示をまとめて最終フレームへ合成する。
pub(crate) fn compose_desktop_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    canvas: CanvasCompositeSource<'_>,
    status_text: &str,
) -> render::RenderFrame {
    let mut frame = render::RenderFrame {
        width,
        height,
        pixels: vec![0; width * height * 4],
    };

    fill_rect(
        &mut frame,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        APP_BACKGROUND,
    );
    fill_rect(
        &mut frame,
        Rect {
            x: 0,
            y: 0,
            width: SIDEBAR_WIDTH.min(width),
            height,
        },
        SIDEBAR_BACKGROUND,
    );
    fill_rect(&mut frame, layout.panel_host_rect, PANEL_FRAME_BACKGROUND);
    stroke_rect(&mut frame, layout.panel_host_rect, PANEL_FRAME_BORDER);
    fill_rect(&mut frame, layout.canvas_host_rect, CANVAS_FRAME_BACKGROUND);
    stroke_rect(&mut frame, layout.canvas_host_rect, CANVAS_FRAME_BORDER);
    fill_rect(&mut frame, layout.canvas_display_rect, CANVAS_BACKGROUND);

    blit_scaled_rgba(
        &mut frame,
        layout.panel_surface_rect,
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels.as_slice(),
    );
    blit_scaled_rgba(
        &mut frame,
        layout.canvas_display_rect,
        canvas.width,
        canvas.height,
        canvas.pixels,
    );

    draw_text(
        &mut frame,
        WINDOW_PADDING,
        WINDOW_PADDING + 4,
        "Panel host (winit + software panel runtime)",
        TEXT_PRIMARY,
    );
    draw_text(
        &mut frame,
        layout.canvas_host_rect.x,
        WINDOW_PADDING + 4,
        "Canvas host (winit + wgpu presenter)",
        TEXT_PRIMARY,
    );
    draw_text(
        &mut frame,
        WINDOW_PADDING,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        "Built-in panels are rendered by the host panel runtime.",
        TEXT_SECONDARY,
    );
    draw_text(
        &mut frame,
        layout.canvas_host_rect.x,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        status_text,
        TEXT_SECONDARY,
    );

    frame
}

pub(crate) fn compose_panel_host_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
) {
    fill_rect(frame, layout.panel_host_rect, PANEL_FRAME_BACKGROUND);
    stroke_rect(frame, layout.panel_host_rect, PANEL_FRAME_BORDER);
    blit_scaled_rgba(
        frame,
        layout.panel_surface_rect,
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels.as_slice(),
    );
}

pub(crate) fn compose_status_region(
    frame: &mut render::RenderFrame,
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    status_text: &str,
) {
    let status_rect = status_text_rect(width, height, layout);
    fill_rect(frame, status_rect, APP_BACKGROUND);
    draw_text(
        frame,
        layout.canvas_host_rect.x,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        status_text,
        TEXT_SECONDARY,
    );
}

pub(crate) fn status_text_rect(width: usize, height: usize, layout: &DesktopLayout) -> Rect {
    Rect {
        x: layout.canvas_host_rect.x,
        y: height.saturating_sub(FOOTER_HEIGHT),
        width: width.saturating_sub(layout.canvas_host_rect.x),
        height: FOOTER_HEIGHT,
    }
}

pub(crate) fn map_view_to_surface(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    x: i32,
    y: i32,
) -> Option<(usize, usize)> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }
    if !rect.contains(x, y) {
        return None;
    }

    let local_x = (x - rect.x as i32) as f32;
    let local_y = (y - rect.y as i32) as f32;
    Some((
        (((local_x / rect.width as f32) * surface_width as f32).floor() as usize)
            .min(surface_width.saturating_sub(1)),
        (((local_y / rect.height as f32) * surface_height as f32).floor() as usize)
            .min(surface_height.saturating_sub(1)),
    ))
}

pub(crate) fn map_view_to_surface_clamped(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    x: i32,
    y: i32,
) -> Option<(usize, usize)> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }

    let clamped_x = x.clamp(
        rect.x as i32,
        (rect.x + rect.width.saturating_sub(1)) as i32,
    );
    let clamped_y = y.clamp(
        rect.y as i32,
        (rect.y + rect.height.saturating_sub(1)) as i32,
    );
    map_view_to_surface(surface_width, surface_height, rect, clamped_x, clamped_y)
}

fn draw_text(frame: &mut render::RenderFrame, x: usize, y: usize, text: &str, color: [u8; 4]) {
    draw_text_rgba(
        frame.pixels.as_mut_slice(),
        frame.width,
        frame.height,
        x,
        y,
        text,
        color,
    );
}

fn fill_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
    let max_x = (rect.x + rect.width).min(frame.width);
    let max_y = (rect.y + rect.height).min(frame.height);
    for yy in rect.y..max_y {
        for xx in rect.x..max_x {
            write_pixel(frame, xx, yy, color);
        }
    }
}

fn stroke_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x + rect.width.saturating_sub(1),
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
}

pub(crate) fn blit_scaled_rgba_region(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<Rect>,
) {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return;
    }

    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination);

    for dst_y in target.y..target.y + target.height {
        let local_y = dst_y - destination.y;
        let src_y = ((local_y * source_height) / destination.height).min(source_height - 1);
        for dst_x in target.x..target.x + target.width {
            let local_x = dst_x - destination.x;
            let src_x = ((local_x * source_width) / destination.width).min(source_width - 1);
            let src_index = (src_y * source_width + src_x) * 4;
            write_pixel(
                frame,
                dst_x,
                dst_y,
                [
                    source_pixels[src_index],
                    source_pixels[src_index + 1],
                    source_pixels[src_index + 2],
                    source_pixels[src_index + 3],
                ],
            );
        }
    }
}

fn blit_scaled_rgba(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
) {
    blit_scaled_rgba_region(
        frame,
        destination,
        source_width,
        source_height,
        source_pixels,
        None,
    );
}

fn write_pixel(frame: &mut render::RenderFrame, x: usize, y: usize, color: [u8; 4]) {
    if x >= frame.width || y >= frame.height {
        return;
    }
    let index = (y * frame.width + x) * 4;
    frame.pixels[index..index + 4].copy_from_slice(&color);
}
