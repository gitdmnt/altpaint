use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::time::{Duration, Instant};

use crate::{
    INPUT_LATENCY_TARGET_MS, INPUT_SAMPLING_TARGET_HZ, PERFORMANCE_SNAPSHOT_WINDOW,
    WINDOW_TITLE, wgpu_canvas::PresentTimings,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StageStats {
    pub(crate) calls: u64,
    pub(crate) total: Duration,
    pub(crate) max: Duration,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct PerformanceSnapshot {
    pub(crate) fps: f64,
    pub(crate) frame_ms: f64,
    pub(crate) prepare_ms: f64,
    pub(crate) ui_update_ms: f64,
    pub(crate) panel_surface_ms: f64,
    pub(crate) present_ms: f64,
    pub(crate) canvas_latency_ms: f64,
    pub(crate) canvas_sample_hz: f64,
}

impl PerformanceSnapshot {
    pub(crate) fn title_text(&self) -> String {
        let latency_marker = if self.canvas_latency_ms > 0.0
            && self.canvas_latency_ms <= INPUT_LATENCY_TARGET_MS
        {
            "ok"
        } else {
            "ng"
        };
        let sample_marker = if self.canvas_sample_hz >= INPUT_SAMPLING_TARGET_HZ {
            "ok"
        } else {
            "ng"
        };
        format!(
            "{WINDOW_TITLE} | {:>5.1} fps | frame {:>5.2}ms | prep {:>5.2}ms | ui {:>5.2}ms | panel {:>5.2}ms | present {:>5.2}ms | ink {:>5.2}ms {} | sample {:>6.1}Hz {}",
            self.fps,
            self.frame_ms,
            self.prepare_ms,
            self.ui_update_ms,
            self.panel_surface_ms,
            self.present_ms,
            self.canvas_latency_ms,
            latency_marker,
            self.canvas_sample_hz,
            sample_marker,
        )
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct FrameStageTotals {
    frame_total: Duration,
    prepare_frame: Duration,
    ui_update: Duration,
    panel_surface: Duration,
    present_total: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameSample {
    finished_at: Instant,
    stages: FrameStageTotals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimedLatencySample {
    recorded_at: Instant,
    latency: Duration,
}

/// レンダリング区間と入力レイテンシを窓付きで集計する軽量プロファイラ。
pub(crate) struct DesktopProfiler {
    logging_enabled: bool,
    pub(crate) stats: BTreeMap<&'static str, StageStats>,
    frames: u64,
    frame_interval_started: Instant,
    last_report: Instant,
    report_interval: Duration,
    snapshot_window: Duration,
    current_frame: FrameStageTotals,
    recent_frames: VecDeque<FrameSample>,
    recent_canvas_inputs: VecDeque<Instant>,
    recent_canvas_latencies: VecDeque<TimedLatencySample>,
    pending_canvas_input_at: Option<Instant>,
    latest_snapshot: Option<PerformanceSnapshot>,
}

impl DesktopProfiler {
    pub(crate) fn new() -> Self {
        Self::new_at(Instant::now())
    }

    pub(crate) fn new_at(now: Instant) -> Self {
        Self {
            logging_enabled: env::var_os("ALTPAINT_PROFILE").is_some(),
            stats: BTreeMap::new(),
            frames: 0,
            frame_interval_started: now,
            last_report: now,
            report_interval: Duration::from_secs(2),
            snapshot_window: PERFORMANCE_SNAPSHOT_WINDOW,
            current_frame: FrameStageTotals::default(),
            recent_frames: VecDeque::new(),
            recent_canvas_inputs: VecDeque::new(),
            recent_canvas_latencies: VecDeque::new(),
            pending_canvas_input_at: None,
            latest_snapshot: None,
        }
    }

    pub(crate) fn measure<T>(&mut self, label: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

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

    pub(crate) fn finish_frame(&mut self, elapsed: Duration) {
        self.finish_frame_at(elapsed, Instant::now());
    }

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

    pub(crate) fn record_present(&mut self, timings: PresentTimings) {
        self.record("present_upload", timings.upload);
        self.record("present_encode", timings.encode_and_submit);
        self.record("present_swap", timings.present);
    }

    pub(crate) fn record_canvas_input(&mut self) {
        self.record_canvas_input_at(Instant::now());
    }

    pub(crate) fn record_canvas_input_at(&mut self, now: Instant) {
        self.pending_canvas_input_at = Some(now);
        self.recent_canvas_inputs.push_back(now);
        self.prune_recent_inputs(now);
    }

    pub(crate) fn record_canvas_present(&mut self) {
        self.record_canvas_present_at(Instant::now());
    }

    pub(crate) fn record_canvas_present_at(&mut self, now: Instant) {
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

    pub(crate) fn title_text(&self) -> String {
        self.latest_snapshot
            .map(|snapshot| snapshot.title_text())
            .unwrap_or_else(|| WINDOW_TITLE.to_string())
    }

    #[cfg(test)]
    pub(crate) fn latest_snapshot(&self) -> Option<PerformanceSnapshot> {
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
            canvas_sample_hz,
        })
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
            "[profile] ---- last {:.2}s | fps={:.1} frame={:.3}ms prep={:.3}ms ui={:.3}ms panel={:.3}ms present={:.3}ms ink={:.3}ms target<={:.1}ms sample={:.1}Hz target>={:.1}Hz ----",
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
                .map_or(0.0, |snapshot| snapshot.canvas_sample_hz),
            INPUT_SAMPLING_TARGET_HZ,
        );
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
    }

    fn reset_interval(&mut self, now: Instant) {
        self.stats.clear();
        self.frames = 0;
        self.frame_interval_started = now;
        self.last_report = now;
    }
}
