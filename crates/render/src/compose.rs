use app_core::{CanvasDisplayPoint, CanvasPoint};
use desktop_support::{
    APP_BACKGROUND, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND, CANVAS_FRAME_BORDER, FOOTER_HEIGHT,
    TEXT_PRIMARY, TEXT_SECONDARY, WINDOW_PADDING,
};

use crate::status::status_text_bounds;
use crate::{
    CanvasCompositeSource, CanvasOverlayState, FramePlan, PanelNavigatorOverlay,
    PanelSurfaceSource, PixelRect, RenderFrame, draw_text_rgba, map_canvas_point_to_display,
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
/// アクティブ UI パネル枠線の色（水色）。
const ACTIVE_UI_PANEL_BORDER: [u8; 4] = [0x42, 0xa5, 0xf5, 0xff];

/// 合成 background フレーム に必要な差分領域だけを描画または合成する。
pub fn compose_background_frame(plan: &FramePlan<'_>) -> RenderFrame {
    let mut frame = RenderFrame {
        width: plan.window_width,
        height: plan.window_height,
        pixels: vec![0; plan.window_width * plan.window_height * 4],
    };

    fill_rect(&mut frame, plan.window_rect(), APP_BACKGROUND);
    fill_canvas_host_background(&mut frame, plan, plan.canvas.host_rect);
    stroke_rect(&mut frame, plan.canvas.host_rect, CANVAS_FRAME_BORDER);

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
            plan.canvas.host_rect.x,
            WINDOW_PADDING + 4,
            "Canvas layer (winit + wgpu canvas texture)",
            TEXT_PRIMARY,
        );
        draw_text(
            &mut frame,
            WINDOW_PADDING,
            plan.window_height.saturating_sub(FOOTER_HEIGHT) + 6,
            "UI panels are rendered into an independent floating UI layer.",
            TEXT_SECONDARY,
        );
    }
    draw_text(
        &mut frame,
        plan.canvas.host_rect.x,
        plan.window_height.saturating_sub(FOOTER_HEIGHT) + 6,
        plan.status_text,
        TEXT_SECONDARY,
    );

    frame
}

/// 合成 temp オーバーレイ フレーム に必要な差分領域だけを描画または合成する。
pub fn compose_temp_overlay_frame(
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
) -> RenderFrame {
    let mut frame = RenderFrame {
        width: plan.window_width,
        height: plan.window_height,
        pixels: vec![0; plan.window_width * plan.window_height * 4],
    };
    compose_temp_overlay_region(&mut frame, plan, overlay, None);
    frame
}

/// 合成 temp オーバーレイ 領域 に必要な差分領域だけを描画または合成する。
pub fn compose_temp_overlay_region(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
    dirty_rect: Option<PixelRect>,
) {
    let clear_rect = dirty_rect.unwrap_or(PixelRect {
        x: 0,
        y: 0,
        width: frame.width,
        height: frame.height,
    });
    fill_rect(frame, clear_rect, [0, 0, 0, 0]);
    draw_canvas_overlay(frame, plan, overlay, dirty_rect);
}

/// 合成 UI パネル フレーム に必要な差分領域だけを描画または合成する。
pub fn compose_ui_panel_frame(plan: &FramePlan<'_>) -> RenderFrame {
    let mut frame = RenderFrame {
        width: plan.window_width,
        height: plan.window_height,
        pixels: vec![0; plan.window_width * plan.window_height * 4],
    };
    compose_ui_panel_region(&mut frame, plan, None);
    frame
}

/// 合成 UI パネル 領域 に必要な差分領域だけを描画または合成する。
pub fn compose_ui_panel_region(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    dirty_rect: Option<PixelRect>,
) {
    let clear_rect = dirty_rect.unwrap_or(PixelRect {
        x: 0,
        y: 0,
        width: frame.width,
        height: frame.height,
    });
    fill_rect(frame, clear_rect, [0, 0, 0, 0]);
    compose_panel_host_region(frame, plan.panel_surface, dirty_rect);
}

/// 合成 desktop フレーム に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
pub fn compose_desktop_frame(plan: &FramePlan<'_>, overlay: &CanvasOverlayState) -> RenderFrame {
    let mut frame = compose_background_frame(plan);
    compose_canvas_host_region(&mut frame, plan, overlay, None);
    frame
}

