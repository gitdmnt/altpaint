mod context_tests;
mod fill_tests;
mod input_tests;
mod stamp_tests;
mod stroke_tests;

use app_core::{Document, PaintInput};
use geometry::PageDirtyRect;

use crate::PaintEngine;

pub(crate) fn apply_input(
    document: &mut Document,
    engine: &PaintEngine,
    input: PaintInput,
) -> Option<PageDirtyRect> {
    let edits = engine.compute_paint_edits(document, &input)?;
    document.apply_bitmap_edits_to_active_layer(&edits)
}
