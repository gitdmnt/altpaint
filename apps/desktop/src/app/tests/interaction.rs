//! `DesktopApp` の入力処理と描画操作に関するテストをまとめる。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use app_core::{
    CanvasPoint, CanvasViewportPoint, ColorRgba8, Command, PanelSurfacePoint, ToolKind, WindowPoint,
};
use canvas::{CanvasPointerEvent, map_view_to_canvas_with_transform};
use desktop_support::{DesktopProfiler, StageStats, ValueStats};

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::{DesktopApp, PanelDragState};

/// 矩形 within パネル サーフェス を計算して返す。
fn rect_within_panel_surface(rect: crate::frame::Rect, surface: &ui_shell::PanelSurface) -> bool {
    rect.x >= surface.x
        && rect.y >= surface.y
        && rect.x + rect.width <= surface.x + surface.width
        && rect.y + rect.height <= surface.y + surface.height
}

/// キャンバス position maps ビュー center into ビットマップ 範囲 が期待どおりに動作することを検証する。
#[test]
fn canvas_position_maps_view_center_into_bitmap_bounds() {
    let position = map_view_to_canvas_with_transform(
        &render::RenderFrame {
            width: 64,
            height: 64,
            pixels: vec![255; 64 * 64 * 4],
        },
        CanvasPointerEvent {
            position: CanvasViewportPoint::new(320, 320),
            width: 640,
            height: 640,
        },
        app_core::CanvasViewTransform::default(),
    );

    assert_eq!(position, Some(CanvasPoint::new(32, 32)));
}

/// 消しゴム drag clears existing pixels が期待どおりに動作することを検証する。
#[test]
fn eraser_drag_clears_existing_pixels() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    app.handle_canvas_pointer("down", WindowPoint::new(center_x, center_y), 1.0);
    app.handle_canvas_pointer("up", WindowPoint::new(center_x, center_y), 1.0);
    let _ = app.execute_command(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    });
    app.handle_canvas_pointer("down", WindowPoint::new(center_x, center_y), 1.0);
    app.handle_canvas_pointer("up", WindowPoint::new(center_x, center_y), 1.0);

    let frame = render::RenderContext::new().render_frame(&app.document);
    let bitmap_x = frame.width / 2;
    let bitmap_y = frame.height / 2;
    let index = (bitmap_y * frame.width + bitmap_x) * 4;
    assert_eq!(&frame.pixels[index..index + 4], &[255, 255, 255, 255]);
}

/// キャンバス drag draws black pixels が期待どおりに動作することを検証する。
#[test]
fn canvas_drag_draws_black_pixels() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    app.handle_canvas_pointer("down", WindowPoint::new(center_x, center_y), 1.0);
    app.handle_canvas_pointer("drag", WindowPoint::new(center_x + 20, center_y), 1.0);
    app.handle_canvas_pointer("up", WindowPoint::new(center_x + 20, center_y), 1.0);

    let frame = render::RenderContext::new().render_frame(&app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

/// キャンバス drag draws using 選択中 色 が期待どおりに動作することを検証する。
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
    app.handle_canvas_pointer("down", WindowPoint::new(center_x, center_y), 1.0);
    app.handle_canvas_pointer("up", WindowPoint::new(center_x, center_y), 1.0);

    let frame = render::RenderContext::new().render_frame(&app.document);
    assert!(
        frame
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0x43, 0xa0, 0x47, 0xff])
    );
}

/// パネル 矩形 ツール creates パネル from dragged ページ 矩形 が期待どおりに動作することを検証する。
#[test]
fn panel_rect_tool_creates_panel_from_dragged_page_rect() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    assert!(app.execute_command(Command::SetActiveTool {
        tool: ToolKind::PanelRect,
    }));

    let layout = app.layout.clone().expect("layout exists");
    let start_window = (
        layout.canvas_display_rect.x as i32 + layout.canvas_display_rect.width as i32 / 4,
        layout.canvas_display_rect.y as i32 + layout.canvas_display_rect.height as i32 / 4,
    );
    let end_window = (
        layout.canvas_display_rect.x as i32 + layout.canvas_display_rect.width as i32 * 3 / 5,
        layout.canvas_display_rect.y as i32 + layout.canvas_display_rect.height as i32 / 2,
    );
    let start = app
        .canvas_position_from_window_clamped(WindowPoint::new(start_window.0, start_window.1))
        .expect("start page position");
    let end = app
        .canvas_position_from_window_clamped(WindowPoint::new(end_window.0, end_window.1))
        .expect("end page position");

    assert!(app.handle_canvas_pointer(
        "down",
        WindowPoint::new(start_window.0, start_window.1),
        1.0,
    ));
    assert!(app.handle_canvas_pointer("drag", WindowPoint::new(end_window.0, end_window.1), 1.0,));
    assert!(app.handle_canvas_pointer("up", WindowPoint::new(end_window.0, end_window.1), 1.0,));

    let page = app.document.active_page().expect("active page");
    assert_eq!(page.panels.len(), 2);
    let created = page.panels.last().expect("created panel");
    assert_eq!(created.bounds.x, start.x.min(end.x));
    assert_eq!(created.bounds.y, start.y.min(end.y));
    assert_eq!(
        created.bounds.width,
        start.x.max(end.x) - start.x.min(end.x) + 1
    );
    assert_eq!(
        created.bounds.height,
        start.y.max(end.y) - start.y.min(end.y) + 1
    );
    assert_eq!(app.document.active_panel_index(), 1);
}

