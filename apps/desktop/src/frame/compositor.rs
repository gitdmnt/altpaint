//! `frame` の高レベル合成手順をまとめる。
//!
//! 固定レイアウトに従って panel・canvas・status の各領域を描き分け、
//! 差分再合成を行う入口だけをここへ集約する。

use app_core::CanvasViewTransform;
use desktop_support::{
    APP_BACKGROUND, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND, CANVAS_FRAME_BORDER, FOOTER_HEIGHT,
    TEXT_PRIMARY, TEXT_SECONDARY, WINDOW_PADDING,
};
use ui_shell::PanelSurface;

use super::raster::{
    blit_canvas_with_transform, blit_rgba_region_at, draw_brush_preview, draw_lasso_preview,
    draw_text, fill_rect, measured_status_width, stroke_rect, stroke_rect_region,
};
use super::{
    CanvasCompositeSource, CanvasOverlayState, DesktopLayout, PanelNavigatorOverlay, Rect,
};

const PANEL_NAVIGATOR_BACKGROUND: [u8; 4] = [0x10, 0x16, 0x21, 0xdd];
const PANEL_NAVIGATOR_BORDER: [u8; 4] = [0x90, 0xa4, 0xae, 0xff];
const PANEL_NAVIGATOR_PANEL: [u8; 4] = [0x4f, 0x5b, 0x6d, 0xd0];
const PANEL_NAVIGATOR_ACTIVE: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
const ACTIVE_PANEL_MASK: [u8; 4] = [0x00, 0x00, 0x00, 0x90];
const ACTIVE_PANEL_BORDER: [u8; 4] = [0xff, 0xc1, 0x07, 0xff];
const ACTIVE_PANEL_FILL: [u8; 4] = [0xff, 0xc1, 0x07, 0x18];
const PANEL_PREVIEW_BORDER: [u8; 4] = [0x80, 0xde, 0xea, 0xff];
const PANEL_PREVIEW_FILL: [u8; 4] = [0x80, 0xde, 0xea, 0x32];

/// パネル面・キャンバス面・ステータス表示をまとめて最終フレームへ合成する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compose_base_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    _panel_surface: &PanelSurface,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
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
    fill_canvas_host_background(
        &mut frame,
        layout,
        canvas,
        transform,
        layout.canvas_host_rect,
    );
    stroke_rect(&mut frame, layout.canvas_host_rect, CANVAS_FRAME_BORDER);

    if std::env::var_os("ALTPAINT_DEBUG_LABELS").is_some() {
        draw_text(
            &mut frame,
            WINDOW_PADDING,
            WINDOW_PADDING + 4,
            "Background layer",
            TEXT_PRIMARY,
        );
        draw_text(
            &mut frame,
            layout.canvas_host_rect.x,
            WINDOW_PADDING + 4,
            "Canvas layer (winit + wgpu canvas texture)",
            TEXT_PRIMARY,
        );
        draw_text(
            &mut frame,
            WINDOW_PADDING,
            height.saturating_sub(FOOTER_HEIGHT) + 6,
            "UI panels are rendered into an independent floating UI layer.",
            TEXT_SECONDARY,
        );
    }
    draw_text(
        &mut frame,
        layout.canvas_host_rect.x,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        status_text,
        TEXT_SECONDARY,
    );

    frame
}

/// オーバーレイ専用 frame を新規構築する。
pub(crate) fn compose_overlay_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    overlay: CanvasOverlayState,
) -> render::RenderFrame {
    let mut frame = render::RenderFrame {
        width,
        height,
        pixels: vec![0; width * height * 4],
    };
    compose_overlay_region(
        &mut frame,
        layout,
        panel_surface,
        canvas,
        transform,
        overlay,
        None,
    );
    frame
}

/// 指定 dirty 領域だけをクリアして overlay を再描画する。
pub(crate) fn compose_overlay_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    overlay: CanvasOverlayState,
    dirty_rect: Option<Rect>,
) {
    let clear_rect = dirty_rect.unwrap_or(Rect {
        x: 0,
        y: 0,
        width: frame.width,
        height: frame.height,
    });
    fill_rect(frame, clear_rect, [0, 0, 0, 0]);
    draw_canvas_overlay(frame, layout, canvas, transform, overlay, dirty_rect);
    compose_panel_host_region(frame, layout, panel_surface, dirty_rect);
}

