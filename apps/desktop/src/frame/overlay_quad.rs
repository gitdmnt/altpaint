//! L3 一時オーバーレイ (アクティブパネルマスク・ブラシプレビュー・ラッソ・コマ作成
//! プレビュー・コマ navigator) の GPU 直描画用 DTO とビルダ群。
//!
//! `SolidQuad` は AABB の塗り/枠用 (背景や枠線と共有)、`CircleQuad` は
//! ブラシプレビュー円リング、`LineQuad` はラッソ線分カプセル。各々
//! `WgpuPresenter` の専用パイプラインへ渡される。

use app_core::CanvasDirtyRect;
use desktop_support::{
    ACTIVE_PANEL_BORDER, ACTIVE_PANEL_FILL, ACTIVE_PANEL_MASK, BRUSH_PREVIEW_RING, LASSO_LINE,
    PANEL_NAVIGATOR_ACTIVE, PANEL_NAVIGATOR_BACKGROUND, PANEL_NAVIGATOR_BORDER,
    PANEL_NAVIGATOR_PANEL, PANEL_PREVIEW_BORDER, PANEL_PREVIEW_FILL,
};
use render_types::{CanvasOverlayState, FramePlan, PanelNavigatorOverlay};

use super::Rect;
use super::solid_quad::{SolidQuad, push_border_quads};

/// ブラシプレビュー円リングの線幅 (px)。
const BRUSH_RING_THICKNESS: f32 = 1.0;
/// ラッソプレビュー線のカプセル半径 (px)。
const LASSO_LINE_THICKNESS: f32 = 1.25;

/// GPU で描画する円リングプリミティブ。
///
/// `radius` を中心からの距離、`thickness` をリング片側の幅 (px) として
/// `|distance(p, center) - radius| <= thickness` の領域に色を載せる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CircleQuad {
    pub(crate) center_px: [f32; 2],
    pub(crate) radius: f32,
    pub(crate) thickness: f32,
    pub(crate) color: [u8; 4],
}

/// GPU で描画する線分カプセル。
///
/// `start_px` から `end_px` まで太さ `thickness` のカプセル形状で塗る。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineQuad {
    pub(crate) start_px: [f32; 2],
    pub(crate) end_px: [f32; 2],
    pub(crate) thickness: f32,
    pub(crate) color: [u8; 4],
}

/// L3 用の AABB 単色矩形を組み立てる。
///
/// 内訳:
/// - active panel mask (外側 4 矩形 fill + 内側 fill + 4 矩形分解枠線)
/// - panel creation preview (fill + 4 矩形分解枠線)
/// - panel navigator (背景 fill + 外枠 + 内枠 + 各 panel fill + 各 panel 枠線)
pub(crate) fn build_overlay_solid_quads(
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
) -> Vec<SolidQuad> {
    let mut quads = Vec::new();
    if let Some(bounds) = overlay.active_panel_bounds {
        push_active_panel_mask(&mut quads, plan, bounds);
    }
    if let Some(bounds) = overlay.panel_creation_preview {
        push_panel_creation_preview(&mut quads, plan, bounds);
    }
    if let Some(navigator) = overlay.panel_navigator.as_ref() {
        push_panel_navigator(&mut quads, plan, navigator);
    }
    quads
}

/// L3 用のブラシプレビュー円リング quad を組み立てる。
pub(crate) fn build_overlay_circle_quads(
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
) -> Vec<CircleQuad> {
    let (Some(position), Some(brush_size)) = (overlay.brush_preview, overlay.brush_size) else {
        return Vec::new();
    };
    let Some(scene) = plan.canvas.scene() else {
        return Vec::new();
    };
    let Some(center) = scene.map_canvas_point_to_display(position) else {
        return Vec::new();
    };
    let radius = ((brush_size.max(1) as f32 * scene.scale()) * 0.5).max(4.0);
    vec![CircleQuad {
        center_px: [center.x, center.y],
        radius,
        thickness: BRUSH_RING_THICKNESS,
        color: BRUSH_PREVIEW_RING,
    }]
}

