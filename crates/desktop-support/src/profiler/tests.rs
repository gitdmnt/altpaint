//! プロファイラ集計ロジックの回帰テストをまとめる。

use std::time::{Duration, Instant};

use super::{DesktopProfiler, PerformanceSnapshot};

/// performance スナップショット formats ウィンドウ title が期待どおりに動作することを検証する。
#[test]
fn performance_snapshot_formats_window_title() {
    let title = PerformanceSnapshot {
        fps: 59.8,
        frame_ms: 16.72,
        prepare_ms: 3.11,
        ui_update_ms: 0.42,
        panel_surface_ms: 0.77,
        present_ms: 1.26,
        canvas_latency_ms: 8.40,
        canvas_present_hz: 144.0,
        canvas_sample_hz: 123.4,
    }
    .title_text();

    assert!(title.contains("59.8 fps"));
    assert!(title.contains("prep  3.11ms"));
    assert!(title.contains("ui  0.42ms"));
    assert!(title.contains("ink  8.40ms ok"));
    assert!(title.contains("motion  144.0Hz ok"));
    assert!(title.contains("input  123.4Hz ok"));
}

/// profiler uses recent ウィンドウ for スナップショット fps が期待どおりに動作することを検証する。
#[test]
fn profiler_uses_recent_window_for_snapshot_fps() {
    let start = Instant::now();
    let mut profiler = DesktopProfiler::new_at(start);

    profiler.record("prepare_frame", Duration::from_millis(2));
    profiler.record("ui_update", Duration::from_millis(1));
    profiler.record("panel_surface", Duration::from_millis(1));
    profiler.record("present_total", Duration::from_millis(2));
    profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_millis(0));

    profiler.record("prepare_frame", Duration::from_millis(2));
    profiler.record("ui_update", Duration::from_millis(1));
    profiler.record("panel_surface", Duration::from_millis(1));
    profiler.record("present_total", Duration::from_millis(2));
    profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_millis(16));

    profiler.record("prepare_frame", Duration::from_millis(2));
    profiler.record("ui_update", Duration::from_millis(1));
    profiler.record("panel_surface", Duration::from_millis(1));
    profiler.record("present_total", Duration::from_millis(2));
    profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_millis(32));

    let snapshot = profiler.latest_snapshot().expect("snapshot exists");
    assert!(snapshot.fps > 60.0);
    assert!(snapshot.fps < 65.0);
}

/// profiler tracks キャンバス latency and sampling rate が期待どおりに動作することを検証する。
#[test]
fn profiler_tracks_canvas_latency_and_sampling_rate() {
    let start = Instant::now();
    let mut profiler = DesktopProfiler::new_at(start);

    for offset_ms in [0_u64, 8, 16] {
        let input_at = start + Duration::from_millis(offset_ms);
        let present_at = input_at + Duration::from_millis(8);

        profiler.record_canvas_input_at(input_at);
        profiler.record("prepare_frame", Duration::from_millis(2));
        profiler.record("ui_update", Duration::from_millis(1));
        profiler.record("panel_surface", Duration::from_millis(1));
        profiler.record("present_total", Duration::from_millis(2));
        profiler.record_canvas_present_at(present_at);
        profiler.finish_frame_at(Duration::from_millis(8), present_at);
    }

    let snapshot = profiler.latest_snapshot().expect("snapshot exists");
    assert!(snapshot.canvas_latency_ms >= 8.0);
    assert!(snapshot.canvas_latency_ms < 9.0);
    assert!(snapshot.canvas_present_hz >= 120.0);
    assert!(snapshot.canvas_present_hz < 130.0);
    assert!(snapshot.canvas_sample_hz >= 120.0);
    assert!(snapshot.canvas_sample_hz < 130.0);
}

/// profiler does not drop to one fps after idle gap が期待どおりに動作することを検証する。
#[test]
fn profiler_does_not_drop_to_one_fps_after_idle_gap() {
    let start = Instant::now();
    let mut profiler = DesktopProfiler::new_at(start);

    for offset_ms in [0_u64, 16, 32, 48] {
        profiler.record("prepare_frame", Duration::from_millis(2));
        profiler.record("ui_update", Duration::from_millis(1));
        profiler.record("panel_surface", Duration::from_millis(1));
        profiler.record("present_total", Duration::from_millis(2));
        profiler.finish_frame_at(
            Duration::from_millis(16),
            start + Duration::from_millis(offset_ms),
        );
    }

    let fps_before_idle = profiler.latest_snapshot().expect("snapshot exists").fps;

    profiler.record("prepare_frame", Duration::from_millis(2));
    profiler.record("ui_update", Duration::from_millis(1));
    profiler.record("panel_surface", Duration::from_millis(1));
    profiler.record("present_total", Duration::from_millis(2));
    profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_secs(3));

    let fps_after_idle = profiler.latest_snapshot().expect("snapshot exists").fps;
    assert!(fps_before_idle > 50.0);
    assert!(fps_after_idle > 50.0);
}
