use app_core::{CanvasPoint, CanvasViewTransform, CanvasViewportPoint, PanelLocalPoint, ToolKind};
use render::RenderFrame;

use crate::{
    CanvasGestureUpdate, CanvasInputState, CanvasPointerAction, CanvasPointerEvent,
    advance_pointer_gesture, map_view_to_canvas_with_transform, panel_creation_preview_bounds,
};

/// Sample フレーム に必要な描画内容を組み立てる。
fn sample_frame() -> RenderFrame {
    RenderFrame {
        width: 64,
        height: 64,
        pixels: vec![255; 64 * 64 * 4],
    }
}

/// map ビュー center into キャンバス center が期待どおりに動作することを検証する。
#[test]
fn map_view_center_into_canvas_center() {
    let mapped = map_view_to_canvas_with_transform(
        &sample_frame(),
        CanvasPointerEvent {
            position: CanvasViewportPoint::new(320, 320),
            width: 640,
            height: 640,
        },
        CanvasViewTransform::default(),
    );

    assert_eq!(mapped, Some(CanvasPoint::new(32, 32)));
}

/// map ビュー returns none outside letterboxed キャンバス が期待どおりに動作することを検証する。
#[test]
fn map_view_returns_none_outside_letterboxed_canvas() {
    let mapped = map_view_to_canvas_with_transform(
        &sample_frame(),
        CanvasPointerEvent {
            position: CanvasViewportPoint::new(10, 10),
            width: 900,
            height: 640,
        },
        CanvasViewTransform::default(),
    );

    assert_eq!(mapped, None);
}

/// 投げ縄 ジェスチャ collects points and emits 塗りつぶし on release が期待どおりに動作することを検証する。
#[test]
fn lasso_gesture_collects_points_and_emits_fill_on_release() {
    let mut state = CanvasInputState::default();
    let to_panel_local = |point: CanvasPoint| Some(PanelLocalPoint::new(point.x, point.y));

    assert_eq!(
        advance_pointer_gesture(
            &mut state,
            CanvasPointerAction::Down,
            CanvasPoint::new(10, 10),
            ToolKind::LassoBucket,
            1.0,
            0,
            to_panel_local,
        ),
        CanvasGestureUpdate::LassoPreviewChanged
    );

    let to_panel_local = |point: CanvasPoint| Some(PanelLocalPoint::new(point.x, point.y));
    let _ = advance_pointer_gesture(
        &mut state,
        CanvasPointerAction::Drag,
        CanvasPoint::new(20, 10),
        ToolKind::LassoBucket,
        1.0,
        0,
        to_panel_local,
    );
    let to_panel_local = |point: CanvasPoint| Some(PanelLocalPoint::new(point.x, point.y));
    let _ = advance_pointer_gesture(
        &mut state,
        CanvasPointerAction::Drag,
        CanvasPoint::new(20, 20),
        ToolKind::LassoBucket,
        1.0,
        0,
        to_panel_local,
    );
    let to_panel_local = |point: CanvasPoint| Some(PanelLocalPoint::new(point.x, point.y));
    let update = advance_pointer_gesture(
        &mut state,
        CanvasPointerAction::Up,
        CanvasPoint::new(10, 20),
        ToolKind::LassoBucket,
        1.0,
        0,
        to_panel_local,
    );

    assert!(matches!(
        update,
        CanvasGestureUpdate::Paint(app_core::PaintInput::LassoFill { .. })
    ));
    assert_eq!(state, CanvasInputState::default());
}

/// パネル 矩形 プレビュー 範囲 are derived from キャンバス 状態 が期待どおりに動作することを検証する。
#[test]
fn panel_rect_preview_bounds_are_derived_from_canvas_state() {
    let state = CanvasInputState {
        panel_rect_anchor: Some(CanvasPoint::new(80, 50)),
        last_position: Some(CanvasPoint::new(20, 30)),
        ..CanvasInputState::default()
    };

    let bounds = panel_creation_preview_bounds(&state, 200, 200).expect("preview bounds");

    assert_eq!(bounds.x, 20);
    assert_eq!(bounds.y, 30);
    assert_eq!(bounds.width, 61);
    assert_eq!(bounds.height, 21);
}
