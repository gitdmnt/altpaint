mod context_tests;
mod fill_tests;
mod input_tests;
mod stamp_tests;
mod stroke_tests;

use app_core::{CanvasDirtyRect, Document, PaintInput};

use crate::CanvasRuntime;

pub(crate) fn apply_input(
    document: &mut Document,
    runtime: &CanvasRuntime,
    input: PaintInput,
) -> Option<CanvasDirtyRect> {
    let edits = runtime.execute_paint_input(document, &input);
    document.apply_bitmap_edits_to_active_layer(&edits)
}