/// 合成 キャンバス ホスト 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
#[cfg_attr(not(test), allow(dead_code))]
pub fn compose_canvas_host_region(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
    dirty_rect: Option<PixelRect>,
) {
    clear_canvas_host_region(frame, plan, dirty_rect);
    blit_canvas_content(frame, plan, dirty_rect);
    draw_canvas_overlay(frame, plan, overlay, dirty_rect);
}

/// Clear キャンバス ホスト 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
#[cfg_attr(not(test), allow(dead_code))]
pub fn clear_canvas_host_region(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    dirty_rect: Option<PixelRect>,
) {
    if let Some(dirty_rect) = dirty_rect {
        fill_canvas_host_background(frame, plan, dirty_rect);
        stroke_rect_region(
            frame,
            plan.canvas.host_rect,
            dirty_rect,
            CANVAS_FRAME_BORDER,
        );
    } else {
        fill_canvas_host_background(frame, plan, plan.canvas.host_rect);
        stroke_rect(frame, plan.canvas.host_rect, CANVAS_FRAME_BORDER);
    }
}

/// 合成 パネル ホスト 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub fn compose_panel_host_region(
    frame: &mut RenderFrame,
    panel_surface: PanelSurfaceSource<'_>,
    dirty_rect: Option<PixelRect>,
) {
    blit_rgba_region_at(
        frame,
        panel_surface.rect(),
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels,
        dirty_rect,
    );
}

/// アクティブ UI パネル枠線を L4 フレームへ描画する。
///
/// `overlay.active_ui_panel_rect` が `Some` のときのみ描画する。
/// `dirty_rect` が指定された場合はクリップして差分描画する。
pub fn compose_active_panel_border(
    frame: &mut RenderFrame,
    overlay: &CanvasOverlayState,
    dirty_rect: Option<PixelRect>,
) {
    let Some(rect) = overlay.active_ui_panel_rect else {
        return;
    };
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    if let Some(dirty) = dirty_rect {
        stroke_rect_region(frame, rect, dirty, ACTIVE_UI_PANEL_BORDER);
    } else {
        stroke_rect(frame, rect, ACTIVE_UI_PANEL_BORDER);
    }
}

/// 合成 ステータス 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub fn compose_status_region(frame: &mut RenderFrame, plan: &FramePlan<'_>) {
    let status_rect = status_text_bounds(
        plan.window_width,
        plan.window_height,
        plan.canvas.host_rect,
        plan.status_text,
    );
    fill_rect(frame, status_rect, APP_BACKGROUND);
    draw_text(
        frame,
        plan.canvas.host_rect.x,
        plan.window_height.saturating_sub(FOOTER_HEIGHT) + 6,
        plan.status_text,
        TEXT_SECONDARY,
    );
}

