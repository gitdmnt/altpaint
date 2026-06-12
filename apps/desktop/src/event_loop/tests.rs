//! `event_loop` モジュールの入力ルーティング回帰テストをまとめる。

use desktop_support::FrameProfiler;
use winit::event::MouseScrollDelta;
use winit::event::TouchPhase;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::app::DesktopApp;
use crate::app::cpu_canvas_snapshot::build_cpu_canvas_snapshot;

use super::DesktopEventLoop;
use super::keyboard::normalized_key_name;

fn test_event_loop() -> DesktopEventLoop {
    DesktopEventLoop {
        app: DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
            crate::app::tests::unique_test_project_path(),
            Box::new(crate::app::tests::TestDialogs::default()),
            crate::app::tests::unique_test_path("event-loop-session"),
            crate::app::tests::unique_test_path("event-loop-workspace-presets"),
        ),
        window: None,
        presenter: None,
        last_cursor_position: None,
        last_cursor_position_f64: None,
        last_touch_pressure: 1.0,
        pending_wheel_pan: (0.0, 0.0),
        pending_wheel_zoom_lines: 0.0,
        active_touch_id: None,
        profiler: FrameProfiler::new(),
        modifiers: ModifiersState::default(),
    }
}

fn canvas_input_point(
    event_loop: &DesktopEventLoop,
    min_right_space: i32,
    min_bottom_space: i32,
) -> (i32, i32) {
    let layout = event_loop.app.layout.clone().expect("layout exists");
    let start_x = layout.canvas_display_rect.x as i32 + 16;
    let end_x = ((layout.canvas_display_rect.x + layout.canvas_display_rect.width) as i32)
        .saturating_sub(min_right_space.max(16));
    let start_y = layout.canvas_display_rect.y as i32 + 16;
    let end_y = ((layout.canvas_display_rect.y + layout.canvas_display_rect.height) as i32)
        .saturating_sub(min_bottom_space.max(16));

    for y in start_y..end_y {
        for x in start_x..end_x {
            if !event_loop.app.panel_is_hovered(geometry::WindowPoint::new(x, y)) {
                return (x, y);
            }
        }
    }

    panic!("expected an uncovered canvas point");
}

#[test]
fn touch_started_and_moved_draws_black_pixels() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 32, 32);

    assert!(event_loop.handle_touch_phase(1, TouchPhase::Started, center_x, center_y, None));
    assert!(event_loop.handle_touch_phase(1, TouchPhase::Moved, center_x + 20, center_y, None));
    let _ = event_loop.handle_touch_phase(1, TouchPhase::Ended, center_x + 20, center_y, None);

    let frame = build_cpu_canvas_snapshot(&event_loop.app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn touch_cancelled_stops_active_touch_tracking() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 16, 16);

    assert!(event_loop.handle_touch_phase(7, TouchPhase::Started, center_x, center_y, None));
    assert_eq!(event_loop.active_touch_id, Some(7));

    assert!(!event_loop.handle_touch_phase(7, TouchPhase::Cancelled, center_x, center_y, None));
    assert_eq!(event_loop.active_touch_id, None);
}

#[test]
fn raw_mouse_motion_draws_between_cursor_events() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 80, 16);

    event_loop.last_cursor_position = Some((center_x, center_y));
    event_loop.last_cursor_position_f64 = Some((center_x as f64, center_y as f64));
    assert!(event_loop.handle_mouse_button(winit::event::ElementState::Pressed));
    assert!(event_loop.handle_raw_mouse_motion(40.0, 0.0));
    let _ = event_loop.handle_mouse_button(winit::event::ElementState::Released);

    let frame = build_cpu_canvas_snapshot(&event_loop.app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

#[test]
fn pixel_wheel_pan_accepts_sub_line_delta() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 16, 16);
    event_loop.last_cursor_position = Some((center_x, center_y));

    let before = event_loop.app.document.view_transform.pan_y;
    assert!(event_loop.handle_mouse_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 1.0),
    )));
    assert!(event_loop.app.document.view_transform.pan_y > before);
}

