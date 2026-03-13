mod context_tests;
mod fill_tests;
mod input_tests;
mod stamp_tests;
mod stroke_tests;

use app_core::{CanvasDirtyRect, Document, PaintInput};

use crate::CanvasRuntime;

/// 入力 を現在の状態へ適用する。
///
/// 必要に応じて dirty 状態も更新します。
pub(crate) fn apply_input(
    document: &mut Document,
    runtime: &CanvasRuntime,
    input: PaintInput,
) -> Option<CanvasDirtyRect> {
    let result = runtime.execute_paint_input(document, &input)?;
    document.apply_bitmap_edits_to_active_layer(&result.edits)
}
