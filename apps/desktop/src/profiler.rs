//! フレーム時間とキャンバス入力レイテンシを集計する軽量プロファイラ。
//!
//! 実行時の計測責務を `runtime` から分離し、タイトル更新やテストが
//! 純粋な集計ロジックへ依存できるようにする。

use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::time::{Duration, Instant};

use crate::config::{
    INPUT_LATENCY_TARGET_MS, INPUT_SAMPLING_TARGET_HZ, PERFORMANCE_SNAPSHOT_WINDOW, WINDOW_TITLE,
};
use crate::wgpu_canvas::PresentTimings;

/// 計測ラベルごとの回数・合計・最大値を保持する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StageStats {
    pub(crate) calls: u64,
    pub(crate) total: Duration,
    pub(crate) max: Duration,
}

/// 数値メトリクスのサンプル数・合計・最大値を保持する。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct ValueStats {
    pub(crate) samples: u64,
    pub(crate) total: f64,
    pub(crate) max: f64,
}

/// ウィンドウタイトルへ表示する集計済みスナップショットを表す。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct PerformanceSnapshot {
    pub(crate) fps: f64,
    pub(crate) frame_ms: f64,
    pub(crate) prepare_ms: f64,
    pub(crate) ui_update_ms: f64,
    pub(crate) panel_surface_ms: f64,
    pub(crate) present_ms: f64,
    pub(crate) canvas_latency_ms: f64,
    pub(crate) canvas_present_hz: f64,
    pub(crate) canvas_sample_hz: f64,
}

impl PerformanceSnapshot {
    /// ウィンドウタイトル向けの短い表示文字列を生成する。
    pub(crate) fn title_text(&self) -> String {
        let latency_marker =
            if self.canvas_latency_ms > 0.0 && self.canvas_latency_ms <= INPUT_LATENCY_TARGET_MS {
                "ok"
            } else {
                "ng"
            };
        let sample_marker = if self.canvas_sample_hz >= INPUT_SAMPLING_TARGET_HZ {
            "ok"
        } else {
            "ng"
        };
        let present_marker = if self.canvas_present_hz >= INPUT_SAMPLING_TARGET_HZ {
            "ok"
        } else {
            "ng"
        };
        format!(
            "{WINDOW_TITLE} | {:>5.1} fps | frame {:>5.2}ms | prep {:>5.2}ms | ui {:>5.2}ms | panel {:>5.2}ms | present {:>5.2}ms | ink {:>5.2}ms {} | motion {:>6.1}Hz {} | input {:>6.1}Hz {}",
            self.fps,
            self.frame_ms,
            self.prepare_ms,
            self.ui_update_ms,
            self.panel_surface_ms,
            self.present_ms,
            self.canvas_latency_ms,
            latency_marker,
            self.canvas_present_hz,
            present_marker,
            self.canvas_sample_hz,
            sample_marker,
        )
    }
}

/// 単一フレームで集計した主要区間の合計時間を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct FrameStageTotals {
    frame_total: Duration,
    prepare_frame: Duration,
    ui_update: Duration,
    panel_surface: Duration,
    present_total: Duration,
}

/// スナップショット窓内に保持するフレーム計測サンプルを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameSample {
    finished_at: Instant,
    stages: FrameStageTotals,
}

/// キャンバス入力から表示までの遅延サンプルを時刻付きで保持する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimedLatencySample {
    recorded_at: Instant,
    latency: Duration,
}

/// レンダリング区間と入力レイテンシを窓付きで集計する軽量プロファイラ。
pub(crate) struct DesktopProfiler {
    logging_enabled: bool,
    pub(crate) stats: BTreeMap<&'static str, StageStats>,
    pub(crate) value_stats: BTreeMap<&'static str, ValueStats>,
    frames: u64,
    frame_interval_started: Instant,
    last_report: Instant,
    report_interval: Duration,
    snapshot_window: Duration,
    current_frame: FrameStageTotals,
    recent_frames: VecDeque<FrameSample>,
    recent_canvas_inputs: VecDeque<Instant>,
    recent_canvas_presents: VecDeque<Instant>,
    recent_canvas_latencies: VecDeque<TimedLatencySample>,
    pending_canvas_input_at: Option<Instant>,
    latest_snapshot: Option<PerformanceSnapshot>,
}

impl DesktopProfiler {
    /// 現在時刻基準でプロファイラを初期化する。
    pub(crate) fn new() -> Self {
        Self::new_at(Instant::now())
    }