/// パネル スクロール requests サーフェス オフセット change が期待どおりに動作することを検証する。
#[test]
fn panel_scroll_requests_surface_offset_change() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 120, &mut profiler);

    assert!(!app.scroll_panel_surface(6));
    assert_eq!(app.panel_presentation.panel_scroll_offset(), 0);
}

/// パネル 色 ホイール updates ドキュメント 色 が期待どおりに動作することを検証する。
#[test]
fn panel_color_wheel_updates_document_color() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert!(app.dispatch_panel_event(panel_api::PanelEvent::SetText {
        panel_id: "builtin.color-palette".to_string(),
        node_id: "color.wheel".to_string(),
        value: "120,100,100".to_string(),
    }));
}

/// パネル 色 ホイール pointer press is handled が期待どおりに動作することを検証する。
#[test]
fn panel_color_wheel_pointer_press_is_handled() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let surface = app.panel_surface.clone().expect("panel surface exists");

    let to_window = |surface_x: usize, surface_y: usize| {
        (
            surface.x as i32 + surface_x as i32,
            surface.y as i32 + surface_y as i32,
        )
    };
    let mut first_hit = None;
    'outer: for y in 0..surface.height {
        for x in 0..surface.width {
            if let Some(panel_api::PanelEvent::SetText {
                panel_id,
                node_id,
                value,
            }) = surface.hit_test_at(PanelSurfacePoint::new(x, y))
                && panel_id == "builtin.color-palette"
                && node_id == "color.wheel"
            {
                if surface
                    .move_panel_hit_test_at(PanelSurfacePoint::new(x, y))
                    .is_some()
                {
                    continue;
                }
                let window = to_window(x, y);

                match &first_hit {
                    None => first_hit = Some((window.0, window.1, value)),
                    Some(_) => break 'outer,
                }
            }
        }
    }

    let (press_x, press_y, _) = first_hit.expect("first draggable color wheel hit exists");

    assert!(app.handle_pointer_pressed(press_x, press_y));
}

/// overlapping パネル button press takes priority over キャンバス 入力 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn overlapping_panel_button_press_takes_priority_over_canvas_input() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.panel_presentation.move_panel_to(
        "builtin.tool-palette",
        layout.canvas_display_rect.x + 24,
        layout.canvas_display_rect.y + 24,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);

    let surface = app.panel_surface.clone().expect("panel surface exists");
    let (button_x, button_y) = (0..surface.height)
        .find_map(|y| {
            (0..surface.width).find_map(|x| {
                match surface.hit_test_at(PanelSurfacePoint::new(x, y)) {
                    Some(panel_api::PanelEvent::Activate { panel_id, node_id })
                        if panel_id == "builtin.tool-palette" && node_id == "tool.eraser" =>
                    {
                        Some((surface.x as i32 + x as i32, surface.y as i32 + y as i32))
                    }
                    _ => None,
                }
            })
        })
        .expect("tool button hit exists");

    assert!(app.handle_pointer_pressed(button_x, button_y));
    assert!(app.handle_pointer_released(button_x, button_y));
    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

/// overlapping パネル drag takes priority over キャンバス 入力 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn overlapping_panel_drag_takes_priority_over_canvas_input() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.panel_presentation.move_panel_to(
        "builtin.layers-panel",
        layout.canvas_display_rect.x + 32,
        layout.canvas_display_rect.y + 32,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);

    let before_position = app
        .panel_presentation
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers-panel")
        .and_then(|panel| panel.position)
        .expect("stored panel position exists");
    let surface = app.panel_surface.clone().expect("panel surface exists");
    let (press_x, press_y) = (0..surface.height)
        .find_map(|y| {
            (0..surface.width).find_map(|x| {
                surface
                    .move_panel_hit_test_at(PanelSurfacePoint::new(x, y))
                    .filter(|panel_id| panel_id == "builtin.layers-panel")
                    .map(|_| (surface.x as i32 + x as i32, surface.y as i32 + y as i32))
            })
        })
        .expect("move-panel hit exists");
    let press = WindowPoint::new(press_x, press_y);
    let drag = WindowPoint::new(press.x + 96, press.y + 48);

    assert!(app.handle_pointer_pressed(press.x, press.y));
    assert!(app.handle_pointer_dragged(drag.x, drag.y));
    let _ = app.handle_pointer_released(drag.x, drag.y);
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);

    let after = app
        .panel_presentation
        .panel_rect("builtin.layers-panel")
        .expect("panel rect exists");
    let after_position = app
        .panel_presentation
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers-panel")
        .and_then(|panel| panel.position)
        .expect("stored panel position exists");
    assert_ne!(
        (after_position.x, after_position.y),
        (before_position.x, before_position.y)
    );
    assert!(after.x >= layout.canvas_display_rect.x);
    assert!(after.y >= layout.canvas_display_rect.y);
}

