use editor_state::ToolKind;
use geometry::{KomaLocalPoint, PagePoint};

use crate::{CanvasGestureUpdate, CanvasInputState, CanvasPointerAction, advance_pointer_gesture};

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
        CanvasGestureUpdate::Paint(crate::painting::PaintInput::LassoFill { .. })
    ));
    assert_eq!(state, CanvasInputState::default());
}
