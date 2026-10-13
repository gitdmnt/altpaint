//! presenter レイヤーグループごとの dirty rect アキュムレータ。

use geometry::{WindowRect, accumulate_dirty_rect};

/// 各レイヤーグループの dirty rect を独立して蓄積する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayerDirtyAccumulator {
    pub(crate) background: Option<WindowRect>,
    pub(crate) temp_overlay: Option<WindowRect>,
    pub(crate) ui_panel: Option<WindowRect>,
}

impl LayerDirtyAccumulator {
    pub(crate) fn mark_temp_overlay(&mut self, rect: WindowRect) {
        accumulate_dirty_rect(&mut self.temp_overlay, rect);
    }

    pub(crate) fn mark_ui_panel(&mut self, rect: WindowRect) {
        accumulate_dirty_rect(&mut self.ui_panel, rect);
    }
}

#[cfg(test)]
mod tests {
    use super::LayerDirtyAccumulator;
    use geometry::WindowRect;

    fn rect(x: usize, y: usize, width: usize, height: usize) -> WindowRect {
        WindowRect { x, y, width, height }
    }

    #[test]
    fn marking_ui_panel_dirty_does_not_affect_other_groups() {
        let mut d = LayerDirtyAccumulator::default();
        d.mark_ui_panel(rect(0, 0, 100, 100));
        assert!(d.temp_overlay.is_none());
        assert!(d.background.is_none());
    }

    #[test]
    fn marking_temp_overlay_dirty_does_not_affect_other_groups() {
        let mut d = LayerDirtyAccumulator::default();
        d.mark_temp_overlay(rect(0, 0, 100, 100));
        assert!(d.ui_panel.is_none());
        assert!(d.background.is_none());
    }

    #[test]
    fn marking_temp_overlay_dirty_leaves_background_clear() {
        let mut d = LayerDirtyAccumulator::default();
        d.mark_temp_overlay(rect(0, 0, 200, 200));
        assert!(d.background.is_none());
    }

    #[test]
    fn dirty_rects_union_within_same_group() {
        let mut d = LayerDirtyAccumulator::default();
        d.mark_temp_overlay(rect(0, 0, 50, 50));
        d.mark_temp_overlay(rect(60, 60, 50, 50));
        let r = d.temp_overlay.unwrap();
        assert!(r.width > 50);
    }
}
