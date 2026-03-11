//! `frame` モジュールの合成・座標変換テストをまとめる。

use app_core::{CanvasDirtyRect, CanvasPoint, CanvasViewTransform, PanelSurfacePoint, WindowPoint};
use render::RenderFrame;
use ui_shell::UiShell;

use super::*;

/// ビュー右下がサーフェス右下へ写像されることを確認する。
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

/// サーフェス外座標が境界へクランプされることを確認する。
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
        CanvasViewTransform::default(),
        CanvasOverlayState::default(),
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
        CanvasDirtyRect {
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

#[test]
fn transformed_canvas_dirty_rect_tracks_zoom_and_pan() {
    let mapped = map_canvas_dirty_to_display_with_transform(
        CanvasDirtyRect {
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
        CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 0.0,
            pan_x: 16.0,
            pan_y: -8.0,
            flip_x: false,
            flip_y: false,
        },
    );

    assert!(mapped.width >= 80);
    assert_eq!(mapped.height, 80);
    assert!(mapped.x >= 100);
    assert_eq!(mapped.y, 50);
}

#[test]
fn brush_preview_rect_expands_with_larger_brush_size() {
    let small = brush_preview_rect(
        Rect {
            x: 100,
            y: 50,
            width: 320,
            height: 320,
        },
        64,
        64,
        CanvasViewTransform::default(),
        CanvasPoint::new(32, 32),
        4,
    )
    .expect("small preview exists");
    let large = brush_preview_rect(
        Rect {
            x: 100,
            y: 50,
            width: 320,
            height: 320,
        },
        64,
        64,
        CanvasViewTransform::default(),
        CanvasPoint::new(32, 32),
        24,
    )
    .expect("large preview exists");

    assert!(large.width > small.width);
    assert!(large.height > small.height);
}

#[test]
fn overlay_frame_draws_panel_navigator_when_multiple_panels_exist() {
    let layout = DesktopLayout::new(640, 480, 64, 64);
    let mut shell = UiShell::new();
    let panel_surface = shell.render_panel_surface(640, 480);
    let overlay = compose_overlay_frame(
        640,
        480,
        &layout,
        &panel_surface,
        CanvasCompositeSource {
            width: 64,
            height: 64,
            pixels: &[0; 64 * 64 * 4],
        },
        CanvasViewTransform::default(),
        CanvasOverlayState {
            brush_preview: None,
            brush_size: None,
            lasso_points: Vec::new(),
            active_panel_bounds: None,
            panel_navigator: Some(PanelNavigatorOverlay {
                page_width: 320,
                page_height: 240,
                panels: vec![
                    PanelNavigatorEntry {
                        bounds: app_core::PanelBounds {
                            x: 12,
                            y: 12,
                            width: 143,
                            height: 216,
                        },
                        active: false,
                    },
                    PanelNavigatorEntry {
                        bounds: app_core::PanelBounds {
                            x: 165,
                            y: 12,
                            width: 143,
                            height: 216,
                        },
                        active: true,
                    },
                ],
            }),
            panel_creation_preview: None,
        },
    );

    assert!(overlay.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn compose_panel_host_region_respects_global_panel_surface_bounds() {
    let layout = DesktopLayout::new(640, 480, 64, 64);
    let mut frame = RenderFrame {
        width: 640,
        height: 480,
        pixels: vec![0; 640 * 480 * 4],
    };
    let panel_surface = ui_shell::PanelSurface::from_pixels(120, 80, 8, 6, vec![0xaa; 8 * 6 * 4]);

    compose_panel_host_region(&mut frame, &layout, &panel_surface, None);

    let inside = ((80 * frame.width) + 120) * 4;
    let outside = ((40 * frame.width) + 40) * 4;
    assert_eq!(&frame.pixels[inside..inside + 4], &[0xaa, 0xaa, 0xaa, 0xaa]);
    assert_eq!(&frame.pixels[outside..outside + 4], &[0, 0, 0, 0]);
}

#[test]
fn canvas_texture_quad_clips_uv_when_panned_outside_display() {
    let quad = canvas_texture_quad(
        Rect {
            x: 100,
            y: 80,
            width: 320,
            height: 320,
        },
        64,
        64,
        CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 0.0,
            pan_x: 48.0,
            pan_y: -16.0,
            flip_x: false,
            flip_y: false,
        },
    )
    .expect("quad exists");

    assert_eq!(quad.destination.width, 320);
    assert!(quad.uv_min[0] > 0.0);
    assert!(quad.uv_max[0] <= 1.0);
    assert!(quad.uv_min[1] >= 0.0);
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

#[test]
fn source_axis_runs_merge_adjacent_pixels_with_same_source_x() {
    let runs = build_source_axis_runs(100, 8, 100.0, 2.0, 64);

    assert_eq!(
        runs,
        vec![
            SourceAxisRun {
                dst_offset: 0,
                len: 2,
                src_index: 0,
            },
            SourceAxisRun {
                dst_offset: 2,
                len: 2,
                src_index: 1,
            },
            SourceAxisRun {
                dst_offset: 4,
                len: 2,
                src_index: 2,
            },
            SourceAxisRun {
                dst_offset: 6,
                len: 2,
                src_index: 3,
            },
        ]
    );
}

#[test]
fn fill_rgba_block_writes_rectangular_region() {
    let mut frame = RenderFrame {
        width: 6,
        height: 4,
        pixels: vec![0; 6 * 4 * 4],
    };

    fill_rgba_block(&mut frame, 2, 1, 3, 2, [9, 8, 7, 6]);

    for y in 1..3 {
        for x in 2..5 {
            let index = (y * frame.width + x) * 4;
            assert_eq!(&frame.pixels[index..index + 4], &[9, 8, 7, 6]);
        }
    }
    let untouched = 0;
    assert_eq!(&frame.pixels[untouched..untouched + 4], &[0, 0, 0, 0]);
}

#[test]
fn scroll_canvas_region_moves_existing_pixels_and_reports_exposed_strip() {
    let mut frame = RenderFrame {
        width: 5,
        height: 4,
        pixels: vec![0; 5 * 4 * 4],
    };
    for y in 0..4 {
        for x in 0..5 {
            let index = (y * frame.width + x) * 4;
            frame.pixels[index..index + 4].copy_from_slice(&[x as u8, y as u8, 0, 255]);
        }
    }

    let exposed = scroll_canvas_region(
        &mut frame,
        Rect {
            x: 0,
            y: 0,
            width: 5,
            height: 4,
        },
        0,
        -1,
    );

    assert_eq!(
        exposed,
        Rect {
            x: 0,
            y: 3,
            width: 5,
            height: 1,
        }
    );
    let moved_index = 2 * 4;
    assert_eq!(&frame.pixels[moved_index..moved_index + 4], &[2, 1, 0, 255]);
}
