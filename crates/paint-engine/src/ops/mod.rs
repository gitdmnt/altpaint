pub(crate) mod composite;
pub(crate) mod flood_fill;
pub(crate) mod lasso_fill;
pub(crate) mod stamp;
pub(crate) mod stroke;

pub use stroke::compute_stamp_positions;
pub(crate) use stamp::stamp_dimensions;
pub(crate) use stroke::stroke_dirty_rect;

use crate::painting::{PaintInput, PaintPluginContext};
use geometry::{KomaLocalPoint, PageDirtyRect};
use raster::{BitmapEdit, RgbaBitmap};

/// ペイント入力と解決済みコンテキストからビットマップ差分列を生成する (CPU 参照実装)。
///
/// R5 で `PaintPlugin` trait + registry を撤去し、この直接ディスパッチへ置換した。
/// 描画バックエンドは `BUILTIN_BITMAP_BACKEND_ID` ただ 1 つであり、ツール種別ごとの
/// 分岐は入力 variant の match で完結する。
pub(crate) fn compute_bitmap_edits(
    input: &PaintInput,
    context: &PaintPluginContext<'_>,
) -> Vec<BitmapEdit> {
    match input {
        PaintInput::Stamp { at, .. } => stamp::stamp_edit(*at, context).into_iter().collect(),
        PaintInput::StrokeSegment { from, to, .. } => {
            stroke::stroke_segment_edit(*from, *to, context)
                .into_iter()
                .collect()
        }
        PaintInput::FloodFill { at } => flood_fill::flood_fill_edit(*at, context)
            .into_iter()
            .collect(),
        PaintInput::LassoFill { points } => lasso_fill::lasso_fill_edit(points, context)
            .into_iter()
            .collect(),
    }
}

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
    let dirty_rect = PageDirtyRect {
        x: min_x,
        y: min_y,
        width: max_x.saturating_sub(min_x).saturating_add(1),
        height: max_y.saturating_sub(min_y).saturating_add(1),
    };
    let mut bitmap = RgbaBitmap::transparent(dirty_rect.width, dirty_rect.height);
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

pub(crate) fn point_in_polygon(x: f32, y: f32, points: &[KomaLocalPoint]) -> bool {
    let mut inside = false;
    let mut previous = *points.last().expect("polygon has points");
    for current in points {
        let (x1, y1) = (previous.x as f32, previous.y as f32);
        let (x2, y2) = (current.x as f32, current.y as f32);
        let intersects = ((y1 > y) != (y2 > y))
            && (x < (x2 - x1) * (y - y1) / (y2 - y1) + x1);
        if intersects {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}
