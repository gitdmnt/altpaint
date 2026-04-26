//! `render-types` は `render` クレートから抽出された描画計画用の純データ型を提供する。
//!
//! wgpu/fontdb 等の重量依存を持たず、`app-core` のドメイン型のみに依存する。

mod brush_preview;
mod canvas_plan;
mod canvas_scene;
mod dirty;
mod frame_plan;
mod layer_group;
mod overlay_plan;
mod panel_plan;

pub use brush_preview::brush_preview_dirty_rect;
pub use canvas_plan::{CanvasCompositeSource, CanvasPlan};
pub use canvas_scene::{
    CanvasScene, PixelRect, TextureQuad, brush_preview_rect, brush_preview_rect_for_diameter,
    canvas_drawn_rect, canvas_texture_quad, exposed_canvas_background_rect,
    exposed_canvas_background_rect_from_scenes, map_canvas_dirty_to_display_with_transform,
    map_canvas_point_to_display, map_view_to_canvas_with_transform, prepare_canvas_scene,
};
pub use dirty::{union_dirty_rect, union_optional_rect};
pub use frame_plan::FramePlan;
pub use layer_group::{LayerGroup, LayerGroupDirtyPlan};
pub use overlay_plan::{CanvasOverlayState, PanelNavigatorEntry, PanelNavigatorOverlay};
pub use panel_plan::{PanelPlan, PanelSurfaceSource};

#[cfg(test)]
mod tests;