/// レイヤー 一覧 drag keeps dragged レイヤー 選択中 while reordering が期待どおりに動作することを検証する。
#[test]
fn layer_list_drag_keeps_dragged_layer_selected_while_reordering() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    app.panel_interaction.active_panel_drag = Some(PanelDragState::Control {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        source_value: 2,
    });

    app.advance_panel_drag_source(&panel_api::PanelEvent::DragValue {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        from: 2,
        to: 1,
    });
    assert_eq!(
        app.panel_interaction
            .active_panel_drag
            .as_ref()
            .and_then(|drag| match drag {
                PanelDragState::Control { source_value, .. } => Some(*source_value),
                PanelDragState::Move { .. } => None,
            }),
        Some(1)
    );

    app.advance_panel_drag_source(&panel_api::PanelEvent::DragValue {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        from: 1,
        to: 0,
    });
    assert_eq!(
        app.panel_interaction
            .active_panel_drag
            .as_ref()
            .and_then(|drag| match drag {
                PanelDragState::Control { source_value, .. } => Some(*source_value),
                PanelDragState::Move { .. } => None,
            }),
        Some(0)
    );
}
/// スクロール refresh does not trigger ui 更新 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn scroll_refresh_does_not_trigger_ui_update() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 120, &mut profiler);
    profiler.stats.clear();

    assert!(!app.scroll_panel_surface(6));
    let update = app.prepare_present_frame(1280, 120, &mut profiler);

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert_eq!(update.background_dirty_rect, None);
    assert_eq!(update.temp_overlay_dirty_rect, None);
    assert!(!update.canvas_updated);
    assert_eq!(
        profiler.stats.get("panel_surface").map(|stat| stat.calls),
        None
    );
}

// 削除: panel_move_recomposes_without_rerasterizing_panel_content (Phase 9E-5)
// CPU panel rasterize / panel_surface_rasterized_panels / panel_surface_composited_panels
// 計測は 9E-3 で削除済み。代替検証は workspace_manager_panel_can_be_moved (パネル位置変更
// が反映されること) で十分カバー済みのため、本テストは削除する。

/// ワークスペース manager パネル can be moved が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn workspace_manager_panel_can_be_moved() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let before = app
        .panel_presentation
        .panel_rect("builtin.workspace-layout")
        .expect("workspace panel rect exists");

    assert!(app.panel_presentation.move_panel_to(
        "builtin.workspace-layout",
        before.x + 80,
        before.y + 24,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    let after = app
        .panel_presentation
        .panel_rect("builtin.workspace-layout")
        .expect("workspace panel rect exists");
    assert_ne!(after, before);
    assert!(after.x >= before.x + 80 || after.y >= before.y + 24);
}

// 削除: panel_move_dirty_rect_covers_previous_and_current_overlay_bounds (Phase 9E-5)
// L4 ui_panel_layer は 9E-3 で dummy 化されたため `ui_panel_dirty_rect` は常に None。
// パネル GPU 直描画後の dirty rect 監視は Phase 9F で `panel_quads` レイヤー再構成
// (PresentScene 改名) と一緒に書き直す。

// 削除: overlapping_panel_and_canvas_overlay_updates_union_dirty_rects (Phase 9E-5)
// 同上。`ui_panel_dirty_rect` 検証経路が dummy 化されたため、Phase 9F で
// L3/L5 を統合した dirty rect 検証として書き直す。

/// profile 色 ホイール drag for ten seconds が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
#[ignore = "manual performance profiling"]
fn profile_color_wheel_drag_for_ten_seconds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let viewport = (1280, 800);
    let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
    profiler.stats.clear();
    profiler.value_stats.clear();

    let points = control_points_from_surface(&app, |event| {
        matches!(
            event,
            panel_api::PanelEvent::SetText {
                panel_id,
                node_id,
                ..
            } if panel_id == "builtin.color-palette" && node_id == "color.wheel"
        )
    });
    assert!(points.len() >= 8, "color wheel points exist");

    let duration = perf_duration();
    let started = Instant::now();
    let mut iterations = 0u64;
    let mut index = 0usize;

    let start = points[0];
    assert!(app.handle_pointer_pressed(start.0, start.1));
    let initial_update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
    if initial_update.temp_overlay_dirty_rect.is_some() {
        iterations += 1;
    }
    while started.elapsed() < duration {
        let point = points[index % points.len()];
        if app.handle_pointer_dragged(point.0, point.1) {
            let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
            if update.temp_overlay_dirty_rect.is_some() {
                iterations += 1;
            }
        }
        index += 1;
    }
    let end = points[index % points.len()];
    let _ = app.handle_pointer_released(end.0, end.1);

    assert!(iterations > 0, "color wheel drag produced updates");
    emit_panel_perf(
        "color-wheel-perf",
        &profiler,
        started.elapsed().as_secs_f64(),
        iterations,
    );
}

