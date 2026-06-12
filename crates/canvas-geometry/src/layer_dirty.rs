use app_core::WindowRect;

use crate::accumulate_dirty_rect;

/// 各レイヤーグループの dirty rect を独立して蓄積する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LayerDirtyAccumulator {
    pub background: Option<WindowRect>,
    pub temp_overlay: Option<WindowRect>,
    pub ui_panel: Option<WindowRect>,
}

impl LayerDirtyAccumulator {
    pub fn mark_background(&mut self, rect: WindowRect) {
        accumulate_dirty_rect(&mut self.background, rect);
    }

    pub fn mark_temp_overlay(&mut self, rect: WindowRect) {
        accumulate_dirty_rect(&mut self.temp_overlay, rect);
    }

    pub fn mark_ui_panel(&mut self, rect: WindowRect) {
        accumulate_dirty_rect(&mut self.ui_panel, rect);
    }
}
