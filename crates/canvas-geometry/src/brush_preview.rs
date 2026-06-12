use geometry::{PagePoint, WindowRect};

use crate::{CanvasViewGeometry, union_optional_rect};

pub fn brush_preview_dirty_rect(
    previous_geometry: Option<CanvasViewGeometry>,
    current_geometry: Option<CanvasViewGeometry>,
    canvas_position: PagePoint,
    brush_diameter: f32,
) -> Option<WindowRect> {
    let previous = previous_geometry
        .and_then(|geometry| geometry.brush_preview_rect_for_diameter(canvas_position, brush_diameter));
    let current = current_geometry
        .and_then(|geometry| geometry.brush_preview_rect_for_diameter(canvas_position, brush_diameter));

    union_optional_rect(previous, current)
}