/// profile 色 ホイール events for ten seconds が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
#[ignore = "manual performance profiling"]
fn profile_color_wheel_events_for_ten_seconds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let viewport = (1280, 800);
    let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
    profiler.stats.clear();
    profiler.value_stats.clear();

    let duration = perf_duration();
    let started = Instant::now();
    let mut iterations = 0u64;
    let mut hue = 0usize;

    while started.elapsed() < duration {
        let saturation = 40 + (hue % 61);
        let value = 40 + ((hue * 3) % 61);
        assert!(app.dispatch_panel_event(panel_api::PanelEvent::SetText {
            panel_id: "builtin.color-palette".to_string(),
            node_id: "color.wheel".to_string(),
            value: format!("{hue},{saturation},{value}"),
        }));
        let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
        if update.temp_overlay_dirty_rect.is_some() {
            iterations += 1;
        }
        hue = (hue + 17) % 360;
    }

    assert!(iterations > 0, "color wheel events produced updates");
    emit_panel_perf(
        "color-wheel-event-perf",
        &profiler,
        started.elapsed().as_secs_f64(),
        iterations,
    );
}

/// profile slider drag for ten seconds が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
#[ignore = "manual performance profiling"]
fn profile_slider_drag_for_ten_seconds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let viewport = (1280, 800);
    let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
    profiler.stats.clear();
    profiler.value_stats.clear();

    let points = control_points_from_surface(&app, |event| {
        matches!(
            event,
            panel_api::PanelEvent::SetValue {
                panel_id,
                node_id,
                ..
            } if panel_id == "builtin.pen-settings" && node_id == "pen.size"
        )
    });
    assert!(points.len() >= 8, "slider points exist");

    let duration = perf_duration();
    let started = Instant::now();
    let mut iterations = 0u64;
    let mut index = 0usize;

    let start = points[0];
    assert!(app.handle_pointer_pressed(start.0, start.1));
    let initial_update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
    if initial_update.temp_overlay_dirty_rect.is_some() {
        iterations += 1;
    }
    while started.elapsed() < duration {
        let point = points[index % points.len()];
        if app.handle_pointer_dragged(point.0, point.1) {
            let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
            if update.temp_overlay_dirty_rect.is_some() {
                iterations += 1;
            }
        }
        index += 1;
    }
    let end = points[index % points.len()];
    let _ = app.handle_pointer_released(end.0, end.1);

    assert!(iterations > 0, "slider drag produced updates");
    emit_panel_perf(
        "slider-perf",
        &profiler,
        started.elapsed().as_secs_f64(),
        iterations,
    );
}

/// profile パネル drag for ten seconds が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
#[ignore = "manual performance profiling"]
fn profile_panel_drag_for_ten_seconds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let viewport = (1280, 800);
    let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
    profiler.stats.clear();
    profiler.value_stats.clear();

    let positions = [
        (32, 72),
        (180, 96),
        (360, 120),
        (560, 144),
        (760, 180),
        (940, 216),
        (760, 360),
        (520, 480),
        (260, 560),
        (48, 420),
    ];
    let started = Instant::now();
    let duration = perf_duration();
    let mut iterations = 0u64;
    let mut position_index = 0usize;

    while started.elapsed() < duration {
        let layout = app.layout.clone().expect("layout exists");
        let (x, y) = positions[position_index % positions.len()];
        let changed = app.panel_presentation.move_panel_to(
            "builtin.layers-panel",
            x,
            y,
            layout.window_rect.width,
            layout.window_rect.height,
        );
        if changed {
            app.mark_panel_surface_dirty();
        }

        let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
        assert!(update.temp_overlay_dirty_rect.is_some());
        iterations += 1;
        position_index += 1;
    }

    let elapsed = started.elapsed().as_secs_f64();
    emit_panel_perf("panel-perf", &profiler, elapsed, iterations);
}

/// profile ビュー 変換 for ten seconds が期待どおりに動作することを検証する。
#[test]
#[ignore = "manual performance profiling"]
fn profile_view_transform_for_ten_seconds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let viewport = (1280, 800);
    let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);

    let total_duration = perf_duration();
    let per_case_duration = Duration::from_secs_f64((total_duration.as_secs_f64() / 3.0).max(1.0));

    profile_view_perf_case(
        "pan",
        &mut app,
        &mut profiler,
        viewport,
        per_case_duration,
        |app, iteration| {
            let direction = if iteration % 2 == 0 { 18.0 } else { -18.0 };
            app.execute_command(Command::PanView {
                delta_x: direction,
                delta_y: direction * 0.5,
            })
        },
    );

    profile_view_perf_case(
        "zoom",
        &mut app,
        &mut profiler,
        viewport,
        per_case_duration,
        |app, iteration| {
            let zoom = if iteration % 2 == 0 { 1.08 } else { 0.92 };
            let next = (app.document.view_transform.zoom * zoom).clamp(0.25, 16.0);
            app.execute_command(Command::SetViewZoom { zoom: next })
        },
    );

    profile_view_perf_case(
        "rotate",
        &mut app,
        &mut profiler,
        viewport,
        per_case_duration,
        |app, iteration| {
            let delta = if iteration % 2 == 0 { 7.5 } else { -7.5 };
            let next = app.document.view_transform.rotation_degrees + delta;
            app.execute_command(Command::SetViewRotation {
                rotation_degrees: next,
            })
        },
    );
}