    /// 指定時刻基準でプロファイラを初期化する。
    pub(crate) fn new_at(now: Instant) -> Self {
        Self {
            logging_enabled: env::var_os("ALTPAINT_PROFILE").is_some(),
            stats: BTreeMap::new(),
            value_stats: BTreeMap::new(),
            frames: 0,
            frame_interval_started: now,
            last_report: now,
            report_interval: Duration::from_secs(2),
            snapshot_window: PERFORMANCE_SNAPSHOT_WINDOW,
            current_frame: FrameStageTotals::default(),
            recent_frames: VecDeque::new(),
            recent_canvas_inputs: VecDeque::new(),
            recent_canvas_presents: VecDeque::new(),
            recent_canvas_latencies: VecDeque::new(),
            pending_canvas_input_at: None,
            latest_snapshot: None,
        }
    }

    /// ラベル付き計測としてクロージャを実行する。
    pub(crate) fn measure<T>(&mut self, label: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

    /// 既知ラベルへ経過時間を加算する。
    pub(crate) fn record(&mut self, label: &'static str, elapsed: Duration) {
        let stat = self.stats.entry(label).or_default();
        stat.calls += 1;
        stat.total += elapsed;
        stat.max = stat.max.max(elapsed);

        match label {
            "frame_total" => self.current_frame.frame_total += elapsed,
            "prepare_frame" => self.current_frame.prepare_frame += elapsed,
            "ui_update" => self.current_frame.ui_update += elapsed,
            "panel_surface" => self.current_frame.panel_surface += elapsed,
            "present_total" => self.current_frame.present_total += elapsed,
            _ => {}
        }
    }

    /// 数値メトリクスをサンプルとして記録する。
    pub(crate) fn record_value(&mut self, label: &'static str, value: f64) {
        let stat = self.value_stats.entry(label).or_default();
        stat.samples += 1;
        stat.total += value;
        stat.max = stat.max.max(value);
    }

    /// 現在時刻でフレームを完了する。
    pub(crate) fn finish_frame(&mut self, elapsed: Duration) {
        self.finish_frame_at(elapsed, Instant::now());
    }

    /// 指定時刻でフレームを完了し、スナップショットを更新する。
    pub(crate) fn finish_frame_at(&mut self, elapsed: Duration, now: Instant) {
        self.record("frame_total", elapsed);
        self.frames += 1;

        self.recent_frames.push_back(FrameSample {
            finished_at: now,
            stages: self.current_frame,
        });
        self.current_frame = FrameStageTotals::default();
        self.prune_recent_frames(now);

        if let Some(snapshot) = self.build_snapshot() {
            self.latest_snapshot = Some(snapshot);
        }

        if self.logging_enabled && now.duration_since(self.last_report) >= self.report_interval {
            self.print_report(now);
            self.reset_interval(now);
        }
    }

    /// GPU 提示の内訳をラベル別に記録する。
    pub(crate) fn record_present(&mut self, timings: PresentTimings) {
        self.record("present_upload", timings.upload);
        self.record("present_encode", timings.encode_and_submit);
        self.record("present_swap", timings.present);
        self.record("present_upload_base", timings.base_upload);
        self.record("present_upload_overlay", timings.overlay_upload);
        self.record("present_upload_canvas", timings.canvas_upload);
        self.record_value(
            "present_upload_base_bytes",
            timings.base_upload_bytes as f64,
        );
        self.record_value(
            "present_upload_overlay_bytes",
            timings.overlay_upload_bytes as f64,
        );
        self.record_value(
            "present_upload_canvas_bytes",
            timings.canvas_upload_bytes as f64,
        );
    }

    /// 現在時刻でキャンバス入力サンプルを記録する。
    pub(crate) fn record_canvas_input(&mut self) {
        self.record_canvas_input_at(Instant::now());
    }

    /// 指定時刻でキャンバス入力サンプルを記録する。
    pub(crate) fn record_canvas_input_at(&mut self, now: Instant) {
        self.pending_canvas_input_at = Some(now);
        self.recent_canvas_inputs.push_back(now);
        self.prune_recent_inputs(now);
    }

    /// 現在時刻でキャンバス提示完了を記録する。
    pub(crate) fn record_canvas_present(&mut self) {
        self.record_canvas_present_at(Instant::now());
    }

    /// 指定時刻でキャンバス提示完了を記録し、入力遅延を確定する。
    pub(crate) fn record_canvas_present_at(&mut self, now: Instant) {
        self.recent_canvas_presents.push_back(now);
        let Some(input_at) = self.pending_canvas_input_at.take() else {
            self.prune_recent_inputs(now);
            return;
        };

        self.recent_canvas_latencies.push_back(TimedLatencySample {
            recorded_at: now,
            latency: now.duration_since(input_at),
        });
        self.prune_recent_inputs(now);
    }

    /// 最新スナップショットからウィンドウタイトル文字列を返す。
    pub(crate) fn title_text(&self) -> String {
        self.latest_snapshot
            .map(|snapshot| snapshot.title_text())
            .unwrap_or_else(|| WINDOW_TITLE.to_string())
    }

    /// テスト用に最新スナップショットを返す。
    #[cfg(test)]
    pub(crate) fn latest_snapshot(&self) -> Option<PerformanceSnapshot> {
        self.latest_snapshot
    }

    /// フレームサンプル窓から期限切れ要素を削除する。
    fn prune_recent_frames(&mut self, now: Instant) {
        while let Some(sample) = self.recent_frames.front() {
            if now.duration_since(sample.finished_at) <= self.snapshot_window {
                break;
            }
            self.recent_frames.pop_front();
        }
    }

    /// 入力サンプル窓から期限切れ要素を削除する。
    fn prune_recent_inputs(&mut self, now: Instant) {
        while let Some(sample) = self.recent_canvas_inputs.front() {
            if now.duration_since(*sample) <= self.snapshot_window {
                break;
            }
            self.recent_canvas_inputs.pop_front();
        }

        while let Some(sample) = self.recent_canvas_presents.front() {
            if now.duration_since(*sample) <= self.snapshot_window {
                break;
            }
            self.recent_canvas_presents.pop_front();
        }

        while let Some(sample) = self.recent_canvas_latencies.front() {
            if now.duration_since(sample.recorded_at) <= self.snapshot_window {
                break;
            }
            self.recent_canvas_latencies.pop_front();
        }
    }

    /// 直近窓からスナップショットを再構築する。
    fn build_snapshot(&self) -> Option<PerformanceSnapshot> {
        let frame_count = self.recent_frames.len();
        if frame_count == 0 {
            return None;
        }

        let mut totals = FrameStageTotals::default();
        for sample in &self.recent_frames {
            totals.frame_total += sample.stages.frame_total;
            totals.prepare_frame += sample.stages.prepare_frame;
            totals.ui_update += sample.stages.ui_update;
            totals.panel_surface += sample.stages.panel_surface;
            totals.present_total += sample.stages.present_total;
        }

        let fps = if frame_count >= 2 {
            let first = self
                .recent_frames
                .front()
                .expect("recent frame window is not empty")
                .finished_at;
            let last = self
                .recent_frames
                .back()
                .expect("recent frame window is not empty")
                .finished_at;
            let span_secs = last.duration_since(first).as_secs_f64();
            if span_secs > 0.0 {
                (frame_count.saturating_sub(1)) as f64 / span_secs
            } else {
                self.latest_snapshot.map_or(0.0, |snapshot| snapshot.fps)
            }
        } else {
            self.latest_snapshot.map_or(0.0, |snapshot| snapshot.fps)
        };

        let canvas_sample_hz = if self.recent_canvas_inputs.len() >= 2 {
            let first = self
                .recent_canvas_inputs
                .front()
                .expect("recent canvas input window is not empty");
            let last = self
                .recent_canvas_inputs
                .back()
                .expect("recent canvas input window is not empty");
            let span_secs = last.duration_since(*first).as_secs_f64();
            if span_secs > 0.0 {
                (self.recent_canvas_inputs.len().saturating_sub(1)) as f64 / span_secs
            } else {
                self.latest_snapshot
                    .map_or(0.0, |snapshot| snapshot.canvas_sample_hz)
            }
        } else {
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_sample_hz)
        };

        let canvas_present_hz = if self.recent_canvas_presents.len() >= 2 {
            let first = self
                .recent_canvas_presents
                .front()
                .expect("recent canvas present window is not empty");
            let last = self
                .recent_canvas_presents
                .back()
                .expect("recent canvas present window is not empty");
            let span_secs = last.duration_since(*first).as_secs_f64();
            if span_secs > 0.0 {
                (self.recent_canvas_presents.len().saturating_sub(1)) as f64 / span_secs
            } else {
                self.latest_snapshot
                    .map_or(0.0, |snapshot| snapshot.canvas_present_hz)
            }
        } else {
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_present_hz)
        };

