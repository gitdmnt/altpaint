use app_core::{
    PageDirtyRect, PagePoint, CanvasViewTransform, CanvasViewportPoint, PanelSurfacePoint,
    WindowPoint,
};

use crate::{
    CanvasViewGeometry, PixelRect, brush_preview_dirty_rect, canvas_texture_quad,
    map_canvas_dirty_to_display_with_transform, map_canvas_point_to_display,
    map_view_to_canvas_with_transform,
};

/// PixelRect::contains が window 座標点の内外を正しく判定することを検証する。
#[test]
fn pixel_rect_contains_judges_window_point() {
    let rect = PixelRect {
        x: 100,
        y: 50,
        width: 30,
        height: 20,
    };

    assert!(rect.contains(WindowPoint::new(100, 50)));
    assert!(rect.contains(WindowPoint::new(129, 69)));
    assert!(!rect.contains(WindowPoint::new(130, 50)));
    assert!(!rect.contains(WindowPoint::new(99, 50)));
    assert!(!rect.contains(WindowPoint::new(-1, -1)));
}

/// PixelRect::to_local_point が矩形原点基準のローカル座標へ変換することを検証する。
#[test]
fn pixel_rect_to_local_point_offsets_by_origin() {
    let rect = PixelRect {
        x: 100,
        y: 50,
        width: 30,
        height: 20,
    };

    assert_eq!(
        rect.to_local_point(WindowPoint::new(112, 58)),
        Some(PanelSurfacePoint::new(12, 8))
    );
    assert_eq!(rect.to_local_point(WindowPoint::new(99, 58)), None);
}

/// PixelRect::contains_local がローカル座標系の点を判定することを検証する。
#[test]
fn pixel_rect_contains_local_judges_same_space_point() {
    let rect = PixelRect {
        x: 4,
        y: 4,
        width: 8,
        height: 8,
    };

    assert!(rect.contains_local(PanelSurfacePoint::new(4, 4)));
    assert!(rect.contains_local(PanelSurfacePoint::new(11, 11)));
    assert!(!rect.contains_local(PanelSurfacePoint::new(12, 4)));
    assert!(!rect.contains_local(PanelSurfacePoint::new(3, 4)));
}

#[test]
fn brush_preview_dirty_rect_unions_previous_and_current_preview() {
    let viewport = PixelRect {
        x: 0,
        y: 0,
        width: 400,
        height: 300,
    };
    let previous = CanvasViewGeometry::compute(viewport, 64, 64, CanvasViewTransform::default());
    let current = CanvasViewGeometry::compute(
        viewport,
        64,
        64,
        CanvasViewTransform {
            pan_x: 20.0,
            ..CanvasViewTransform::default()
        },
    );

    let dirty = brush_preview_dirty_rect(previous, current, PagePoint::new(20, 20), 12.0)
        .expect("dirty rect exists");

    assert!(dirty.width > 0);
    assert!(dirty.height > 0);
}

#[test]
fn transformed_canvas_dirty_rect_tracks_zoom_and_pan() {
    let mapped = map_canvas_dirty_to_display_with_transform(
        PageDirtyRect {
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
fn map_view_to_canvas_tracks_shifted_view_geometry() {
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

    assert_eq!(mapped, Some(PagePoint::new(32, 32)));
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
    let display =
        map_canvas_point_to_display(viewport, 64, 32, transform, PagePoint::new(24, 12))
            .expect("display point exists");

    let mapped = map_view_to_canvas_with_transform(
        viewport,
        64,
        32,
        CanvasViewportPoint::new(display.x.round() as i32, display.y.round() as i32),
        transform,
    );

    assert_eq!(mapped, Some(PagePoint::new(24, 12)));
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

    let base_geometry =
        CanvasViewGeometry::compute(viewport, 64, 32, base_transform).expect("base geometry exists");
    let rotated_geometry =
        CanvasViewGeometry::compute(viewport, 64, 32, rotated_transform).expect("rotated geometry exists");

    assert!((base_geometry.scale() - rotated_geometry.scale()).abs() < 0.001);
}

#[test]
fn map_view_to_canvas_tracks_rotated_geometry() {
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
