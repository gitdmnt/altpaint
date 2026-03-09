//! デスクトップ UI の固定レイアウト計算とソフトウェア合成を担当する。
//!
//! `DesktopApp` や `runtime` から純粋に近い描画計算を切り離し、
//! フレーム差分更新の基礎部品を提供する。

use app_core::{CanvasViewTransform, DirtyRect};
use desktop_support::{
    APP_BACKGROUND, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND, CANVAS_FRAME_BORDER,
    FOOTER_HEIGHT, HEADER_HEIGHT, PANEL_FRAME_BACKGROUND, PANEL_FRAME_BORDER,
    SIDEBAR_BACKGROUND, SIDEBAR_WIDTH, TEXT_PRIMARY, TEXT_SECONDARY, WINDOW_PADDING,
};
use ui_shell::{PanelSurface, draw_text_rgba, measure_text_width};

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

/// キャンバス合成元を、`RenderFrame` へ依存させずに渡すための軽量ビュー。
#[derive(Clone, Copy)]
pub(crate) struct CanvasCompositeSource<'a> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) pixels: &'a [u8],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CanvasOverlayState {
    pub(crate) brush_preview: Option<(usize, usize)>,
}

/// GPU 上で提示するテクスチャ付き矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextureQuad {
    pub(crate) destination: Rect,
    pub(crate) uv_min: [f32; 2],
    pub(crate) uv_max: [f32; 2],
}

/// 元画像を target 内へアスペクト比維持で収めた矩形を返す。
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

/// ビットマップ dirty rect を表示先の矩形へ写像する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn map_canvas_dirty_to_display(
    dirty: DirtyRect,
    destination: Rect,
    source_width: usize,
    source_height: usize,
) -> Rect {
    map_canvas_dirty_to_display_with_transform(
        dirty,
        destination,
        source_width,
        source_height,
        CanvasViewTransform::default(),
    )
}

pub(crate) fn map_canvas_dirty_to_display_with_transform(
    dirty: DirtyRect,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Rect {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return destination;
    }

    let clamped = dirty.clamp_to_bitmap(source_width, source_height);
    let metrics = canvas_transform_metrics(destination, source_width, source_height, transform);
    let start_x = (metrics.offset_x + clamped.x as f32 * metrics.scale).floor();
    let start_y = (metrics.offset_y + clamped.y as f32 * metrics.scale).floor();
    let end_x = (metrics.offset_x + (clamped.x + clamped.width) as f32 * metrics.scale).ceil();
    let end_y = (metrics.offset_y + (clamped.y + clamped.height) as f32 * metrics.scale).ceil();

    let clipped_left = start_x.max(destination.x as f32);
    let clipped_top = start_y.max(destination.y as f32);
    let clipped_right = end_x.min((destination.x + destination.width) as f32);
    let clipped_bottom = end_y.min((destination.y + destination.height) as f32);

    if clipped_left >= clipped_right || clipped_top >= clipped_bottom {
        return Rect {
            x: destination.x,
            y: destination.y,
            width: 0,
            height: 0,
        };
    }

    Rect {
        x: clipped_left as usize,
        y: clipped_top as usize,
        width: (clipped_right - clipped_left) as usize,
        height: (clipped_bottom - clipped_top) as usize,
    }
}

pub(crate) fn brush_preview_rect(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
    canvas_position: (usize, usize),
) -> Option<Rect> {
    if source_width == 0 || source_height == 0 || destination.width == 0 || destination.height == 0
    {
        return None;
    }

    let metrics = canvas_transform_metrics(destination, source_width, source_height, transform);
    let center_x = metrics.offset_x + (canvas_position.0 as f32 + 0.5) * metrics.scale;
    let center_y = metrics.offset_y + (canvas_position.1 as f32 + 0.5) * metrics.scale;
    let radius = metrics.scale.max(4.0);

    destination.intersect(Rect {
        x: (center_x - radius - 2.0).floor().max(destination.x as f32) as usize,
        y: (center_y - radius - 2.0).floor().max(destination.y as f32) as usize,
        width: ((center_x + radius + 2.0)
            .ceil()
            .min((destination.x + destination.width) as f32)
            - (center_x - radius - 2.0).floor().max(destination.x as f32))
        .max(1.0) as usize,
        height: ((center_y + radius + 2.0)
            .ceil()
            .min((destination.y + destination.height) as f32)
            - (center_y - radius - 2.0).floor().max(destination.y as f32))
        .max(1.0) as usize,
    })
}

