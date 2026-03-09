//! `DesktopApp` の入力処理と描画操作に関するテストをまとめる。

use std::path::PathBuf;

use app_core::{ColorRgba8, Command, ToolKind};

use super::TestDialogs;
use crate::app::DesktopApp;
use crate::canvas_bridge::{CanvasPointerEvent, command_for_canvas_gesture, map_view_to_canvas};
use crate::profiler::DesktopProfiler;

/// ビュー中央がキャンバス中央へ変換されることを確認する。
#[test]
fn canvas_position_maps_view_center_into_bitmap_bounds() {
    let position = map_view_to_canvas(
        &render::RenderFrame {
            width: 64,
            height: 64,
            pixels: vec![255; 64 * 64 * 4],
        },
        CanvasPointerEvent {
            x: 320,
            y: 320,
            width: 640,
            height: 640,
        },
    );

    assert_eq!(position, Some((32, 32)));
}

/// 消しゴムドラッグが erase stroke コマンドになることを確認する。
#[test]
fn eraser_drag_becomes_erase_stroke_command() {
    let command = command_for_canvas_gesture(ToolKind::Eraser, (7, 8), Some((3, 4)));

    assert_eq!(
        command,
        Command::EraseStroke {
            from_x: 3,
            from_y: 4,
            to_x: 7,
            to_y: 8,
        }
    );
}

/// キャンバスドラッグで黒いピクセルが描画されることを確認する。
#[test]
fn canvas_drag_draws_black_pixels() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    app.handle_canvas_pointer("down", center_x, center_y);
    app.handle_canvas_pointer("drag", center_x + 20, center_y);
    app.handle_canvas_pointer("up", center_x + 20, center_y);

    let frame = app.ui_shell.render_frame(&app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

/// 選択色でキャンバス描画できることを確認する。
#[test]
fn canvas_drag_draws_using_selected_color() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    let _ = app.execute_command(Command::SetActiveColor {
        color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
    });
    app.handle_canvas_pointer("down", center_x, center_y);
    app.handle_canvas_pointer("up", center_x, center_y);

    let frame = app.ui_shell.render_frame(&app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0x43, 0xa0, 0x47, 0xff])
    );
}

/// パネルスクロール要求でスクロールオフセットが更新されることを確認する。
#[test]
fn panel_scroll_requests_surface_offset_change() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 120, &mut profiler);

    assert!(app.scroll_panel_surface(6));
    assert!(app.ui_shell.panel_scroll_offset() > 0);
}

/// カラースライダードラッグでドキュメント色が更新されることを確認する。
#[test]
fn panel_slider_drag_updates_document_color() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let surface = app.panel_surface.clone().expect("panel surface exists");

    let mut start = None;
    let mut end = None;
    'outer: for y in 0..surface.height {
        for x in 0..surface.width {
            if let Some(plugin_api::PanelEvent::SetValue {
                panel_id,
                node_id,
                value,
            }) = surface.hit_test(x, y)
                && panel_id == "builtin.color-palette"
                && node_id == "color.slider.red"
            {
                start = Some((x, y, value));
                end = Some((surface.width - 1, y));
                break 'outer;
            }
        }
    }

    let (start_x, start_y, _) = start.expect("slider hit region exists");
    let (end_x, end_y) = end.expect("slider end exists");
    let window_start_x = layout.panel_surface_rect.x as i32 + start_x as i32;
    let window_start_y = layout.panel_surface_rect.y as i32 + start_y as i32;
    let window_end_x = layout.panel_surface_rect.x as i32 + end_x as i32;
    let window_end_y = layout.panel_surface_rect.y as i32 + end_y as i32;

    assert!(app.handle_pointer_pressed(window_start_x, window_start_y));
    assert!(app.handle_pointer_dragged(window_end_x, window_end_y));
    assert!(!app.handle_pointer_released(window_end_x, window_end_y));
    assert_eq!(app.document.active_color.r, 255);
}

/// スクロール時の差分更新が UI 全体再同期を引き起こさないことを確認する。
#[test]
fn scroll_refresh_does_not_trigger_ui_update() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 120, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.scroll_panel_surface(6));
    let update = app.prepare_present_frame(1280, 120, &mut profiler);

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert_eq!(update.base_dirty_rect, Some(layout.panel_host_rect));
    assert_eq!(update.overlay_dirty_rect, None);
    assert!(!update.canvas_updated);
    assert_eq!(
        profiler.stats.get("panel_surface").map(|stat| stat.calls),
        Some(1)
    );
}

/// フォーカス移動時の差分更新が UI 全体再同期を引き起こさないことを確認する。
#[test]
fn focus_refresh_does_not_trigger_ui_update() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.focus_next_panel_control());
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert_eq!(update.base_dirty_rect, Some(layout.panel_host_rect));
    assert_eq!(update.overlay_dirty_rect, None);
    assert!(!update.canvas_updated);
    assert_eq!(
        profiler.stats.get("panel_surface").map(|stat| stat.calls),
        Some(1)
    );
}

/// ツール切替時に全面再合成なしで状態表示だけ更新できることを確認する。
#[test]
fn tool_change_updates_status_without_full_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_command(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(profiler.stats.contains_key("compose_dirty_panel"));
    assert!(profiler.stats.contains_key("compose_dirty_status"));
    assert!(!update.canvas_updated);
    assert_eq!(
        update.base_dirty_rect,
        Some(
            layout
                .panel_host_rect
                .union(crate::frame::status_text_bounds(1280, 200, &layout, &app.status_text()))
        )
    );
    assert_eq!(update.overlay_dirty_rect, None);
}

/// パン時は CPU 再合成なしで GPU キャンバス変換だけ更新できることを確認する。
#[test]
fn pan_view_updates_canvas_without_status_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_command(Command::PanView {
        delta_x: 32.0,
        delta_y: 0.0,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(!profiler.stats.contains_key("compose_dirty_status"));
    assert!(profiler.stats.contains_key("prepare_canvas_scene"));
    assert!(profiler.stats.contains_key("compose_dirty_canvas_base"));
    assert!(!profiler.stats.contains_key("compose_dirty_overlay"));
    assert!(update.canvas_updated);
    assert!(update.canvas_transform_changed);
    assert_eq!(update.canvas_dirty_rect, None);
    assert!(update
        .base_dirty_rect
        .expect("base dirty rect")
        .width
        <= layout.canvas_host_rect.width);
    assert_eq!(update.overlay_dirty_rect, None);
}

#[test]
fn pan_view_updates_canvas_quad_without_bitmap_reupload() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let original_quad = app.canvas_texture_quad().expect("canvas quad exists");

    assert!(app.execute_command(Command::PanView {
        delta_x: 0.0,
        delta_y: -32.0,
    }));
    let update = app.prepare_present_frame(1280, 800, &mut profiler);
    let moved_quad = app.canvas_texture_quad().expect("canvas quad exists");

    assert!(update.canvas_transform_changed);
    assert_eq!(update.canvas_dirty_rect, None);
    assert_ne!(original_quad.destination, moved_quad.destination);
}

#[test]
fn pan_can_expand_canvas_quad_into_host_margin() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_command(Command::PanView {
        delta_x: -96.0,
        delta_y: 0.0,
    }));
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let moved_quad = app.canvas_texture_quad().expect("canvas quad exists");

    assert!(moved_quad.destination.x < layout.canvas_display_rect.x);
}

/// `NewDocument` 用のテストダイアログ付きアプリでも描画系の初期化が行えることを確認する。
#[test]
fn test_dialog_app_can_prepare_frame() {
    let mut app = super::test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();

    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(update.canvas_updated);
}
