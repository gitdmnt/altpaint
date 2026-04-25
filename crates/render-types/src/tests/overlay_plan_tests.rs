use app_core::CanvasViewTransform;

use crate::{
    CanvasCompositeSource, CanvasOverlayState, FramePlan, PanelSurfaceSource, PixelRect,
};

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
