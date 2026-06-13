//! ブラシプレビュー dirty rect 演算。
//!
//! B7 で `app/paint_preview.rs` から features/paint へ移設した。

use canvas_geometry::CanvasViewGeometry;
use geometry::{PagePoint, WindowRect, union_optional_rect};

/// 移動前後のブラシプレビュー矩形を合算した dirty rect を返す。
pub(crate) fn brush_preview_dirty_rect(
    previous_geometry: Option<CanvasViewGeometry>,
    current_geometry: Option<CanvasViewGeometry>,
    canvas_position: PagePoint,
    brush_diameter: f32,
) -> Option<WindowRect> {
    let previous = previous_geometry.and_then(|geometry| {
        geometry.brush_preview_rect_for_diameter(canvas_position, brush_diameter)
    });
    let current = current_geometry.and_then(|geometry| {
        geometry.brush_preview_rect_for_diameter(canvas_position, brush_diameter)
    });

    union_optional_rect(previous, current)
}

#[cfg(test)]
mod tests {
    use super::brush_preview_dirty_rect;
    use canvas_geometry::CanvasViewGeometry;
    use editor_state::CanvasViewTransform;
    use geometry::{PagePoint, WindowRect};

    #[test]
    fn brush_preview_dirty_rect_unions_previous_and_current_preview() {
        let viewport = WindowRect {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        let previous =
            CanvasViewGeometry::compute(viewport, 64, 64, CanvasViewTransform::default());
        let current = CanvasViewGeometry::compute(
            viewport,
            64,
            64,
            CanvasViewTransform {
                pan_x: 20.0,
                ..CanvasViewTransform::default()
            },
        );

        let dirty = brush_preview_dirty_rect(previous, current, PagePoint::new(20, 20), 12.0)
            .expect("dirty rect exists");

        assert!(dirty.width > 0);
        assert!(dirty.height > 0);
    }
}
