//! `desktop` は最小のデスクトップエントリポイント。
//!
//! `winit` がウィンドウと入力を受け持ち、`wgpu` が合成済みフレームを提示する。

mod canvas_bridge;
mod wgpu_canvas;

use anyhow::{Context, Result};
use app_core::{Command, DirtyRect, Document};
use canvas_bridge::{
    CanvasInputState, CanvasPointerEvent, command_for_canvas_gesture, map_view_to_canvas,
};
use plugin_api::{HostAction, PanelEvent};
use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{load_document_from_path, save_document_to_path};
use ui_shell::{PanelSurface, UiShell, draw_text_rgba};
use wgpu_canvas::{PresentTimings, UploadRegion, WgpuPresenter};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_PROJECT_PATH: &str = "altpaint-project.altp.json";
const WINDOW_TITLE: &str = "altpaint";
const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 800;
const SIDEBAR_WIDTH: usize = 280;
const WINDOW_PADDING: usize = 8;
const HEADER_HEIGHT: usize = 24;
const FOOTER_HEIGHT: usize = 24;
const APP_BACKGROUND: [u8; 4] = [0x18, 0x18, 0x18, 0xff];
const SIDEBAR_BACKGROUND: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
const PANEL_FRAME_BACKGROUND: [u8; 4] = [0x1f, 0x1f, 0x1f, 0xff];
const PANEL_FRAME_BORDER: [u8; 4] = [0x3f, 0x3f, 0x3f, 0xff];
const CANVAS_BACKGROUND: [u8; 4] = [0x60, 0x60, 0x60, 0xff];
const CANVAS_FRAME_BACKGROUND: [u8; 4] = [0x40, 0x40, 0x40, 0xff];
const CANVAS_FRAME_BORDER: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
const TEXT_PRIMARY: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
const TEXT_SECONDARY: [u8; 4] = [0xd8, 0xd8, 0xd8, 0xff];
const PERFORMANCE_SNAPSHOT_WINDOW: Duration = Duration::from_millis(1000);
const INPUT_LATENCY_TARGET_MS: f64 = 10.0;
const INPUT_SAMPLING_TARGET_HZ: f64 = 120.0;

