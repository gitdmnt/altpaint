use app_core::CanvasDirtyRect;

use crate::{PixelRect, union_dirty_rect};

/// 描画レイヤーグループの識別子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerGroup {
    Background,
    Canvas,
    TempOverlay,
    UiPanel,
}

/// 各レイヤーグループの dirty rect を独立して管理する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LayerGroupDirtyPlan {
    pub background: Option<PixelRect>,
    pub canvas: Option<CanvasDirtyRect>,
    pub canvas_transform_changed: bool,
    pub temp_overlay: Option<PixelRect>,
    pub ui_panel: Option<PixelRect>,
}

impl LayerGroupDirtyPlan {
    /// Background を更新し、必要な dirty 状態も記録する。
    pub fn mark_background(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.background, rect);
    }

    /// TempOverlay を更新し、必要な dirty 状態も記録する。
    pub fn mark_temp_overlay(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.temp_overlay, rect);
    }

    /// UiPanel を更新し、必要な dirty 状態も記録する。
    pub fn mark_ui_panel(&mut self, rect: PixelRect) {
        union_dirty_rect(&mut self.ui_panel, rect);
    }
}
