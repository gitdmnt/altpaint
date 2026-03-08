use app_core::{Command, ToolKind};
use render::RenderFrame;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanvasInputState {
    pub is_drawing: bool,
    pub last_position: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasPointerEvent {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn map_view_to_canvas(
    frame: &RenderFrame,
    event: CanvasPointerEvent,
) -> Option<(usize, usize)> {
    if frame.width == 0 || frame.height == 0 || event.width <= 0 || event.height <= 0 {
        return None;
    }

    let scale_x = event.width as f32 / frame.width as f32;
    let scale_y = event.height as f32 / frame.height as f32;
    let scale = scale_x.min(scale_y);
    if scale <= 0.0 {
        return None;
    }

    let drawn_width = frame.width as f32 * scale;
    let drawn_height = frame.height as f32 * scale;
    let offset_x = (event.width as f32 - drawn_width) * 0.5;
    let offset_y = (event.height as f32 - drawn_height) * 0.5;

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

pub fn command_for_canvas_gesture(
    tool: ToolKind,
    current: (usize, usize),
    previous: Option<(usize, usize)>,
) -> Command {
    match (tool, previous) {
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