pub(crate) fn canvas_drawn_rect(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<Rect> {
    if source_width == 0 || source_height == 0 || destination.width == 0 || destination.height == 0
    {
        return None;
    }

    let metrics = canvas_transform_metrics(destination, source_width, source_height, transform);
    let left = metrics.offset_x.floor();
    let top = metrics.offset_y.floor();
    let right = (metrics.offset_x + source_width as f32 * metrics.scale).ceil();
    let bottom = (metrics.offset_y + source_height as f32 * metrics.scale).ceil();

    let clipped_left = left.max(destination.x as f32);
    let clipped_top = top.max(destination.y as f32);
    let clipped_right = right.min((destination.x + destination.width) as f32);
    let clipped_bottom = bottom.min((destination.y + destination.height) as f32);

    (clipped_left < clipped_right && clipped_top < clipped_bottom).then_some(Rect {
        x: clipped_left as usize,
        y: clipped_top as usize,
        width: (clipped_right - clipped_left) as usize,
        height: (clipped_bottom - clipped_top) as usize,
    })
}

pub(crate) fn exposed_canvas_background_rect(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    previous_transform: CanvasViewTransform,
    current_transform: CanvasViewTransform,
) -> Option<Rect> {
    let previous = canvas_drawn_rect(destination, source_width, source_height, previous_transform)?;
    let current = canvas_drawn_rect(destination, source_width, source_height, current_transform);

    let Some(current) = current else {
        return Some(previous);
    };

    if previous == current {
        return None;
    }

    let overlap = previous.intersect(current);
    let mut exposed = Vec::with_capacity(4);
    match overlap {
        None => exposed.push(previous),
        Some(overlap) => {
            let candidates = [
                Rect {
                    x: previous.x,
                    y: previous.y,
                    width: previous.width,
                    height: overlap.y.saturating_sub(previous.y),
                },
                Rect {
                    x: previous.x,
                    y: overlap.y + overlap.height,
                    width: previous.width,
                    height: (previous.y + previous.height)
                        .saturating_sub(overlap.y + overlap.height),
                },
                Rect {
                    x: previous.x,
                    y: overlap.y,
                    width: overlap.x.saturating_sub(previous.x),
                    height: overlap.height,
                },
                Rect {
                    x: overlap.x + overlap.width,
                    y: overlap.y,
                    width: (previous.x + previous.width).saturating_sub(overlap.x + overlap.width),
                    height: overlap.height,
                },
            ];
            for rect in candidates {
                if rect.width > 0 && rect.height > 0 {
                    exposed.push(rect);
                }
            }
        }
    }

    exposed.into_iter().reduce(|acc, rect| acc.union(rect))
}

pub(crate) fn canvas_texture_quad(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<TextureQuad> {
    if source_width == 0 || source_height == 0 || destination.width == 0 || destination.height == 0
    {
        return None;
    }

    let metrics = canvas_transform_metrics(destination, source_width, source_height, transform);
    let drawn_rect = canvas_drawn_rect(destination, source_width, source_height, transform)?;
    let left =
        ((drawn_rect.x as f32 - metrics.offset_x) / metrics.scale).clamp(0.0, source_width as f32);
    let top =
        ((drawn_rect.y as f32 - metrics.offset_y) / metrics.scale).clamp(0.0, source_height as f32);
    let right = (((drawn_rect.x + drawn_rect.width) as f32 - metrics.offset_x) / metrics.scale)
        .clamp(0.0, source_width as f32);
    let bottom = (((drawn_rect.y + drawn_rect.height) as f32 - metrics.offset_y) / metrics.scale)
        .clamp(0.0, source_height as f32);

    Some(TextureQuad {
        destination: drawn_rect,
        uv_min: [left / source_width as f32, top / source_height as f32],
        uv_max: [right / source_width as f32, bottom / source_height as f32],
    })
}

struct CanvasTransformMetrics {
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

fn canvas_transform_metrics(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> CanvasTransformMetrics {
    let scale_x = destination.width as f32 / source_width as f32;
    let scale_y = destination.height as f32 / source_height as f32;
    let scale = (scale_x.min(scale_y) * transform.zoom.max(0.25)).max(f32::EPSILON);
    let drawn_width = source_width as f32 * scale;
    let drawn_height = source_height as f32 * scale;

    CanvasTransformMetrics {
        scale,
        offset_x: destination.x as f32
            + (destination.width as f32 - drawn_width) * 0.5
            + transform.pan_x,
        offset_y: destination.y as f32
            + (destination.height as f32 - drawn_height) * 0.5
            + transform.pan_y,
    }
}

/// パネル面・キャンバス面・ステータス表示をまとめて最終フレームへ合成する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compose_base_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
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
    fill_canvas_host_background(
        &mut frame,
        layout,
        canvas,
        transform,
        layout.canvas_host_rect,
    );
    stroke_rect(&mut frame, layout.canvas_host_rect, CANVAS_FRAME_BORDER);

    blit_scaled_rgba(
        &mut frame,
        layout.panel_surface_rect,
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels.as_slice(),
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
        "Canvas host (winit + wgpu canvas texture)",
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

pub(crate) fn compose_overlay_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    overlay: CanvasOverlayState,
) -> render::RenderFrame {
    let mut frame = render::RenderFrame {
        width,
        height,
        pixels: vec![0; width * height * 4],
    };
    compose_overlay_region(&mut frame, layout, canvas, transform, overlay, None);
    frame
}

pub(crate) fn compose_overlay_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
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
}

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

fn fill_canvas_host_background(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    canvas: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    dirty_rect: Rect,
) {
    let display = layout.canvas_host_rect;
    if let Some(display_region) = display.intersect(dirty_rect) {
        if let Some(drawn_rect) = canvas_drawn_rect(display, canvas.width, canvas.height, transform)
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
}

/// パネルホスト領域だけを差分再合成する。
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
    let text_width = measure_text_width(status_text).saturating_add(16).max(1);
    Rect {
        x: layout.canvas_host_rect.x,
        y: height.saturating_sub(FOOTER_HEIGHT),
        width: text_width.min(width.saturating_sub(layout.canvas_host_rect.x)),
        height: FOOTER_HEIGHT,
    }
}

/// ビュー座標をパネルサーフェス座標へ変換する。
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

/// ビュー外座標もクランプしたうえでサーフェス座標へ変換する。
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

/// ビットマップ文字描画を `RenderFrame` 向けに薄くラップする。
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

/// 単色矩形をフレームへ塗り込む。
fn fill_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
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

/// 単色枠線をフレームへ描画する。
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

#[cfg_attr(not(test), allow(dead_code))]
fn stroke_rect_region(
    frame: &mut render::RenderFrame,
    rect: Rect,
    dirty_rect: Rect,
    color: [u8; 4],
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let edges = [
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        Rect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        Rect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        Rect {
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

/// RGBA ソースをスケーリングしつつ dirty rect 範囲だけ転送する。
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

/// RGBA ソース全体を destination へスケーリング転送する。
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

#[cfg_attr(not(test), allow(dead_code))]
fn blit_canvas_with_transform(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    dirty_rect: Option<Rect>,
) {
    if destination.width == 0
        || destination.height == 0
        || source.width == 0
        || source.height == 0
        || source.pixels.len() < source.width * source.height * 4
    {
        return;
    }

    let metrics = canvas_transform_metrics(destination, source.width, source.height, transform);
    let Some(drawn_rect) = canvas_drawn_rect(destination, source.width, source.height, transform)
    else {
        return;
    };
    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination)
        .intersect(drawn_rect)
        .unwrap_or(Rect {
            x: destination.x,
            y: destination.y,
            width: 0,
            height: 0,
        });

    if target.width == 0 || target.height == 0 {
        return;
    }

    let src_x_runs = build_source_axis_runs(
        target.x,
        target.width,
        metrics.offset_x,
        metrics.scale,
        source.width,
    );
    let src_y_runs = build_source_axis_runs(
        target.y,
        target.height,
        metrics.offset_y,
        metrics.scale,
        source.height,
    );

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

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceAxisRun {
    dst_offset: usize,
    len: usize,
    src_index: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
fn build_source_axis_runs(
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

#[cfg_attr(not(test), allow(dead_code))]
fn fill_rgba_block(
    frame: &mut render::RenderFrame,
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

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn scroll_canvas_region(
    frame: &mut render::RenderFrame,
    region: Rect,
    delta_x: i32,
    delta_y: i32,
) -> Rect {
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

#[cfg_attr(not(test), allow(dead_code))]
fn exposed_scroll_rect(region: Rect, shift_x: i32, shift_y: i32) -> Rect {
    if shift_x.unsigned_abs() as usize >= region.width
        || shift_y.unsigned_abs() as usize >= region.height
    {
        return region;
    }

    let mut exposed = None;
    if shift_x > 0 {
        exposed = Some(Rect {
            x: region.x,
            y: region.y,
            width: shift_x as usize,
            height: region.height,
        });
    } else if shift_x < 0 {
        let width = shift_x.unsigned_abs() as usize;
        exposed = Some(Rect {
            x: region.x + region.width - width,
            y: region.y,
            width,
            height: region.height,
        });
    }

    if shift_y > 0 {
        let rect = Rect {
            x: region.x,
            y: region.y,
            width: region.width,
            height: shift_y as usize,
        };
        exposed = Some(exposed.map_or(rect, |existing| existing.union(rect)));
    } else if shift_y < 0 {
        let height = shift_y.unsigned_abs() as usize;
        let rect = Rect {
            x: region.x,
            y: region.y + region.height - height,
            width: region.width,
            height,
        };
        exposed = Some(exposed.map_or(rect, |existing| existing.union(rect)));
    }

    exposed.unwrap_or(region)
}

fn draw_brush_preview(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    canvas_position: (usize, usize),
    dirty_rect: Option<Rect>,
) {
    if source.width == 0 || source.height == 0 {
        return;
    }
    let metrics = canvas_transform_metrics(destination, source.width, source.height, transform);
    let center_x = metrics.offset_x + (canvas_position.0 as f32 + 0.5) * metrics.scale;
    let center_y = metrics.offset_y + (canvas_position.1 as f32 + 0.5) * metrics.scale;
    let radius = metrics.scale.max(4.0);
    let Some(target) = brush_preview_rect(
        destination,
        source.width,
        source.height,
        transform,
        canvas_position,
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
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            if (distance - radius).abs() <= 1.0 {
                let index = row_start + x * 4;
                frame.pixels[index..index + 4].copy_from_slice(&[0x9f, 0xb7, 0xff, 0xff]);
            }
        }
    }
}

#[cfg(test)]
mod tests;
