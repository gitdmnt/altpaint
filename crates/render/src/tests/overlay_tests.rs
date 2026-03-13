use app_core::CanvasViewTransform;

use crate::{
    CanvasCompositeSource, CanvasOverlayState, FramePlan, PanelSurfaceSource, PixelRect,
    RenderFrame, blit_scaled_rgba_region, compose_panel_host_region, compose_status_region,
    status_text_bounds,
};

/// 合成 パネル ホスト 領域 respects global パネル サーフェス 範囲 が期待どおりに動作することを検証する。
#[test]
fn compose_panel_host_region_respects_global_panel_surface_bounds() {
    let mut frame = RenderFrame {
        width: 640,
        height: 480,
        pixels: vec![0; 640 * 480 * 4],
    };
    let panel_surface = PanelSurfaceSource {
        x: 120,
        y: 80,
        width: 8,
        height: 6,
        pixels: &[0xaa; 8 * 6 * 4],
    };

    compose_panel_host_region(&mut frame, panel_surface, None);

    let inside = ((80 * frame.width) + 120) * 4;
    let outside = ((40 * frame.width) + 40) * 4;
    assert_eq!(&frame.pixels[inside..inside + 4], &[0xaa, 0xaa, 0xaa, 0xaa]);
    assert_eq!(&frame.pixels[outside..outside + 4], &[0, 0, 0, 0]);
}

/// blit scaled RGBA 領域 updates only 差分 area が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn blit_scaled_rgba_region_updates_only_dirty_area() {
    let mut frame = RenderFrame {
        width: 8,
        height: 8,
        pixels: vec![0; 8 * 8 * 4],
    };
    let source = vec![255; 4 * 4 * 4];

    blit_scaled_rgba_region(
        &mut frame,
        PixelRect {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        },
        4,
        4,
        source.as_slice(),
        Some(PixelRect {
            x: 3,
            y: 3,
            width: 1,
            height: 1,
        }),
    );

    let dirty_index = (3 * frame.width + 3) * 4;
    let untouched_index = (2 * frame.width + 2) * 4;
    assert_eq!(
        &frame.pixels[dirty_index..dirty_index + 4],
        &[255, 255, 255, 255]
    );
    assert_eq!(
        &frame.pixels[untouched_index..untouched_index + 4],
        &[0, 0, 0, 0]
    );
}

/// 合成 ステータス 領域 updates expected footer 範囲 が期待どおりに動作することを検証する。
#[test]
fn compose_status_region_updates_expected_footer_bounds() {
    let canvas_pixels = vec![0; 32 * 32 * 4];
    let panel_pixels = vec![0; 640 * 480 * 4];
    let plan = FramePlan::new(
        640,
        480,
        PixelRect {
            x: 24,
            y: 40,
            width: 400,
            height: 320,
        },
        PanelSurfaceSource {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
            pixels: panel_pixels.as_slice(),
        },
        CanvasCompositeSource {
            width: 32,
            height: 32,
            pixels: canvas_pixels.as_slice(),
        },
        CanvasViewTransform::default(),
        "status text",
    );
    let mut frame = crate::compose_base_frame(&plan);

    compose_status_region(&mut frame, &plan);

    let expected = status_text_bounds(
        plan.window_width,
        plan.window_height,
        plan.canvas.host_rect,
        plan.status_text,
    );
    let index = (expected.y * frame.width + expected.x) * 4;
    assert_ne!(&frame.pixels[index..index + 4], &[0, 0, 0, 0]);
}

/// オーバーレイ plan roundtrip keeps オーバーレイ payload が期待どおりに動作することを検証する。
#[test]
fn overlay_plan_roundtrip_keeps_overlay_payload() {
    let canvas_pixels = vec![0; 8 * 8 * 4];
    let panel_pixels = vec![0; 64 * 64 * 4];
    let plan = FramePlan::new(
        64,
        64,
        PixelRect {
            x: 4,
            y: 4,
            width: 56,
            height: 48,
        },
        PanelSurfaceSource {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
            pixels: panel_pixels.as_slice(),
        },
        CanvasCompositeSource {
            width: 8,
            height: 8,
            pixels: canvas_pixels.as_slice(),
        },
        CanvasViewTransform::default(),
        "",
    );

    let overlay = CanvasOverlayState {
        brush_preview: Some(app_core::CanvasPoint::new(1, 2)),
        brush_size: Some(12),
        ..CanvasOverlayState::default()
    };
    let overlay_plan = plan.overlay_plan(overlay.clone());

    assert_eq!(overlay_plan.overlay, overlay);
    assert_eq!(overlay_plan.canvas.host_rect, plan.canvas.host_rect);
    assert_eq!(
        overlay_plan.panel_surface.rect(),
        PixelRect {
            x: 0,
            y: 0,
            width: 64,
            height: 64
        }
    );
}