/// L3 用のラッソ線分 quad を組み立てる。
pub(crate) fn build_overlay_line_quads(
    plan: &FramePlan<'_>,
    overlay: &CanvasOverlayState,
) -> Vec<LineQuad> {
    if overlay.lasso_points.len() < 2 {
        return Vec::new();
    }
    let Some(scene) = plan.canvas.scene() else {
        return Vec::new();
    };
    let mut quads = Vec::with_capacity(overlay.lasso_points.len().saturating_sub(1));
    for window in overlay.lasso_points.windows(2) {
        let (Some(start), Some(end)) = (
            scene.map_canvas_point_to_display(window[0]),
            scene.map_canvas_point_to_display(window[1]),
        ) else {
            continue;
        };
        quads.push(LineQuad {
            start_px: [start.x, start.y],
            end_px: [end.x, end.y],
            thickness: LASSO_LINE_THICKNESS,
            color: LASSO_LINE,
        });
    }
    quads
}

fn push_active_panel_mask(
    out: &mut Vec<SolidQuad>,
    plan: &FramePlan<'_>,
    bounds: app_core::PanelBounds,
) {
    let source_width = plan.canvas.source_width;
    let source_height = plan.canvas.source_height;
    if source_width == 0 || source_height == 0 || bounds.width == 0 || bounds.height == 0 {
        return;
    }

    let outside_regions = [
        CanvasDirtyRect {
            x: 0,
            y: 0,
            width: source_width,
            height: bounds.y,
        },
        CanvasDirtyRect {
            x: 0,
            y: bounds.y.saturating_add(bounds.height),
            width: source_width,
            height: source_height.saturating_sub(bounds.y.saturating_add(bounds.height)),
        },
        CanvasDirtyRect {
            x: 0,
            y: bounds.y,
            width: bounds.x,
            height: bounds.height,
        },
        CanvasDirtyRect {
            x: bounds.x.saturating_add(bounds.width),
            y: bounds.y,
            width: source_width.saturating_sub(bounds.x.saturating_add(bounds.width)),
            height: bounds.height,
        },
    ];
    for region in outside_regions
        .into_iter()
        .filter(|r| r.width > 0 && r.height > 0)
    {
        let rect = plan.canvas.map_dirty_rect(region);
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        out.push(SolidQuad {
            rect,
            color: ACTIVE_PANEL_MASK,
        });
    }

    let panel_rect = plan.canvas.map_dirty_rect(CanvasDirtyRect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    });
    if panel_rect.width == 0 || panel_rect.height == 0 {
        return;
    }
    out.push(SolidQuad {
        rect: panel_rect,
        color: ACTIVE_PANEL_FILL,
    });
    push_border_quads(out, panel_rect, ACTIVE_PANEL_BORDER);
}

fn push_panel_creation_preview(
    out: &mut Vec<SolidQuad>,
    plan: &FramePlan<'_>,
    bounds: app_core::PanelBounds,
) {
    if plan.canvas.source_width == 0
        || plan.canvas.source_height == 0
        || bounds.width == 0
        || bounds.height == 0
    {
        return;
    }
    let rect = plan.canvas.map_dirty_rect(CanvasDirtyRect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    });
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    out.push(SolidQuad {
        rect,
        color: PANEL_PREVIEW_FILL,
    });
    push_border_quads(out, rect, PANEL_PREVIEW_BORDER);
}

