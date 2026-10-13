//! プロファイラ集計ロジックの回帰テストをまとめる。

use std::time::{Duration, Instant};

use crate::FrameProfiler;
use crate::types::FrameStage;

#[test]
fn profiler_uses_recent_window_for_snapshot_fps() {
    let start = Instant::now();
    let mut profiler = FrameProfiler::new_at(start);

    profiler.record_stage(FrameStage::PrepareFrame, Duration::from_millis(2));
    profiler.record_stage(FrameStage::UiUpdate, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PanelSurface, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PresentTotal, Duration::from_millis(2));
    let _ = profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_millis(0));

    profiler.record_stage(FrameStage::PrepareFrame, Duration::from_millis(2));
    profiler.record_stage(FrameStage::UiUpdate, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PanelSurface, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PresentTotal, Duration::from_millis(2));
    let _ = profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_millis(16));

    profiler.record_stage(FrameStage::PrepareFrame, Duration::from_millis(2));
    profiler.record_stage(FrameStage::UiUpdate, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PanelSurface, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PresentTotal, Duration::from_millis(2));
    let _ = profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_millis(32));

    let snapshot = profiler.latest_snapshot().expect("snapshot exists");
    assert!(snapshot.fps > 60.0);
    assert!(snapshot.fps < 65.0);
}

#[test]
fn profiler_tracks_canvas_latency_and_sampling_rate() {
    let start = Instant::now();
    let mut profiler = FrameProfiler::new_at(start);

    for offset_ms in [0_u64, 8, 16] {
        let input_at = start + Duration::from_millis(offset_ms);
        let present_at = input_at + Duration::from_millis(8);

        profiler.record_canvas_input_at(input_at);
        profiler.record_stage(FrameStage::PrepareFrame, Duration::from_millis(2));
        profiler.record_stage(FrameStage::UiUpdate, Duration::from_millis(1));
        profiler.record_stage(FrameStage::PanelSurface, Duration::from_millis(1));
        profiler.record_stage(FrameStage::PresentTotal, Duration::from_millis(2));
        profiler.record_canvas_present_at(present_at);
        let _ = profiler.finish_frame_at(Duration::from_millis(8), present_at);
    }

    let snapshot = profiler.latest_snapshot().expect("snapshot exists");
    assert!(snapshot.canvas_latency_ms >= 8.0);
    assert!(snapshot.canvas_latency_ms < 9.0);
    assert!(snapshot.canvas_present_hz >= 120.0);
    assert!(snapshot.canvas_present_hz < 130.0);
    assert!(snapshot.canvas_sample_hz >= 120.0);
    assert!(snapshot.canvas_sample_hz < 130.0);
}

#[test]
fn profiler_does_not_drop_to_one_fps_after_idle_gap() {
    let start = Instant::now();
    let mut profiler = FrameProfiler::new_at(start);

    for offset_ms in [0_u64, 16, 32, 48] {
        profiler.record_stage(FrameStage::PrepareFrame, Duration::from_millis(2));
        profiler.record_stage(FrameStage::UiUpdate, Duration::from_millis(1));
        profiler.record_stage(FrameStage::PanelSurface, Duration::from_millis(1));
        profiler.record_stage(FrameStage::PresentTotal, Duration::from_millis(2));
        let _ = profiler.finish_frame_at(
            Duration::from_millis(16),
            start + Duration::from_millis(offset_ms),
        );
    }

    let fps_before_idle = profiler.latest_snapshot().expect("snapshot exists").fps;

    profiler.record_stage(FrameStage::PrepareFrame, Duration::from_millis(2));
    profiler.record_stage(FrameStage::UiUpdate, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PanelSurface, Duration::from_millis(1));
    profiler.record_stage(FrameStage::PresentTotal, Duration::from_millis(2));
    let _ = profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_secs(3));

    let fps_after_idle = profiler.latest_snapshot().expect("snapshot exists").fps;
    assert!(fps_before_idle > 50.0);
    assert!(fps_after_idle > 50.0);
}