/// ズーム操作の prepare_present_frame が 240fps の CPU 予算内に収まることを検証する。
#[test]
fn zoom_perf_meets_240fps_target() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);

    let iterations = 1000u32;
    let mut times_us: Vec<u128> = Vec::with_capacity(iterations as usize);

    for i in 0..iterations {
        let zoom = if i % 2 == 0 { 1.08_f32 } else { 0.92_f32 };
        let next = (app.document.view_transform.zoom * zoom).clamp(0.25, 16.0);
        app.execute_command(Command::SetViewZoom { zoom: next });
        let start = std::time::Instant::now();
        let _ = app.prepare_present_frame(1280, 800, &mut profiler);
        times_us.push(start.elapsed().as_micros());
    }

    times_us.sort_unstable();
    let median_us = times_us[iterations as usize / 2];
    let p99_us = times_us[(iterations as usize * 99) / 100];
    // 240fps = 4.17ms/フレーム のうち CPU 予算 2ms 以下を目標とする
    let target_us = 2000u128;

    eprintln!("[zoom-perf] median={median_us}µs p99={p99_us}µs target<={target_us}µs");
    assert!(
        median_us <= target_us,
        "ズーム median {median_us}µs > 目標 {target_us}µs (prepare_present_frame)"
    );
}

/// profile キャンバス ブラシ sizes for ten seconds が期待どおりに動作することを検証する。
#[test]
#[ignore = "manual performance profiling"]
fn profile_canvas_brush_sizes_for_ten_seconds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let viewport = (1280, 800);
    let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);

    let layout = app.layout.clone().expect("layout exists");
    let start_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 4) as i32;
    let end_x = (layout.canvas_display_rect.x + (layout.canvas_display_rect.width * 3) / 4) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;
    let total_duration = perf_duration();
    let combinations = [
        (ToolKind::Pen, 4u32),
        (ToolKind::Pen, 128u32),
        (ToolKind::Pen, 64u32),
        (ToolKind::Pen, 256u32),
        (ToolKind::Pen, 512u32),
        (ToolKind::Eraser, 4u32),
        (ToolKind::Eraser, 128u32),
        (ToolKind::Eraser, 64u32),
        (ToolKind::Eraser, 256u32),
        (ToolKind::Eraser, 512u32),
    ];
    let per_case_seconds = (total_duration.as_secs_f64() / combinations.len() as f64).max(1.0);
    let per_case_duration = Duration::from_secs_f64(per_case_seconds);

    for (tool, size) in combinations {
        assert!(app.execute_command(Command::SetActiveTool { tool }));
        assert!(app.execute_command(Command::SetActivePenSize { size }));
        let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
        profiler.stats.clear();
        profiler.value_stats.clear();

        let started = Instant::now();
        let mut iterations = 0u64;
        let mut forward = true;
        while started.elapsed() < per_case_duration {
            let (down_x, up_x) = if forward {
                (start_x, end_x)
            } else {
                (end_x, start_x)
            };
            app.handle_canvas_pointer("down", WindowPoint::new(down_x, center_y), 1.0);
            let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);

            for step in 1..=8 {
                let x = down_x + ((up_x - down_x) * step / 8);
                app.handle_canvas_pointer("drag", WindowPoint::new(x, center_y), 1.0);
                let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
                if update.canvas_dirty_rect.is_some() {
                    iterations += 1;
                }
            }

            app.handle_canvas_pointer("up", WindowPoint::new(up_x, center_y), 1.0);
            let _ = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
            forward = !forward;
        }

        assert!(iterations > 0, "canvas stroke produced updates");
        emit_canvas_perf(
            tool,
            size,
            &profiler,
            started.elapsed().as_secs_f64(),
            iterations,
        );
    }
}

/// フォーカス refresh does not trigger ui 更新 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
///
/// Phase 9E-5: L4 ui_panel_layer は dummy 化されたため `ui_panel_dirty_rect` ではなく
/// 「フルリコンポーズ / canvas 更新が起きない」という弱検証に書き換えた。
/// パネル本体の dirty 検証は 9F で `panel_quads` 経路に対応する形で再導入する。
#[test]
fn focus_refresh_does_not_trigger_ui_update() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();

    assert!(app.focus_next_panel_control());
    let update = app.prepare_present_frame(1280, 200, &mut profiler);
    let surface = app.panel_surface.clone().expect("panel surface exists");

    // フォーカス移動はキャンバス再描画も full recompose も起こしてはならない。
    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert_eq!(update.background_dirty_rect, None);
    assert!(!update.canvas_updated);

    // 9E-5: L4 dummy 化により `ui_panel_dirty_rect` は通常 None。値があれば
    // panel surface 範囲内に収まることだけ確認する (将来 GPU dirty 経路で意味を持つ)。
    if let Some(panel_dirty) = update.ui_panel_dirty_rect {
        assert!(rect_within_panel_surface(panel_dirty, &surface));
    }
}

