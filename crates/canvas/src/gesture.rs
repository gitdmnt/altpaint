use app_core::{CanvasPoint, PaintInput, PanelLocalPoint, ToolKind};

use crate::CanvasInputState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasPointerAction {
    Down,
    Drag,
    Up,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CanvasGestureUpdate {
    None,
    Paint(PaintInput),
    LassoPreviewChanged,
    PanelRectPreviewChanged,
    PanelRectCommitted { anchor: CanvasPoint, current: CanvasPoint },
}

pub fn advance_pointer_gesture<F>(
    state: &mut CanvasInputState,
    action: CanvasPointerAction,
    point: CanvasPoint,
    active_tool: ToolKind,
    pressure: f32,
    stabilization: u8,
    mut to_panel_local: F,
) -> CanvasGestureUpdate
where
    F: FnMut(CanvasPoint) -> Option<PanelLocalPoint>,
{
    match action {
        CanvasPointerAction::Down => handle_pointer_down(
            state,
            point,
            active_tool,
            pressure,
            stabilization,
            &mut to_panel_local,
        ),
        CanvasPointerAction::Drag => handle_pointer_drag(
            state,
            point,
            active_tool,
            pressure,
            stabilization,
            &mut to_panel_local,
        ),
        CanvasPointerAction::Up => handle_pointer_up(
            state,
            point,
            active_tool,
            pressure,
            &mut to_panel_local,
        ),
    }
}

fn handle_pointer_down<F>(
    state: &mut CanvasInputState,
    point: CanvasPoint,
    active_tool: ToolKind,
    pressure: f32,
    stabilization: u8,
    to_panel_local: &mut F,
) -> CanvasGestureUpdate
where
    F: FnMut(CanvasPoint) -> Option<PanelLocalPoint>,
{
    match active_tool {
        ToolKind::Bucket => to_panel_local(point)
            .map(|at| CanvasGestureUpdate::Paint(PaintInput::FloodFill { at }))
            .unwrap_or(CanvasGestureUpdate::None),
        ToolKind::LassoBucket => {
            state.is_drawing = true;
            state.last_position = Some(point);
            state.last_smoothed_position = Some((point.x as f32, point.y as f32));
            state.lasso_points.clear();
            state.lasso_points.push(point);
            CanvasGestureUpdate::LassoPreviewChanged
        }
        ToolKind::PanelRect => {
            state.is_drawing = true;
            state.panel_rect_anchor = Some(point);
            state.last_position = Some(point);
            CanvasGestureUpdate::PanelRectPreviewChanged
        }
        ToolKind::Pen | ToolKind::Eraser => {
            state.is_drawing = true;
            state.last_position = Some(point);
            state.last_smoothed_position = Some((point.x as f32, point.y as f32));
            let _ = stabilization;
            to_panel_local(point)
                .map(|at| CanvasGestureUpdate::Paint(PaintInput::Stamp { at, pressure }))
                .unwrap_or(CanvasGestureUpdate::None)
        }
    }
}

fn handle_pointer_drag<F>(
    state: &mut CanvasInputState,
    point: CanvasPoint,
    active_tool: ToolKind,
    pressure: f32,
    stabilization: u8,
    to_panel_local: &mut F,
) -> CanvasGestureUpdate
where
    F: FnMut(CanvasPoint) -> Option<PanelLocalPoint>,
{
    if !state.is_drawing {
        return CanvasGestureUpdate::None;
    }

    match active_tool {
        ToolKind::LassoBucket => {
            if state.lasso_points.last().copied() != Some(point) {
                state.lasso_points.push(point);
                state.last_position = Some(point);
                CanvasGestureUpdate::LassoPreviewChanged
            } else {
                CanvasGestureUpdate::None
            }
        }
        ToolKind::PanelRect => {
            if state.last_position == Some(point) {
                return CanvasGestureUpdate::None;
            }
            state.last_position = Some(point);
            CanvasGestureUpdate::PanelRectPreviewChanged
        }
        ToolKind::Pen | ToolKind::Eraser => {
            let next_position = stabilized_canvas_position(state, point, active_tool, stabilization);
            let previous = state.last_position;
            if previous == Some(next_position) {
                return CanvasGestureUpdate::None;
            }
            state.last_position = Some(next_position);
            previous
                .and_then(|from| Some((to_panel_local(from)?, to_panel_local(next_position)?)))
                .map(|(from, to)| {
                    CanvasGestureUpdate::Paint(PaintInput::StrokeSegment { from, to, pressure })
                })
                .unwrap_or(CanvasGestureUpdate::None)
        }
        ToolKind::Bucket => CanvasGestureUpdate::None,
    }
}

fn handle_pointer_up<F>(
    state: &mut CanvasInputState,
    point: CanvasPoint,
    active_tool: ToolKind,
    pressure: f32,
    to_panel_local: &mut F,
) -> CanvasGestureUpdate
where
    F: FnMut(CanvasPoint) -> Option<PanelLocalPoint>,
{
    match active_tool {
        ToolKind::LassoBucket => {
            let update = if state.lasso_points.len() >= 3 {
                state
                    .lasso_points
                    .iter()
                    .copied()
                    .map(&mut *to_panel_local)
                    .collect::<Option<Vec<_>>>()
                    .map(|points| CanvasGestureUpdate::Paint(PaintInput::LassoFill { points }))
                    .unwrap_or(CanvasGestureUpdate::None)
            } else {
                CanvasGestureUpdate::LassoPreviewChanged
            };
            state.reset();
            update
        }
        ToolKind::PanelRect => {
            let anchor = state.panel_rect_anchor;
            let current = state.last_position.or(Some(point));
            state.reset();
            match (anchor, current) {
                (Some(anchor), Some(current)) => {
                    CanvasGestureUpdate::PanelRectCommitted { anchor, current }
                }
                _ => CanvasGestureUpdate::None,
            }
        }
        ToolKind::Pen | ToolKind::Eraser => {
            let previous = state.last_position;
            let update = if state.is_drawing && previous != Some(point) {
                previous
                    .and_then(|from| Some((to_panel_local(from)?, to_panel_local(point)?)))
                    .map(|(from, to)| {
                        CanvasGestureUpdate::Paint(PaintInput::StrokeSegment { from, to, pressure })
                    })
                    .unwrap_or(CanvasGestureUpdate::None)
            } else {
                CanvasGestureUpdate::None
            };
            state.reset();
            update
        }
        ToolKind::Bucket => CanvasGestureUpdate::None,
    }
}

fn stabilized_canvas_position(
    state: &mut CanvasInputState,
    point: CanvasPoint,
    active_tool: ToolKind,
    stabilization: u8,
) -> CanvasPoint {
    if active_tool != ToolKind::Pen {
        state.last_smoothed_position = Some((point.x as f32, point.y as f32));
        return point;
    }
    if stabilization == 0 {
        state.last_smoothed_position = Some((point.x as f32, point.y as f32));
        return point;
    }

    let blend = (1.0 / (1.0 + stabilization as f32 / 12.0)).clamp(0.05, 1.0);
    let previous = state
        .last_smoothed_position
        .unwrap_or((point.x as f32, point.y as f32));
    let next = (
        previous.0 + (point.x as f32 - previous.0) * blend,
        previous.1 + (point.y as f32 - previous.1) * blend,
    );
    state.last_smoothed_position = Some(next);
    CanvasPoint::new(
        next.0.round().max(0.0) as usize,
        next.1.round().max(0.0) as usize,
    )
}
