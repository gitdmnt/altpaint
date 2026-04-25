use render_types::{PanelSurfaceSource, PixelRect};

use crate::{RenderFrame, blit_scaled_rgba_region, compose_panel_host_region};

/// 合成 パネル ホスト 領域 respects global パネル サーフェス 範囲 が期待どおりに動作することを検証する。
#[test]
fn compose_panel_host_region_respects_global_panel_surface_bounds() {
    let mut frame = RenderFrame {
        width: 640,
        height: 480,
        pixels: vec![0; 640 * 480 * 4],
    };
    let panel_surface = PanelSurfaceSource {
        x: 120,
        y: 80,
        width: 8,
        height: 6,
        pixels: &[0xaa; 8 * 6 * 4],
    };

    compose_panel_host_region(&mut frame, panel_surface, None);

    let inside = ((80 * frame.width) + 120) * 4;
    let outside = ((40 * frame.width) + 40) * 4;
    assert_eq!(&frame.pixels[inside..inside + 4], &[0xaa, 0xaa, 0xaa, 0xaa]);
    assert_eq!(&frame.pixels[outside..outside + 4], &[0, 0, 0, 0]);
}

/// blit scaled RGBA 領域 updates only 差分 area が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
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
        PixelRect {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        },
        4,
        4,
        source.as_slice(),
        Some(PixelRect {
            x: 3,
            y: 3,
            width: 1,
            height: 1,
        }),
    );

    let dirty_index = (3 * frame.width + 3) * 4;
    let untouched_index = (2 * frame.width + 2) * 4;
    assert_eq!(
        &frame.pixels[dirty_index..dirty_index + 4],
        &[255, 255, 255, 255]
    );
    assert_eq!(
        &frame.pixels[untouched_index..untouched_index + 4],
        &[0, 0, 0, 0]
    );
}
