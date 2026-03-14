pub(crate) mod composite;
pub(crate) mod flood_fill;
pub(crate) mod lasso_fill;
pub(crate) mod stamp;
pub(crate) mod stroke;
pub mod text;

use app_core::{BitmapEdit, CanvasBitmap, CanvasDirtyRect, PaintPluginContext, PanelLocalPoint};

/// ビットマップ from points に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub(crate) fn bitmap_from_points(
    points: Vec<(usize, usize)>,
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    fill: [u8; 4],
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    if points.is_empty() || min_x == usize::MAX || min_y == usize::MAX {
        return None;
    }
    let dirty_rect = CanvasDirtyRect {
        x: min_x,
        y: min_y,
        width: max_x.saturating_sub(min_x).saturating_add(1),
        height: max_y.saturating_sub(min_y).saturating_add(1),
    };
    let mut bitmap = CanvasBitmap::transparent(dirty_rect.width, dirty_rect.height);
    for (x, y) in points {
        let local_x = x.saturating_sub(min_x);
        let local_y = y.saturating_sub(min_y);
        let index = (local_y * bitmap.width + local_x) * 4;
        bitmap.pixels[index..index + 4].copy_from_slice(&fill);
    }
    Some(BitmapEdit::new(
        dirty_rect,
        bitmap,
        composite::edit_composite(context),
    ))
}

/// 点 in polygon を計算して返す。
pub(crate) fn point_in_polygon(x: f32, y: f32, points: &[PanelLocalPoint]) -> bool {
    let mut inside = false;
    let mut previous = *points.last().expect("polygon has points");
    for current in points {
        let (x1, y1) = (previous.x as f32, previous.y as f32);
        let (x2, y2) = (current.x as f32, current.y as f32);
        let intersects = ((y1 > y) != (y2 > y))
            && (x < (x2 - x1) * (y - y1) / ((y2 - y1).abs().max(f32::EPSILON)) + x1);
        if intersects {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}