/// 塗りつぶし キャンバス ホスト 背景 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn fill_canvas_host_background(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    dirty_rect: PixelRect,
) {
    let display = plan.canvas.host_rect;
    if let Some(display_region) = display.intersect(dirty_rect) {
        fill_rect(frame, display_region, CANVAS_BACKGROUND);
    }

    let host = plan.canvas.host_rect;
    let margins = [
        PixelRect {
            x: host.x,
            y: host.y,
            width: host.width,
            height: display.y.saturating_sub(host.y),
        },
        PixelRect {
            x: host.x,
            y: display.y + display.height,
            width: host.width,
            height: (host.y + host.height).saturating_sub(display.y + display.height),
        },
        PixelRect {
            x: host.x,
            y: display.y,
            width: display.x.saturating_sub(host.x),
            height: display.height,
        },
        PixelRect {
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

/// Blit キャンバス content に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
#[cfg_attr(not(test), allow(dead_code))]
fn blit_canvas_content(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    dirty_rect: Option<PixelRect>,
) {
    blit_canvas_with_transform(
        frame,
        plan.canvas.host_rect,
        plan.canvas_source,
        plan.canvas.transform,
        dirty_rect,
    );
}

/// 描画 キャンバス オーバーレイ に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_canvas_overlay(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
    dirty_rect: Option<PixelRect>,
) {
    if let Some(active_panel_bounds) = overlay.active_panel_bounds {
        draw_active_panel_mask(frame, plan, active_panel_bounds, dirty_rect);
    }
    if let (Some(position), Some(brush_size)) = (overlay.brush_preview, overlay.brush_size) {
        draw_brush_preview(
            frame,
            plan.canvas.host_rect,
            plan.canvas_source,
            plan.canvas.transform,
            position,
            brush_size,
            dirty_rect,
        );
    }
    if overlay.lasso_points.len() >= 2 {
        draw_lasso_preview(
            frame,
            plan.canvas.host_rect,
            plan.canvas_source,
            plan.canvas.transform,
            overlay.lasso_points.as_slice(),
            dirty_rect,
        );
    }
    if let Some(preview_bounds) = overlay.panel_creation_preview {
        draw_panel_creation_preview(frame, plan, preview_bounds, dirty_rect);
    }
    if let Some(panel_navigator) = overlay.panel_navigator.as_ref() {
        draw_panel_navigator(frame, plan.canvas.host_rect, panel_navigator, dirty_rect);
    }
}

/// 描画 アクティブ パネル マスク に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_active_panel_mask(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    bounds: app_core::PanelBounds,
    dirty_rect: Option<PixelRect>,
) {
    let source = plan.canvas_source;
    if source.width == 0 || source.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return;
    }

    let outside_regions = [
        app_core::CanvasDirtyRect {
            x: 0,
            y: 0,
            width: source.width,
            height: bounds.y,
        },
        app_core::CanvasDirtyRect {
            x: 0,
            y: bounds.y.saturating_add(bounds.height),
            width: source.width,
            height: source
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
            width: source
                .width
                .saturating_sub(bounds.x.saturating_add(bounds.width)),
            height: bounds.height,
        },
    ];
    for region in outside_regions
        .into_iter()
        .filter(|region| region.width > 0 && region.height > 0)
    {
        let rect = plan.canvas.map_dirty_rect(region);
        if let Some(dirty_rect) = dirty_rect
            && rect.intersect(dirty_rect).is_none()
        {
            continue;
        }
        fill_rect(frame, rect, ACTIVE_PANEL_MASK);
    }

    let panel_rect = plan.canvas.map_dirty_rect(app_core::CanvasDirtyRect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    });
    if let Some(dirty_rect) = dirty_rect
        && panel_rect.intersect(dirty_rect).is_none()
    {
        return;
    }
    fill_rect(frame, panel_rect, ACTIVE_PANEL_FILL);
    stroke_rect(frame, panel_rect, ACTIVE_PANEL_BORDER);
}

/// 描画 パネル 生成 プレビュー に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_panel_creation_preview(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    bounds: app_core::PanelBounds,
    dirty_rect: Option<PixelRect>,
) {
    let source = plan.canvas_source;
    if source.width == 0 || source.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return;
    }

    let rect = plan.canvas.map_dirty_rect(app_core::CanvasDirtyRect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    });
    if let Some(dirty_rect) = dirty_rect
        && rect.intersect(dirty_rect).is_none()
    {
        return;
    }
    fill_rect(frame, rect, PANEL_PREVIEW_FILL);
    stroke_rect(frame, rect, PANEL_PREVIEW_BORDER);
}