/// デスクトップ UI のベースとキャンバス領域を一度に合成する。
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compose_desktop_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    overlay: CanvasOverlayState,
    status_text: &str,
) -> render::RenderFrame {
    let mut frame = compose_base_frame(
        width,
        height,
        layout,
        panel_surface,
        canvas,
        transform,
        status_text,
    );
    compose_canvas_host_region(&mut frame, layout, canvas, transform, overlay, None);
    frame
}

/// キャンバスホスト領域だけを差分再合成する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compose_canvas_host_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    overlay: CanvasOverlayState,
    dirty_rect: Option<Rect>,
) {
    clear_canvas_host_region(frame, layout, canvas, transform, dirty_rect);
    blit_canvas_content(frame, layout, canvas, transform, dirty_rect);
    draw_canvas_overlay(frame, layout, canvas, transform, overlay, dirty_rect);
}

/// キャンバスホスト背景と枠線だけを再描画する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn clear_canvas_host_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    dirty_rect: Option<Rect>,
) {
    if let Some(dirty_rect) = dirty_rect {
        fill_canvas_host_background(frame, layout, canvas, transform, dirty_rect);
        stroke_rect_region(
            frame,
            layout.canvas_host_rect,
            dirty_rect,
            CANVAS_FRAME_BORDER,
        );
    } else {
        fill_canvas_host_background(frame, layout, canvas, transform, layout.canvas_host_rect);
        stroke_rect(frame, layout.canvas_host_rect, CANVAS_FRAME_BORDER);
    }
}

/// 描画済みキャンバス以外のホスト背景を塗り戻す。
fn fill_canvas_host_background(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    dirty_rect: Rect,
) {
    let display = layout.canvas_host_rect;
    if let Some(display_region) = display.intersect(dirty_rect) {
        let _ = (canvas, transform);
        fill_rect(frame, display_region, CANVAS_BACKGROUND);
    }

    let host = layout.canvas_host_rect;
    let margins = [
        Rect {
            x: host.x,
            y: host.y,
            width: host.width,
            height: display.y.saturating_sub(host.y),
        },
        Rect {
            x: host.x,
            y: display.y + display.height,
            width: host.width,
            height: (host.y + host.height).saturating_sub(display.y + display.height),
        },
        Rect {
            x: host.x,
            y: display.y,
            width: display.x.saturating_sub(host.x),
            height: display.height,
        },
        Rect {
            x: display.x + display.width,
            y: display.y,
            width: (host.x + host.width).saturating_sub(display.x + display.width),
            height: display.height,
        },
    ];

    for margin in margins {
        if let Some(region) = margin.intersect(dirty_rect)
            && region.width > 0
            && region.height > 0
        {
            fill_rect(frame, region, CANVAS_FRAME_BACKGROUND);
        }
    }
}

/// キャンバス内容だけを host 領域へ転送する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn blit_canvas_content(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    dirty_rect: Option<Rect>,
) {
    blit_canvas_with_transform(
        frame,
        layout.canvas_host_rect,
        canvas,
        transform,
        dirty_rect,
    );
}

/// オーバーレイ状態をホスト上へ描画する。
pub(crate) fn draw_canvas_overlay(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    overlay: CanvasOverlayState,
    dirty_rect: Option<Rect>,
) {
    if let Some(active_panel_bounds) = overlay.active_panel_bounds {
        draw_active_panel_mask(
            frame,
            layout,
            canvas,
            transform,
            active_panel_bounds,
            dirty_rect,
        );
    }
    if let (Some(position), Some(brush_size)) = (overlay.brush_preview, overlay.brush_size) {
        draw_brush_preview(
            frame,
            layout.canvas_host_rect,
            canvas,
            transform,
            position,
            brush_size,
            dirty_rect,
        );
    }
    if overlay.lasso_points.len() >= 2 {
        draw_lasso_preview(
            frame,
            layout.canvas_host_rect,
            canvas,
            transform,
            overlay.lasso_points.as_slice(),
            dirty_rect,
        );
    }
    if let Some(preview_bounds) = overlay.panel_creation_preview {
        draw_panel_creation_preview(frame, layout, canvas, transform, preview_bounds, dirty_rect);
    }
    if let Some(panel_navigator) = overlay.panel_navigator.as_ref() {
        draw_panel_navigator(frame, layout, panel_navigator, dirty_rect);
    }
}

