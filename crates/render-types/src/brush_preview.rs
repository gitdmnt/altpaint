use app_core::CanvasPoint;

use crate::{CanvasScene, PixelRect, union_optional_rect};

/// ブラシ プレビュー 差分 矩形 に必要な処理を行う。
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
