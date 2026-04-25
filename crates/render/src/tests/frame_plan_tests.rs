use app_core::CanvasViewTransform;
use render_types::{
    CanvasCompositeSource, CanvasOverlayState, FramePlan, PanelNavigatorEntry,
    PanelNavigatorOverlay, PanelSurfaceSource, PixelRect,
};

use crate::{RenderContext, compose_temp_overlay_frame};

/// 描画 フレーム places アクティブ パネル ビットマップ inside ページ が期待どおりに動作することを検証する。
#[test]
fn render_frame_places_active_panel_bitmap_inside_page() {
    let mut document = app_core::Document::new(320, 240);
    let _ = document.apply_command(&app_core::Command::CreatePanel {
        x: 40,
        y: 32,
        width: 120,
        height: 80,
    });
    if let Some(panel) = document.active_panel_mut() {
        let _ = panel.layers[0]
            .bitmap
            .draw_line_sized_rgba(1, 2, 4, 2, [0, 0, 0, 255], 1, true);
        panel.bitmap = panel.layers[0].bitmap.clone();
    }

    let context = RenderContext::new();
    let frame = context.render_frame(&document);

    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 240);

    let index = ((32 + 2) * frame.width + (40 + 1)) * 4;
    assert_eq!(&frame.pixels[index..index + 4], &[0, 0, 0, 255]);
    let end_index = ((32 + 2) * frame.width + (40 + 4)) * 4;
    assert_eq!(&frame.pixels[end_index..end_index + 4], &[0, 0, 0, 255]);
    assert_eq!(&frame.pixels[0..4], &[255, 255, 255, 255]);
}

/// オーバーレイ フレーム draws パネル navigator when multiple panels exist が期待どおりに動作することを検証する。
#[test]
fn overlay_frame_draws_panel_navigator_when_multiple_panels_exist() {
    let plan = FramePlan::new(
        640,
        480,
        PixelRect {
            x: 20,
            y: 40,
            width: 400,
            height: 320,
        },
        PanelSurfaceSource {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            pixels: &[0; 4],
        },
        CanvasCompositeSource {
            width: 64,
            height: 64,
            pixels: &[0; 64 * 64 * 4],
        },
        CanvasViewTransform::default(),
        "status",
    );

    let overlay = compose_temp_overlay_frame(
        &plan,
        &CanvasOverlayState {
            brush_preview: None,
            brush_size: None,
            lasso_points: Vec::new(),
            active_panel_bounds: None,
            panel_navigator: Some(PanelNavigatorOverlay {
                page_width: 320,
                page_height: 240,
                panels: vec![
                    PanelNavigatorEntry {
                        bounds: app_core::PanelBounds {
                            x: 12,
                            y: 12,
                            width: 143,
                            height: 216,
                        },
                        active: false,
                    },
                    PanelNavigatorEntry {
                        bounds: app_core::PanelBounds {
                            x: 165,
                            y: 12,
                            width: 143,
                            height: 216,
                        },
                        active: true,
                    },
                ],
            }),
            panel_creation_preview: None,
            active_ui_panel_rect: None,
        },
    );

    assert!(overlay.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
}
