use app_core::{PagePoint, CanvasViewTransform, CanvasViewportPoint, KomaLocalPoint, ToolKind};

use crate::{
    CanvasGestureUpdate, CanvasInputState, CanvasPointerAction, CanvasPointerEvent,
    advance_pointer_gesture, map_view_to_canvas_with_transform, koma_creation_preview_bounds,
};

const SAMPLE_CANVAS_WIDTH: usize = 64;
const SAMPLE_CANVAS_HEIGHT: usize = 64;

#[test]
fn map_view_center_into_canvas_center() {
    let mapped = map_view_to_canvas_with_transform(
        SAMPLE_CANVAS_WIDTH,
        SAMPLE_CANVAS_HEIGHT,
        CanvasPointerEvent {
            position: CanvasViewportPoint::new(320, 320),
            width: 640,
            height: 640,
        },
        CanvasViewTransform::default(),
    );

    assert_eq!(mapped, Some(PagePoint::new(32, 32)));
}

#[test]
fn map_view_returns_none_outside_letterboxed_canvas() {
    let mapped = map_view_to_canvas_with_transform(
        SAMPLE_CANVAS_WIDTH,
        SAMPLE_CANVAS_HEIGHT,
        CanvasPointerEvent {
            position: CanvasViewportPoint::new(10, 10),
            width: 900,
            height: 640,
        },
        CanvasViewTransform::default(),
    );

    assert_eq!(mapped, None);
}

#[test]
fn lasso_gesture_collects_points_and_emits_fill_on_release() {
    let mut state = CanvasInputState::default();
    let to_koma_local = |point: PagePoint| Some(KomaLocalPoint::new(point.x, point.y));

    assert_eq!(
        advance_pointer_gesture(
            &mut state,
            CanvasPointerAction::Down,
            PagePoint::new(10, 10),
            ToolKind::LassoBucket,
            1.0,
            0,
            to_koma_local,
        ),
        CanvasGestureUpdate::LassoPreviewChanged
    );

    let to_koma_local = |point: PagePoint| Some(KomaLocalPoint::new(point.x, point.y));
    let _ = advance_pointer_gesture(
        &mut state,
        CanvasPointerAction::Drag,
        PagePoint::new(20, 10),
        ToolKind::LassoBucket,
        1.0,
        0,
        to_koma_local,
    );
    let to_koma_local = |point: PagePoint| Some(KomaLocalPoint::new(point.x, point.y));
    let _ = advance_pointer_gesture(
        &mut state,
        CanvasPointerAction::Drag,
        PagePoint::new(20, 20),
        ToolKind::LassoBucket,
        1.0,
        0,
        to_koma_local,
    );
    let to_koma_local = |point: PagePoint| Some(KomaLocalPoint::new(point.x, point.y));
    let update = advance_pointer_gesture(
        &mut state,
        CanvasPointerAction::Up,
        PagePoint::new(10, 20),
        ToolKind::LassoBucket,
        1.0,
        0,
        to_koma_local,
    );

    assert!(matches!(
        update,
        CanvasGestureUpdate::Paint(app_core::PaintInput::LassoFill { .. })
    ));
    assert_eq!(state, CanvasInputState::default());
}

#[test]
fn koma_rect_preview_bounds_are_derived_from_canvas_state() {
    let state = CanvasInputState {
        koma_rect_anchor: Some(PagePoint::new(80, 50)),
        last_position: Some(PagePoint::new(20, 30)),
        ..CanvasInputState::default()
    };

    let bounds = koma_creation_preview_bounds(&state, 200, 200).expect("preview bounds");

    assert_eq!(bounds.x, 20);
    assert_eq!(bounds.y, 30);
    assert_eq!(bounds.width, 61);
    assert_eq!(bounds.height, 21);
}