fn push_panel_navigator(
    out: &mut Vec<SolidQuad>,
    plan: &FramePlan<'_>,
    navigator: &PanelNavigatorOverlay,
) {
    let canvas_host = plan.canvas.host_rect;
    if navigator.page_width == 0
        || navigator.page_height == 0
        || navigator.panels.len() <= 1
        || canvas_host.width == 0
        || canvas_host.height == 0
    {
        return;
    }

    let max_width = canvas_host.width.clamp(96, 180);
    let max_height = canvas_host.height.clamp(96, 180);
    let inner_max_width = max_width.saturating_sub(16).max(1);
    let inner_max_height = max_height.saturating_sub(16).max(1);
    let scale_x = inner_max_width as f32 / navigator.page_width as f32;
    let scale_y = inner_max_height as f32 / navigator.page_height as f32;
    let scale = scale_x.min(scale_y).max(f32::EPSILON);
    let scaled_width = ((navigator.page_width as f32 * scale).round() as usize).max(1);
    let scaled_height = ((navigator.page_height as f32 * scale).round() as usize).max(1);
    let outer = Rect {
        x: canvas_host
            .x
            .saturating_add(canvas_host.width)
            .saturating_sub(scaled_width + 16)
            .saturating_sub(12),
        y: canvas_host.y + 12,
        width: scaled_width + 16,
        height: scaled_height + 16,
    };

    out.push(SolidQuad {
        rect: outer,
        color: PANEL_NAVIGATOR_BACKGROUND,
    });
    push_border_quads(out, outer, PANEL_NAVIGATOR_BORDER);
    let inner = Rect {
        x: outer.x + 8,
        y: outer.y + 8,
        width: scaled_width,
        height: scaled_height,
    };
    push_border_quads(out, inner, PANEL_NAVIGATOR_BORDER);

    for panel in &navigator.panels {
        let rect = Rect {
            x: inner.x + ((panel.bounds.x as f32 * scale).round() as usize),
            y: inner.y + ((panel.bounds.y as f32 * scale).round() as usize),
            width: ((panel.bounds.width as f32 * scale).round() as usize).max(1),
            height: ((panel.bounds.height as f32 * scale).round() as usize).max(1),
        };
        let fill_color = if panel.active {
            [
                PANEL_NAVIGATOR_ACTIVE[0],
                PANEL_NAVIGATOR_ACTIVE[1],
                PANEL_NAVIGATOR_ACTIVE[2],
                0x40,
            ]
        } else {
            PANEL_NAVIGATOR_PANEL
        };
        out.push(SolidQuad {
            rect,
            color: fill_color,
        });
        let border_color = if panel.active {
            PANEL_NAVIGATOR_ACTIVE
        } else {
            PANEL_NAVIGATOR_BORDER
        };
        push_border_quads(out, rect, border_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::{CanvasPoint, CanvasViewTransform, PanelBounds};
    use render_types::{CanvasCompositeSource, PanelNavigatorEntry, PanelSurfaceSource, PixelRect};

    fn pixels_for(width: usize, height: usize) -> Vec<u8> {
        vec![0; width * height * 4]
    }

    fn make_plan<'a>(
        canvas_pixels: &'a [u8],
        canvas_width: usize,
        canvas_height: usize,
        panel_pixels: &'a [u8],
    ) -> FramePlan<'a> {
        FramePlan::new(
            canvas_width,
            canvas_height,
            PixelRect {
                x: 0,
                y: 0,
                width: canvas_width,
                height: canvas_height,
            },
            PanelSurfaceSource {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                pixels: panel_pixels,
            },
            CanvasCompositeSource {
                width: canvas_width,
                height: canvas_height,
                pixels: canvas_pixels,
            },
            CanvasViewTransform::default(),
            "",
        )
    }

    #[test]
    fn empty_overlay_returns_no_quads() {
        let canvas = pixels_for(64, 64);
        let panel = pixels_for(1, 1);
        let plan = make_plan(&canvas, 64, 64, &panel);
        let overlay = CanvasOverlayState::default();
        assert!(build_overlay_solid_quads(&plan, &overlay).is_empty());
        assert!(build_overlay_circle_quads(&plan, &overlay).is_empty());
        assert!(build_overlay_line_quads(&plan, &overlay).is_empty());
    }

    #[test]
    fn active_panel_mask_emits_inside_fill_and_four_borders() {
        let canvas = pixels_for(64, 64);
        let panel = pixels_for(1, 1);
        let plan = make_plan(&canvas, 64, 64, &panel);
        let overlay = CanvasOverlayState {
            active_panel_bounds: Some(PanelBounds {
                x: 0,
                y: 0,
                width: 64,
                height: 64,
            }),
            ..CanvasOverlayState::default()
        };
        let quads = build_overlay_solid_quads(&plan, &overlay);
        let fills = quads
            .iter()
            .filter(|q| q.color == ACTIVE_PANEL_FILL)
            .count();
        let borders = quads
            .iter()
            .filter(|q| q.color == ACTIVE_PANEL_BORDER)
            .count();
        assert_eq!(fills, 1);
        assert_eq!(borders, 4);
    }

    #[test]
    fn brush_preview_emits_single_circle_quad_with_expected_radius() {
        let canvas = pixels_for(64, 64);
        let panel = pixels_for(1, 1);
        let plan = make_plan(&canvas, 64, 64, &panel);
        let overlay = CanvasOverlayState {
            brush_preview: Some(CanvasPoint::new(32, 32)),
            brush_size: Some(10),
            ..CanvasOverlayState::default()
        };
        let quads = build_overlay_circle_quads(&plan, &overlay);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].color, BRUSH_PREVIEW_RING);
        assert_eq!(quads[0].thickness, BRUSH_RING_THICKNESS);
        let scale = plan.canvas.scene().expect("scene").scale();
        let expected_radius = ((10.0_f32 * scale) * 0.5).max(4.0);
        assert!((quads[0].radius - expected_radius).abs() < 0.001);
    }

    #[test]
    fn lasso_three_points_produce_two_segments() {
        let canvas = pixels_for(64, 64);
        let panel = pixels_for(1, 1);
        let plan = make_plan(&canvas, 64, 64, &panel);
        let overlay = CanvasOverlayState {
            lasso_points: vec![
                CanvasPoint::new(8, 8),
                CanvasPoint::new(40, 24),
                CanvasPoint::new(56, 56),
            ],
            ..CanvasOverlayState::default()
        };
        let quads = build_overlay_line_quads(&plan, &overlay);
        assert_eq!(quads.len(), 2);
        for quad in &quads {
            assert_eq!(quad.color, LASSO_LINE);
            assert_eq!(quad.thickness, LASSO_LINE_THICKNESS);
        }
    }

    #[test]
    fn panel_navigator_emits_background_and_per_panel_quads() {
        let canvas = pixels_for(120, 120);
        let panel = pixels_for(1, 1);
        let plan = make_plan(&canvas, 120, 120, &panel);
        let overlay = CanvasOverlayState {
            panel_navigator: Some(PanelNavigatorOverlay {
                page_width: 100,
                page_height: 80,
                panels: vec![
                    PanelNavigatorEntry {
                        bounds: PanelBounds {
                            x: 0,
                            y: 0,
                            width: 50,
                            height: 80,
                        },
                        active: true,
                    },
                    PanelNavigatorEntry {
                        bounds: PanelBounds {
                            x: 50,
                            y: 0,
                            width: 50,
                            height: 80,
                        },
                        active: false,
                    },
                ],
            }),
            ..CanvasOverlayState::default()
        };
        let quads = build_overlay_solid_quads(&plan, &overlay);
        let backgrounds = quads
            .iter()
            .filter(|q| q.color == PANEL_NAVIGATOR_BACKGROUND)
            .count();
        let outer_borders = quads
            .iter()
            .filter(|q| q.color == PANEL_NAVIGATOR_BORDER)
            .count();
        let active_quads = quads
            .iter()
            .filter(|q| q.color == PANEL_NAVIGATOR_ACTIVE)
            .count();
        let panel_fills = quads
            .iter()
            .filter(|q| q.color == PANEL_NAVIGATOR_PANEL)
            .count();
        assert_eq!(backgrounds, 1, "navigator outer fill");
        // outer 枠 (4) + inner 枠 (4) + 非 active panel の枠線 (4) = 12
        assert_eq!(outer_borders, 4 + 4 + 4);
        // active panel の枠線 (4 矩形分解。fill は alpha 0x40 で別色)
        assert_eq!(active_quads, 4);
        // 非 active panel の fill 1 個
        assert_eq!(panel_fills, 1);
    }

    #[test]
    fn panel_creation_preview_emits_fill_and_border() {
        let canvas = pixels_for(64, 64);
        let panel = pixels_for(1, 1);
        let plan = make_plan(&canvas, 64, 64, &panel);
        let overlay = CanvasOverlayState {
            panel_creation_preview: Some(PanelBounds {
                x: 8,
                y: 8,
                width: 32,
                height: 32,
            }),
            ..CanvasOverlayState::default()
        };
        let quads = build_overlay_solid_quads(&plan, &overlay);
        let fills = quads
            .iter()
            .filter(|q| q.color == PANEL_PREVIEW_FILL)
            .count();
        let borders = quads
            .iter()
            .filter(|q| q.color == PANEL_PREVIEW_BORDER)
            .count();
        assert_eq!(fills, 1);
        assert_eq!(borders, 4);
    }
}
