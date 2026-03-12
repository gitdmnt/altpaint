use app_core::{CanvasPoint, CanvasViewTransform};

use crate::{
    PixelRect, RenderFrame, brush_preview_dirty_rect, build_source_axis_runs,
    exposed_canvas_background_rect, fill_rgba_block, scroll_canvas_region,
};

#[test]
fn brush_preview_dirty_rect_unions_previous_and_current_preview() {
    let viewport = PixelRect {
        x: 0,
        y: 0,
        width: 400,
        height: 300,
    };
    let previous = crate::prepare_canvas_scene(viewport, 64, 64, CanvasViewTransform::default());
    let current = crate::prepare_canvas_scene(
        viewport,
        64,
        64,
        CanvasViewTransform {
            pan_x: 20.0,
            ..CanvasViewTransform::default()
        },
    );

    let dirty = brush_preview_dirty_rect(previous, current, CanvasPoint::new(20, 20), 12.0)
        .expect("dirty rect exists");

    assert!(dirty.width > 0);
    assert!(dirty.height > 0);
}

#[test]
fn exposed_canvas_background_rect_reports_pan_exposure() {
    let dirty = exposed_canvas_background_rect(
        PixelRect {
            x: 0,
            y: 0,
            width: 320,
            height: 240,
        },
        64,
        64,
        CanvasViewTransform::default(),
        CanvasViewTransform {
            pan_x: 24.0,
            ..CanvasViewTransform::default()
        },
    )
    .expect("dirty rect exists");

    assert!(dirty.width > 0 || dirty.height > 0);
}

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
