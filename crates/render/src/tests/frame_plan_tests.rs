use app_core::{CanvasDirtyRect, CanvasPoint, CanvasViewTransform, CanvasViewportPoint};

use crate::{
    CanvasCompositeSource, CanvasOverlayState, FramePlan, PanelNavigatorEntry,
    PanelNavigatorOverlay, PanelSurfaceSource, PixelRect, RenderContext, canvas_texture_quad,
    compose_desktop_frame, compose_overlay_frame, map_canvas_dirty_to_display_with_transform,
    map_canvas_point_to_display, map_view_to_canvas_with_transform, prepare_canvas_scene,
};

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

#[test]
fn transformed_canvas_dirty_rect_tracks_zoom_and_pan() {
    let mapped = map_canvas_dirty_to_display_with_transform(
        CanvasDirtyRect {
            x: 16,
            y: 16,
            width: 8,
            height: 8,
        },
        PixelRect {
            x: 100,
            y: 50,
            width: 320,
            height: 320,
        },
        64,
        64,
        CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 0.0,
            pan_x: 16.0,
            pan_y: -8.0,
            flip_x: false,
            flip_y: false,
        },
    );

    assert!(mapped.width >= 80);
    assert_eq!(mapped.height, 80);
    assert!(mapped.x >= 100);
    assert_eq!(mapped.y, 50);
}

#[test]
fn canvas_texture_quad_clips_uv_when_panned_outside_display() {
    let quad = canvas_texture_quad(
        PixelRect {
            x: 100,
            y: 80,
            width: 320,
            height: 320,
        },
        64,
        64,
        CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 0.0,
            pan_x: 48.0,
            pan_y: -16.0,
            flip_x: false,
            flip_y: false,
        },
    )
    .expect("quad exists");

    assert_eq!(quad.destination.width, 320);
    assert!(quad.uv_min[0] > 0.0);
    assert!(quad.uv_max[0] <= 1.0);
    assert!(quad.uv_min[1] >= 0.0);
}

#[test]
fn map_view_to_canvas_tracks_shifted_scene() {
    let mapped = map_view_to_canvas_with_transform(
        PixelRect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        },
        64,
        64,
        CanvasViewportPoint::new(352, 320),
        CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 0.0,
            pan_x: 32.0,
            pan_y: 0.0,
            flip_x: false,
            flip_y: false,
        },
    );

    assert_eq!(mapped, Some(CanvasPoint::new(32, 32)));
}

#[test]
fn canvas_texture_quad_carries_rotation_and_flip_flags() {
    let quad = canvas_texture_quad(
        PixelRect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        },
        64,
        32,
        CanvasViewTransform {
            zoom: 1.0,
            rotation_degrees: 90.0,
            pan_x: 0.0,
            pan_y: 0.0,
            flip_x: true,
            flip_y: false,
        },
    )
    .expect("quad exists");

    assert_eq!(quad.rotation_degrees, 90.0);
    assert!(quad.bbox_size[0] > 0.0);
    assert!(quad.bbox_size[1] > 0.0);
    assert!(quad.flip_x);
    assert!(!quad.flip_y);
}

#[test]
fn arbitrary_rotation_roundtrips_view_to_canvas() {
    let viewport = PixelRect {
        x: 0,
        y: 0,
        width: 640,
        height: 640,
    };
    let transform = CanvasViewTransform {
        zoom: 1.0,
        rotation_degrees: 37.5,
        pan_x: 0.0,
        pan_y: 0.0,
        flip_x: false,
        flip_y: false,
    };
    let display = map_canvas_point_to_display(viewport, 64, 32, transform, CanvasPoint::new(24, 12))
        .expect("display point exists");

    let mapped = map_view_to_canvas_with_transform(
        viewport,
        64,
        32,
        CanvasViewportPoint::new(display.x.round() as i32, display.y.round() as i32),
        transform,
    );

    assert_eq!(mapped, Some(CanvasPoint::new(24, 12)));
}

#[test]
fn arbitrary_rotation_keeps_canvas_scale_stable() {
    let viewport = PixelRect {
        x: 0,
        y: 0,
        width: 640,
        height: 640,
    };
    let base_transform = CanvasViewTransform::default();
    let rotated_transform = CanvasViewTransform {
        rotation_degrees: 37.5,
        ..base_transform
    };

    let base_scene = prepare_canvas_scene(viewport, 64, 32, base_transform).expect("base scene exists");
    let rotated_scene =
        prepare_canvas_scene(viewport, 64, 32, rotated_transform).expect("rotated scene exists");

    assert!((base_scene.scale() - rotated_scene.scale()).abs() < 0.001);
}

#[test]
fn map_view_to_canvas_tracks_rotated_scene() {
    let mapped = map_view_to_canvas_with_transform(
        PixelRect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        },
        64,
        32,
        CanvasViewportPoint::new(320, 160),
        CanvasViewTransform {
            zoom: 1.0,
            rotation_degrees: 90.0,
            pan_x: 0.0,
            pan_y: 0.0,
            flip_x: false,
            flip_y: false,
        },
    );

    assert!(mapped.is_some());
}

#[test]
fn compose_desktop_frame_writes_panel_and_canvas_regions() {
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
            x: 8,
            y: 6,
            width: 32,
            height: 16,
            pixels: &[0xaa; 32 * 16 * 4],
        },
        CanvasCompositeSource {
            width: 2,
            height: 2,
            pixels: &[16; 16],
        },
        CanvasViewTransform::default(),
        "status",
    );

    let frame = compose_desktop_frame(&plan, &CanvasOverlayState::default());

    assert_eq!(frame.width, 640);
    assert_eq!(frame.height, 480);
    assert!(frame.pixels.chunks_exact(4).any(|pixel| pixel == [16, 16, 16, 16]));
}

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

    let overlay = compose_overlay_frame(
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
        },
    );

    assert!(overlay.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
}
