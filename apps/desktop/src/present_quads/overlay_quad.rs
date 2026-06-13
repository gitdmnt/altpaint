//! L3 一時オーバーレイ (アクティブパネルマスク・ブラシプレビュー・ラッソ・コマ作成
//! プレビュー・コマ navigator) の GPU 直描画用 DTO とビルダ群。
//!
//! `SolidQuad` は AABB の塗り/枠用 (背景や枠線と共有)、`CircleQuad` は
//! ブラシプレビュー円リング、`LineQuad` はラッソ線分カプセル。各々
//! `WgpuPresenter` の専用パイプラインへ渡される。

use ::geometry::{PageDirtyRect, WindowRect};
use crate::theme::{
    ACTIVE_KOMA_BORDER, ACTIVE_KOMA_FILL, ACTIVE_KOMA_MASK, BRUSH_PREVIEW_RING, LASSO_LINE,
    KOMA_NAVIGATOR_ACTIVE, KOMA_NAVIGATOR_BACKGROUND, KOMA_NAVIGATOR_BORDER,
    KOMA_NAVIGATOR_KOMA, KOMA_PREVIEW_BORDER, KOMA_PREVIEW_FILL,
};
use super::canvas_plan::CanvasPlan;
use super::overlay_state::{CanvasOverlayState, KomaNavigatorOverlay};
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
/// - koma creation preview (fill + 4 矩形分解枠線)
/// - koma navigator (背景 fill + 外枠 + 内枠 + 各コマ fill + 各コマ枠線)
pub(crate) fn build_overlay_solid_quads(
    plan: &CanvasPlan,
    overlay: &CanvasOverlayState,
) -> Vec<SolidQuad> {
    let mut quads = Vec::new();
    if let Some(bounds) = overlay.active_koma_bounds {
        push_active_koma_mask(&mut quads, plan, bounds);
    }
    if let Some(bounds) = overlay.panel_creation_preview {
        push_koma_creation_preview(&mut quads, plan, bounds);
    }
    if let Some(navigator) = overlay.koma_navigator.as_ref() {
        push_koma_navigator(&mut quads, plan, navigator);
    }
    quads
}

/// L3 用のブラシプレビュー円リング quad を組み立てる。
pub(crate) fn build_overlay_circle_quads(
    plan: &CanvasPlan,
    overlay: &CanvasOverlayState,
) -> Vec<CircleQuad> {
    let (Some(position), Some(brush_size)) = (overlay.brush_preview, overlay.brush_size) else {
        return Vec::new();
    };
    let Some(geometry) = plan.view_geometry() else {
        return Vec::new();
    };
    let Some(center) = geometry.map_canvas_point_to_display(position) else {
        return Vec::new();
    };
    let radius = ((brush_size.max(1) as f32 * geometry.scale()) * 0.5).max(4.0);
    vec![CircleQuad {
        center_px: [center.x, center.y],
        radius,
        thickness: BRUSH_RING_THICKNESS,
        color: BRUSH_PREVIEW_RING,
    }]
}

