//! `frame` モジュールの合成・座標変換テストをまとめる。

use app_core::DirtyRect;
use render::RenderFrame;
use ui_shell::UiShell;

use super::*;

/// ビュー右下がサーフェス右下へ写像されることを確認する。
#[test]
fn map_view_to_surface_maps_bottom_right_corner() {
    let mapped = map_view_to_surface(
        264,
        800,
        Rect {
            x: 8,
            y: 40,
            width: 264,
            height: 800,
        },
        271,
        839,
    );

    assert_eq!(mapped, Some((263, 799)));
}

/// サーフェス外座標が境界へクランプされることを確認する。
#[test]
fn map_view_to_surface_clamped_limits_outside_coordinates() {
    let mapped = map_view_to_surface_clamped(
        264,
        800,
        Rect {
            x: 8,
            y: 40,
            width: 264,
            height: 800,
        },
        500,
        -10,
    );

    assert_eq!(mapped, Some((263, 0)));
}

/// レイアウト計算がキャンバスをホスト矩形へ収めることを確認する。
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

/// パネル表示面はホスト領域全体を占めることを確認する。
#[test]
fn panel_surface_fills_panel_host_rect() {
    let layout = DesktopLayout::new(1280, 800, 64, 64);

    assert_eq!(layout.panel_surface_rect, layout.panel_host_rect);
}

/// 全面合成がパネルとキャンバスの双方を書き込むことを確認する。
#[test]
fn compose_desktop_frame_writes_panel_and_canvas_regions() {
    let layout = DesktopLayout::new(640, 480, 64, 64);
    let mut shell = UiShell::new();
    let panel_surface = shell.render_panel_surface(264, 800);
    let frame = compose_desktop_frame(
        640,
        480,
        &layout,
        &panel_surface,
        CanvasCompositeSource {
            width: 2,
            height: 2,
            pixels: &[16; 16],
        },
        "status",
    );

    assert_eq!(frame.width, 640);
    assert_eq!(frame.height, 480);
    assert!(frame.pixels.chunks_exact(4).any(|pixel| pixel == [16, 16, 16, 16]));
}

/// キャンバス dirty rect が表示矩形へ正しく拡大写像されることを確認する。
#[test]
fn canvas_dirty_rect_maps_into_display_rect() {
    let mapped = map_canvas_dirty_to_display(
        DirtyRect {
            x: 16,
            y: 16,
            width: 8,
            height: 8,
        },
        Rect {
            x: 100,
            y: 50,
            width: 320,
            height: 320,
        },
        64,
        64,
    );

    assert_eq!(mapped.x, 180);
    assert_eq!(mapped.y, 130);
    assert_eq!(mapped.width, 40);
    assert_eq!(mapped.height, 40);
}

/// dirty rect 転送が指定領域だけを書き換えることを確認する。
#[test]
fn blit_scaled_rgba_region_updates_only_dirty_area() {
    let mut frame = RenderFrame {
        width: 8,
        height: 8,
        pixels: vec![0; 8 * 8 * 4],
    };
    let source = vec![255; 4 * 4 * 4];

    blit_scaled_rgba_region(
        &mut frame,
        Rect {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        },
        4,
        4,
        source.as_slice(),
        Some(Rect {
            x: 3,
            y: 3,
            width: 1,
            height: 1,
        }),
    );

    let dirty_index = (3 * frame.width + 3) * 4;
    let untouched_index = (2 * frame.width + 2) * 4;
    assert_eq!(&frame.pixels[dirty_index..dirty_index + 4], &[255, 255, 255, 255]);
    assert_eq!(&frame.pixels[untouched_index..untouched_index + 4], &[0, 0, 0, 0]);
}
