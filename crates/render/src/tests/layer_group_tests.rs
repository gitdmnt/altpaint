use crate::PixelRect;
use crate::layer_group::LayerGroupDirtyPlan;

fn rect(x: usize, y: usize, width: usize, height: usize) -> PixelRect {
    PixelRect { x, y, width, height }
}

#[test]
fn marking_ui_panel_dirty_does_not_affect_other_groups() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_ui_panel(rect(0, 0, 100, 100));
    assert!(d.temp_overlay.is_none());
    assert!(d.canvas.is_none());
    assert!(d.background.is_none());
}

#[test]
fn marking_temp_overlay_dirty_does_not_affect_other_groups() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_temp_overlay(rect(0, 0, 100, 100));
    assert!(d.ui_panel.is_none());
    assert!(d.canvas.is_none());
    assert!(d.background.is_none());
}

#[test]
fn marking_background_dirty_does_not_affect_overlay_groups() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_background(rect(0, 0, 200, 200));
    assert!(d.temp_overlay.is_none());
    assert!(d.ui_panel.is_none());
    assert!(d.canvas.is_none());
}

#[test]
fn dirty_rects_union_within_same_group() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_temp_overlay(rect(0, 0, 50, 50));
    d.mark_temp_overlay(rect(60, 60, 50, 50));
    let r = d.temp_overlay.unwrap();
    assert!(r.width > 50);
}
