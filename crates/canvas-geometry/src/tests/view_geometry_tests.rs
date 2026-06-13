use editor_state::CanvasViewTransform;
use geometry::{CanvasViewportPoint, PageDirtyRect, PagePoint, WindowRect};

use crate::{CanvasViewGeometry, brush_preview_dirty_rect};

#[test]
fn brush_preview_dirty_rect_unions_previous_and_current_preview() {
    let viewport = WindowRect {
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
    let mapped = CanvasViewGeometry::compute(
        WindowRect {
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
    )
    .expect("geometry exists")
    .map_canvas_dirty_rect(PageDirtyRect {
        x: 16,
        y: 16,
        width: 8,
        height: 8,
    });

    assert!(mapped.width >= 80);
    assert_eq!(mapped.height, 80);
    assert!(mapped.x >= 100);
    assert_eq!(mapped.y, 50);
}

#[test]
fn canvas_texture_quad_clips_uv_when_panned_outside_display() {
    let quad = CanvasViewGeometry::compute(
        WindowRect {
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
    .expect("geometry exists")
    .texture_quad()
    .expect("quad exists");

    assert_eq!(quad.destination.width, 320);
    assert!(quad.uv_min[0] > 0.0);
    assert!(quad.uv_max[0] <= 1.0);
    assert!(quad.uv_min[1] >= 0.0);
}

#[test]
fn map_view_to_canvas_tracks_shifted_view_geometry() {
    let mapped = CanvasViewGeometry::compute(
        WindowRect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        },
        64,
        64,
        CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 0.0,
            pan_x: 32.0,
            pan_y: 0.0,
            flip_x: false,
            flip_y: false,
        },
    )
    .expect("geometry exists")
    .map_view_to_canvas(CanvasViewportPoint::new(352, 320));

    assert_eq!(mapped, Some(PagePoint::new(32, 32)));
}

/// 正方形キャンバスの中心 view 座標がキャンバス中心へ写像されることを検証する。
/// (BL-042 で削除した paint-engine ラッパーのテストを統合先へ移設)
#[test]
fn map_view_center_into_canvas_center() {
    let mapped = CanvasViewGeometry::compute(
        WindowRect::new(0, 0, 640, 640),
        64,
        64,
        CanvasViewTransform::default(),
    )
    .expect("geometry exists")
    .map_view_to_canvas(CanvasViewportPoint::new(320, 320));

    assert_eq!(mapped, Some(PagePoint::new(32, 32)));
}

/// letterbox された余白上の view 座標が `None` を返すことを検証する。
/// (BL-042 で削除した paint-engine ラッパーのテストを統合先へ移設)
#[test]
fn map_view_returns_none_outside_letterboxed_canvas() {
    let mapped = CanvasViewGeometry::compute(
        WindowRect::new(0, 0, 900, 640),
        64,
        64,
        CanvasViewTransform::default(),
    )
    .expect("geometry exists")
    .map_view_to_canvas(CanvasViewportPoint::new(10, 10));

    assert_eq!(mapped, None);
}

#[test]
fn canvas_texture_quad_carries_rotation_and_flip_flags() {
    let quad = CanvasViewGeometry::compute(
        WindowRect {
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
    .expect("geometry exists")
    .texture_quad()
    .expect("quad exists");

    assert_eq!(quad.rotation_degrees, 90.0);
    assert!(quad.bbox_size[0] > 0.0);
    assert!(quad.bbox_size[1] > 0.0);
    assert!(quad.flip_x);
    assert!(!quad.flip_y);
}

#[test]
fn arbitrary_rotation_roundtrips_view_to_canvas() {
    let viewport = WindowRect {
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
    let geometry =
        CanvasViewGeometry::compute(viewport, 64, 32, transform).expect("geometry exists");
    let display = geometry
        .map_canvas_point_to_display(PagePoint::new(24, 12))
        .expect("display point exists");

    let mapped = geometry.map_view_to_canvas(CanvasViewportPoint::new(
        display.x.round() as i32,
        display.y.round() as i32,
    ));

    assert_eq!(mapped, Some(PagePoint::new(24, 12)));
}

#[test]
fn arbitrary_rotation_keeps_canvas_scale_stable() {
    let viewport = WindowRect {
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
    let mapped = CanvasViewGeometry::compute(
        WindowRect {
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
            flip_x: false,
            flip_y: false,
        },
    )
    .expect("geometry exists")
    .map_view_to_canvas(CanvasViewportPoint::new(320, 160));

    assert!(mapped.is_some());
}