/// 描画 パネル navigator に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_panel_navigator(
    frame: &mut RenderFrame,
    canvas_host_rect: PixelRect,
    panel_navigator: &PanelNavigatorOverlay,
    dirty_rect: Option<PixelRect>,
) {
    if panel_navigator.page_width == 0
        || panel_navigator.page_height == 0
        || panel_navigator.panels.len() <= 1
    {
        return;
    }

    let max_width = canvas_host_rect.width.clamp(96, 180);
    let max_height = canvas_host_rect.height.clamp(96, 180);
    let inner_max_width = max_width.saturating_sub(16).max(1);
    let inner_max_height = max_height.saturating_sub(16).max(1);
    let scale_x = inner_max_width as f32 / panel_navigator.page_width as f32;
    let scale_y = inner_max_height as f32 / panel_navigator.page_height as f32;
    let scale = scale_x.min(scale_y).max(f32::EPSILON);
    let scaled_width = ((panel_navigator.page_width as f32 * scale).round() as usize).max(1);
    let scaled_height = ((panel_navigator.page_height as f32 * scale).round() as usize).max(1);
    let navigator = PixelRect {
        x: canvas_host_rect
            .x
            .saturating_add(canvas_host_rect.width)
            .saturating_sub(scaled_width + 16)
            .saturating_sub(12),
        y: canvas_host_rect.y + 12,
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
    let inner = PixelRect {
        x: navigator.x + 8,
        y: navigator.y + 8,
        width: scaled_width,
        height: scaled_height,
    };
    stroke_rect(frame, inner, PANEL_NAVIGATOR_BORDER);

    for panel in &panel_navigator.panels {
        let rect = PixelRect {
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

/// 描画 テキスト に必要な描画内容を組み立てる。
fn draw_text(frame: &mut RenderFrame, x: usize, y: usize, text: &str, color: [u8; 4]) {
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

/// 塗りつぶし 矩形 に必要な描画内容を組み立てる。
fn fill_rect(frame: &mut RenderFrame, rect: PixelRect, color: [u8; 4]) {
    let max_x = (rect.x + rect.width).min(frame.width);
    let max_y = (rect.y + rect.height).min(frame.height);
    if rect.x >= max_x || rect.y >= max_y {
        return;
    }

    let row_start_x = rect.x * 4;
    let row_len = (max_x - rect.x) * 4;
    for yy in rect.y..max_y {
        let row_start = yy * frame.width * 4 + row_start_x;
        fill_rgba_slice(&mut frame.pixels[row_start..row_start + row_len], color);
    }
}

/// 塗りつぶし RGBA slice に必要な描画内容を組み立てる。
fn fill_rgba_slice(target: &mut [u8], color: [u8; 4]) {
    if target.is_empty() {
        return;
    }

    target[..4].copy_from_slice(&color);
    let mut filled = 4;
    while filled < target.len() {
        let copy_len = filled.min(target.len() - filled);
        let (head, tail) = target.split_at_mut(filled);
        tail[..copy_len].copy_from_slice(&head[..copy_len]);
        filled += copy_len;
    }
}

/// ストローク 矩形 に必要な描画内容を組み立てる。
fn stroke_rect(frame: &mut RenderFrame, rect: PixelRect, color: [u8; 4]) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    fill_rect(
        frame,
        PixelRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        PixelRect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        PixelRect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
    fill_rect(
        frame,
        PixelRect {
            x: rect.x + rect.width.saturating_sub(1),
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
}

/// ストローク 矩形 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn stroke_rect_region(
    frame: &mut RenderFrame,
    rect: PixelRect,
    dirty_rect: PixelRect,
    color: [u8; 4],
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let edges = [
        PixelRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        PixelRect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        PixelRect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        PixelRect {
            x: rect.x + rect.width.saturating_sub(1),
            y: rect.y,
            width: 1,
            height: rect.height,
        },
    ];

    for edge in edges {
        if let Some(region) = edge.intersect(dirty_rect) {
            fill_rect(frame, region, color);
        }
    }
}

/// Blit scaled RGBA 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub fn blit_scaled_rgba_region(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<PixelRect>,
) {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return;
    }

    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination);

    if target.width == 0 || target.height == 0 {
        return;
    }

    if destination.width == source_width && destination.height == source_height {
        let src_start_x = target.x.saturating_sub(destination.x);
        let src_start_y = target.y.saturating_sub(destination.y);
        let row_bytes = target.width * 4;
        for row in 0..target.height {
            let src_y = src_start_y + row;
            let dst_y = target.y + row;
            let src_row_start = (src_y * source_width + src_start_x) * 4;
            let dst_row_start = (dst_y * frame.width + target.x) * 4;
            frame.pixels[dst_row_start..dst_row_start + row_bytes]
                .copy_from_slice(&source_pixels[src_row_start..src_row_start + row_bytes]);
        }
        return;
    }

    if destination.width < source_width || destination.height < source_height {
        // 縮小時は bilinear 補間でアンチエイリアスを適用する
        for dst_y in target.y..target.y + target.height {
            let local_y = dst_y - destination.y;
            let src_y_f =
                (local_y as f32 + 0.5) * source_height as f32 / destination.height as f32 - 0.5;
            let y0 = src_y_f.floor() as i64;
            let y1 = y0 + 1;
            let wy = src_y_f - y0 as f32;
            let y0c = y0.clamp(0, source_height as i64 - 1) as usize;
            let y1c = y1.clamp(0, source_height as i64 - 1) as usize;
            let dst_row_start = dst_y * frame.width * 4;
            for dst_x in target.x..target.x + target.width {
                let local_x = dst_x - destination.x;
                let src_x_f = (local_x as f32 + 0.5) * source_width as f32
                    / destination.width as f32
                    - 0.5;
                let x0 = src_x_f.floor() as i64;
                let x1 = x0 + 1;
                let wx = src_x_f - x0 as f32;
                let x0c = x0.clamp(0, source_width as i64 - 1) as usize;
                let x1c = x1.clamp(0, source_width as i64 - 1) as usize;

                let sample = |sx: usize, sy: usize| -> [f32; 4] {
                    let i = (sy * source_width + sx) * 4;
                    [
                        source_pixels[i] as f32,
                        source_pixels[i + 1] as f32,
                        source_pixels[i + 2] as f32,
                        source_pixels[i + 3] as f32,
                    ]
                };
                let p00 = sample(x0c, y0c);
                let p10 = sample(x1c, y0c);
                let p01 = sample(x0c, y1c);
                let p11 = sample(x1c, y1c);

                let lerp = |a: f32, b: f32, t: f32| -> f32 { a + (b - a) * t };
                let to_u8 = |v: f32| -> u8 { v.round().clamp(0.0, 255.0) as u8 };
                let pixel = [
                    to_u8(lerp(lerp(p00[0], p10[0], wx), lerp(p01[0], p11[0], wx), wy)),
                    to_u8(lerp(lerp(p00[1], p10[1], wx), lerp(p01[1], p11[1], wx), wy)),
                    to_u8(lerp(lerp(p00[2], p10[2], wx), lerp(p01[2], p11[2], wx), wy)),
                    to_u8(lerp(lerp(p00[3], p10[3], wx), lerp(p01[3], p11[3], wx), wy)),
                ];
                let dst_index = dst_row_start + dst_x * 4;
                frame.pixels[dst_index..dst_index + 4].copy_from_slice(&pixel);
            }
        }
        return;
    }

    for dst_y in target.y..target.y + target.height {
        let local_y = dst_y - destination.y;
        let src_y = ((local_y * source_height) / destination.height).min(source_height - 1);
        let dst_row_start = dst_y * frame.width * 4;
        for dst_x in target.x..target.x + target.width {
            let local_x = dst_x - destination.x;
            let src_x = ((local_x * source_width) / destination.width).min(source_width - 1);
            let src_index = (src_y * source_width + src_x) * 4;
            let dst_index = dst_row_start + dst_x * 4;
            frame.pixels[dst_index..dst_index + 4]
                .copy_from_slice(&source_pixels[src_index..src_index + 4]);
        }
    }
}

/// Blit RGBA 領域 at に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn blit_rgba_region_at(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<PixelRect>,
) {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return;
    }

    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination);
    if target.width == 0 || target.height == 0 {
        return;
    }

    let src_start_x = target.x.saturating_sub(destination.x);
    let src_start_y = target.y.saturating_sub(destination.y);
    let row_bytes = target.width * 4;
    for row in 0..target.height {
        let src_y = src_start_y + row;
        let dst_y = target.y + row;
        let src_row_start = (src_y * source_width + src_start_x) * 4;
        let dst_row_start = (dst_y * frame.width + target.x) * 4;
        frame.pixels[dst_row_start..dst_row_start + row_bytes]
            .copy_from_slice(&source_pixels[src_row_start..src_row_start + row_bytes]);
    }
}

