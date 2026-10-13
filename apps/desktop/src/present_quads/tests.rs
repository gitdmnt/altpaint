//! `frame` モジュールの desktop 固有レイアウトテストをまとめる。

use ::geometry::WindowPoint;

use super::*;

#[test]
fn desktop_layout_letterboxes_canvas_inside_host_rect() {
    let layout = DesktopLayout::new(1280, 800, 64, 64);

    assert!(layout.canvas_display_rect.width <= layout.canvas_host_rect.width);
    assert!(layout.canvas_display_rect.height <= layout.canvas_host_rect.height);
    assert!(layout.canvas_host_rect.contains(WindowPoint::new(
        layout.canvas_display_rect.x as i32,
        layout.canvas_display_rect.y as i32,
    )));
}
