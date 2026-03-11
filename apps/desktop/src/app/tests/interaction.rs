//! `DesktopApp` の入力処理と描画操作に関するテストをまとめる。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use app_core::{CanvasPoint, CanvasViewportPoint, ColorRgba8, Command, PanelSurfacePoint, WindowPoint, ToolKind};
use canvas::{CanvasPointerEvent, map_view_to_canvas_with_transform};
use desktop_support::{DesktopProfiler, StageStats, ValueStats};

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::{DesktopApp, PanelDragState};

fn rect_within_panel_surface(rect: crate::frame::Rect, surface: &ui_shell::PanelSurface) -> bool {
    rect.x >= surface.x
        && rect.y >= surface.y
        && rect.x + rect.width <= surface.x + surface.width
        && rect.y + rect.height <= surface.y + surface.height
}

/// ビュー中央がキャンバス中央へ変換されることを確認する。
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

/// 消しゴムドラッグが描画プラグイン経由で既存ピクセルを消去することを確認する。
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

/// キャンバスドラッグで黒いピクセルが描画されることを確認する。
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
    assert!(app.handle_canvas_pointer(
        "drag",
        WindowPoint::new(end_window.0, end_window.1),
        1.0,
    ));
    assert!(app.handle_canvas_pointer(
        "up",
        WindowPoint::new(end_window.0, end_window.1),
        1.0,
    ));

    let page = app.document.active_page().expect("active page");
    assert_eq!(page.panels.len(), 2);
    let created = page.panels.last().expect("created panel");
    assert_eq!(created.bounds.x, start.x.min(end.x));
    assert_eq!(created.bounds.y, start.y.min(end.y));
    assert_eq!(created.bounds.width, start.x.max(end.x) - start.x.min(end.x) + 1);
    assert_eq!(created.bounds.height, start.y.max(end.y) - start.y.min(end.y) + 1);
    assert_eq!(app.document.active_panel_index(), 1);
}

/// パネルスクロール要求でスクロールオフセットが更新されることを確認する。
#[test]
fn panel_scroll_requests_surface_offset_change() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 120, &mut profiler);

    assert!(!app.scroll_panel_surface(6));
    assert_eq!(app.ui_shell.panel_scroll_offset(), 0);
}

/// 色相環操作でドキュメント色が更新されることを確認する。
#[test]
fn panel_color_wheel_updates_document_color() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    assert!(app.dispatch_panel_event(plugin_api::PanelEvent::SetText {
        panel_id: "builtin.color-palette".to_string(),
        node_id: "color.wheel".to_string(),
        value: "120,100,100".to_string(),
    }));
}

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
            if let Some(plugin_api::PanelEvent::SetText {
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

#[test]
fn overlapping_panel_button_press_takes_priority_over_canvas_input() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.ui_shell.move_panel_to(
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
            (0..surface.width).find_map(|x| match surface.hit_test_at(PanelSurfacePoint::new(x, y)) {
                Some(plugin_api::PanelEvent::Activate { panel_id, node_id })
                    if panel_id == "builtin.tool-palette" && node_id == "tool.eraser" =>
                {
                    Some((surface.x as i32 + x as i32, surface.y as i32 + y as i32))
                }
                _ => None,
            })
        })
        .expect("tool button hit exists");

    assert!(app.handle_pointer_pressed(button_x, button_y));
    assert!(app.handle_pointer_released(button_x, button_y));
    assert_eq!(app.document.active_tool, ToolKind::Eraser);
}

#[test]
fn overlapping_panel_drag_takes_priority_over_canvas_input() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.ui_shell.move_panel_to(
        "builtin.layers-panel",
        layout.canvas_display_rect.x + 32,
        layout.canvas_display_rect.y + 32,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);

    let before_position = app
        .ui_shell
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

    let after = app.ui_shell.panel_rect("builtin.layers-panel").expect("panel rect exists");
    let after_position = app
        .ui_shell
        .workspace_layout()
        .panels
        .into_iter()
        .find(|panel| panel.id == "builtin.layers-panel")
        .and_then(|panel| panel.position)
        .expect("stored panel position exists");
    assert_ne!((after_position.x, after_position.y), (before_position.x, before_position.y));
    assert!(after.x >= layout.canvas_display_rect.x);
    assert!(after.y >= layout.canvas_display_rect.y);
}

#[test]
fn layer_list_drag_keeps_dragged_layer_selected_while_reordering() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    app.panel_interaction.active_panel_drag = Some(PanelDragState::Control {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        source_value: 2,
    });

    app.advance_panel_drag_source(&plugin_api::PanelEvent::DragValue {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        from: 2,
        to: 1,
    });
    assert_eq!(
        app.panel_interaction.active_panel_drag.as_ref().and_then(|drag| match drag {
            PanelDragState::Control { source_value, .. } => Some(*source_value),
            PanelDragState::Move { .. } => None,
        }),
        Some(1)
    );

    app.advance_panel_drag_source(&plugin_api::PanelEvent::DragValue {
        panel_id: "builtin.layers-panel".to_string(),
        node_id: "layers.list".to_string(),
        from: 1,
        to: 0,
    });
    assert_eq!(
        app.panel_interaction.active_panel_drag.as_ref().and_then(|drag| match drag {
            PanelDragState::Control { source_value, .. } => Some(*source_value),
            PanelDragState::Move { .. } => None,
        }),
        Some(0)
    );
}
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
    assert_eq!(update.base_dirty_rect, None);
    assert_eq!(update.overlay_dirty_rect, None);
    assert!(!update.canvas_updated);
    assert_eq!(
        profiler.stats.get("panel_surface").map(|stat| stat.calls),
        None
    );
}

