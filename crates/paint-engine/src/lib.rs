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

pub use context::ResolvedPaintContext;
pub use context_builder::{build_paint_context, resolved_size_for_input};
pub use gesture::{CanvasGestureUpdate, CanvasPointerAction, advance_pointer_gesture};
pub use input_state::CanvasInputState;
pub use ops::compute_stamp_positions;
pub use plugins::{BUILTIN_BITMAP_BACKEND_ID, PaintPluginRegistry, default_paint_plugins};
pub use engine::PaintEngine;