/// Blit キャンバス with 変換 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn blit_canvas_with_transform(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source: CanvasCompositeSource<'_>,
    transform: app_core::CanvasViewTransform,
    dirty_rect: Option<PixelRect>,
) {
    if destination.width == 0
        || destination.height == 0
        || source.width == 0
        || source.height == 0
        || source.pixels.len() < source.width * source.height * 4
    {
        return;
    }

    let Some(scene) =
        crate::prepare_canvas_scene(destination, source.width, source.height, transform)
    else {
        return;
    };
    let Some(drawn_rect) = scene.drawn_rect() else {
        return;
    };
    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination)
        .intersect(drawn_rect)
        .unwrap_or(PixelRect {
            x: destination.x,
            y: destination.y,
            width: 0,
            height: 0,
        });

    if target.width == 0 || target.height == 0 {
        return;
    }

    if transform.rotation_degrees.rem_euclid(360.0) != 0.0 || transform.flip_x || transform.flip_y {
        for dst_y in target.y..target.y + target.height {
            for dst_x in target.x..target.x + target.width {
                let Some(source_point) = scene.map_view_to_canvas(
                    app_core::CanvasViewportPoint::new(dst_x as i32, dst_y as i32),
                ) else {
                    continue;
                };
                let src_index = (source_point.y * source.width + source_point.x) * 4;
                let dst_index = (dst_y * frame.width + dst_x) * 4;
                frame.pixels[dst_index..dst_index + 4]
                    .copy_from_slice(&source.pixels[src_index..src_index + 4]);
            }
        }
        return;
    }

    let (offset_x, offset_y) = scene.offset();
    let scale = scene.scale();

    if scale < 1.0 {
        // ズームアウト時は bilinear 補間を使って縮小アンチエイリアスを適用する
        for dst_y in target.y..target.y + target.height {
            let src_y_f = (dst_y as f32 + 0.5 - offset_y) / scale;
            let y0 = (src_y_f - 0.5).floor() as i64;
            let y1 = y0 + 1;
            let wy = src_y_f - 0.5 - y0 as f32;
            let y0c = y0.clamp(0, source.height as i64 - 1) as usize;
            let y1c = y1.clamp(0, source.height as i64 - 1) as usize;
            for dst_x in target.x..target.x + target.width {
                let src_x_f = (dst_x as f32 + 0.5 - offset_x) / scale;
                let x0 = (src_x_f - 0.5).floor() as i64;
                let x1 = x0 + 1;
                let wx = src_x_f - 0.5 - x0 as f32;
                let x0c = x0.clamp(0, source.width as i64 - 1) as usize;
                let x1c = x1.clamp(0, source.width as i64 - 1) as usize;

                let sample = |sx: usize, sy: usize| -> [f32; 4] {
                    let i = (sy * source.width + sx) * 4;
                    [
                        source.pixels[i] as f32,
                        source.pixels[i + 1] as f32,
                        source.pixels[i + 2] as f32,
                        source.pixels[i + 3] as f32,
                    ]
                };
                let p00 = sample(x0c, y0c);
                let p10 = sample(x1c, y0c);
                let p01 = sample(x0c, y1c);
                let p11 = sample(x1c, y1c);

                let lerp = |a: f32, b: f32, t: f32| -> f32 { a + (b - a) * t };
                let to_u8 = |v: f32| -> u8 { v.round().clamp(0.0, 255.0) as u8 };
                let pixel = [
                    to_u8(lerp(lerp(p00[0], p10[0], wx), lerp(p01[0], p11[0], wx), wy)),
                    to_u8(lerp(lerp(p00[1], p10[1], wx), lerp(p01[1], p11[1], wx), wy)),
                    to_u8(lerp(lerp(p00[2], p10[2], wx), lerp(p01[2], p11[2], wx), wy)),
                    to_u8(lerp(lerp(p00[3], p10[3], wx), lerp(p01[3], p11[3], wx), wy)),
                ];
                let dst_index = (dst_y * frame.width + dst_x) * 4;
                frame.pixels[dst_index..dst_index + 4].copy_from_slice(&pixel);
            }
        }
        return;
    }

    let src_x_runs = build_source_axis_runs(target.x, target.width, offset_x, scale, source.width);
    let src_y_runs =
        build_source_axis_runs(target.y, target.height, offset_y, scale, source.height);

    for y_run in &src_y_runs {
        let src_row_start = y_run.src_index * source.width * 4;
        for x_run in &src_x_runs {
            let src_offset = src_row_start + x_run.src_index * 4;
            let pixel = [
                source.pixels[src_offset],
                source.pixels[src_offset + 1],
                source.pixels[src_offset + 2],
                source.pixels[src_offset + 3],
            ];
            fill_rgba_block(
                frame,
                target.x + x_run.dst_offset,
                target.y + y_run.dst_offset,
                x_run.len,
                y_run.len,
                pixel,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAxisRun {
    pub dst_offset: usize,
    pub len: usize,
    pub src_index: usize,
}

/// ソース axis runs を構築する。
pub fn build_source_axis_runs(
    destination_start: usize,
    destination_len: usize,
    offset: f32,
    scale: f32,
    source_len: usize,
) -> Vec<SourceAxisRun> {
    let mut runs: Vec<SourceAxisRun> = Vec::new();

    for index in 0..destination_len {
        let src = {
            let dst = destination_start + index;
            let src = ((dst as f32 + 0.5 - offset) / scale).floor();
            (0.0..source_len as f32)
                .contains(&src)
                .then_some(src as usize)
        };

        let Some(src_index) = src else {
            continue;
        };

        if let Some(last) = runs.last_mut()
            && last.src_index == src_index
            && last.dst_offset + last.len == index
        {
            last.len += 1;
            continue;
        }

        runs.push(SourceAxisRun {
            dst_offset: index,
            len: 1,
            src_index,
        });
    }

    runs
}

/// 塗りつぶし RGBA block に必要な描画内容を組み立てる。
pub fn fill_rgba_block(
    frame: &mut RenderFrame,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    if width == 0 || height == 0 || x >= frame.width || y >= frame.height {
        return;
    }

    let row_start_x = x * 4;
    let row_len = width.min(frame.width - x) * 4;
    let max_y = (y + height).min(frame.height);
    for yy in y..max_y {
        let row_start = yy * frame.width * 4 + row_start_x;
        fill_rgba_slice(&mut frame.pixels[row_start..row_start + row_len], color);
    }
}

/// スクロール キャンバス 領域 に必要な描画内容を組み立てる。
pub fn scroll_canvas_region(
    frame: &mut RenderFrame,
    region: PixelRect,
    delta_x: i32,
    delta_y: i32,
) -> PixelRect {
    if region.width == 0 || region.height == 0 {
        return region;
    }

    let shift_x = delta_x.clamp(-(region.width as i32), region.width as i32);
    let shift_y = delta_y.clamp(-(region.height as i32), region.height as i32);
    let overlap_width = region.width.saturating_sub(shift_x.unsigned_abs() as usize);
    let overlap_height = region
        .height
        .saturating_sub(shift_y.unsigned_abs() as usize);

    if overlap_width == 0 || overlap_height == 0 {
        return region;
    }

    let src_x = if shift_x >= 0 {
        region.x
    } else {
        region.x + shift_x.unsigned_abs() as usize
    };
    let src_y = if shift_y >= 0 {
        region.y
    } else {
        region.y + shift_y.unsigned_abs() as usize
    };
    let dst_x = if shift_x >= 0 {
        region.x + shift_x as usize
    } else {
        region.x
    };
    let dst_y = if shift_y >= 0 {
        region.y + shift_y as usize
    } else {
        region.y
    };

    let mut copied = vec![0; overlap_width * overlap_height * 4];
    for row in 0..overlap_height {
        let src_row_start = ((src_y + row) * frame.width + src_x) * 4;
        let src_row_end = src_row_start + overlap_width * 4;
        let dst_row_start = row * overlap_width * 4;
        copied[dst_row_start..dst_row_start + overlap_width * 4]
            .copy_from_slice(&frame.pixels[src_row_start..src_row_end]);
    }

    for row in 0..overlap_height {
        let dst_row_start = ((dst_y + row) * frame.width + dst_x) * 4;
        let src_row_start = row * overlap_width * 4;
        frame.pixels[dst_row_start..dst_row_start + overlap_width * 4]
            .copy_from_slice(&copied[src_row_start..src_row_start + overlap_width * 4]);
    }

    exposed_scroll_rect(region, shift_x, shift_y)
}

/// exposed スクロール 矩形 を計算して返す。
fn exposed_scroll_rect(region: PixelRect, shift_x: i32, shift_y: i32) -> PixelRect {
    if shift_x.unsigned_abs() as usize >= region.width
        || shift_y.unsigned_abs() as usize >= region.height
    {
        return region;
    }

    let mut exposed = None;
    if shift_x > 0 {
        exposed = Some(PixelRect {
            x: region.x,
            y: region.y,
            width: shift_x as usize,
            height: region.height,
        });
    } else if shift_x < 0 {
        let width = shift_x.unsigned_abs() as usize;
        exposed = Some(PixelRect {
            x: region.x + region.width - width,
            y: region.y,
            width,
            height: region.height,
        });
    }

    if shift_y > 0 {
        let rect = PixelRect {
            x: region.x,
            y: region.y,
            width: region.width,
            height: shift_y as usize,
        };
        exposed = Some(exposed.map_or(rect, |existing: PixelRect| existing.union(rect)));
    } else if shift_y < 0 {
        let height = shift_y.unsigned_abs() as usize;
        let rect = PixelRect {
            x: region.x,
            y: region.y + region.height - height,
            width: region.width,
            height,
        };
        exposed = Some(exposed.map_or(rect, |existing: PixelRect| existing.union(rect)));
    }

    exposed.unwrap_or(region)
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_brush_preview(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source: CanvasCompositeSource<'_>,
    transform: app_core::CanvasViewTransform,
    canvas_position: CanvasPoint,
    brush_size: u32,
    dirty_rect: Option<PixelRect>,
) {
    if source.width == 0 || source.height == 0 {
        return;
    }
    let Some(scene) =
        crate::prepare_canvas_scene(destination, source.width, source.height, transform)
    else {
        return;
    };
    let Some(center) = scene.map_canvas_point_to_display(canvas_position) else {
        return;
    };
    let radius = ((brush_size.max(1) as f32 * scene.scale()) * 0.5).max(4.0);
    let Some(target) = crate::brush_preview_rect_for_diameter(
        destination,
        source.width,
        source.height,
        transform,
        canvas_position,
        brush_size as f32,
    )
    .and_then(|rect| match dirty_rect {
        Some(dirty) => rect.intersect(dirty),
        None => Some(rect),
    }) else {
        return;
    };
    for y in target.y..target.y + target.height {
        let row_start = y * frame.width * 4;
        for x in target.x..target.x + target.width {
            let dx = x as f32 + 0.5 - center.x;
            let dy = y as f32 + 0.5 - center.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if (distance - radius).abs() <= 1.0 {
                let index = row_start + x * 4;
                frame.pixels[index..index + 4].copy_from_slice(&[0x9f, 0xb7, 0xff, 0xff]);
            }
        }
    }
}

/// 描画 投げ縄 プレビュー に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_lasso_preview(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source: CanvasCompositeSource<'_>,
    transform: app_core::CanvasViewTransform,
    points: &[CanvasPoint],
    dirty_rect: Option<PixelRect>,
) {
    if points.len() < 2 || source.width == 0 || source.height == 0 {
        return;
    }

    for window in points.windows(2) {
        let Some(start) = map_canvas_point_to_display(
            destination,
            source.width,
            source.height,
            transform,
            window[0],
        ) else {
            continue;
        };
        let Some(end) = map_canvas_point_to_display(
            destination,
            source.width,
            source.height,
            transform,
            window[1],
        ) else {
            continue;
        };
        draw_overlay_line(frame, start, end, dirty_rect, [0xff, 0xc1, 0x07, 0xff]);
    }
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_overlay_line(
    frame: &mut RenderFrame,
    start: CanvasDisplayPoint,
    end: CanvasDisplayPoint,
    dirty_rect: Option<PixelRect>,
    color: [u8; 4],
) {
    let min_x = start.x.min(end.x).floor().max(0.0) as usize;
    let min_y = start.y.min(end.y).floor().max(0.0) as usize;
    let max_x = start.x.max(end.x).ceil().min(frame.width as f32) as usize;
    let max_y = start.y.max(end.y).ceil().min(frame.height as f32) as usize;
    let bounds = PixelRect {
        x: min_x.saturating_sub(2),
        y: min_y.saturating_sub(2),
        width: max_x.saturating_sub(min_x).saturating_add(4),
        height: max_y.saturating_sub(min_y).saturating_add(4),
    };
    let Some(target) = (match dirty_rect {
        Some(dirty) => bounds.intersect(dirty),
        None => Some(bounds),
    }) else {
        return;
    };

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f32::EPSILON {
        return;
    }

    for y in target.y..target.y + target.height {
        let row_start = y * frame.width * 4;
        for x in target.x..target.x + target.width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let t = (((px - start.x) * dx + (py - start.y) * dy) / length_sq).clamp(0.0, 1.0);
            let closest_x = start.x + dx * t;
            let closest_y = start.y + dy * t;
            let distance = ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt();
            if distance <= 1.25 {
                let index = row_start + x * 4;
                frame.pixels[index..index + 4].copy_from_slice(&color);
            }
        }
    }
}
