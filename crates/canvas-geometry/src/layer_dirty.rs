use crate::{PixelRect, accumulate_dirty_rect};

/// 各レイヤーグループの dirty rect を独立して蓄積する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LayerDirtyAccumulator {
    pub background: Option<PixelRect>,
    pub temp_overlay: Option<PixelRect>,
    pub ui_panel: Option<PixelRect>,
}

impl LayerDirtyAccumulator {
    pub fn mark_background(&mut self, rect: PixelRect) {
        accumulate_dirty_rect(&mut self.background, rect);
    }

    pub fn mark_temp_overlay(&mut self, rect: PixelRect) {
        accumulate_dirty_rect(&mut self.temp_overlay, rect);
    }

    pub fn mark_ui_panel(&mut self, rect: PixelRect) {
        accumulate_dirty_rect(&mut self.ui_panel, rect);
    }
}
