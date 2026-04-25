use app_core::CanvasViewTransform;
use render_types::{
    CanvasCompositeSource, CanvasOverlayState, FramePlan, PanelSurfaceSource, PixelRect,
};

use crate::{
    RenderFrame, blit_scaled_rgba_region, build_source_axis_runs, compose_canvas_host_region,
    fill_rgba_block, scroll_canvas_region,
};

/// ソース axis runs merge adjacent pixels with same ソース x が期待どおりに動作することを検証する。
#[test]
fn source_axis_runs_merge_adjacent_pixels_with_same_source_x() {
    let runs = build_source_axis_runs(100, 8, 100.0, 2.0, 64);

    assert_eq!(
        runs,
        vec![
            crate::SourceAxisRun {
                dst_offset: 0,
                len: 2,
                src_index: 0,
            },
            crate::SourceAxisRun {
                dst_offset: 2,
                len: 2,
                src_index: 1,
            },
            crate::SourceAxisRun {
                dst_offset: 4,
                len: 2,
                src_index: 2,
            },
            crate::SourceAxisRun {
                dst_offset: 6,
                len: 2,
                src_index: 3,
            },
        ]
    );
}

/// 塗りつぶし RGBA block writes rectangular 領域 が期待どおりに動作することを検証する。
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
    assert_eq!(&frame.pixels[0..4], &[0, 0, 0, 0]);
}

/// スクロール キャンバス 領域 moves existing pixels and reports exposed strip が期待どおりに動作することを検証する。
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
        PixelRect {
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
        PixelRect {
            x: 0,
            y: 3,
            width: 5,
            height: 1,
        }
    );
    let moved_index = 2 * 4;
    assert_eq!(&frame.pixels[moved_index..moved_index + 4], &[2, 1, 0, 255]);
}

/// blit_canvas_with_transform がズームアウト時に bilinear 補間を使うことを検証する。
///
/// 8x8 の白黒チェッカーパターンを 4x4 の画面に scale=0.5 で描画した場合、
/// ニアレストネイバーだと純白か純黒になる。bilinear 補間だとグレー（～128）になる。
#[test]
fn blit_canvas_with_transform_bilinear_at_zoom_out() {
    let canvas_size = 8usize;
    let viewport_size = 4usize;

    // 白黒チェッカーパターン: (x+y) % 2 == 0 → 白、それ以外 → 黒
    let mut canvas_pixels = vec![0u8; canvas_size * canvas_size * 4];
    for y in 0..canvas_size {
        for x in 0..canvas_size {
            let idx = (y * canvas_size + x) * 4;
            if (x + y) % 2 == 0 {
                canvas_pixels[idx..idx + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else {
                canvas_pixels[idx..idx + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }

    let panel_pixels = vec![0u8; viewport_size * viewport_size * 4];
    let host_rect = PixelRect {
        x: 0,
        y: 0,
        width: viewport_size,
        height: viewport_size,
    };
    let plan = FramePlan::new(
        viewport_size,
        viewport_size,
        host_rect,
        PanelSurfaceSource {
            x: 0,
            y: 0,
            width: viewport_size,
            height: viewport_size,
            pixels: &panel_pixels,
        },
        CanvasCompositeSource {
            width: canvas_size,
            height: canvas_size,
            pixels: &canvas_pixels,
        },
        CanvasViewTransform::default(), // zoom=1.0 → fit_scale=0.5 → scale=0.5
        "",
    );

    let mut frame = RenderFrame {
        width: viewport_size,
        height: viewport_size,
        pixels: vec![0; viewport_size * viewport_size * 4],
    };
    let overlay = CanvasOverlayState::default();
    compose_canvas_host_region(&mut frame, &plan, &overlay, None);

    // bilinear 補間後はグレー（30〜225 の中間値）になるはず
    // ニアレストネイバーだと pure white (255) になる（現状の挙動）
    let r = frame.pixels[0];
    assert!(
        r > 30 && r < 225,
        "bilinear補間でグレーになるはずが R={r} になった（純白/純黒はニアレストネイバーの証拠）"
    );
}

/// blit_scaled_rgba_region がパネルサーフェス縮小時に bilinear 補間を使うことを検証する。
///
/// 4x4 の白黒チェッカーパターンを 2x2 に縮小した場合、
/// ニアレストネイバーだと純白か純黒になる。bilinear 補間だとグレーになる。
#[test]
fn blit_scaled_rgba_region_bilinear_at_scale_down() {
    // 4x4 チェッカーパターン
    let src_size = 4usize;
    let dst_size = 2usize;
    let mut source = vec![0u8; src_size * src_size * 4];
    for y in 0..src_size {
        for x in 0..src_size {
            let idx = (y * src_size + x) * 4;
            if (x + y) % 2 == 0 {
                source[idx..idx + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else {
                source[idx..idx + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }

    let mut frame = RenderFrame {
        width: dst_size,
        height: dst_size,
        pixels: vec![0; dst_size * dst_size * 4],
    };
    let destination = PixelRect {
        x: 0,
        y: 0,
        width: dst_size,
        height: dst_size,
    };
    blit_scaled_rgba_region(&mut frame, destination, src_size, src_size, &source, None);

    // bilinear 補間後はグレー（30〜225）になるはず
    let r = frame.pixels[0];
    assert!(
        r > 30 && r < 225,
        "bilinear補間でグレーになるはずが R={r} になった（純白/純黒はニアレストネイバーの証拠）"
    );
}
