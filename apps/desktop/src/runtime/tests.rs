//! `runtime` モジュールの入力ルーティング回帰テストをまとめる。

use std::path::PathBuf;

use desktop_support::DesktopProfiler;
use winit::event::MouseScrollDelta;
use winit::event::TouchPhase;

use super::DesktopRuntime;

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

/// raw mouse delta でも描画を継続できることを確認する。
#[test]
fn raw_mouse_motion_draws_between_cursor_events() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    runtime.last_cursor_position = Some((center_x, center_y));
    runtime.last_cursor_position_f64 = Some((center_x as f64, center_y as f64));
    assert!(runtime.handle_mouse_button(winit::event::ElementState::Pressed));
    assert!(runtime.handle_raw_mouse_motion(40.0, 0.0));
    assert!(!runtime.handle_mouse_button(winit::event::ElementState::Released));

    let frame = runtime.app.ui_shell.render_frame(&runtime.app.document);
    assert!(frame.pixels.chunks_exact(4).any(|pixel| pixel == [0, 0, 0, 255]));
}

#[test]
fn pixel_wheel_pan_accepts_sub_line_delta() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;
    runtime.last_cursor_position = Some((center_x, center_y));

    let before = runtime.app.document.view_transform.pan_y;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 1.0),
    )));
    assert_ne!(runtime.app.document.view_transform.pan_y, before);
}

#[test]
fn wheel_pan_animation_continues_after_initial_event() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;
    runtime.last_cursor_position = Some((center_x, center_y));

    let before = runtime.app.document.view_transform.pan_y;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 16.0),
    )));
    let after_first = runtime.app.document.view_transform.pan_y;
    assert_ne!(after_first, before);
    assert!(runtime.has_pending_wheel_animation());

    assert!(runtime.advance_wheel_animation());
    assert_ne!(runtime.app.document.view_transform.pan_y, after_first);
}