fn default_panel_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    let mut runtime = DesktopRuntime::new(PathBuf::from(DEFAULT_PROJECT_PATH));
    event_loop
        .run_app(&mut runtime)
        .context("failed to run desktop runtime")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl Rect {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x as i32
            && y >= self.y as i32
            && x < (self.x + self.width) as i32
            && y < (self.y + self.height) as i32
    }

    fn union(&self, other: Rect) -> Rect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        Rect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }

    fn intersect(&self, other: Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        if left >= right || top >= bottom {
            return None;
        }

        Some(Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PresentFrameUpdate {
    dirty_rect: Option<Rect>,
    canvas_updated: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct StageStats {
    calls: u64,
    total: Duration,
    max: Duration,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct PerformanceSnapshot {
    fps: f64,
    frame_ms: f64,
    prepare_ms: f64,
    ui_update_ms: f64,
    panel_surface_ms: f64,
    present_ms: f64,
    canvas_latency_ms: f64,
    canvas_sample_hz: f64,
}

impl PerformanceSnapshot {
    fn title_text(&self) -> String {
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

struct DesktopProfiler {
    logging_enabled: bool,
    stats: BTreeMap<&'static str, StageStats>,
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
    fn new() -> Self {
        Self::new_at(Instant::now())
    }

    fn new_at(now: Instant) -> Self {
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

    fn measure<T>(&mut self, label: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

    fn record(&mut self, label: &'static str, elapsed: Duration) {
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

    fn finish_frame(&mut self, elapsed: Duration) {
        self.finish_frame_at(elapsed, Instant::now());
    }

    fn finish_frame_at(&mut self, elapsed: Duration, now: Instant) {
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

    fn record_present(&mut self, timings: PresentTimings) {
        self.record("present_upload", timings.upload);
        self.record("present_encode", timings.encode_and_submit);
        self.record("present_swap", timings.present);
    }

    fn record_canvas_input(&mut self) {
        self.record_canvas_input_at(Instant::now());
    }

    fn record_canvas_input_at(&mut self, now: Instant) {
        self.pending_canvas_input_at = Some(now);
        self.recent_canvas_inputs.push_back(now);
        self.prune_recent_inputs(now);
    }

    fn record_canvas_present(&mut self) {
        self.record_canvas_present_at(Instant::now());
    }

    fn record_canvas_present_at(&mut self, now: Instant) {
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

    fn title_text(&self) -> String {
        self.latest_snapshot
            .map(|snapshot| snapshot.title_text())
            .unwrap_or_else(|| WINDOW_TITLE.to_string())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopLayout {
    panel_host_rect: Rect,
    panel_surface_rect: Rect,
    canvas_host_rect: Rect,
    canvas_display_rect: Rect,
}

impl DesktopLayout {
    fn new(
        window_width: usize,
        window_height: usize,
        canvas_width: usize,
        canvas_height: usize,
    ) -> Self {
        let sidebar_width = SIDEBAR_WIDTH.min(window_width);
        let sidebar_inner_width = sidebar_width.saturating_sub(WINDOW_PADDING * 2).max(1);
        let panel_host_rect = Rect {
            x: WINDOW_PADDING,
            y: WINDOW_PADDING + HEADER_HEIGHT + WINDOW_PADDING,
            width: sidebar_inner_width,
            height: window_height
                .saturating_sub(HEADER_HEIGHT)
                .saturating_sub(FOOTER_HEIGHT)
                .saturating_sub(WINDOW_PADDING * 3)
                .max(1),
        };
        let panel_surface_rect = panel_host_rect;

        let canvas_host_rect = Rect {
            x: sidebar_width + WINDOW_PADDING,
            y: WINDOW_PADDING + HEADER_HEIGHT + WINDOW_PADDING,
            width: window_width
                .saturating_sub(sidebar_width)
                .saturating_sub(WINDOW_PADDING * 2)
                .max(1),
            height: window_height
                .saturating_sub(HEADER_HEIGHT)
                .saturating_sub(FOOTER_HEIGHT)
                .saturating_sub(WINDOW_PADDING * 3)
                .max(1),
        };
        let canvas_display_rect =
            fit_rect(canvas_width.max(1), canvas_height.max(1), canvas_host_rect);

        Self {
            panel_host_rect,
            panel_surface_rect,
            canvas_host_rect,
            canvas_display_rect,
        }
    }
}

struct CanvasCompositeSource<'a> {
    width: usize,
    height: usize,
    pixels: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelDragState {
    panel_id: String,
    node_id: String,
}

struct DesktopApp {
    document: Document,
    ui_shell: UiShell,
    project_path: PathBuf,
    canvas_input: CanvasInputState,
    panel_surface: Option<PanelSurface>,
    layout: Option<DesktopLayout>,
    present_frame: Option<render::RenderFrame>,
    pending_canvas_dirty_rect: Option<DirtyRect>,
    active_panel_drag: Option<PanelDragState>,
    needs_ui_sync: bool,
    needs_panel_surface_refresh: bool,
    needs_status_refresh: bool,
    needs_full_present_rebuild: bool,
}

impl DesktopApp {
    fn new(project_path: PathBuf) -> Self {
        let document = load_document_from_path(&project_path).unwrap_or_default();
        let mut ui_shell = UiShell::new();
        let _ = ui_shell.load_panel_directory(default_panel_dir());
        ui_shell.update(&document);

        Self {
            document,
            ui_shell,
            project_path,
            canvas_input: CanvasInputState::default(),
            panel_surface: None,
            layout: None,
            present_frame: None,
            pending_canvas_dirty_rect: None,
            active_panel_drag: None,
            needs_ui_sync: true,
            needs_panel_surface_refresh: true,
            needs_status_refresh: false,
            needs_full_present_rebuild: true,
        }
    }

    fn prepare_present_frame(
        &mut self,
        window_width: usize,
        window_height: usize,
        profiler: &mut DesktopProfiler,
    ) -> PresentFrameUpdate {
        let (canvas_width, canvas_height) = self.canvas_dimensions();
        let next_layout = profiler.measure("layout", || {
            DesktopLayout::new(window_width, window_height, canvas_width, canvas_height)
        });

        if self.layout.as_ref() != Some(&next_layout) {
            self.layout = Some(next_layout.clone());
            self.needs_panel_surface_refresh = true;
            self.needs_full_present_rebuild = true;
        }

        if self.needs_ui_sync {
            profiler.measure("ui_update", || self.ui_shell.update(&self.document));
            self.needs_ui_sync = false;
        }

        let mut panel_surface_refreshed = false;
        if self.needs_panel_surface_refresh {
            let panel_surface_size = self
                .layout
                .as_ref()
                .map(|layout| {
                    (
                        layout.panel_surface_rect.width,
                        layout.panel_surface_rect.height,
                    )
                })
                .unwrap_or((1, 1));
            let panel_surface = profiler.measure("panel_surface", || {
                self.ui_shell
                    .render_panel_surface(panel_surface_size.0, panel_surface_size.1)
            });
            self.panel_surface = Some(panel_surface);
            self.needs_panel_surface_refresh = false;
            panel_surface_refreshed = true;
        }

        if self.needs_full_present_rebuild || self.present_frame.is_none() {
            let layout = self.layout.clone().expect("layout exists");
            let panel_surface = self.panel_surface.clone().unwrap_or_else(|| {
                self.ui_shell.render_panel_surface(
                    layout.panel_surface_rect.width,
                    layout.panel_surface_rect.height,
                )
            });
            let status_text = self.status_text();
            let bitmap = self.document.active_bitmap();
            let present_frame = profiler.measure("compose_full_frame", || {
                compose_desktop_frame(
                    window_width,
                    window_height,
                    &layout,
                    &panel_surface,
                    CanvasCompositeSource {
                        width: bitmap.map_or(1, |bitmap| bitmap.width),
                        height: bitmap.map_or(1, |bitmap| bitmap.height),
                        pixels: bitmap.map_or(&[][..], |bitmap| bitmap.pixels.as_slice()),
                    },
                    &status_text,
                )
            });
            self.present_frame = Some(present_frame);
            self.pending_canvas_dirty_rect = None;
            self.needs_status_refresh = false;
            self.needs_full_present_rebuild = false;
            return PresentFrameUpdate {
                dirty_rect: None,
                canvas_updated: true,
            };
        }

        let layout = self.layout.clone().expect("layout exists");
        let status_text = self.needs_status_refresh.then(|| self.status_text());
        let Some(present_frame) = self.present_frame.as_mut() else {
            self.needs_full_present_rebuild = true;
            return PresentFrameUpdate {
                dirty_rect: None,
                canvas_updated: false,
            };
        };

        let mut dirty_rect = None;
        let mut canvas_updated = false;
        if panel_surface_refreshed && let Some(panel_surface) = self.panel_surface.as_ref() {
            profiler.measure("compose_dirty_panel", || {
                compose_panel_host_region(present_frame, &layout, panel_surface);
            });
            dirty_rect = Some(layout.panel_host_rect);
        }

        if let Some(status_text) = status_text.as_deref() {
            let status_rect = status_text_rect(window_width, window_height, &layout);
            profiler.measure("compose_dirty_status", || {
                compose_status_region(
                    present_frame,
                    window_width,
                    window_height,
                    &layout,
                    status_text,
                );
            });
            dirty_rect =
                Some(dirty_rect.map_or(status_rect, |existing| existing.union(status_rect)));
            self.needs_status_refresh = false;
        }

        if let Some(dirty) = self.pending_canvas_dirty_rect.take() {
            let Some(bitmap) = self.document.active_bitmap() else {
                self.needs_full_present_rebuild = true;
                return PresentFrameUpdate {
                    dirty_rect: None,
                    canvas_updated: false,
                };
            };
            let canvas_dirty_rect = map_canvas_dirty_to_display(
                dirty,
                layout.canvas_display_rect,
                bitmap.width,
                bitmap.height,
            );
            profiler.measure("compose_dirty_canvas", || {
                blit_scaled_rgba_region(
                    present_frame,
                    layout.canvas_display_rect,
                    bitmap.width,
                    bitmap.height,
                    bitmap.pixels.as_slice(),
                    Some(canvas_dirty_rect),
                );
            });
            canvas_updated = true;
            dirty_rect = Some(dirty_rect.map_or(canvas_dirty_rect, |existing| {
                existing.union(canvas_dirty_rect)
            }));
        }

        PresentFrameUpdate {
            dirty_rect,
            canvas_updated,
        }
    }

    fn present_frame(&self) -> Option<&render::RenderFrame> {
        self.present_frame.as_ref()
    }

    fn handle_pointer_pressed(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_position_from_window(x, y).is_some() {
            return self.handle_canvas_pointer("down", x, y);
        }

        self.begin_panel_interaction(x, y)
    }

    fn handle_pointer_released(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("up", x, y);
        }
        if self.active_panel_drag.take().is_some() {
            return false;
        }
        self.handle_panel_pointer(x, y)
    }

    fn handle_pointer_dragged(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("drag", x, y);
        }

        if self.active_panel_drag.is_some() {
            return self.drag_panel_interaction(x, y);
        }

        false
    }

    fn begin_panel_interaction(&mut self, x: i32, y: i32) -> bool {
        let Some(event) = self.panel_event_from_window(x, y) else {
            self.active_panel_drag = None;
            return false;
        };

        match &event {
            PanelEvent::SetValue {
                panel_id, node_id, ..
            } => {
                self.active_panel_drag = Some(PanelDragState {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                });
                self.dispatch_panel_event(event)
            }
            PanelEvent::Activate { .. } => false,
        }
    }

    fn drag_panel_interaction(&mut self, x: i32, y: i32) -> bool {
        let Some(state) = self.active_panel_drag.clone() else {
            return false;
        };
        let Some(event) = self.panel_drag_event_from_window(&state, x, y) else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    fn dispatch_panel_event(&mut self, event: PanelEvent) -> bool {
        let mut changed = false;
        if let PanelEvent::Activate { panel_id, node_id } = &event {
            changed |= self.ui_shell.focus_panel_node(panel_id, node_id);
        }

        self.needs_panel_surface_refresh = true;
        let mut needs_redraw = true;

        for action in self.ui_shell.handle_panel_event(&event) {
            needs_redraw |= self.execute_host_action(action);
        }

        changed || needs_redraw
    }

    fn handle_panel_pointer(&mut self, x: i32, y: i32) -> bool {
        let Some(event) = self.panel_event_from_window(x, y) else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    fn focus_next_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_next();
        if changed {
            self.needs_panel_surface_refresh = true;
        }
        changed
    }

    fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_previous();
        if changed {
            self.needs_panel_surface_refresh = true;
        }
        changed
    }

    fn activate_focused_panel_control(&mut self) -> Option<Command> {
        let actions = self.ui_shell.activate_focused();
        let mut dispatched = None;
        for action in actions {
            if let HostAction::DispatchCommand(command) = &action
                && dispatched.is_none()
            {
                dispatched = Some(command.clone());
            }
            let _ = self.execute_host_action(action);
        }
        dispatched
    }

    fn scroll_panel_surface(&mut self, delta_lines: i32) -> bool {
        let viewport_height = self
            .layout
            .as_ref()
            .map(|layout| layout.panel_surface_rect.height)
            .unwrap_or(0);
        if viewport_height == 0 {
            return false;
        }

        let changed = self.ui_shell.scroll_panels(delta_lines, viewport_height);
        if changed {
            self.needs_panel_surface_refresh = true;
        }
        changed
    }

    fn handle_canvas_pointer(&mut self, action: &str, x: i32, y: i32) -> bool {
        let Some((canvas_x, canvas_y)) = self.canvas_position_from_window(x, y) else {
            if action == "up" {
                self.canvas_input = CanvasInputState::default();
            }
            return false;
        };

        match action {
            "down" => {
                self.canvas_input.is_drawing = true;
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
                self.execute_canvas_command(canvas_x, canvas_y, None)
            }
            "drag" if self.canvas_input.is_drawing => {
                let from = self.canvas_input.last_position;
                let changed = self.execute_canvas_command(canvas_x, canvas_y, from);
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
                changed
            }
            "up" => {
                self.canvas_input.is_drawing = false;
                self.canvas_input.last_position = None;
                false
            }
            _ => false,
        }
    }

    fn execute_canvas_command(&mut self, x: usize, y: usize, from: Option<(usize, usize)>) -> bool {
        let command = command_for_canvas_gesture(self.document.active_tool, (x, y), from);
        self.execute_command(command)
    }

    fn canvas_position_from_window(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let layout = self.layout.as_ref()?;
        if !layout.canvas_display_rect.contains(x, y) {
            return None;
        }

        let bitmap = self.document.active_bitmap()?;
        map_view_to_canvas(
            &render::RenderFrame {
                width: bitmap.width,
                height: bitmap.height,
                pixels: Vec::new(),
            },
            CanvasPointerEvent {
                x: x - layout.canvas_display_rect.x as i32,
                y: y - layout.canvas_display_rect.y as i32,
                width: layout.canvas_display_rect.width as i32,
                height: layout.canvas_display_rect.height as i32,
            },
        )
    }

    fn execute_command(&mut self, command: Command) -> bool {
        match command {
            Command::SaveProject => {
                if let Err(error) = save_document_to_path(&self.project_path, &self.document) {
                    eprintln!("failed to save project: {error}");
                    return false;
                }
                true
            }
            Command::LoadProject => match load_document_from_path(&self.project_path) {
                Ok(document) => {
                    self.document = document;
                    self.canvas_input = CanvasInputState::default();
                    self.pending_canvas_dirty_rect = None;
                    self.active_panel_drag = None;
                    self.needs_ui_sync = true;
                    self.needs_panel_surface_refresh = true;
                    self.needs_full_present_rebuild = true;
                    true
                }
                Err(error) => {
                    eprintln!("failed to load project: {error}");
                    false
                }
            },
            other => {
                let dirty = self.document.apply_command(&other);
                match other {
                    Command::DrawPoint { .. }
                    | Command::DrawStroke { .. }
                    | Command::ErasePoint { .. }
                    | Command::EraseStroke { .. } => {
                        if let Some(dirty) = dirty {
                            self.pending_canvas_dirty_rect = Some(
                                self.pending_canvas_dirty_rect
                                    .map_or(dirty, |existing| existing.union(dirty)),
                            );
                        }
                        dirty.is_some()
                    }
                    Command::SetActiveTool { .. } => {
                        self.needs_ui_sync = true;
                        self.needs_panel_surface_refresh = true;
                        self.needs_status_refresh = true;
                        true
                    }
                    Command::SetActiveColor { .. } => {
                        self.needs_ui_sync = true;
                        self.needs_panel_surface_refresh = true;
                        self.needs_status_refresh = true;
                        true
                    }
                    Command::NewDocument => {
                        self.canvas_input = CanvasInputState::default();
                        self.pending_canvas_dirty_rect = None;
                        self.active_panel_drag = None;
                        self.needs_ui_sync = true;
                        self.needs_panel_surface_refresh = true;
                        self.needs_full_present_rebuild = true;
                        true
                    }
                    Command::Noop | Command::SaveProject | Command::LoadProject => false,
                }
            }
        }
    }

    fn execute_host_action(&mut self, action: HostAction) -> bool {
        match action {
            HostAction::DispatchCommand(command) => self.execute_command(command),
            HostAction::InvokePanelHandler { .. } => false,
        }
    }

    fn canvas_dimensions(&self) -> (usize, usize) {
        self.document
            .active_bitmap()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    fn panel_event_from_window(&self, x: i32, y: i32) -> Option<PanelEvent> {
        let layout = self.layout.as_ref()?;
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = map_view_to_surface(
            panel_surface.width,
            panel_surface.height,
            layout.panel_surface_rect,
            x,
            y,
        )?;
        panel_surface.hit_test(surface_x, surface_y)
    }

    fn panel_drag_event_from_window(
        &self,
        state: &PanelDragState,
        x: i32,
        y: i32,
    ) -> Option<PanelEvent> {
        let layout = self.layout.as_ref()?;
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = map_view_to_surface_clamped(
            panel_surface.width,
            panel_surface.height,
            layout.panel_surface_rect,
            x,
            y,
        )?;
        panel_surface.drag_event(&state.panel_id, &state.node_id, surface_x, surface_y)
    }

    fn status_text(&self) -> String {
        format!(
            "tool={:?} / color={} / pages={} / panels={}",
            self.document.active_tool,
            self.document.active_color.hex_rgb(),
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.panels.len())
                .sum::<usize>()
        )
    }

    fn is_canvas_interacting(&self) -> bool {
        self.canvas_input.is_drawing
    }
}

struct DesktopRuntime {
    app: DesktopApp,
    window: Option<Arc<Window>>,
    presenter: Option<WgpuPresenter>,
    last_cursor_position: Option<(i32, i32)>,
    active_touch_id: Option<u64>,
    profiler: DesktopProfiler,
    modifiers: ModifiersState,
}

impl DesktopRuntime {
    fn new(project_path: PathBuf) -> Self {
        Self {
            app: DesktopApp::new(project_path),
            window: None,
            presenter: None,
            last_cursor_position: None,
            active_touch_id: None,
            profiler: DesktopProfiler::new(),
            modifiers: ModifiersState::default(),
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn active_window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    fn handle_mouse_cursor_moved(&mut self, x: i32, y: i32) -> bool {
        if self.active_touch_id.is_some() {
            return false;
        }

        let position = (x, y);
        self.last_cursor_position = Some(position);
        let changed = self.app.handle_pointer_dragged(position.0, position.1);
        if changed && self.app.is_canvas_interacting() {
            self.profiler.record_canvas_input();
        }
        changed
    }

    fn handle_mouse_button(&mut self, state: ElementState) -> bool {
        if self.active_touch_id.is_some() {
            return false;
        }

        let Some((x, y)) = self.last_cursor_position else {
            return false;
        };

        match state {
            ElementState::Pressed => {
                let changed = self.app.handle_pointer_pressed(x, y);
                if changed && self.app.is_canvas_interacting() {
                    self.profiler.record_canvas_input();
                }
                changed
            }
            ElementState::Released => self.app.handle_pointer_released(x, y),
        }
    }

    fn handle_touch_phase(&mut self, touch_id: u64, phase: TouchPhase, x: i32, y: i32) -> bool {
        let position = (x, y);

        match phase {
            TouchPhase::Started => {
                if matches!(self.active_touch_id, Some(active_id) if active_id != touch_id) {
                    return false;
                }

                self.active_touch_id = Some(touch_id);
                self.last_cursor_position = Some(position);
                let changed = self.app.handle_pointer_pressed(position.0, position.1);
                if changed && self.app.is_canvas_interacting() {
                    self.profiler.record_canvas_input();
                }
                changed
            }
            TouchPhase::Moved => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                self.last_cursor_position = Some(position);
                let changed = self.app.handle_pointer_dragged(position.0, position.1);
                if changed && self.app.is_canvas_interacting() {
                    self.profiler.record_canvas_input();
                }
                changed
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                self.last_cursor_position = Some(position);
                self.active_touch_id = None;
                self.app.handle_pointer_released(position.0, position.1)
            }
        }
    }
}

impl ApplicationHandler for DesktopRuntime {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("failed to create window: {error}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        let presenter = match pollster::block_on(WgpuPresenter::new(window.clone())) {
            Ok(presenter) => presenter,
            Err(error) => {
                eprintln!("failed to initialize wgpu presenter: {error}");
                event_loop.exit();
                return;
            }
        };

        let _ = self.app.prepare_present_frame(
            size.width as usize,
            size.height as usize,
            &mut self.profiler,
        );
        self.presenter = Some(presenter);
        self.window = Some(window);
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.active_window_id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(presenter) = &mut self.presenter {
                    presenter.resize(size);
                }
                let _ = self.app.prepare_present_frame(
                    size.width as usize,
                    size.height as usize,
                    &mut self.profiler,
                );
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = (position.x as i32, position.y as i32);
                if self.handle_mouse_cursor_moved(position.0, position.1) {
                    self.request_redraw();
                }
            }
            WindowEvent::Touch(touch) => {
                let position = (touch.location.x as i32, touch.location.y as i32);
                if self.handle_touch_phase(touch.id, touch.phase, position.0, position.1) {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some((x, y)) = self.last_cursor_position else {
                    return;
                };
                let Some(layout) = self.app.layout.as_ref() else {
                    return;
                };
                if !layout.panel_host_rect.contains(x, y) {
                    return;
                }

                let delta_lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y.round() as i32),
                    MouseScrollDelta::PixelDelta(position) => {
                        let lines = position.y / ui_shell::text_line_height() as f64;
                        -(lines.round() as i32)
                    }
                };
                if delta_lines != 0 && self.app.scroll_panel_surface(delta_lines) {
                    self.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed || event.repeat {
                    return;
                }

                let changed = match &event.logical_key {
                    Key::Named(NamedKey::Tab) if self.modifiers.shift_key() => {
                        self.app.focus_previous_panel_control()
                    }
                    Key::Named(NamedKey::Tab) => self.app.focus_next_panel_control(),
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.app.activate_focused_panel_control().is_some()
                    }
                    _ => false,
                };

                if changed {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if self.handle_mouse_button(state) {
                    self.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(window) = &self.window else {
                    return;
                };
                let Some(presenter) = &mut self.presenter else {
                    return;
                };

                let size = window.inner_size();
                let frame_started = Instant::now();
                let prepare_started = Instant::now();
                let update = self.app.prepare_present_frame(
                    size.width as usize,
                    size.height as usize,
                    &mut self.profiler,
                );
                self.profiler
                    .record("prepare_frame", prepare_started.elapsed());
                let Some(frame) = self.app.present_frame() else {
                    return;
                };
                let upload_region = update.dirty_rect.map(|rect| UploadRegion {
                    x: rect.x as u32,
                    y: rect.y as u32,
                    width: rect.width as u32,
                    height: rect.height as u32,
                });
                let present_started = Instant::now();
                let timings = match presenter.render(frame, upload_region) {
                    Ok(timings) => timings,
                    Err(error) => {
                        eprintln!("render failed: {error}");
                        event_loop.exit();
                        return;
                    }
                };
                self.profiler
                    .record("present_total", present_started.elapsed());
                self.profiler.record_present(timings);
                if update.canvas_updated {
                    self.profiler.record_canvas_present();
                }
                self.profiler.finish_frame(frame_started.elapsed());
                window.set_title(&self.profiler.title_text());
                if self.app.is_canvas_interacting() {
                    self.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn fit_rect(source_width: usize, source_height: usize, target: Rect) -> Rect {
    if source_width == 0 || source_height == 0 || target.width == 0 || target.height == 0 {
        return Rect {
            x: target.x,
            y: target.y,
            width: 0,
            height: 0,
        };
    }

    let scale_x = target.width as f32 / source_width as f32;
    let scale_y = target.height as f32 / source_height as f32;
    let scale = scale_x.min(scale_y);
    let fitted_width = ((source_width as f32 * scale).floor() as usize).max(1);
    let fitted_height = ((source_height as f32 * scale).floor() as usize).max(1);

    Rect {
        x: target.x + (target.width.saturating_sub(fitted_width)) / 2,
        y: target.y + (target.height.saturating_sub(fitted_height)) / 2,
        width: fitted_width,
        height: fitted_height,
    }
}

fn map_canvas_dirty_to_display(
    dirty: DirtyRect,
    destination: Rect,
    source_width: usize,
    source_height: usize,
) -> Rect {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return destination;
    }

    let clamped = dirty.clamp_to_bitmap(source_width, source_height);
    let start_x = destination.x + (clamped.x * destination.width) / source_width;
    let start_y = destination.y + (clamped.y * destination.height) / source_height;
    let end_x =
        destination.x + ((clamped.x + clamped.width) * destination.width).div_ceil(source_width);
    let end_y =
        destination.y + ((clamped.y + clamped.height) * destination.height).div_ceil(source_height);

    Rect {
        x: start_x.min(destination.x + destination.width.saturating_sub(1)),
        y: start_y.min(destination.y + destination.height.saturating_sub(1)),
        width: end_x.saturating_sub(start_x).max(1),
        height: end_y.saturating_sub(start_y).max(1),
    }
}

fn compose_desktop_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    canvas: CanvasCompositeSource<'_>,
    status_text: &str,
) -> render::RenderFrame {
    let mut frame = render::RenderFrame {
        width,
        height,
        pixels: vec![0; width * height * 4],
    };

    fill_rect(
        &mut frame,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        APP_BACKGROUND,
    );
    fill_rect(
        &mut frame,
        Rect {
            x: 0,
            y: 0,
            width: SIDEBAR_WIDTH.min(width),
            height,
        },
        SIDEBAR_BACKGROUND,
    );
    fill_rect(&mut frame, layout.panel_host_rect, PANEL_FRAME_BACKGROUND);
    stroke_rect(&mut frame, layout.panel_host_rect, PANEL_FRAME_BORDER);
    fill_rect(&mut frame, layout.canvas_host_rect, CANVAS_FRAME_BACKGROUND);
    stroke_rect(&mut frame, layout.canvas_host_rect, CANVAS_FRAME_BORDER);
    fill_rect(&mut frame, layout.canvas_display_rect, CANVAS_BACKGROUND);

    blit_scaled_rgba(
        &mut frame,
        layout.panel_surface_rect,
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels.as_slice(),
    );
    blit_scaled_rgba(
        &mut frame,
        layout.canvas_display_rect,
        canvas.width,
        canvas.height,
        canvas.pixels,
    );

    draw_text(
        &mut frame,
        WINDOW_PADDING,
        WINDOW_PADDING + 4,
        "Panel host (winit + software panel runtime)",
        TEXT_PRIMARY,
    );
    draw_text(
        &mut frame,
        layout.canvas_host_rect.x,
        WINDOW_PADDING + 4,
        "Canvas host (winit + wgpu presenter)",
        TEXT_PRIMARY,
    );
    draw_text(
        &mut frame,
        WINDOW_PADDING,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        "Built-in panels are rendered by the host panel runtime.",
        TEXT_SECONDARY,
    );
    draw_text(
        &mut frame,
        layout.canvas_host_rect.x,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        status_text,
        TEXT_SECONDARY,
    );

    frame
}

fn compose_panel_host_region(
    frame: &mut render::RenderFrame,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
) {
    fill_rect(frame, layout.panel_host_rect, PANEL_FRAME_BACKGROUND);
    stroke_rect(frame, layout.panel_host_rect, PANEL_FRAME_BORDER);
    blit_scaled_rgba(
        frame,
        layout.panel_surface_rect,
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels.as_slice(),
    );
}

fn compose_status_region(
    frame: &mut render::RenderFrame,
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    status_text: &str,
) {
    let status_rect = status_text_rect(width, height, layout);
    fill_rect(frame, status_rect, APP_BACKGROUND);
    draw_text(
        frame,
        layout.canvas_host_rect.x,
        height.saturating_sub(FOOTER_HEIGHT) + 6,
        status_text,
        TEXT_SECONDARY,
    );
}

fn status_text_rect(width: usize, height: usize, layout: &DesktopLayout) -> Rect {
    Rect {
        x: layout.canvas_host_rect.x,
        y: height.saturating_sub(FOOTER_HEIGHT),
        width: width.saturating_sub(layout.canvas_host_rect.x),
        height: FOOTER_HEIGHT,
    }
}

fn map_view_to_surface(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    x: i32,
    y: i32,
) -> Option<(usize, usize)> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }
    if !rect.contains(x, y) {
        return None;
    }

    let local_x = (x - rect.x as i32) as f32;
    let local_y = (y - rect.y as i32) as f32;
    Some((
        (((local_x / rect.width as f32) * surface_width as f32).floor() as usize)
            .min(surface_width.saturating_sub(1)),
        (((local_y / rect.height as f32) * surface_height as f32).floor() as usize)
            .min(surface_height.saturating_sub(1)),
    ))
}

fn map_view_to_surface_clamped(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    x: i32,
    y: i32,
) -> Option<(usize, usize)> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }

    let clamped_x = x.clamp(
        rect.x as i32,
        (rect.x + rect.width.saturating_sub(1)) as i32,
    );
    let clamped_y = y.clamp(
        rect.y as i32,
        (rect.y + rect.height.saturating_sub(1)) as i32,
    );
    map_view_to_surface(surface_width, surface_height, rect, clamped_x, clamped_y)
}

fn draw_text(frame: &mut render::RenderFrame, x: usize, y: usize, text: &str, color: [u8; 4]) {
    draw_text_rgba(
        frame.pixels.as_mut_slice(),
        frame.width,
        frame.height,
        x,
        y,
        text,
        color,
    );
}

fn fill_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
    let max_x = (rect.x + rect.width).min(frame.width);
    let max_y = (rect.y + rect.height).min(frame.height);
    for yy in rect.y..max_y {
        for xx in rect.x..max_x {
            write_pixel(frame, xx, yy, color);
        }
    }
}

fn stroke_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x + rect.width.saturating_sub(1),
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
}

fn blit_scaled_rgba(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
) {
    blit_scaled_rgba_region(
        frame,
        destination,
        source_width,
        source_height,
        source_pixels,
        None,
    );
}

fn blit_scaled_rgba_region(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<Rect>,
) {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return;
    }

    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination);

    for dst_y in target.y..target.y + target.height {
        let local_y = dst_y - destination.y;
        let src_y = ((local_y * source_height) / destination.height).min(source_height - 1);
        for dst_x in target.x..target.x + target.width {
            let local_x = dst_x - destination.x;
            let src_x = ((local_x * source_width) / destination.width).min(source_width - 1);
            let src_index = (src_y * source_width + src_x) * 4;
            write_pixel(
                frame,
                dst_x,
                dst_y,
                [
                    source_pixels[src_index],
                    source_pixels[src_index + 1],
                    source_pixels[src_index + 2],
                    source_pixels[src_index + 3],
                ],
            );
        }
    }
}

fn write_pixel(frame: &mut render::RenderFrame, x: usize, y: usize, color: [u8; 4]) {
    if x >= frame.width || y >= frame.height {
        return;
    }
    let index = (y * frame.width + x) * 4;
    frame.pixels[index..index + 4].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_bridge::{
        CanvasPointerEvent, command_for_canvas_gesture, map_view_to_canvas,
    };
    use app_core::{ColorRgba8, ToolKind};
    use render::RenderFrame;
    use winit::event::TouchPhase;

    #[test]
    fn map_view_to_surface_maps_bottom_right_corner() {
        let mapped = map_view_to_surface(
            264,
            800,
            Rect {
                x: 8,
                y: 40,
                width: 264,
                height: 800,
            },
            271,
            839,
        );

        assert_eq!(mapped, Some((263, 799)));
    }

    #[test]
    fn map_view_to_surface_clamped_limits_outside_coordinates() {
        let mapped = map_view_to_surface_clamped(
            264,
            800,
            Rect {
                x: 8,
                y: 40,
                width: 264,
                height: 800,
            },
            500,
            -10,
        );

        assert_eq!(mapped, Some((263, 0)));
    }

    #[test]
    fn desktop_layout_letterboxes_canvas_inside_host_rect() {
        let layout = DesktopLayout::new(1280, 800, 64, 64);

        assert!(layout.canvas_display_rect.width <= layout.canvas_host_rect.width);
        assert!(layout.canvas_display_rect.height <= layout.canvas_host_rect.height);
        assert!(layout.canvas_host_rect.contains(
            layout.canvas_display_rect.x as i32,
            layout.canvas_display_rect.y as i32,
        ));
    }

    #[test]
    fn panel_surface_fills_panel_host_rect() {
        let layout = DesktopLayout::new(1280, 800, 64, 64);

        assert_eq!(layout.panel_surface_rect, layout.panel_host_rect);
    }

    #[test]
    fn execute_command_updates_document_tool() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        let _ = app.execute_command(Command::SetActiveTool {
            tool: ToolKind::Eraser,
        });

        assert_eq!(app.document.active_tool, ToolKind::Eraser);
    }

    #[test]
    fn execute_command_updates_document_color() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        let _ = app.execute_command(Command::SetActiveColor {
            color: ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff),
        });

        assert_eq!(
            app.document.active_color,
            ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)
        );
    }

    #[test]
    fn execute_command_new_document_resets_tool_to_default() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        app.document.set_active_tool(ToolKind::Eraser);

        let _ = app.execute_command(Command::NewDocument);

        assert_eq!(app.document.active_tool, ToolKind::Brush);
    }

    #[test]
    fn canvas_position_maps_view_center_into_bitmap_bounds() {
        let position = map_view_to_canvas(
            &RenderFrame {
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

    #[test]
    fn canvas_drag_draws_black_pixels() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 800, &mut profiler);
        let layout = app.layout.clone().expect("layout exists");
        let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
        let center_y =
            (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

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

    #[test]
    fn touch_started_and_moved_draws_black_pixels() {
        let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
        let layout = runtime.app.layout.clone().expect("layout exists");
        let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
        let center_y =
            (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

        assert!(runtime.handle_touch_phase(1, TouchPhase::Started, center_x, center_y));
        assert!(runtime.handle_touch_phase(1, TouchPhase::Moved, center_x + 20, center_y));
        assert!(!runtime.handle_touch_phase(1, TouchPhase::Ended, center_x + 20, center_y));

        let frame = runtime.app.ui_shell.render_frame(&runtime.app.document);
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [0, 0, 0, 255])
        );
    }

    #[test]
    fn touch_cancelled_stops_active_touch_tracking() {
        let mut runtime = DesktopRuntime::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = runtime.app.prepare_present_frame(1280, 800, &mut profiler);
        let layout = runtime.app.layout.clone().expect("layout exists");
        let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
        let center_y =
            (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

        assert!(runtime.handle_touch_phase(7, TouchPhase::Started, center_x, center_y));
        assert_eq!(runtime.active_touch_id, Some(7));

        assert!(!runtime.handle_touch_phase(7, TouchPhase::Cancelled, center_x, center_y));
        assert_eq!(runtime.active_touch_id, None);
    }

    #[test]
    fn canvas_drag_draws_using_selected_color() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 800, &mut profiler);
        let layout = app.layout.clone().expect("layout exists");
        let center_x = (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32;
        let center_y =
            (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32;

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

    #[test]
    fn host_action_dispatches_tool_switch_command() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        let _ = app.execute_host_action(HostAction::DispatchCommand(Command::SetActiveTool {
            tool: ToolKind::Eraser,
        }));

        assert_eq!(app.document.active_tool, ToolKind::Eraser);
    }

    #[test]
    fn keyboard_panel_focus_can_activate_app_action() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 200, &mut profiler);

        assert!(app.focus_next_panel_control());
        assert_eq!(
            app.activate_focused_panel_control(),
            Some(Command::NewDocument)
        );
    }

    #[test]
    fn desktop_app_loads_phase6_sample_panel_from_default_ui_directory() {
        let app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        assert!(
            default_panel_dir()
                .join("phase6-sample.altp-panel")
                .exists()
        );
        assert!(
            app.ui_shell
                .panel_trees()
                .iter()
                .any(|panel| panel.id == "builtin.dsl-sample")
        );
    }

    #[test]
    fn desktop_app_replaces_builtin_panels_with_phase7_dsl_variants() {
        let app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let panels = app.ui_shell.panel_trees();

        for panel_id in [
            "builtin.app-actions",
            "builtin.tool-palette",
            "builtin.layers-panel",
        ] {
            assert_eq!(
                panels.iter().filter(|panel| panel.id == panel_id).count(),
                1,
                "expected a single panel for {panel_id}"
            );
        }

        let app_actions = panels
            .iter()
            .find(|panel| panel.id == "builtin.app-actions")
            .expect("app actions panel exists");
        let layers = panels
            .iter()
            .find(|panel| panel.id == "builtin.layers-panel")
            .expect("layers panel exists");

        assert!(tree_contains_text(
            &app_actions.children,
            "Hosted via Rust SDK + Wasm"
        ));
        assert!(tree_contains_text(&layers.children, "Untitled"));
    }

    #[test]
    fn panel_scroll_requests_surface_offset_change() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 120, &mut profiler);

        assert!(app.scroll_panel_surface(6));
        assert!(app.ui_shell.panel_scroll_offset() > 0);
    }

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
        assert_eq!(update.dirty_rect, Some(layout.panel_host_rect));
        assert!(!update.canvas_updated);
        assert_eq!(
            profiler.stats.get("panel_surface").map(|stat| stat.calls),
            Some(1)
        );
    }

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
        assert_eq!(update.dirty_rect, Some(layout.panel_host_rect));
        assert!(!update.canvas_updated);
        assert_eq!(
            profiler.stats.get("panel_surface").map(|stat| stat.calls),
            Some(1)
        );
    }

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
            update.dirty_rect,
            Some(
                layout
                    .panel_host_rect
                    .union(status_text_rect(1280, 200, &layout))
            )
        );
    }

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
            canvas_sample_hz: 123.4,
        }
        .title_text();

        assert!(title.contains("59.8 fps"));
        assert!(title.contains("prep  3.11ms"));
        assert!(title.contains("ui  0.42ms"));
        assert!(title.contains("ink  8.40ms ok"));
        assert!(title.contains("sample  123.4Hz ok"));
    }

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

        let snapshot = profiler.latest_snapshot.expect("snapshot exists");
        assert!(snapshot.fps > 60.0);
        assert!(snapshot.fps < 65.0);
    }

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

        let snapshot = profiler.latest_snapshot.expect("snapshot exists");
        assert!(snapshot.canvas_latency_ms >= 8.0);
        assert!(snapshot.canvas_latency_ms < 9.0);
        assert!(snapshot.canvas_sample_hz >= 120.0);
        assert!(snapshot.canvas_sample_hz < 130.0);
    }

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

        let fps_before_idle = profiler.latest_snapshot.expect("snapshot exists").fps;

        profiler.record("prepare_frame", Duration::from_millis(2));
        profiler.record("ui_update", Duration::from_millis(1));
        profiler.record("panel_surface", Duration::from_millis(1));
        profiler.record("present_total", Duration::from_millis(2));
        profiler.finish_frame_at(Duration::from_millis(16), start + Duration::from_secs(3));

        let fps_after_idle = profiler.latest_snapshot.expect("snapshot exists").fps;
        assert!(fps_before_idle > 50.0);
        assert!(fps_after_idle > 50.0);
    }

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
                if let Some(PanelEvent::SetValue {
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

    #[test]
    fn compose_desktop_frame_writes_panel_and_canvas_regions() {
        let layout = DesktopLayout::new(640, 480, 64, 64);
        let mut shell = UiShell::new();
        let panel_surface = shell.render_panel_surface(264, 800);
        let frame = compose_desktop_frame(
            640,
            480,
            &layout,
            &panel_surface,
            CanvasCompositeSource {
                width: 2,
                height: 2,
                pixels: &[16; 16],
            },
            "status",
        );

        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [16, 16, 16, 16])
        );
    }

    #[test]
    fn canvas_dirty_rect_maps_into_display_rect() {
        let mapped = map_canvas_dirty_to_display(
            DirtyRect {
                x: 16,
                y: 16,
                width: 8,
                height: 8,
            },
            Rect {
                x: 100,
                y: 50,
                width: 320,
                height: 320,
            },
            64,
            64,
        );

        assert_eq!(mapped.x, 180);
        assert_eq!(mapped.y, 130);
        assert_eq!(mapped.width, 40);
        assert_eq!(mapped.height, 40);
    }

    #[test]
    fn blit_scaled_rgba_region_updates_only_dirty_area() {
        let mut frame = RenderFrame {
            width: 8,
            height: 8,
            pixels: vec![0; 8 * 8 * 4],
        };
        let source = vec![255; 4 * 4 * 4];

        blit_scaled_rgba_region(
            &mut frame,
            Rect {
                x: 2,
                y: 2,
                width: 4,
                height: 4,
            },
            4,
            4,
            source.as_slice(),
            Some(Rect {
                x: 3,
                y: 3,
                width: 1,
                height: 1,
            }),
        );

        let dirty_index = (3 * frame.width + 3) * 4;
        let untouched_index = (2 * frame.width + 2) * 4;
        assert_eq!(
            &frame.pixels[dirty_index..dirty_index + 4],
            &[255, 255, 255, 255]
        );
        assert_eq!(
            &frame.pixels[untouched_index..untouched_index + 4],
            &[0, 0, 0, 0]
        );
    }

    fn tree_contains_text(nodes: &[plugin_api::PanelNode], target: &str) -> bool {
        nodes.iter().any(|node| match node {
            plugin_api::PanelNode::Text { text, .. } => text == target,
            plugin_api::PanelNode::Column { children, .. }
            | plugin_api::PanelNode::Row { children, .. }
            | plugin_api::PanelNode::Section { children, .. } => {
                tree_contains_text(children, target)
            }
            plugin_api::PanelNode::ColorPreview { .. }
            | plugin_api::PanelNode::Button { .. }
            | plugin_api::PanelNode::Slider { .. } => false,
        })
    }
}