        let canvas_latency_ms = if self.recent_canvas_latencies.is_empty() {
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_latency_ms)
        } else {
            self.recent_canvas_latencies
                .iter()
                .map(|sample| sample.latency.as_secs_f64() * 1000.0)
                .sum::<f64>()
                / self.recent_canvas_latencies.len() as f64
        };

        Some(PerformanceSnapshot {
            fps,
            frame_ms: totals.frame_total.as_secs_f64() * 1000.0 / frame_count as f64,
            prepare_ms: totals.prepare_frame.as_secs_f64() * 1000.0 / frame_count as f64,
            ui_update_ms: totals.ui_update.as_secs_f64() * 1000.0 / frame_count as f64,
            panel_surface_ms: totals.panel_surface.as_secs_f64() * 1000.0 / frame_count as f64,
            present_ms: totals.present_total.as_secs_f64() * 1000.0 / frame_count as f64,
            canvas_latency_ms,
            canvas_present_hz,
            canvas_sample_hz,
        })
    }

    /// ラベル別の平均時間をミリ秒で返す。
    fn average_ms(&self, label: &'static str) -> f64 {
        self.stats.get(label).map_or(0.0, |stat| {
            if stat.calls == 0 {
                0.0
            } else {
                stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
            }
        })
    }

    /// 環境変数有効時に標準エラーへ集計レポートを出力する。
    fn print_report(&self, now: Instant) {
        let interval_secs = now
            .duration_since(self.frame_interval_started)
            .as_secs_f64()
            .max(f64::EPSILON);
        eprintln!(
            "[profile] ---- last {:.2}s | fps={:.1} frame={:.3}ms prep={:.3}ms ui={:.3}ms panel={:.3}ms present={:.3}ms ink={:.3}ms target<={:.1}ms motion={:.1}Hz target>={:.1}Hz input={:.1}Hz target>={:.1}Hz ----",
            now.duration_since(self.frame_interval_started)
                .as_secs_f64(),
            self.frames as f64 / interval_secs,
            self.average_ms("frame_total"),
            self.average_ms("prepare_frame"),
            self.average_ms("ui_update"),
            self.average_ms("panel_surface"),
            self.average_ms("present_total"),
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_latency_ms),
            INPUT_LATENCY_TARGET_MS,
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_present_hz),
            INPUT_SAMPLING_TARGET_HZ,
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_sample_hz),
            INPUT_SAMPLING_TARGET_HZ,
        );
        if let (Some(window_events), Some(raw_events), Some(dispatches)) = (
            self.stats.get("canvas_input_window_event"),
            self.stats.get("canvas_input_raw_event"),
            self.stats.get("canvas_input_dispatch"),
        ) {
            let wheel_events = self
                .stats
                .get("canvas_input_wheel_event")
                .map_or(0, |stat| stat.calls);
            eprintln!(
                "[profile] input sources window={} raw={} wheel={} dispatch={}",
                window_events.calls, raw_events.calls, wheel_events, dispatches.calls,
            );
        }
        for (label, stat) in &self.stats {
            let avg = if stat.calls == 0 {
                0.0
            } else {
                stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
            };
            eprintln!(
                "[profile] {:>18} calls={:>5} avg={:>8.3}ms max={:>8.3}ms total={:>8.3}ms",
                label,
                stat.calls,
                avg,
                stat.max.as_secs_f64() * 1000.0,
                stat.total.as_secs_f64() * 1000.0,
            );
        }
        for (label, stat) in &self.value_stats {
            let avg = if stat.samples == 0 {
                0.0
            } else {
                stat.total / stat.samples as f64
            };
            eprintln!(
                "[profile] {:>18} samples={:>5} avg={:>10.1} max={:>10.1}",
                label, stat.samples, avg, stat.max,
            );
        }
    }

    /// ログ用集計区間をリセットする。
    fn reset_interval(&mut self, now: Instant) {
        self.stats.clear();
        self.value_stats.clear();
        self.frames = 0;
        self.frame_interval_started = now;
        self.last_report = now;
    }
}

#[cfg(test)]
mod tests {
    //! プロファイラ集計ロジックの回帰テストをまとめる。

    use super::*;

    /// タイトル文字列へ主要指標が埋め込まれることを確認する。
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

    /// スナップショット fps が直近窓のフレームから計算されることを確認する。
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

    /// キャンバス入力レイテンシとサンプル周波数が計算されることを確認する。
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

    /// アイドルギャップ後も fps が直近窓基準で維持されることを確認する。
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
}
