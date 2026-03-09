//! `runtime` モジュールの入力ルーティング回帰テストをまとめる。

use std::path::PathBuf;

use winit::event::TouchPhase;

use super::DesktopRuntime;
use crate::profiler::DesktopProfiler;

/// タッチ開始と移動でキャンバス描画が行われることを確認する。
#[test]
fn touch_started_and_moved_draws_black_pixels() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    assert!(runtime.handle_touch_phase(1, TouchPhase::Started, center_x, center_y));
    assert!(runtime.handle_touch_phase(1, TouchPhase::Moved, center_x + 20, center_y));
    assert!(!runtime.handle_touch_phase(1, TouchPhase::Ended, center_x + 20, center_y));

    let frame = runtime.app.ui_shell.render_frame(&runtime.app.document);
    assert!(frame.pixels.chunks_exact(4).any(|pixel| pixel == [0, 0, 0, 255]));
}

/// タッチキャンセルで追跡中のタッチ ID が解除されることを確認する。
#[test]
fn touch_cancelled_stops_active_touch_tracking() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    assert!(runtime.handle_touch_phase(7, TouchPhase::Started, center_x, center_y));
    assert_eq!(runtime.active_touch_id, Some(7));

    assert!(!runtime.handle_touch_phase(7, TouchPhase::Cancelled, center_x, center_y));
    assert_eq!(runtime.active_touch_id, None);
}
