//! `render-types` は `render` クレートから抽出された描画計画用の純データ型を提供する。
//!
//! wgpu/fontdb 等の重量依存を持たず、`app-core` のドメイン型のみに依存する。

mod brush_preview;
mod canvas_plan;
mod canvas_scene;
mod dirty;
mod layer_group;
mod overlay_plan;

#[cfg(test)]
pub mod test_support;

pub use brush_preview::brush_preview_dirty_rect;
pub use canvas_plan::CanvasPlan;
pub use canvas_scene::{
    CanvasScene, PixelRect, TextureQuad, brush_preview_rect_for_diameter, canvas_texture_quad,
    map_canvas_dirty_to_display_with_transform, map_canvas_point_to_display,
    map_view_to_canvas_with_transform, prepare_canvas_scene,
};
pub use dirty::{union_dirty_rect, union_optional_rect};
pub use layer_group::LayerGroupDirtyPlan;
pub use overlay_plan::{CanvasOverlayState, PanelNavigatorEntry, PanelNavigatorOverlay};

#[cfg(test)]
mod tests;