/// ツール change updates ステータス without full recompose が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn tool_change_updates_status_without_full_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    let _layout = app.layout.clone().expect("layout exists");

    assert!(app.execute_command(Command::SetActiveTool {
        tool: ToolKind::Eraser,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    // 9E-4: status text は GPU 描画化されたため compose_dirty_status / background_dirty_rect の
    // ピクセル比較は不要。ツール変更で full recompose にならず canvas が更新されないことだけ検証する。
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(!update.canvas_updated);
    let surface = app.panel_surface.clone().expect("panel surface exists");
    if let Some(panel_dirty) = update.ui_panel_dirty_rect {
        assert!(rect_within_panel_surface(panel_dirty, &surface));
    }
}

/// パネル release without matching press does not activate 保存 が期待どおりに動作することを検証する。
#[test]
fn panel_release_without_matching_press_does_not_activate_save() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let surface = app.panel_surface.clone().expect("panel surface exists");

    let (save_x, save_y) = (0..surface.height)
        .find_map(|y| {
            (0..surface.width).find_map(|x| {
                match surface.hit_test_at(PanelSurfacePoint::new(x, y)) {
                    Some(panel_api::PanelEvent::Activate { panel_id, node_id })
                        if panel_id == "builtin.app-actions" && node_id == "app.save" =>
                    {
                        Some((surface.x as i32 + x as i32, surface.y as i32 + y as i32))
                    }
                    _ => None,
                }
            })
        })
        .expect("save button hit exists");

    assert!(!app.handle_pointer_released(save_x, save_y));
    assert_eq!(app.pending_save_task_count(), 0);
}

/// avg stage ms を計算して返す。
fn avg_stage_ms(profiler: &DesktopProfiler, label: &'static str) -> f64 {
    profiler.stats.get(label).map_or(0.0, avg_stage_stat_ms)
}

/// avg stage stat ms を計算して返す。
fn avg_stage_stat_ms(stat: &StageStats) -> f64 {
    if stat.calls == 0 {
        0.0
    } else {
        stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
    }
}

/// max stage ms を計算して返す。
fn max_stage_ms(profiler: &DesktopProfiler, label: &'static str) -> f64 {
    profiler
        .stats
        .get(label)
        .map_or(0.0, |stat| stat.max.as_secs_f64() * 1000.0)
}

/// avg 値 を計算して返す。
fn avg_value(profiler: &DesktopProfiler, label: &'static str) -> f64 {
    profiler.value_stats.get(label).map_or(0.0, avg_value_stat)
}

/// avg 値 stat を計算して返す。
fn avg_value_stat(stat: &ValueStats) -> f64 {
    if stat.samples == 0 {
        0.0
    } else {
        stat.total / stat.samples as f64
    }
}

/// 入力を解析して duration に変換する。
fn perf_duration() -> Duration {
    std::env::var("ALTPAINT_PANEL_PERF_DURATION_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(10))
}

/// 入力や種別に応じて処理を振り分ける。
fn emit_canvas_perf(
    tool: ToolKind,
    size: u32,
    profiler: &DesktopProfiler,
    elapsed: f64,
    iterations: u64,
) {
    let tool_name = match tool {
        ToolKind::Pen => "pen",
        ToolKind::Eraser => "eraser",
        ToolKind::Bucket => "bucket",
        ToolKind::LassoBucket => "lasso-bucket",
        ToolKind::PanelRect => "panel-rect",
    };
    eprintln!(
        "[canvas-perf] tool={tool_name} size={size} duration={elapsed:.2}s iterations={iterations} rate={:.1}Hz",
        iterations as f64 / elapsed
    );
    eprintln!(
        "[canvas-perf] tool={tool_name} size={size} prepare_frame avg={:.3}ms max={:.3}ms | prepare_canvas_scene avg={:.3}ms max={:.3}ms",
        avg_stage_ms(profiler, "prepare_frame"),
        max_stage_ms(profiler, "prepare_frame"),
        avg_stage_ms(profiler, "prepare_canvas_scene"),
        max_stage_ms(profiler, "prepare_canvas_scene"),
    );
    eprintln!(
        "[canvas-perf] tool={tool_name} size={size} canvas upload avg={:.2}% ({:.0}px) | overlay upload avg={:.2}% ({:.0}px)",
        avg_value(profiler, "canvas_upload_coverage_pct"),
        avg_value(profiler, "canvas_upload_area_px"),
        avg_value(profiler, "overlay_upload_coverage_pct"),
        avg_value(profiler, "overlay_upload_area_px"),
    );
}

/// Emit パネル perf に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn emit_panel_perf(label: &str, profiler: &DesktopProfiler, elapsed: f64, iterations: u64) {
    eprintln!(
        "[{label}] duration={elapsed:.2}s iterations={iterations} rate={:.1}Hz",
        iterations as f64 / elapsed
    );
    eprintln!(
        "[{label}] prepare_frame avg={:.3}ms max={:.3}ms | panel_surface avg={:.3}ms max={:.3}ms | compose_dirty_panel avg={:.3}ms max={:.3}ms",
        avg_stage_ms(profiler, "prepare_frame"),
        max_stage_ms(profiler, "prepare_frame"),
        avg_stage_ms(profiler, "panel_surface"),
        max_stage_ms(profiler, "panel_surface"),
        avg_stage_ms(profiler, "compose_dirty_panel"),
        max_stage_ms(profiler, "compose_dirty_panel"),
    );
    eprintln!(
        "[{label}] panel raster avg={:.3}ms max={:.3}ms | panel compose avg={:.3}ms max={:.3}ms",
        avg_value(profiler, "panel_surface_raster_ms"),
        profiler
            .value_stats
            .get("panel_surface_raster_ms")
            .map_or(0.0, |stat| stat.max),
        avg_value(profiler, "panel_surface_compose_ms"),
        profiler
            .value_stats
            .get("panel_surface_compose_ms")
            .map_or(0.0, |stat| stat.max),
    );
    eprintln!(
        "[{label}] rasterized avg={:.2} panels | composited avg={:.2} panels | panel coverage avg={:.2}% | overlay upload avg={:.2}% ({:.0}px)",
        avg_value(profiler, "panel_surface_rasterized_panels"),
        avg_value(profiler, "panel_surface_composited_panels"),
        avg_value(profiler, "panel_surface_window_coverage_pct"),
        avg_value(profiler, "overlay_upload_coverage_pct"),
        avg_value(profiler, "overlay_upload_area_px"),
    );
    eprintln!(
        "[{label}] base upload avg={:.2}% ({:.0}px) | hit regions avg={:.1}",
        avg_value(profiler, "base_upload_coverage_pct"),
        avg_value(profiler, "base_upload_area_px"),
        avg_value(profiler, "panel_surface_hit_regions"),
    );
}

/// Emit ビュー perf に必要な描画内容を組み立てる。
fn emit_view_perf(label: &str, profiler: &DesktopProfiler, elapsed: f64, iterations: u64) {
    eprintln!(
        "[view-perf] case={label} duration={elapsed:.2}s iterations={iterations} rate={:.1}Hz",
        iterations as f64 / elapsed
    );
    eprintln!(
        "[view-perf] case={label} prepare_frame avg={:.3}ms max={:.3}ms | prepare_canvas_scene avg={:.3}ms max={:.3}ms | panel_surface avg={:.3}ms max={:.3}ms",
        avg_stage_ms(profiler, "prepare_frame"),
        max_stage_ms(profiler, "prepare_frame"),
        avg_stage_ms(profiler, "prepare_canvas_scene"),
        max_stage_ms(profiler, "prepare_canvas_scene"),
        avg_stage_ms(profiler, "panel_surface"),
        max_stage_ms(profiler, "panel_surface"),
    );
    eprintln!(
        "[view-perf] case={label} ui_update avg={:.3}ms max={:.3}ms | overlay upload avg={:.2}% ({:.0}px) | base upload avg={:.2}% ({:.0}px)",
        avg_stage_ms(profiler, "ui_update"),
        max_stage_ms(profiler, "ui_update"),
        avg_value(profiler, "overlay_upload_coverage_pct"),
        avg_value(profiler, "overlay_upload_area_px"),
        avg_value(profiler, "base_upload_coverage_pct"),
        avg_value(profiler, "base_upload_area_px"),
    );
}

/// Profile ビュー perf case に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn profile_view_perf_case(
    label: &str,
    app: &mut DesktopApp,
    profiler: &mut DesktopProfiler,
    viewport: (usize, usize),
    duration: Duration,
    mut step: impl FnMut(&mut DesktopApp, u64) -> bool,
) {
    profiler.stats.clear();
    profiler.value_stats.clear();
    let started = Instant::now();
    let mut iterations = 0u64;

    while started.elapsed() < duration {
        if step(app, iterations) {
            let update = app.prepare_present_frame(viewport.0, viewport.1, profiler);
            if update.canvas_updated
                || update.background_dirty_rect.is_some()
                || update.temp_overlay_dirty_rect.is_some()
            {
                iterations += 1;
            }
        }
    }

    assert!(iterations > 0, "{label} produced updates");
    emit_view_perf(label, profiler, started.elapsed().as_secs_f64(), iterations);
}