fn draw_active_panel_mask(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    bounds: app_core::PanelBounds,
    dirty_rect: Option<Rect>,
) {
    if canvas.width == 0 || canvas.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return;
    }
    let viewport = render::PixelRect {
        x: layout.canvas_host_rect.x,
        y: layout.canvas_host_rect.y,
        width: layout.canvas_host_rect.width,
        height: layout.canvas_host_rect.height,
    };

    let outside_regions = [
        app_core::CanvasDirtyRect {
            x: 0,
            y: 0,
            width: canvas.width,
            height: bounds.y,
        },
        app_core::CanvasDirtyRect {
            x: 0,
            y: bounds.y.saturating_add(bounds.height),
            width: canvas.width,
            height: canvas
                .height
                .saturating_sub(bounds.y.saturating_add(bounds.height)),
        },
        app_core::CanvasDirtyRect {
            x: 0,
            y: bounds.y,
            width: bounds.x,
            height: bounds.height,
        },
        app_core::CanvasDirtyRect {
            x: bounds.x.saturating_add(bounds.width),
            y: bounds.y,
            width: canvas
                .width
                .saturating_sub(bounds.x.saturating_add(bounds.width)),
            height: bounds.height,
        },
    ];
    for region in outside_regions
        .into_iter()
        .filter(|region| region.width > 0 && region.height > 0)
    {
        let mapped = render::map_canvas_dirty_to_display_with_transform(
            region,
            viewport,
            canvas.width,
            canvas.height,
            transform,
        );
        let rect = Rect {
            x: mapped.x,
            y: mapped.y,
            width: mapped.width,
            height: mapped.height,
        };
        if let Some(dirty_rect) = dirty_rect
            && rect.intersect(dirty_rect).is_none()
        {
            continue;
        }
        fill_rect(frame, rect, ACTIVE_PANEL_MASK);
    }

    let mapped = render::map_canvas_dirty_to_display_with_transform(
        app_core::CanvasDirtyRect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        },
        viewport,
        canvas.width,
        canvas.height,
        transform,
    );
    let panel_rect = Rect {
        x: mapped.x,
        y: mapped.y,
        width: mapped.width,
        height: mapped.height,
    };
    if let Some(dirty_rect) = dirty_rect
        && panel_rect.intersect(dirty_rect).is_none()
    {
        return;
    }
    fill_rect(frame, panel_rect, ACTIVE_PANEL_FILL);
    stroke_rect(frame, panel_rect, ACTIVE_PANEL_BORDER);
}

fn draw_panel_creation_preview(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    bounds: app_core::PanelBounds,
    dirty_rect: Option<Rect>,
) {
    if canvas.width == 0 || canvas.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return;
    }
    let mapped = render::map_canvas_dirty_to_display_with_transform(
        app_core::CanvasDirtyRect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        },
        render::PixelRect {
            x: layout.canvas_host_rect.x,
            y: layout.canvas_host_rect.y,
            width: layout.canvas_host_rect.width,
            height: layout.canvas_host_rect.height,
        },
        canvas.width,
        canvas.height,
        transform,
    );
    let rect = Rect {
        x: mapped.x,
        y: mapped.y,
        width: mapped.width,
        height: mapped.height,
    };
    if let Some(dirty_rect) = dirty_rect
        && rect.intersect(dirty_rect).is_none()
    {
        return;
    }
    fill_rect(frame, rect, PANEL_PREVIEW_FILL);
    stroke_rect(frame, rect, PANEL_PREVIEW_BORDER);
}

