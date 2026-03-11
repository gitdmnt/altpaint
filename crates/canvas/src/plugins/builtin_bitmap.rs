use app_core::{BitmapEdit, PaintInput, PaintPlugin, PaintPluginContext};

use crate::{STANDARD_BITMAP_PLUGIN_ID, ops};

pub struct BuiltinBitmapPaintPlugin;

impl PaintPlugin for BuiltinBitmapPaintPlugin {
    fn id(&self) -> &'static str {
        STANDARD_BITMAP_PLUGIN_ID
    }

    fn process(&self, input: &PaintInput, context: &PaintPluginContext<'_>) -> Vec<BitmapEdit> {
        match input {
            PaintInput::Stamp { at, pressure } => ops::stamp::stamp_edit(*at, *pressure, context)
                .into_iter()
                .collect(),
            PaintInput::StrokeSegment { from, to, pressure } => {
                ops::stroke::stroke_segment_edit(*from, *to, *pressure, context)
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
