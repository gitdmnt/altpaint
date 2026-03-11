use app_core::{CanvasPoint, CanvasViewTransform, CanvasViewportPoint};
use render::RenderFrame;

/// ビュー空間で受け取ったキャンバスポインタイベントを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasPointerEvent {
    pub position: CanvasViewportPoint,
    pub width: i32,
    pub height: i32,
}

pub fn map_view_to_canvas_with_transform(
    frame: &RenderFrame,
    event: CanvasPointerEvent,
    transform: CanvasViewTransform,
) -> Option<CanvasPoint> {
    render::map_view_to_canvas_with_transform(
        render::PixelRect {
            x: 0,
            y: 0,
            width: event.width.max(0) as usize,
            height: event.height.max(0) as usize,
        },
        frame.width,
        frame.height,
        event.position,
        transform,
    )
}
