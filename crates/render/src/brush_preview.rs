use app_core::CanvasPoint;

use crate::{CanvasScene, PixelRect};

/// ビュー変換前後のブラシプレビューが占有する dirty 領域を返す。
pub fn brush_preview_dirty_rect(
    previous_scene: Option<CanvasScene>,
    current_scene: Option<CanvasScene>,
    canvas_position: CanvasPoint,
    brush_diameter: f32,
) -> Option<PixelRect> {
    let previous = previous_scene
        .and_then(|scene| scene.brush_preview_rect_for_diameter(canvas_position, brush_diameter));
    let current = current_scene
        .and_then(|scene| scene.brush_preview_rect_for_diameter(canvas_position, brush_diameter));

    union_optional_rect(previous, current)
}

fn union_optional_rect(left: Option<PixelRect>, right: Option<PixelRect>) -> Option<PixelRect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}
