//! フレーム区間と入力遅延を窓付きで集計する本体を保持する。

use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::time::{Duration, Instant};

use crate::config::{
    INPUT_LATENCY_TARGET_MS, INPUT_SAMPLING_TARGET_HZ, PERFORMANCE_SNAPSHOT_WINDOW, WINDOW_TITLE,
};

use super::types::{PerformanceSnapshot, PresentTimings, StageStats, ValueStats};

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
pub struct DesktopProfiler {
    logging_enabled: bool,
    pub stats: BTreeMap<&'static str, StageStats>,
    pub value_stats: BTreeMap<&'static str, ValueStats>,
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

impl Default for DesktopProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopProfiler {
    /// 現在時刻基準でプロファイラを初期化する。
    pub fn new() -> Self {
        Self::new_at(Instant::now())
    }

    /// 指定時刻基準でプロファイラを初期化する。
    pub fn new_at(now: Instant) -> Self {
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
    pub fn measure<T>(&mut self, label: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

    /// 既知ラベルへ経過時間を加算する。
    pub fn record(&mut self, label: &'static str, elapsed: Duration) {
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
    pub fn record_value(&mut self, label: &'static str, value: f64) {
        let stat = self.value_stats.entry(label).or_default();
        stat.samples += 1;
        stat.total += value;
        stat.max = stat.max.max(value);
    }

    /// 現在時刻でフレームを完了する。
    pub fn finish_frame(&mut self, elapsed: Duration) {
        self.finish_frame_at(elapsed, Instant::now());
    }

    /// 指定時刻でフレームを完了し、スナップショットを更新する。
    pub fn finish_frame_at(&mut self, elapsed: Duration, now: Instant) {
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
    pub fn record_present(&mut self, timings: PresentTimings) {
        self.record("present_upload", timings.upload);
        self.record("present_encode", timings.encode_and_submit);
        self.record("present_swap", timings.present);
        self.record("present_upload_base", timings.base_upload);
        self.record("present_upload_overlay", timings.overlay_upload);
        self.record("present_upload_canvas", timings.canvas_upload);
        self.record_value("present_upload_base_bytes", timings.base_upload_bytes as f64);
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
    pub fn record_canvas_input(&mut self) {
        self.record_canvas_input_at(Instant::now());
    }

    /// 指定時刻でキャンバス入力サンプルを記録する。
    pub fn record_canvas_input_at(&mut self, now: Instant) {
        self.pending_canvas_input_at = Some(now);
        self.recent_canvas_inputs.push_back(now);
        self.prune_recent_inputs(now);
    }

    /// 現在時刻でキャンバス提示完了を記録する。
    pub fn record_canvas_present(&mut self) {
        self.record_canvas_present_at(Instant::now());
    }

    /// 指定時刻でキャンバス提示完了を記録し、入力遅延を確定する。
    pub fn record_canvas_present_at(&mut self, now: Instant) {
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
    pub fn title_text(&self) -> String {
        self.latest_snapshot
            .map(|snapshot| snapshot.title_text())
            .unwrap_or_else(|| WINDOW_TITLE.to_string())
    }

    /// テストや外部観測向けに最新スナップショットを返す。
    pub fn latest_snapshot(&self) -> Option<PerformanceSnapshot> {
        self.latest_snapshot
    }

    fn prune_recent_frames(&mut self, now: Instant) {
        while let Some(sample) = self.recent_frames.front() {
            if now.duration_since(sample.finished_at) <= self.snapshot_window {
                break;
            }
            self.recent_frames.pop_front();
        }
    }

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

    fn build_snapshot(&self) -> Option<PerformanceSnapshot> {
        let frame_count = self.recent_frames.len();
        if frame_count == 0 {
            return None;
        }

        let totals = self.aggregate_recent_stages();
        let fps = self.window_rate(
            &self.recent_frames,
            |sample| sample.finished_at,
            self.latest_snapshot.map_or(0.0, |snapshot| snapshot.fps),
        );
        let canvas_sample_hz = self.window_rate(
            &self.recent_canvas_inputs,
            |sample| *sample,
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_sample_hz),
        );
        let canvas_present_hz = self.window_rate(
            &self.recent_canvas_presents,
            |sample| *sample,
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_present_hz),
        );
        let canvas_latency_ms = self.average_canvas_latency_ms();

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

    fn aggregate_recent_stages(&self) -> FrameStageTotals {
        let mut totals = FrameStageTotals::default();
        for sample in &self.recent_frames {
            totals.frame_total += sample.stages.frame_total;
            totals.prepare_frame += sample.stages.prepare_frame;
            totals.ui_update += sample.stages.ui_update;
            totals.panel_surface += sample.stages.panel_surface;
            totals.present_total += sample.stages.present_total;
        }
        totals
    }

    fn window_rate<T>(
        &self,
        samples: &VecDeque<T>,
        instant_of: impl Fn(&T) -> Instant,
        fallback: f64,
    ) -> f64 {
        if samples.len() < 2 {
            return fallback;
        }
        let first = instant_of(samples.front().expect("sample window is not empty"));
        let last = instant_of(samples.back().expect("sample window is not empty"));
        let span_secs = last.duration_since(first).as_secs_f64();
        if span_secs > 0.0 {
            (samples.len().saturating_sub(1)) as f64 / span_secs
        } else {
            fallback
        }
    }

    fn average_canvas_latency_ms(&self) -> f64 {
        if self.recent_canvas_latencies.is_empty() {
            self.latest_snapshot
                .map_or(0.0, |snapshot| snapshot.canvas_latency_ms)
        } else {
            self.recent_canvas_latencies
                .iter()
                .map(|sample| sample.latency.as_secs_f64() * 1000.0)
                .sum::<f64>()
                / self.recent_canvas_latencies.len() as f64
        }
    }

    fn average_ms(&self, label: &'static str) -> f64 {
        self.stats.get(label).map_or(0.0, |stat| {
            if stat.calls == 0 {
                0.0
            } else {
                stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
            }
        })
    }

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

    fn reset_interval(&mut self, now: Instant) {
        self.stats.clear();
        self.value_stats.clear();
        self.frames = 0;
        self.frame_interval_started = now;
        self.last_report = now;
    }
}
