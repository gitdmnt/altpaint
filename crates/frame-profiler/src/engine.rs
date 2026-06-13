//! フレーム区間と入力遅延を窓付きで集計する本体を保持する。

use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::time::{Duration, Instant};

use crate::types::{
    FrameStage, PerformanceSnapshot, PresentTimings, StageStats, ValueStats,
};

/// パフォーマンス表示を集計する既定の時間窓。
pub const PERFORMANCE_SNAPSHOT_WINDOW: Duration = Duration::from_millis(1000);
/// レポート出力の既定インターバル。
const REPORT_INTERVAL: Duration = Duration::from_secs(2);

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

/// レポート整形に必要な計測データを呼び出し側へ引き渡す純データ。
///
/// 文字列整形 (eprintln) は呼び出し側 (desktop) が担う。プロファイラ本体は
/// インターバル経過判定と統計の蓄積/リセットのみを行う。
#[derive(Debug, Clone)]
pub struct FrameReport {
    /// このレポート区間の経過秒数。
    pub interval_secs: f64,
    /// 区間内に集計したフレーム数。
    pub frames: u64,
    /// 最新スナップショット (存在すれば)。
    pub snapshot: Option<PerformanceSnapshot>,
    /// 計測ラベルごとの集計 (リセット前のスナップショット)。
    pub stats: BTreeMap<&'static str, StageStats>,
    /// 数値メトリクスの集計 (リセット前のスナップショット)。
    pub value_stats: BTreeMap<&'static str, ValueStats>,
}

/// レンダリング区間と入力レイテンシを窓付きで集計する軽量プロファイラ。
pub struct FrameProfiler {
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

impl Default for FrameProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameProfiler {
    pub fn new() -> Self {
        Self::new_at(Instant::now())
    }

    pub fn new_at(now: Instant) -> Self {
        Self {
            logging_enabled: env::var_os("ALTPAINT_PROFILE").is_some(),
            stats: BTreeMap::new(),
            value_stats: BTreeMap::new(),
            frames: 0,
            frame_interval_started: now,
            last_report: now,
            report_interval: REPORT_INTERVAL,
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

    /// `logging_enabled` を返す。レポート整形を呼び出し側で行うため、有効判定も
    /// 公開する (整形コストを掛けるかの判断に使う)。
    pub fn logging_enabled(&self) -> bool {
        self.logging_enabled
    }

    pub fn measure<T>(&mut self, label: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

    /// 自由形式ラベルの計測を記録する (レポート専用メトリクス)。
    ///
    /// フレーム集計対象の区間は `record_stage` を使う。
    pub fn record(&mut self, label: &'static str, elapsed: Duration) {
        Self::accumulate_stat(&mut self.stats, label, elapsed);
    }

    /// フレーム集計対象のステージ区間を記録する。
    ///
    /// `stats` へ正準ラベルで集計するとともに、フレーム合計へ加算する。
    pub fn record_stage(&mut self, stage: FrameStage, elapsed: Duration) {
        Self::accumulate_stat(&mut self.stats, stage.label(), elapsed);
        match stage {
            FrameStage::FrameTotal => self.current_frame.frame_total += elapsed,
            FrameStage::PrepareFrame => self.current_frame.prepare_frame += elapsed,
            FrameStage::UiUpdate => self.current_frame.ui_update += elapsed,
            FrameStage::PanelSurface => self.current_frame.panel_surface += elapsed,
            FrameStage::PresentTotal => self.current_frame.present_total += elapsed,
        }
    }

    fn accumulate_stat(
        stats: &mut BTreeMap<&'static str, StageStats>,
        label: &'static str,
        elapsed: Duration,
    ) {
        let stat = stats.entry(label).or_default();
        stat.calls += 1;
        stat.total += elapsed;
        stat.max = stat.max.max(elapsed);
    }

    pub fn record_value(&mut self, label: &'static str, value: f64) {
        let stat = self.value_stats.entry(label).or_default();
        stat.samples += 1;
        stat.total += value;
        stat.max = stat.max.max(value);
    }

    /// フレーム計測を確定する。レポートインターバルが経過していれば、整形用の
    /// `FrameReport` を返し (内部でインターバルをリセット)、呼び出し側が出力する。
    pub fn finish_frame(&mut self, elapsed: Duration) -> Option<FrameReport> {
        self.finish_frame_at(elapsed, Instant::now())
    }

    pub fn finish_frame_at(&mut self, elapsed: Duration, now: Instant) -> Option<FrameReport> {
        self.record_stage(FrameStage::FrameTotal, elapsed);
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
            let report = self.build_report(now);
            self.reset_interval(now);
            Some(report)
        } else {
            None
        }
    }

    pub fn record_present(&mut self, timings: PresentTimings) {
        self.record("present_upload", timings.upload);
        self.record("present_encode", timings.encode_and_submit);
        self.record("present_swap", timings.present);
        self.record("present_upload_canvas", timings.canvas_upload);
        self.record_value(
            "present_upload_canvas_bytes",
            timings.canvas_upload_bytes as f64,
        );
    }

    pub fn record_canvas_input(&mut self) {
        self.record_canvas_input_at(Instant::now());
    }

    pub fn record_canvas_input_at(&mut self, now: Instant) {
        self.pending_canvas_input_at = Some(now);
        self.recent_canvas_inputs.push_back(now);
        self.prune_recent_inputs(now);
    }

    pub fn record_canvas_present(&mut self) {
        self.record_canvas_present_at(Instant::now());
    }

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

    pub fn latest_snapshot(&self) -> Option<PerformanceSnapshot> {
        self.latest_snapshot
    }

    fn build_report(&self, now: Instant) -> FrameReport {
        FrameReport {
            interval_secs: now.duration_since(self.frame_interval_started).as_secs_f64(),
            frames: self.frames,
            snapshot: self.latest_snapshot,
            stats: self.stats.clone(),
            value_stats: self.value_stats.clone(),
        }
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

    fn reset_interval(&mut self, now: Instant) {
        self.stats.clear();
        self.value_stats.clear();
        self.frames = 0;
        self.frame_interval_started = now;
        self.last_report = now;
    }
}
