//! デスクトップアプリ内には、キャンバス自体の持つピクセル座標と
//! ユーザーの操作するビュー画面の座標が存在する。
//!
//! このモジュールは、ランタイムやアプリ状態から独立した純粋関数として、
//! キャンバス表示座標と編集コマンドの間の座標変換、
//! およびビューへの入力イベントからのコマンド生成を担当する。

use app_core::CanvasViewTransform;
use app_core::{Command, ToolKind};
use render::RenderFrame;

/// キャンバス入力中の最小状態を表す。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanvasInputState {
    pub is_drawing: bool,
    pub last_position: Option<(usize, usize)>,
}

/// ビュー空間で受け取ったキャンバスポインタイベントを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasPointerEvent {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// ビューの座標をキャンバス上のビットマップの座標へ変換する。
#[cfg_attr(not(test), allow(dead_code))]
pub fn map_view_to_canvas(
    frame: &RenderFrame,
    event: CanvasPointerEvent,
) -> Option<(usize, usize)> {
    map_view_to_canvas_with_transform(frame, event, CanvasViewTransform::default())
}

pub fn map_view_to_canvas_with_transform(
    frame: &RenderFrame,
    event: CanvasPointerEvent,
    transform: CanvasViewTransform,
) -> Option<(usize, usize)> {
    if frame.width == 0 || frame.height == 0 || event.width <= 0 || event.height <= 0 {
        return None;
    }

    let scale_x = event.width as f32 / frame.width as f32;
    let scale_y = event.height as f32 / frame.height as f32;
    let scale = (scale_x.min(scale_y) * transform.zoom.max(0.25)).max(f32::EPSILON);
    if scale <= 0.0 {
        return None;
    }

    let drawn_width = frame.width as f32 * scale;
    let drawn_height = frame.height as f32 * scale;
    let offset_x = (event.width as f32 - drawn_width) * 0.5 + transform.pan_x;
    let offset_y = (event.height as f32 - drawn_height) * 0.5 + transform.pan_y;

    let local_x = event.x as f32 - offset_x;
    let local_y = event.y as f32 - offset_y;
    if local_x < 0.0 || local_y < 0.0 || local_x >= drawn_width || local_y >= drawn_height {
        return None;
    }

    let canvas_x = (local_x / scale).floor() as usize;
    let canvas_y = (local_y / scale).floor() as usize;

    Some((
        canvas_x.min(frame.width.saturating_sub(1)),
        canvas_y.min(frame.height.saturating_sub(1)),
    ))
}

/// ツール種別と前回位置からキャンバス編集コマンドを生成する。
pub fn command_for_canvas_gesture(
    tool: ToolKind,
    current: (usize, usize),
    previous: Option<(usize, usize)>,
) -> Command {
    match (tool, previous) {
        (ToolKind::Pen, Some((from_x, from_y))) => Command::DrawStroke {
            from_x,
            from_y,
            to_x: current.0,
            to_y: current.1,
        },
        (ToolKind::Brush, Some((from_x, from_y))) => Command::DrawStroke {
            from_x,
            from_y,
            to_x: current.0,
            to_y: current.1,
        },
        (ToolKind::Eraser, Some((from_x, from_y))) => Command::EraseStroke {
            from_x,
            from_y,
            to_x: current.0,
            to_y: current.1,
        },
        (ToolKind::Brush, None) => Command::DrawPoint {
            x: current.0,
            y: current.1,
        },
        (ToolKind::Pen, None) => Command::DrawPoint {
            x: current.0,
            y: current.1,
        },
        (ToolKind::Eraser, None) => Command::ErasePoint {
            x: current.0,
            y: current.1,
        },
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
        let mapped = map_view_to_canvas(
            &sample_frame(),
            CanvasPointerEvent {
                x: 320,
                y: 320,
                width: 640,
                height: 640,
            },
        );

        assert_eq!(mapped, Some((32, 32)));
    }

    #[test]
    fn map_view_returns_none_outside_letterboxed_canvas() {
        let mapped = map_view_to_canvas(
            &sample_frame(),
            CanvasPointerEvent {
                x: 10,
                y: 10,
                width: 900,
                height: 640,
            },
        );

        assert_eq!(mapped, None);
    }

    #[test]
    fn map_view_with_zoom_and_pan_tracks_shifted_canvas() {
        let mapped = map_view_to_canvas_with_transform(
            &sample_frame(),
            CanvasPointerEvent {
                x: 352,
                y: 320,
                width: 640,
                height: 640,
            },
            CanvasViewTransform {
                zoom: 2.0,
                rotation_degrees: 0.0,
                pan_x: 32.0,
                pan_y: 0.0,
            },
        );

        assert_eq!(mapped, Some((32, 32)));
    }

    #[test]
    fn brush_drag_becomes_draw_stroke() {
        let command = command_for_canvas_gesture(ToolKind::Brush, (4, 5), Some((1, 2)));

        assert_eq!(
            command,
            Command::DrawStroke {
                from_x: 1,
                from_y: 2,
                to_x: 4,
                to_y: 5,
            }
        );
    }
}
