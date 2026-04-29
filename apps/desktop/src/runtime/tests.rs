//! `runtime` モジュールの入力ルーティング回帰テストをまとめる。

use std::path::PathBuf;

use desktop_support::DesktopProfiler;
use winit::event::MouseScrollDelta;
use winit::event::TouchPhase;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::app::DesktopApp;
use crate::app::canvas_frame::build_canvas_frame;

use super::DesktopRuntime;
use super::keyboard::{normalized_key_name, supports_editing_repeat};

/// test runtime を計算して返す。
fn test_runtime() -> DesktopRuntime {
    DesktopRuntime {
        app: DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
            PathBuf::from("/tmp/altpaint-test.altp.json"),
            Box::new(crate::app::tests::TestDialogs::default()),
            crate::app::tests::unique_test_path("runtime-session"),
            crate::app::tests::unique_test_path("runtime-workspace-presets"),
        ),
        window: None,
        presenter: None,
        last_cursor_position: None,
        last_cursor_position_f64: None,
        last_touch_pressure: 1.0,
        pending_wheel_pan: (0.0, 0.0),
        pending_wheel_zoom_lines: 0.0,
        active_touch_id: None,
        profiler: DesktopProfiler::new(),
        modifiers: ModifiersState::default(),
    }
}

/// キャンバス 入力 点 に必要な処理を行う。
fn canvas_input_point(
    runtime: &DesktopRuntime,
    min_right_space: i32,
    min_bottom_space: i32,
) -> (i32, i32) {
    let layout = runtime.app.layout.clone().expect("layout exists");
    let start_x = layout.canvas_display_rect.x as i32 + 16;
    let end_x = ((layout.canvas_display_rect.x + layout.canvas_display_rect.width) as i32)
        .saturating_sub(min_right_space.max(16));
    let start_y = layout.canvas_display_rect.y as i32 + 16;
    let end_y = ((layout.canvas_display_rect.y + layout.canvas_display_rect.height) as i32)
        .saturating_sub(min_bottom_space.max(16));

    for y in start_y..end_y {
        for x in start_x..end_x {
            if !runtime.app.panel_is_hovered(x, y) {
                return (x, y);
            }
        }
    }

    panic!("expected an uncovered canvas point");
}

/// touch started and moved draws black pixels が期待どおりに動作することを検証する。
#[test]
fn touch_started_and_moved_draws_black_pixels() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 32, 32);

    assert!(runtime.handle_touch_phase(1, TouchPhase::Started, center_x, center_y, None));
    assert!(runtime.handle_touch_phase(1, TouchPhase::Moved, center_x + 20, center_y, None));
    let _ = runtime.handle_touch_phase(1, TouchPhase::Ended, center_x + 20, center_y, None);

    let frame = build_canvas_frame(&runtime.app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

/// touch cancelled stops アクティブ touch tracking が期待どおりに動作することを検証する。
#[test]
fn touch_cancelled_stops_active_touch_tracking() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 16, 16);

    assert!(runtime.handle_touch_phase(7, TouchPhase::Started, center_x, center_y, None));
    assert_eq!(runtime.active_touch_id, Some(7));

    assert!(!runtime.handle_touch_phase(7, TouchPhase::Cancelled, center_x, center_y, None));
    assert_eq!(runtime.active_touch_id, None);
}

/// raw mouse motion draws between cursor events が期待どおりに動作することを検証する。
#[test]
fn raw_mouse_motion_draws_between_cursor_events() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 80, 16);

    runtime.last_cursor_position = Some((center_x, center_y));
    runtime.last_cursor_position_f64 = Some((center_x as f64, center_y as f64));
    assert!(runtime.handle_mouse_button(winit::event::ElementState::Pressed));
    assert!(runtime.handle_raw_mouse_motion(40.0, 0.0));
    let _ = runtime.handle_mouse_button(winit::event::ElementState::Released);

    let frame = build_canvas_frame(&runtime.app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

/// ピクセル ホイール pan accepts sub line delta が期待どおりに動作することを検証する。
#[test]
fn pixel_wheel_pan_accepts_sub_line_delta() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 16, 16);
    runtime.last_cursor_position = Some((center_x, center_y));

    let before = runtime.app.document.view_transform.pan_y;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 1.0),
    )));
    assert!(runtime.app.document.view_transform.pan_y > before);
}

