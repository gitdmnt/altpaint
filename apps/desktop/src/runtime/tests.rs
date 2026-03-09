//! `runtime` モジュールの入力ルーティング回帰テストをまとめる。

use std::path::PathBuf;

use desktop_support::DesktopProfiler;
use winit::event::MouseScrollDelta;
use winit::event::TouchPhase;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::DesktopRuntime;
use super::keyboard::{normalized_key_name, supports_editing_repeat};

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

    let frame = render::RenderContext::new().render_frame(&runtime.app.document);
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

    let frame = render::RenderContext::new().render_frame(&runtime.app.document);
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

#[test]
fn shift_wheel_converts_vertical_scroll_into_horizontal_pan() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;
    runtime.last_cursor_position = Some((center_x, center_y));
    runtime.modifiers = ModifiersState::SHIFT;

    let before = runtime.app.document.view_transform.pan_x;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 2.0)));
    assert_ne!(runtime.app.document.view_transform.pan_x, before);
}

#[test]
fn control_wheel_changes_zoom() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = runtime.app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;
    runtime.last_cursor_position = Some((center_x, center_y));
    runtime.modifiers = ModifiersState::CONTROL;

    let before = runtime.app.document.view_transform.zoom;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0)));
    assert_ne!(runtime.app.document.view_transform.zoom, before);
}

#[test]
fn mouse_button_without_cursor_position_is_ignored() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

    assert!(!runtime.handle_mouse_button(winit::event::ElementState::Pressed));
}

#[test]
fn normalized_key_name_uppercases_character_keys() {
    assert_eq!(normalized_key_name(&Key::Character(" a ".into())), Some("A".to_string()));
    assert_eq!(normalized_key_name(&Key::Named(NamedKey::Enter)), Some("Enter".to_string()));
    assert_eq!(normalized_key_name(&Key::Named(NamedKey::Shift)), None);
}

#[test]
fn editing_repeat_support_matches_text_navigation_keys() {
    assert!(supports_editing_repeat(&Key::Character("x".into())));
    assert!(supports_editing_repeat(&Key::Named(NamedKey::ArrowLeft)));
    assert!(!supports_editing_repeat(&Key::Named(NamedKey::Tab)));
}

#[test]
fn normalized_shortcut_includes_active_modifiers() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    runtime.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;

    assert_eq!(
        runtime.normalized_shortcut(&Key::Character("s".into())),
        Some(("Ctrl+Shift+S".to_string(), "S".to_string()))
    );
}

#[test]
fn builtin_shortcut_dispatches_save_project() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    runtime.app.project_path = std::env::temp_dir().join(format!(
        "altpaint-runtime-save-shortcut-{}.altp.json",
        std::process::id()
    ));
    runtime.modifiers = ModifiersState::CONTROL;

    assert!(runtime.handle_builtin_shortcut(&Key::Character("s".into())));
    assert_eq!(runtime.app.pending_save_task_count(), 1);
    runtime.app.wait_for_pending_save_tasks();
}

#[test]
fn builtin_shortcut_can_move_focus_backward() {
    let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 200, &mut profiler);
    runtime.modifiers = ModifiersState::SHIFT;

    assert!(runtime.handle_builtin_shortcut(&Key::Named(NamedKey::Tab)));
}