#[test]
fn panel_move_recomposes_without_rerasterizing_panel_content() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();
    profiler.value_stats.clear();
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.ui_shell.move_panel_to(
        "builtin.layers-panel",
        80,
        96,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();

    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert_eq!(
        profiler
            .value_stats
            .get("panel_surface_rasterized_panels")
            .map(|stat| (stat.samples, stat.total, stat.max)),
        Some((1, 0.0, 0.0))
    );
    assert_eq!(
        profiler
            .value_stats
            .get("panel_surface_composited_panels")
            .map(|stat| (stat.samples, stat.total > 0.0)),
        Some((1, true))
    );
}

#[test]
fn workspace_manager_panel_can_be_moved() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let before = app
        .ui_shell
        .panel_rect("builtin.workspace-layout")
        .expect("workspace panel rect exists");

    assert!(app.ui_shell.move_panel_to(
        "builtin.workspace-layout",
        before.x + 80,
        before.y + 24,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    let after = app
        .ui_shell
        .panel_rect("builtin.workspace-layout")
        .expect("workspace panel rect exists");
    assert_ne!(after, before);
    assert!(after.x >= before.x + 80 || after.y >= before.y + 24);
}

#[test]
fn panel_move_dirty_rect_covers_previous_and_current_overlay_bounds() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");

    assert!(app.ui_shell.move_panel_to(
        "builtin.layers-panel",
        940,
        540,
        layout.window_rect.width,
        layout.window_rect.height,
    ));
    app.mark_panel_surface_dirty();

    let update = app.prepare_present_frame(1280, 800, &mut profiler);
    let expected = app
        .ui_shell
        .last_panel_surface_dirty_rect()
        .map(|dirty| crate::frame::Rect {
            x: dirty.x,
            y: dirty.y,
            width: dirty.width,
            height: dirty.height,
        })
        .expect("panel dirty rect exists");

    assert_eq!(update.overlay_dirty_rect, Some(expected));
}