fn draw_panel_navigator(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    panel_navigator: &PanelNavigatorOverlay,
    dirty_rect: Option<Rect>,
) {
    if panel_navigator.page_width == 0
        || panel_navigator.page_height == 0
        || panel_navigator.panels.len() <= 1
    {
        return;
    }

    let max_width = layout.canvas_host_rect.width.clamp(96, 180);
    let max_height = layout.canvas_host_rect.height.clamp(96, 180);
    let inner_max_width = max_width.saturating_sub(16).max(1);
    let inner_max_height = max_height.saturating_sub(16).max(1);
    let scale_x = inner_max_width as f32 / panel_navigator.page_width as f32;
    let scale_y = inner_max_height as f32 / panel_navigator.page_height as f32;
    let scale = scale_x.min(scale_y).max(f32::EPSILON);
    let scaled_width = ((panel_navigator.page_width as f32 * scale).round() as usize).max(1);
    let scaled_height = ((panel_navigator.page_height as f32 * scale).round() as usize).max(1);
    let navigator = Rect {
        x: layout
            .canvas_host_rect
            .x
            .saturating_add(layout.canvas_host_rect.width)
            .saturating_sub(scaled_width + 16)
            .saturating_sub(12),
        y: layout.canvas_host_rect.y + 12,
        width: scaled_width + 16,
        height: scaled_height + 16,
    };

    if let Some(dirty_rect) = dirty_rect
        && navigator.intersect(dirty_rect).is_none()
    {
        return;
    }

    fill_rect(frame, navigator, PANEL_NAVIGATOR_BACKGROUND);
    stroke_rect(frame, navigator, PANEL_NAVIGATOR_BORDER);
    let inner = Rect {
        x: navigator.x + 8,
        y: navigator.y + 8,
        width: scaled_width,
        height: scaled_height,
    };
    stroke_rect(frame, inner, PANEL_NAVIGATOR_BORDER);

    for panel in &panel_navigator.panels {
        let rect = Rect {
            x: inner.x + ((panel.bounds.x as f32 * scale).round() as usize),
            y: inner.y + ((panel.bounds.y as f32 * scale).round() as usize),
            width: ((panel.bounds.width as f32 * scale).round() as usize).max(1),
            height: ((panel.bounds.height as f32 * scale).round() as usize).max(1),
        };
        fill_rect(
            frame,
            rect,
            if panel.active {
                [
                    PANEL_NAVIGATOR_ACTIVE[0],
                    PANEL_NAVIGATOR_ACTIVE[1],
                    PANEL_NAVIGATOR_ACTIVE[2],
                    0x40,
                ]
            } else {
                PANEL_NAVIGATOR_PANEL
            },
        );
        stroke_rect(
            frame,
            rect,
            if panel.active {
                PANEL_NAVIGATOR_ACTIVE
            } else {
                PANEL_NAVIGATOR_BORDER
            },
        );
    }
}

/// パネルホスト領域だけを overlay 層へ差分反映する。
pub(crate) fn compose_panel_host_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    dirty_rect: Option<Rect>,
) {
    let _ = layout;
    blit_rgba_region_at(
        frame,
        Rect {
            x: panel_surface.x,
            y: panel_surface.y,
            width: panel_surface.width,
            height: panel_surface.height,
        },
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels.as_slice(),
        dirty_rect,
    );
}

/// ステータス行だけを差分再合成する。
pub(crate) fn compose_status_region(
    frame: &mut render::RenderFrame,
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    status_text: &str,
) {
    let status_rect = status_text_bounds(width, height, layout, status_text);
    fill_rect(frame, status_rect, APP_BACKGROUND);
    draw_text(
        frame,
        layout.canvas_host_rect.x,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        status_text,
        TEXT_SECONDARY,
    );
}

/// フッター右側のステータス表示領域を返す。
#[allow(dead_code)]
pub(crate) fn status_text_rect(width: usize, height: usize, layout: &DesktopLayout) -> Rect {
    status_text_bounds(width, height, layout, "")
}

/// 現在のステータス文字列に必要な最小表示領域を返す。
pub(crate) fn status_text_bounds(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    status_text: &str,
) -> Rect {
    let text_width = measured_status_width(status_text);
    Rect {
        x: layout.canvas_host_rect.x,
        y: height.saturating_sub(FOOTER_HEIGHT),
        width: text_width.min(width.saturating_sub(layout.canvas_host_rect.x)),
        height: FOOTER_HEIGHT,
    }
}