#[test]
fn wheel_pan_animation_continues_after_initial_event() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 16, 16);
    event_loop.last_cursor_position = Some((center_x, center_y));

    let before = event_loop.app.document.view_transform.pan_y;
    assert!(event_loop.handle_mouse_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 16.0),
    )));
    let after_first = event_loop.app.document.view_transform.pan_y;
    assert!(after_first > before);
    assert!(event_loop.has_pending_wheel_animation());

    assert!(event_loop.advance_wheel_animation());
    assert_ne!(event_loop.app.document.view_transform.pan_y, after_first);
}

#[test]
fn shift_wheel_converts_vertical_scroll_into_horizontal_pan() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 16, 16);
    event_loop.last_cursor_position = Some((center_x, center_y));
    event_loop.modifiers = ModifiersState::SHIFT;

    let before = event_loop.app.document.view_transform.pan_x;
    assert!(event_loop.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 2.0)));
    assert!(event_loop.app.document.view_transform.pan_x > before);
}

#[test]
fn control_wheel_changes_zoom() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&event_loop, 16, 16);
    event_loop.last_cursor_position = Some((center_x, center_y));
    event_loop.modifiers = ModifiersState::CONTROL;

    let before = event_loop.app.document.view_transform.zoom;
    assert!(event_loop.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0)));
    assert!(event_loop.app.document.view_transform.zoom > before);
}

#[test]
fn mouse_button_without_cursor_position_is_ignored() {
    let mut event_loop = test_event_loop();

    assert!(!event_loop.handle_mouse_button(winit::event::ElementState::Pressed));
}

#[test]
fn normalized_key_name_uppercases_character_keys() {
    assert_eq!(
        normalized_key_name(&Key::Character(" a ".into())),
        Some("A".to_string())
    );
    assert_eq!(
        normalized_key_name(&Key::Named(NamedKey::Enter)),
        Some("Enter".to_string())
    );
    assert_eq!(normalized_key_name(&Key::Named(NamedKey::Shift)), None);
}

#[test]
fn normalized_shortcut_includes_active_modifiers() {
    let mut event_loop = test_event_loop();
    event_loop.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;

    assert_eq!(
        event_loop.normalized_shortcut(&Key::Character("s".into())),
        Some(("Ctrl+Shift+S".to_string(), "S".to_string()))
    );
}

#[test]
fn builtin_shortcut_dispatches_save_project() {
    let mut event_loop = test_event_loop();
    event_loop.app.io_state.project_path = std::env::temp_dir().join(format!(
        "altpaint-event-loop-save-shortcut-{}.altp.json",
        std::process::id()
    ));
    event_loop.modifiers = ModifiersState::CONTROL;

    assert!(event_loop.handle_builtin_shortcut(&Key::Character("s".into())));
    assert_eq!(event_loop.app.pending_save_task_count(), 1);
    event_loop.app.wait_for_pending_save_tasks();
}

#[test]
fn builtin_shortcut_can_move_focus_backward() {
    let mut event_loop = test_event_loop();
    let mut profiler = FrameProfiler::new();
    let _ = event_loop.app.prepare_present_frame(1280, 200, &mut profiler);
    // ADR 014 以降、focus は HTML hit table を辿るため事前に hit を 1 件 inject する。
    event_loop.app.panel_workspace.update_panel_hits(
        "builtin.app-actions",
        geometry::WindowRect {
            x: 100,
            y: 50,
            width: 200,
            height: 32,
        },
        vec![(
            "app.save".to_string(),
            geometry::WindowRect {
                x: 8,
                y: 4,
                width: 80,
                height: 24,
            },
        )],
    );
    event_loop.modifiers = ModifiersState::SHIFT;

    assert!(event_loop.handle_builtin_shortcut(&Key::Named(NamedKey::Tab)));
}