/// ホイール pan animation continues after initial イベント が期待どおりに動作することを検証する。
#[test]
fn wheel_pan_animation_continues_after_initial_event() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 16, 16);
    runtime.last_cursor_position = Some((center_x, center_y));

    let before = runtime.app.document.view_transform.pan_y;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 16.0),
    )));
    let after_first = runtime.app.document.view_transform.pan_y;
    assert!(after_first > before);
    assert!(runtime.has_pending_wheel_animation());

    assert!(runtime.advance_wheel_animation());
    assert_ne!(runtime.app.document.view_transform.pan_y, after_first);
}

/// shift ホイール converts vertical スクロール into horizontal pan が期待どおりに動作することを検証する。
#[test]
fn shift_wheel_converts_vertical_scroll_into_horizontal_pan() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 16, 16);
    runtime.last_cursor_position = Some((center_x, center_y));
    runtime.modifiers = ModifiersState::SHIFT;

    let before = runtime.app.document.view_transform.pan_x;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 2.0)));
    assert!(runtime.app.document.view_transform.pan_x > before);
}

/// control ホイール changes ズーム が期待どおりに動作することを検証する。
#[test]
fn control_wheel_changes_zoom() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
    let (center_x, center_y) = canvas_input_point(&runtime, 16, 16);
    runtime.last_cursor_position = Some((center_x, center_y));
    runtime.modifiers = ModifiersState::CONTROL;

    let before = runtime.app.document.view_transform.zoom;
    assert!(runtime.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0)));
    assert!(runtime.app.document.view_transform.zoom > before);
}

/// mouse button without cursor position is ignored が期待どおりに動作することを検証する。
#[test]
fn mouse_button_without_cursor_position_is_ignored() {
    let mut runtime = test_runtime();

    assert!(!runtime.handle_mouse_button(winit::event::ElementState::Pressed));
}

/// normalized key 名前 uppercases character keys が期待どおりに動作することを検証する。
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

/// editing repeat support matches テキスト navigation keys が期待どおりに動作することを検証する。
#[test]
fn editing_repeat_support_matches_text_navigation_keys() {
    assert!(supports_editing_repeat(&Key::Character("x".into())));
    assert!(supports_editing_repeat(&Key::Named(NamedKey::ArrowLeft)));
    assert!(!supports_editing_repeat(&Key::Named(NamedKey::Tab)));
}

/// normalized ショートカット includes アクティブ modifiers が期待どおりに動作することを検証する。
#[test]
fn normalized_shortcut_includes_active_modifiers() {
    let mut runtime = test_runtime();
    runtime.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;

    assert_eq!(
        runtime.normalized_shortcut(&Key::Character("s".into())),
        Some(("Ctrl+Shift+S".to_string(), "S".to_string()))
    );
}

/// builtin ショートカット dispatches 保存 プロジェクト が期待どおりに動作することを検証する。
#[test]
fn builtin_shortcut_dispatches_save_project() {
    let mut runtime = test_runtime();
    runtime.app.io_state.project_path = std::env::temp_dir().join(format!(
        "altpaint-runtime-save-shortcut-{}.altp.json",
        std::process::id()
    ));
    runtime.modifiers = ModifiersState::CONTROL;

    assert!(runtime.handle_builtin_shortcut(&Key::Character("s".into())));
    assert_eq!(runtime.app.pending_save_task_count(), 1);
    runtime.app.wait_for_pending_save_tasks();
}

/// builtin ショートカット can move フォーカス backward が期待どおりに動作することを検証する。
#[test]
fn builtin_shortcut_can_move_focus_backward() {
    let mut runtime = test_runtime();
    let mut profiler = DesktopProfiler::new();
    let _ = runtime.app.prepare_present_frame(1280, 200, &mut profiler);
    runtime.modifiers = ModifiersState::SHIFT;

    assert!(runtime.handle_builtin_shortcut(&Key::Named(NamedKey::Tab)));
}
