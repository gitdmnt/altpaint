//! `frame` モジュールの desktop 固有レイアウトと座標変換テストをまとめる。

use app_core::{PanelSurfacePoint, WindowPoint};

use super::*;

/// map ビュー to サーフェス maps bottom right corner が期待どおりに動作することを検証する。
#[test]
fn map_view_to_surface_maps_bottom_right_corner() {
    let mapped = map_window_to_panel_surface(
        264,
        800,
        Rect {
            x: 8,
            y: 40,
            width: 264,
            height: 800,
        },
        WindowPoint::new(271, 839),
    );

    assert_eq!(mapped, Some(PanelSurfacePoint::new(263, 799)));
}

/// map ビュー to サーフェス clamped limits outside coordinates が期待どおりに動作することを検証する。
#[test]
fn map_view_to_surface_clamped_limits_outside_coordinates() {
    let mapped = map_window_to_panel_surface_clamped(
        264,
        800,
        Rect {
            x: 8,
            y: 40,
            width: 264,
            height: 800,
        },
        WindowPoint::new(500, -10),
    );

    assert_eq!(mapped, Some(PanelSurfacePoint::new(263, 0)));
}

/// desktop レイアウト letterboxes キャンバス inside ホスト 矩形 が期待どおりに動作することを検証する。
#[test]
fn desktop_layout_letterboxes_canvas_inside_host_rect() {
    let layout = DesktopLayout::new(1280, 800, 64, 64);

    assert!(layout.canvas_display_rect.width <= layout.canvas_host_rect.width);
    assert!(layout.canvas_display_rect.height <= layout.canvas_host_rect.height);
    assert!(layout.canvas_host_rect.contains(
        layout.canvas_display_rect.x as i32,
        layout.canvas_display_rect.y as i32,
    ));
}

/// パネル サーフェス fills パネル ホスト 矩形 が期待どおりに動作することを検証する。
#[test]
fn panel_surface_fills_panel_host_rect() {
    let layout = DesktopLayout::new(1280, 800, 64, 64);

    assert_eq!(layout.panel_surface_rect, layout.panel_host_rect);
}