/// L3 用のラッソ線分 quad を組み立てる。
pub(crate) fn build_overlay_line_quads(
    plan: &CanvasPlan,
    overlay: &CanvasOverlayState,
) -> Vec<LineQuad> {
    if overlay.lasso_points.len() < 2 {
        return Vec::new();
    }
    let Some(geometry) = plan.view_geometry() else {
        return Vec::new();
    };
    let mut quads = Vec::with_capacity(overlay.lasso_points.len().saturating_sub(1));
    for window in overlay.lasso_points.windows(2) {
        let (Some(start), Some(end)) = (
            geometry.map_canvas_point_to_display(window[0]),
            geometry.map_canvas_point_to_display(window[1]),
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

fn push_active_koma_mask(
    out: &mut Vec<SolidQuad>,
    plan: &CanvasPlan,
    bounds: document_model::KomaBounds,
) {
    let source_width = plan.source_width;
    let source_height = plan.source_height;
    if source_width == 0 || source_height == 0 || bounds.width == 0 || bounds.height == 0 {
        return;
    }

    let outside_regions = [
        PageDirtyRect {
            x: 0,
            y: 0,
            width: source_width,
            height: bounds.y,
        },
        PageDirtyRect {
            x: 0,
            y: bounds.y.saturating_add(bounds.height),
            width: source_width,
            height: source_height.saturating_sub(bounds.y.saturating_add(bounds.height)),
        },
        PageDirtyRect {
            x: 0,
            y: bounds.y,
            width: bounds.x,
            height: bounds.height,
        },
        PageDirtyRect {
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
        let rect = plan.map_dirty_rect(region);
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        out.push(SolidQuad {
            rect,
            color: ACTIVE_KOMA_MASK,
        });
    }

    let koma_rect = plan.map_dirty_rect(PageDirtyRect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    });
    if koma_rect.width == 0 || koma_rect.height == 0 {
        return;
    }
    out.push(SolidQuad {
        rect: koma_rect,
        color: ACTIVE_KOMA_FILL,
    });
    push_border_quads(out, koma_rect, ACTIVE_KOMA_BORDER);
}

fn push_koma_creation_preview(
    out: &mut Vec<SolidQuad>,
    plan: &CanvasPlan,
    bounds: document_model::KomaBounds,
) {
    if plan.source_width == 0 || plan.source_height == 0 || bounds.width == 0 || bounds.height == 0
    {
        return;
    }
    let rect = plan.map_dirty_rect(PageDirtyRect {
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
        color: KOMA_PREVIEW_FILL,
    });
    push_border_quads(out, rect, KOMA_PREVIEW_BORDER);
}

fn push_koma_navigator(
    out: &mut Vec<SolidQuad>,
    plan: &CanvasPlan,
    navigator: &KomaNavigatorOverlay,
) {
    let canvas_host = plan.host_rect;
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
    let outer = WindowRect {
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
        color: KOMA_NAVIGATOR_BACKGROUND,
    });
    push_border_quads(out, outer, KOMA_NAVIGATOR_BORDER);
    let inner = WindowRect {
        x: outer.x + 8,
        y: outer.y + 8,
        width: scaled_width,
        height: scaled_height,
    };
    push_border_quads(out, inner, KOMA_NAVIGATOR_BORDER);

    for koma in &navigator.panels {
        let rect = WindowRect {
            x: inner.x + ((koma.bounds.x as f32 * scale).round() as usize),
            y: inner.y + ((koma.bounds.y as f32 * scale).round() as usize),
            width: ((koma.bounds.width as f32 * scale).round() as usize).max(1),
            height: ((koma.bounds.height as f32 * scale).round() as usize).max(1),
        };
        let fill_color = if koma.active {
            [
                KOMA_NAVIGATOR_ACTIVE[0],
                KOMA_NAVIGATOR_ACTIVE[1],
                KOMA_NAVIGATOR_ACTIVE[2],
                0x40,
            ]
        } else {
            KOMA_NAVIGATOR_KOMA
        };
        out.push(SolidQuad {
            rect,
            color: fill_color,
        });
        let border_color = if koma.active {
            KOMA_NAVIGATOR_ACTIVE
        } else {
            KOMA_NAVIGATOR_BORDER
        };
        push_border_quads(out, rect, border_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::overlay_state::KomaNavigatorEntry;
    use document_model::KomaBounds;
    use editor_state::CanvasViewTransform;
    use ::geometry::{PagePoint, WindowRect};

    fn make_plan(canvas_width: usize, canvas_height: usize) -> CanvasPlan {
        CanvasPlan {
            host_rect: WindowRect {
                x: 0,
                y: 0,
                width: canvas_width,
                height: canvas_height,
            },
            source_width: canvas_width,
            source_height: canvas_height,
            transform: CanvasViewTransform::default(),
        }
    }

    #[test]
    fn empty_overlay_returns_no_quads() {
        let plan = make_plan(64, 64);
        let overlay = CanvasOverlayState::default();
        assert!(build_overlay_solid_quads(&plan, &overlay).is_empty());
        assert!(build_overlay_circle_quads(&plan, &overlay).is_empty());
        assert!(build_overlay_line_quads(&plan, &overlay).is_empty());
    }

    #[test]
    fn active_koma_mask_emits_inside_fill_and_four_borders() {
        let plan = make_plan(64, 64);
        let overlay = CanvasOverlayState {
            active_koma_bounds: Some(KomaBounds {
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
            .filter(|q| q.color == ACTIVE_KOMA_FILL)
            .count();
        let borders = quads
            .iter()
            .filter(|q| q.color == ACTIVE_KOMA_BORDER)
            .count();
        assert_eq!(fills, 1);
        assert_eq!(borders, 4);
    }

    #[test]
    fn brush_preview_emits_single_circle_quad_with_expected_radius() {
        let plan = make_plan(64, 64);
        let overlay = CanvasOverlayState {
            brush_preview: Some(PagePoint::new(32, 32)),
            brush_size: Some(10),
            ..CanvasOverlayState::default()
        };
        let quads = build_overlay_circle_quads(&plan, &overlay);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].color, BRUSH_PREVIEW_RING);
        assert_eq!(quads[0].thickness, BRUSH_RING_THICKNESS);
        let scale = plan.view_geometry().expect("view geometry").scale();
        let expected_radius = ((10.0_f32 * scale) * 0.5).max(4.0);
        assert!((quads[0].radius - expected_radius).abs() < 0.001);
    }

    #[test]
    fn lasso_three_points_produce_two_segments() {
        let plan = make_plan(64, 64);
        let overlay = CanvasOverlayState {
            lasso_points: vec![
                PagePoint::new(8, 8),
                PagePoint::new(40, 24),
                PagePoint::new(56, 56),
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
    fn koma_navigator_emits_background_and_per_koma_quads() {
        let plan = make_plan(120, 120);
        let overlay = CanvasOverlayState {
            koma_navigator: Some(KomaNavigatorOverlay {
                page_width: 100,
                page_height: 80,
                panels: vec![
                    KomaNavigatorEntry {
                        bounds: KomaBounds {
                            x: 0,
                            y: 0,
                            width: 50,
                            height: 80,
                        },
                        active: true,
                    },
                    KomaNavigatorEntry {
                        bounds: KomaBounds {
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
            .filter(|q| q.color == KOMA_NAVIGATOR_BACKGROUND)
            .count();
        let outer_borders = quads
            .iter()
            .filter(|q| q.color == KOMA_NAVIGATOR_BORDER)
            .count();
        let active_quads = quads
            .iter()
            .filter(|q| q.color == KOMA_NAVIGATOR_ACTIVE)
            .count();
        let koma_fills = quads
            .iter()
            .filter(|q| q.color == KOMA_NAVIGATOR_KOMA)
            .count();
        assert_eq!(backgrounds, 1, "navigator outer fill");
        // outer 枠 (4) + inner 枠 (4) + 非 active コマの枠線 (4) = 12
        assert_eq!(outer_borders, 4 + 4 + 4);
        // active コマの枠線 (4 矩形分解。fill は alpha 0x40 で別色)
        assert_eq!(active_quads, 4);
        // 非 active コマの fill 1 個
        assert_eq!(koma_fills, 1);
    }

    #[test]
    fn koma_creation_preview_emits_fill_and_border() {
        let plan = make_plan(64, 64);
        let overlay = CanvasOverlayState {
            panel_creation_preview: Some(KomaBounds {
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
            .filter(|q| q.color == KOMA_PREVIEW_FILL)
            .count();
        let borders = quads
            .iter()
            .filter(|q| q.color == KOMA_PREVIEW_BORDER)
            .count();
        assert_eq!(fills, 1);
        assert_eq!(borders, 4);
    }
}
