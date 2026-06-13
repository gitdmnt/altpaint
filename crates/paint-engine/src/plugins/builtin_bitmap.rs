use crate::painting::{PaintInput, PaintPlugin, PaintPluginContext};
use raster::BitmapEdit;

use crate::{BUILTIN_BITMAP_BACKEND_ID, ops};

pub struct BuiltinBitmapPaintPlugin;

impl PaintPlugin for BuiltinBitmapPaintPlugin {
    fn id(&self) -> &'static str {
        BUILTIN_BITMAP_BACKEND_ID
    }

    fn process(&self, input: &PaintInput, context: &PaintPluginContext<'_>) -> Vec<BitmapEdit> {
        match input {
            PaintInput::Stamp { at, .. } => ops::stamp::stamp_edit(*at, context)
                .into_iter()
                .collect(),
            PaintInput::StrokeSegment { from, to, .. } => {
                ops::stroke::stroke_segment_edit(*from, *to, context)
                    .into_iter()
                    .collect()
            }
            PaintInput::FloodFill { at } => ops::flood_fill::flood_fill_edit(*at, context)
                .into_iter()
                .collect(),
            PaintInput::LassoFill { points } => ops::lasso_fill::lasso_fill_edit(points, context)
                .into_iter()
                .collect(),
        }
    }
}
