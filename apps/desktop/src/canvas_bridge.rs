//! デスクトップアプリ内には、キャンバス自体の持つピクセル座標と
//! ユーザーの操作するビュー画面の座標が存在する。
//!
//! このモジュールは、ランタイムやアプリ状態から独立した純粋関数として、
//! キャンバス表示座標と編集コマンドの間の座標変換、
//! およびビューへの入力イベントからのコマンド生成を担当する。

use app_core::{
    CanvasPoint, CanvasViewTransform, CanvasViewportPoint, Command, PanelLocalPoint, ToolKind,
};
use render::RenderFrame;

/// キャンバス入力中の最小状態を表す。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasInputState {
    pub is_drawing: bool,
    pub last_position: Option<CanvasPoint>,
    pub last_smoothed_position: Option<(f32, f32)>,
    pub lasso_points: Vec<CanvasPoint>,
    pub panel_rect_anchor: Option<CanvasPoint>,
}

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

/// ツール種別と前回位置からキャンバス編集コマンドを生成する。
pub fn command_for_canvas_gesture(
    tool: ToolKind,
    current: PanelLocalPoint,
    previous: Option<PanelLocalPoint>,
    pressure: f32,
) -> Command {
    match (tool, previous) {
        (ToolKind::Pen, Some(previous)) => Command::DrawStroke {
            from_x: previous.x,
            from_y: previous.y,
            to_x: current.x,
            to_y: current.y,
            pressure,
        },
        (ToolKind::Eraser, Some(previous)) => Command::EraseStroke {
            from_x: previous.x,
            from_y: previous.y,
            to_x: current.x,
            to_y: current.y,
            pressure,
        },
        (ToolKind::Pen, None) => Command::DrawPoint {
            x: current.x,
            y: current.y,
            pressure,
        },
        (ToolKind::Eraser, None) => Command::ErasePoint {
            x: current.x,
            y: current.y,
            pressure,
        },
        (ToolKind::Bucket, _) => Command::FillRegion {
            x: current.x,
            y: current.y,
        },
        (ToolKind::LassoBucket | ToolKind::PanelRect, _) => Command::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame() -> RenderFrame {
        RenderFrame {
            width: 64,
            height: 64,
            pixels: vec![255; 64 * 64 * 4],
        }
    }

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

    #[test]
    fn map_view_with_zoom_and_pan_tracks_shifted_canvas() {
        let mapped = map_view_to_canvas_with_transform(
            &sample_frame(),
            CanvasPointerEvent {
                position: CanvasViewportPoint::new(352, 320),
                width: 640,
                height: 640,
            },
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
    fn pen_drag_becomes_draw_stroke() {
        let command = command_for_canvas_gesture(
            ToolKind::Pen,
            PanelLocalPoint::new(4, 5),
            Some(PanelLocalPoint::new(1, 2)),
            1.0,
        );

        assert_eq!(
            command,
            Command::DrawStroke {
                from_x: 1,
                from_y: 2,
                to_x: 4,
                to_y: 5,
                pressure: 1.0,
            }
        );
    }
}