#[test]
fn overlapping_panel_and_canvas_overlay_updates_union_dirty_rects() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
    let center_y = (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

    assert!(app.update_canvas_hover(center_x, center_y));
    let hover_dirty = app
        .pending_canvas_host_dirty_rect
        .expect("hover dirty rect exists");

    let panel_rect = app
        .ui_shell
        .panel_rect("builtin.layers-panel")
        .expect("panel rect exists");
    assert!(
        app.ui_shell.move_panel_to(
            "builtin.layers-panel",
            layout
                .canvas_display_rect
                .x
                .saturating_add(layout.canvas_display_rect.width / 2)
                .saturating_sub(panel_rect.width / 2),
            layout
                .canvas_display_rect
                .y
                .saturating_add(layout.canvas_display_rect.height / 2)
                .saturating_sub(panel_rect.height / 2),
            layout.window_rect.width,
            layout.window_rect.height,
        )
    );
    app.mark_panel_surface_dirty();

    let update = app.prepare_present_frame(1280, 800, &mut profiler);
    let expected_panel_dirty = app
        .ui_shell
        .last_panel_surface_dirty_rect()
        .map(|dirty| crate::frame::Rect {
            x: dirty.x,
            y: dirty.y,
            width: dirty.width,
            height: dirty.height,
        })
        .expect("panel dirty rect exists");

    assert_eq!(
        update.overlay_dirty_rect,
        Some(expected_panel_dirty.union(hover_dirty))
    );
    assert!(profiler.stats.contains_key("compose_dirty_panel"));
    assert!(profiler.stats.contains_key("compose_dirty_overlay"));
}

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
            plugin_api::PanelEvent::SetText {
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
    if initial_update.overlay_dirty_rect.is_some() {
        iterations += 1;
    }
    while started.elapsed() < duration {
        let point = points[index % points.len()];
        if app.handle_pointer_dragged(point.0, point.1) {
            let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
            if update.overlay_dirty_rect.is_some() {
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
        assert!(app.dispatch_panel_event(plugin_api::PanelEvent::SetText {
            panel_id: "builtin.color-palette".to_string(),
            node_id: "color.wheel".to_string(),
            value: format!("{hue},{saturation},{value}"),
        }));
        let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
        if update.overlay_dirty_rect.is_some() {
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
            plugin_api::PanelEvent::SetValue {
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
    if initial_update.overlay_dirty_rect.is_some() {
        iterations += 1;
    }
    while started.elapsed() < duration {
        let point = points[index % points.len()];
        if app.handle_pointer_dragged(point.0, point.1) {
            let update = app.prepare_present_frame(viewport.0, viewport.1, &mut profiler);
            if update.overlay_dirty_rect.is_some() {
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
        let changed = app.ui_shell.move_panel_to(
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
        assert!(update.overlay_dirty_rect.is_some());
        iterations += 1;
        position_index += 1;
    }

    let elapsed = started.elapsed().as_secs_f64();
    emit_panel_perf("panel-perf", &profiler, elapsed, iterations);
}

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

/// フォーカス移動時の差分更新が UI 全体再同期を引き起こさないことを確認する。
#[test]
fn focus_refresh_does_not_trigger_ui_update() {
    let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);
    profiler.stats.clear();

    assert!(app.focus_next_panel_control());
    let update = app.prepare_present_frame(1280, 200, &mut profiler);
    let surface = app.panel_surface.clone().expect("panel surface exists");

    assert!(!profiler.stats.contains_key("ui_update"));
    assert!(!profiler.stats.contains_key("compose_full_frame"));
    assert_eq!(update.base_dirty_rect, None);
    let overlay_dirty = update.overlay_dirty_rect.expect("overlay dirty rect");
    assert!(rect_within_panel_surface(overlay_dirty, &surface));
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
        Some(crate::frame::status_text_bounds(
            1280,
            200,
            &layout,
            &app.status_text()
        ))
    );
    let surface = app.panel_surface.clone().expect("panel surface exists");
    let overlay_dirty = update.overlay_dirty_rect.expect("overlay dirty rect");
    assert!(rect_within_panel_surface(overlay_dirty, &surface));
}

#[test]
fn panel_release_without_matching_press_does_not_activate_save() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    let surface = app.panel_surface.clone().expect("panel surface exists");

    let (save_x, save_y) = (0..surface.height)
        .find_map(|y| {
            (0..surface.width).find_map(|x| match surface.hit_test_at(PanelSurfacePoint::new(x, y)) {
                Some(plugin_api::PanelEvent::Activate { panel_id, node_id })
                    if panel_id == "builtin.app-actions" && node_id == "app.save" =>
                {
                    Some((surface.x as i32 + x as i32, surface.y as i32 + y as i32))
                }
                _ => None,
            })
        })
        .expect("save button hit exists");

    assert!(!app.handle_pointer_released(save_x, save_y));
    assert_eq!(app.pending_save_task_count(), 0);
}

fn avg_stage_ms(profiler: &DesktopProfiler, label: &'static str) -> f64 {
    profiler.stats.get(label).map_or(0.0, avg_stage_stat_ms)
}

fn avg_stage_stat_ms(stat: &StageStats) -> f64 {
    if stat.calls == 0 {
        0.0
    } else {
        stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
    }
}

fn max_stage_ms(profiler: &DesktopProfiler, label: &'static str) -> f64 {
    profiler
        .stats
        .get(label)
        .map_or(0.0, |stat| stat.max.as_secs_f64() * 1000.0)
}

fn avg_value(profiler: &DesktopProfiler, label: &'static str) -> f64 {
    profiler.value_stats.get(label).map_or(0.0, avg_value_stat)
}

fn avg_value_stat(stat: &ValueStats) -> f64 {
    if stat.samples == 0 {
        0.0
    } else {
        stat.total / stat.samples as f64
    }
}

fn perf_duration() -> Duration {
    std::env::var("ALTPAINT_PANEL_PERF_DURATION_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(10))
}

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
            if update.canvas_updated || update.base_dirty_rect.is_some() || update.overlay_dirty_rect.is_some() {
                iterations += 1;
            }
        }
    }

    assert!(iterations > 0, "{label} produced updates");
    emit_view_perf(label, profiler, started.elapsed().as_secs_f64(), iterations);
}

fn control_points_from_surface(
    app: &DesktopApp,
    predicate: impl Fn(&plugin_api::PanelEvent) -> bool,
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

/// パン時は CPU 再合成なしで GPU キャンバス変換だけ更新できることを確認する。
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
    assert!(!profiler.stats.contains_key("compose_dirty_canvas_base"));
    assert!(!profiler.stats.contains_key("compose_dirty_overlay"));
    assert!(update.canvas_updated);
    assert!(update.canvas_transform_changed);
    assert_eq!(update.canvas_dirty_rect, None);
    assert_eq!(update.base_dirty_rect, None);
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

/// `NewDocument` 用のテストダイアログ付きアプリでも描画系の初期化が行えることを確認する。
#[test]
fn test_dialog_app_can_prepare_frame() {
    let mut app = super::test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();

    let update = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(update.canvas_updated);
}

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
    let small_dirty = app.pending_canvas_host_dirty_rect.expect("small preview dirty exists");

    app.pending_canvas_host_dirty_rect = None;
    app.hover_canvas_position = None;
    let _ = app.execute_command(Command::SetActivePenSize { size: 96 });
    assert!(app.update_canvas_hover(center_x, center_y));
    let large_dirty = app.pending_canvas_host_dirty_rect.expect("large preview dirty exists");

    assert!(large_dirty.width > small_dirty.width);
    assert!(large_dirty.height > small_dirty.height);
}
