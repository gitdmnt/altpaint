//! `paint-engine` は入力解釈・描画ランタイム・ビットマップ操作を集約する。

mod context;
mod context_builder;
mod engine;
mod gesture;
mod input_state;
pub mod ops;
pub mod plugins;
#[cfg(test)]
mod tests;
mod view_mapping;

pub use context::ResolvedPaintContext;
pub use context_builder::{build_paint_context, resolved_size_for_input};
pub use gesture::{CanvasGestureUpdate, CanvasPointerAction, advance_pointer_gesture};
pub use input_state::{CanvasInputState, koma_creation_preview_bounds};
pub use ops::compute_stamp_positions;
pub use plugins::{BUILTIN_BITMAP_BACKEND_ID, PaintPluginRegistry, default_paint_plugins};
pub use engine::PaintEngine;
pub use view_mapping::{CanvasPointerEvent, map_view_to_canvas_with_transform};
