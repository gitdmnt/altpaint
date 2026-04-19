//! `canvas` は入力解釈・描画ランタイム・ビットマップ操作を集約する。

mod context;
mod context_builder;
pub mod edit_record;
mod gesture;
mod input_state;
pub mod ops;
pub mod plugins;
mod registry;
mod render_bridge;
mod runtime;
#[cfg(test)]
mod tests;
mod view_mapping;

pub use context::ResolvedPaintContext;
pub use context_builder::{build_paint_context, resolved_size_for_input};
pub use ops::compute_stamp_positions;
pub use edit_record::{BitmapEditOperation, BitmapEditRecord};
pub use gesture::{CanvasGestureUpdate, CanvasPointerAction, advance_pointer_gesture};
pub use input_state::CanvasInputState;
pub use registry::{PaintPluginRegistry, STANDARD_BITMAP_PLUGIN_ID, default_paint_plugins};
pub use render_bridge::panel_creation_preview_bounds;
pub use runtime::{CanvasRuntime, PaintResult};
pub use view_mapping::{CanvasPointerEvent, map_view_to_canvas_with_transform};