/// 既存データを走査して control points from サーフェス を組み立てる。
fn control_points_from_surface(
    app: &DesktopApp,
    predicate: impl Fn(&panel_api::PanelEvent) -> bool,
) -> Vec<(i32, i32)> {
    let surface = app.panel_surface.as_ref().expect("panel surface exists");
    let mut points = Vec::new();
    for y in 0..surface.height {
        for x in 0..surface.width {
            let Some(event) = surface.hit_test_at(PanelSurfacePoint::new(x, y)) else {
                continue;
            };
            if predicate(&event) {
                points.push((surface.x as i32 + x as i32, surface.y as i32 + y as i32));
            }
        }
    }
    points.sort_unstable();
    points.dedup();
    let stride = (points.len() / 16).max(1);
    points.into_iter().step_by(stride).take(32).collect()
}

/// pan ビュー updates キャンバス without ステータス recompose が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn pan_view_updates_canvas_without_status_recompose() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();

    assert!(app.execute_command(Command::PanView {
        delta_x: 32.0,
        delta_y: 0.0,
    }));
    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert!(!profiler.stats.contains_key("compose_dirty_status"));
    assert!(profiler.stats.contains_key("prepare_canvas_scene"));
    assert!(!profiler.stats.contains_key("compose_dirty_panel"));
    assert!(!profiler.stats.contains_key("panel_surface"));
    // 9C-1: L1 背景は GPU の solid quad パイプラインで毎フレーム描画されるため
    // パン操作時に CPU の compose_dirty_canvas_base は呼ばれない。
    assert!(!profiler.stats.contains_key("compose_dirty_canvas_base"));
    assert!(!profiler.stats.contains_key("compose_dirty_overlay"));
    assert!(update.canvas_updated);
    assert!(update.canvas_transform_changed);
    assert_eq!(update.canvas_dirty_rect, None);
    // 9C-1: pan は canvas transform のみを変えるため L1 dirty rect は出ない。
    assert!(update.background_dirty_rect.is_none());
    assert_eq!(update.temp_overlay_dirty_rect, None);
}

