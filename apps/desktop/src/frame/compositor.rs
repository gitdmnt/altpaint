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

use super::geometry::canvas_scene;
use super::raster::{
    blit_canvas_with_transform, blit_scaled_rgba_region, draw_brush_preview,
    draw_lasso_preview, draw_text, fill_rect, measured_status_width, stroke_rect,
    stroke_rect_region,
};
use super::{CanvasCompositeSource, CanvasOverlayState, DesktopLayout, Rect};

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
    compose_overlay_region(&mut frame, layout, panel_surface, canvas, transform, overlay, None);
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
        if let Some(drawn_rect) = canvas_scene(display, canvas.width, canvas.height, transform)
            .and_then(|scene| scene.drawn_rect())
            .map(super::geometry::from_render_rect)
        {
            fill_rect_excluding(frame, display_region, drawn_rect, CANVAS_BACKGROUND);
        } else {
            fill_rect(frame, display_region, CANVAS_BACKGROUND);
        }
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

/// 除外矩形を避けつつ背景色を塗る。
fn fill_rect_excluding(
    frame: &mut render::RenderFrame,
    target: Rect,
    exclude: Rect,
    color: [u8; 4],
) {
    let Some(overlap) = target.intersect(exclude) else {
        fill_rect(frame, target, color);
        return;
    };

    let regions = [
        Rect {
            x: target.x,
            y: target.y,
            width: target.width,
            height: overlap.y.saturating_sub(target.y),
        },
        Rect {
            x: target.x,
            y: overlap.y + overlap.height,
            width: target.width,
            height: (target.y + target.height).saturating_sub(overlap.y + overlap.height),
        },
        Rect {
            x: target.x,
            y: overlap.y,
            width: overlap.x.saturating_sub(target.x),
            height: overlap.height,
        },
        Rect {
            x: overlap.x + overlap.width,
            y: overlap.y,
            width: (target.x + target.width).saturating_sub(overlap.x + overlap.width),
            height: overlap.height,
        },
    ];

    for region in regions {
        if region.width > 0 && region.height > 0 {
            fill_rect(frame, region, color);
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
    if let Some(position) = overlay.brush_preview {
        draw_brush_preview(
            frame,
            layout.canvas_host_rect,
            canvas,
            transform,
            position,
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
}

/// パネルホスト領域だけを差分再合成する。
pub(crate) fn compose_panel_host_region(
    frame: &mut render::RenderFrame,
    _layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    dirty_rect: Option<Rect>,
) {
    let destination = Rect {
        x: panel_surface.x,
        y: panel_surface.y,
        width: panel_surface.width,
        height: panel_surface.height,
    };
    if let Some(region) = dirty_rect.and_then(|dirty| destination.intersect(dirty)) {
        fill_rect(frame, region, [0, 0, 0, 0]);
    } else if dirty_rect.is_none() {
        fill_rect(frame, destination, [0, 0, 0, 0]);
    }
    blit_scaled_rgba_region(
        frame,
        destination,
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
