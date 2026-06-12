use app_core::{PagePoint, PaintInput, KomaLocalPoint, ToolKind};

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
    KomaRectPreviewChanged,
    KomaRectCommitted {
        anchor: PagePoint,
        current: PagePoint,
    },
}

pub fn advance_pointer_gesture<F>(
    state: &mut CanvasInputState,
    action: CanvasPointerAction,
    point: PagePoint,
    active_tool: ToolKind,
    pressure: f32,
    stabilization: u8,
    mut to_koma_local: F,
) -> CanvasGestureUpdate
where
    F: FnMut(PagePoint) -> Option<KomaLocalPoint>,
{
    match action {
        CanvasPointerAction::Down => handle_pointer_down(
            state,
            point,
            active_tool,
            pressure,
            stabilization,
            &mut to_koma_local,
        ),
        CanvasPointerAction::Drag => handle_pointer_drag(
            state,
            point,
            active_tool,
            pressure,
            stabilization,
            &mut to_koma_local,
        ),
        CanvasPointerAction::Up => {
            handle_pointer_up(state, point, active_tool, pressure, &mut to_koma_local)
        }
    }
}

fn handle_pointer_down<F>(
    state: &mut CanvasInputState,
    point: PagePoint,
    active_tool: ToolKind,
    pressure: f32,
    stabilization: u8,
    to_koma_local: &mut F,
) -> CanvasGestureUpdate
where
    F: FnMut(PagePoint) -> Option<KomaLocalPoint>,
{
    match active_tool {
        ToolKind::Bucket => to_koma_local(point)
            .map(|at| CanvasGestureUpdate::Paint(PaintInput::FloodFill { at }))
            .unwrap_or(CanvasGestureUpdate::None),
        ToolKind::LassoBucket => {
            state.is_drawing = true;
            state.last_position = Some(point);
            state.last_smoothed_position = Some(point.into());
            state.lasso_points.clear();
            state.lasso_points.push(point);
            CanvasGestureUpdate::LassoPreviewChanged
        }
        ToolKind::KomaRect => {
            state.is_drawing = true;
            state.koma_rect_anchor = Some(point);
            state.last_position = Some(point);
            CanvasGestureUpdate::KomaRectPreviewChanged
        }
        ToolKind::Pen | ToolKind::Eraser => {
            state.is_drawing = true;
            state.last_position = Some(point);
            state.last_smoothed_position = Some(point.into());
            let _ = stabilization;
            to_koma_local(point)
                .map(|at| CanvasGestureUpdate::Paint(PaintInput::Stamp { at, pressure }))
                .unwrap_or(CanvasGestureUpdate::None)
        }
    }
}

fn handle_pointer_drag<F>(
    state: &mut CanvasInputState,
    point: PagePoint,
    active_tool: ToolKind,
    pressure: f32,
    stabilization: u8,
    to_koma_local: &mut F,
) -> CanvasGestureUpdate
where
    F: FnMut(PagePoint) -> Option<KomaLocalPoint>,
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
        ToolKind::KomaRect => {
            if state.last_position == Some(point) {
                return CanvasGestureUpdate::None;
            }
            state.last_position = Some(point);
            CanvasGestureUpdate::KomaRectPreviewChanged
        }
        ToolKind::Pen | ToolKind::Eraser => {
            let next_position =
                stabilized_canvas_position(state, point, active_tool, stabilization);
            let previous = state.last_position;
            if previous == Some(next_position) {
                return CanvasGestureUpdate::None;
            }
            state.last_position = Some(next_position);
            previous
                .and_then(|from| Some((to_koma_local(from)?, to_koma_local(next_position)?)))
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
    point: PagePoint,
    active_tool: ToolKind,
    pressure: f32,
    to_koma_local: &mut F,
) -> CanvasGestureUpdate
where
    F: FnMut(PagePoint) -> Option<KomaLocalPoint>,
{
    match active_tool {
        ToolKind::LassoBucket => {
            let update = if state.lasso_points.len() >= 3 {
                state
                    .lasso_points
                    .iter()
                    .copied()
                    .map(&mut *to_koma_local)
                    .collect::<Option<Vec<_>>>()
                    .map(|points| CanvasGestureUpdate::Paint(PaintInput::LassoFill { points }))
                    .unwrap_or(CanvasGestureUpdate::None)
            } else {
                CanvasGestureUpdate::LassoPreviewChanged
            };
            state.reset();
            update
        }
        ToolKind::KomaRect => {
            let anchor = state.koma_rect_anchor;
            let current = state.last_position.or(Some(point));
            state.reset();
            match (anchor, current) {
                (Some(anchor), Some(current)) => {
                    CanvasGestureUpdate::KomaRectCommitted { anchor, current }
                }
                _ => CanvasGestureUpdate::None,
            }
        }
        ToolKind::Pen | ToolKind::Eraser => {
            let previous = state.last_position;
            let update = if state.is_drawing && previous != Some(point) {
                previous
                    .and_then(|from| Some((to_koma_local(from)?, to_koma_local(point)?)))
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
    point: PagePoint,
    active_tool: ToolKind,
    stabilization: u8,
) -> PagePoint {
    if active_tool != ToolKind::Pen || stabilization == 0 {
        state.last_smoothed_position = Some(point.into());
        return point;
    }

    let blend = (1.0 / (1.0 + stabilization as f32 / 12.0)).clamp(0.05, 1.0);
    let previous = state.last_smoothed_position.unwrap_or(point.into());
    let next = previous.lerp_toward(point.into(), blend);
    state.last_smoothed_position = Some(next);
    next.to_page_point()
}
