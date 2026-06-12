use app_core::{BitmapEdit, PaintPluginContext, PanelLocalPoint};

use super::{bitmap_from_points, composite};

pub(crate) fn flood_fill_edit(
    at: PanelLocalPoint,
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    let width = context.composited_bitmap.width;
    let height = context.composited_bitmap.height;
    if at.x >= width || at.y >= height {
        return None;
    }
    let target = context.composited_bitmap.pixel_rgba(at.x, at.y)?;
    let fill = composite::fill_color(context);
    if target == fill {
        return None;
    }

    let mut visited = vec![false; width.saturating_mul(height)];
    let mut stack = vec![(at.x, at.y)];
    let mut points = Vec::new();
    let mut min_x = usize::MAX;
    let mut min_y = usize::MAX;
    let mut max_x = 0usize;
    let mut max_y = 0usize;

    while let Some((x, y)) = stack.pop() {
        if x >= width || y >= height {
            continue;
        }
        let index = y * width + x;
        if visited[index] {
            continue;
        }
        visited[index] = true;
        if context.composited_bitmap.pixel_rgba(x, y) != Some(target) {
            continue;
        }
        points.push((x, y));
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);

        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < width {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < height {
            stack.push((x, y + 1));
        }
    }

    bitmap_from_points(points, min_x, min_y, max_x, max_y, fill, context)
}
