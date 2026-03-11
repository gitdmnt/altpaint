use app_core::{BitmapEdit, PaintPluginContext, PanelLocalPoint};

use super::{bitmap_from_points, composite, point_in_polygon};

pub(crate) fn lasso_fill_edit(
    points: &[PanelLocalPoint],
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    if points.len() < 3 {
        return None;
    }
    let min_x = points.iter().map(|point| point.x).min()?;
    let min_y = points.iter().map(|point| point.y).min()?;
    let max_x = points.iter().map(|point| point.x).max()?;
    let max_y = points.iter().map(|point| point.y).max()?;
    let width_limit = context.composited_bitmap.width.saturating_sub(1);
    let height_limit = context.composited_bitmap.height.saturating_sub(1);
    let fill = composite::fill_color(context);
    let mut hits = Vec::new();

    for y in min_y..=max_y.min(height_limit) {
        for x in min_x..=max_x.min(width_limit) {
            if point_in_polygon((x as f32) + 0.5, (y as f32) + 0.5, points) {
                hits.push((x, y));
            }
        }
    }

    bitmap_from_points(hits, min_x, min_y, max_x, max_y, fill, context)
}