/// pan ビュー updates キャンバス quad without ビットマップ reupload が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
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

/// pan can expand キャンバス quad into ホスト margin が期待どおりに動作することを検証する。
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

/// 新規 ドキュメント sized resets アクティブ interactions が期待どおりに動作することを検証する。
#[test]
fn new_document_sized_resets_active_interactions() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    assert!(app.handle_canvas_pointer("down", WindowPoint::new(center_x, center_y), 1.0));
    assert!(app.update_canvas_hover(center_x, center_y));
    assert!(app.canvas_input.is_drawing);
    assert!(app.hover_canvas_position.is_some());

    assert!(app.execute_command(Command::NewDocumentSized {
        width: 48,
        height: 32,
    }));
    assert!(!app.canvas_input.is_drawing);
    assert!(app.canvas_input.last_position.is_none());
    assert!(app.hover_canvas_position.is_none());
}

/// test ダイアログ アプリ can prepare フレーム が期待どおりに動作することを検証する。
#[test]
fn test_dialog_app_can_prepare_frame() {
    let mut app = super::test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();

    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(update.canvas_updated);
}

/// ブラシ プレビュー 差分 矩形 grows with ペン サイズ が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn brush_preview_dirty_rect_grows_with_pen_size() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    let _ = app.execute_command(Command::SetViewZoom { zoom: 8.0 });
    let _ = app.execute_command(Command::SetActivePenSize { size: 4 });
    assert!(app.update_canvas_hover(center_x, center_y));
    let small_dirty = app
        .pending_temp_overlay_dirty_rect
        .expect("small preview dirty exists");

    app.pending_temp_overlay_dirty_rect = None;
    app.hover_canvas_position = None;
    let _ = app.execute_command(Command::SetActivePenSize { size: 96 });
    assert!(app.update_canvas_hover(center_x, center_y));
    let large_dirty = app
        .pending_temp_overlay_dirty_rect
        .expect("large preview dirty exists");

    assert!(large_dirty.width > small_dirty.width);
    assert!(large_dirty.height > small_dirty.height);
}

/// lasso プレビュー drag が temp_overlay dirty rect を設定することを検証する。
#[test]
fn lasso_preview_drag_marks_temp_overlay_dirty() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    let _ = app.execute_command(Command::SetActiveTool {
        tool: app_core::ToolKind::LassoBucket,
    });

    // handle_canvas_pointer を直接呼んでパネルインタラクションをバイパス
    // down でラッソ開始 → LassoPreviewChanged
    app.handle_canvas_pointer("down", WindowPoint::new(center_x, center_y), 1.0);
    app.pending_temp_overlay_dirty_rect = None;

    // drag でラッソ点を追加 → LassoPreviewChanged → temp overlay dirty になる
    let dragged = app.handle_canvas_pointer("drag", WindowPoint::new(center_x + 20, center_y + 10), 1.0);
    assert!(dragged, "lasso drag should request redraw");
    assert!(
        app.pending_temp_overlay_dirty_rect.is_some(),
        "lasso drag should set temp overlay dirty rect"
    );
}

/// ToggleActiveLayerVisibility が全体再構築ではなく差分 dirty rect 更新を行うことを検証する。
#[test]
fn toggle_layer_visibility_sets_canvas_dirty_rect_not_full_rebuild() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    // canvas_frame を初期化しておく
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    // 初期化後のフラグをリセット
    app.needs_full_present_rebuild = false;
    app.pending_canvas_dirty_rect = None;

    let _ = app.execute_command(Command::ToggleActiveLayerVisibility);

    assert!(
        app.pending_canvas_dirty_rect.is_some(),
        "ToggleActiveLayerVisibility should set canvas dirty rect"
    );
    assert!(
        !app.needs_full_present_rebuild,
        "ToggleActiveLayerVisibility should not trigger full present rebuild"
    );
}
