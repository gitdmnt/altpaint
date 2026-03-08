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
use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{load_document_from_path, save_document_to_path};
use ui_shell::{PanelSurface, UiShell, draw_text_rgba};
use wgpu_canvas::{PresentTimings, UploadRegion, WgpuPresenter};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_PROJECT_PATH: &str = "altpaint-project.altp.json";
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
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct StageStats {
    calls: u64,
    total: Duration,
    max: Duration,
}

struct DesktopProfiler {
    enabled: bool,
    stats: BTreeMap<&'static str, StageStats>,
    last_report: Instant,
    report_interval: Duration,
}

impl DesktopProfiler {
    fn new() -> Self {
        Self {
            enabled: env::var_os("ALTPAINT_PROFILE").is_some(),
            stats: BTreeMap::new(),
            last_report: Instant::now(),
            report_interval: Duration::from_secs(2),
        }
    }

    fn measure<T>(&mut self, label: &'static str, f: impl FnOnce() -> T) -> T {
        if !self.enabled {
            return f();
        }

        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

    fn record(&mut self, label: &'static str, elapsed: Duration) {
        if !self.enabled {
            return;
        }

        let stat = self.stats.entry(label).or_default();
        stat.calls += 1;
        stat.total += elapsed;
        stat.max = stat.max.max(elapsed);

        let now = Instant::now();
        if now.duration_since(self.last_report) >= self.report_interval {
            eprintln!(
                "[profile] ---- last {}s ----",
                self.report_interval.as_secs()
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
            self.stats.clear();
            self.last_report = now;
        }
    }

    fn record_present(&mut self, timings: PresentTimings) {
        self.record("present_upload", timings.upload);
        self.record("present_encode", timings.encode_and_submit);
        self.record("present_swap", timings.present);
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

struct DesktopApp {
    document: Document,
    ui_shell: UiShell,
    project_path: PathBuf,
    canvas_input: CanvasInputState,
    panel_surface: Option<PanelSurface>,
    layout: Option<DesktopLayout>,
    present_frame: Option<render::RenderFrame>,
    pending_canvas_dirty_rect: Option<DirtyRect>,
    needs_panel_refresh: bool,
    needs_full_present_rebuild: bool,
}

impl DesktopApp {
    fn new(project_path: PathBuf) -> Self {
        let document = load_document_from_path(&project_path).unwrap_or_default();
        let mut ui_shell = UiShell::new();
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
            needs_panel_refresh: true,
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
            self.needs_panel_refresh = true;
            self.needs_full_present_rebuild = true;
        }

        if self.needs_panel_refresh {
            profiler.measure("ui_update", || self.ui_shell.update(&self.document));
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
            self.needs_panel_refresh = false;
            self.needs_full_present_rebuild = true;
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
            self.needs_full_present_rebuild = false;
            return PresentFrameUpdate { dirty_rect: None };
        }

        if let Some(dirty) = self.pending_canvas_dirty_rect.take() {
            let layout = self.layout.as_ref().expect("layout exists");
            let Some(present_frame) = self.present_frame.as_mut() else {
                self.needs_full_present_rebuild = true;
                return PresentFrameUpdate { dirty_rect: None };
            };
            let Some(bitmap) = self.document.active_bitmap() else {
                self.needs_full_present_rebuild = true;
                return PresentFrameUpdate { dirty_rect: None };
            };
            let dirty_rect = map_canvas_dirty_to_display(
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
                    Some(dirty_rect),
                );
            });
            return PresentFrameUpdate {
                dirty_rect: Some(dirty_rect),
            };
        }

        PresentFrameUpdate::default()
    }

    fn present_frame(&self) -> Option<&render::RenderFrame> {
        self.present_frame.as_ref()
    }

    fn handle_pointer_pressed(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_position_from_window(x, y).is_some() {
            return self.handle_canvas_pointer("down", x, y);
        }

        false
    }

    fn handle_pointer_released(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("up", x, y);
        }
        self.handle_panel_pointer(x, y)
    }

    fn handle_pointer_dragged(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("drag", x, y);
        }

        false
    }

    fn handle_panel_pointer(&mut self, x: i32, y: i32) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let Some(panel_surface) = self.panel_surface.as_ref() else {
            return false;
        };

        let Some((surface_x, surface_y)) = map_view_to_surface(
            panel_surface.width,
            panel_surface.height,
            layout.panel_surface_rect,
            x,
            y,
        ) else {
            return false;
        };

        let Some(event) = panel_surface.hit_test(surface_x, surface_y) else {
            return false;
        };

        let mut changed = false;
        let PanelEvent::Activate { panel_id, node_id } = &event;
        changed |= self.ui_shell.focus_panel_node(panel_id, node_id);

        for action in self.ui_shell.handle_panel_event(&event) {
            changed |= self.execute_host_action(action);
        }

        changed
    }

    fn focus_next_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_next();
        if changed {
            self.needs_panel_refresh = true;
            self.needs_full_present_rebuild = true;
        }
        changed
    }

    fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_previous();
        if changed {
            self.needs_panel_refresh = true;
            self.needs_full_present_rebuild = true;
        }
        changed
    }

    fn activate_focused_panel_control(&mut self) -> Option<Command> {
        let actions = self.ui_shell.activate_focused();
        let mut dispatched = None;
        for action in actions {
            let HostAction::DispatchCommand(command) = &action;
            if dispatched.is_none() {
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
            self.needs_panel_refresh = true;
            self.needs_full_present_rebuild = true;
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
                    self.needs_panel_refresh = true;
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
                        self.needs_panel_refresh = true;
                        self.needs_full_present_rebuild = true;
                        true
                    }
                    Command::SetActiveColor { .. } => {
                        self.needs_panel_refresh = true;
                        self.needs_full_present_rebuild = true;
                        true
                    }
                    Command::NewDocument => {
                        self.canvas_input = CanvasInputState::default();
                        self.pending_canvas_dirty_rect = None;
                        self.needs_panel_refresh = true;
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
        }
    }

    fn canvas_dimensions(&self) -> (usize, usize) {
        self.document
            .active_bitmap()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
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
}

struct DesktopRuntime {
    app: DesktopApp,
    window: Option<Arc<Window>>,
    presenter: Option<WgpuPresenter>,
    last_cursor_position: Option<(i32, i32)>,
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
}

impl ApplicationHandler for DesktopRuntime {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("altpaint")
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
                self.last_cursor_position = Some(position);
                if self.app.handle_pointer_dragged(position.0, position.1) {
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
                if let Some((x, y)) = self.last_cursor_position {
                    let changed = match state {
                        ElementState::Pressed => self.app.handle_pointer_pressed(x, y),
                        ElementState::Released => self.app.handle_pointer_released(x, y),
                    };
                    if changed {
                        self.request_redraw();
                    }
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
                let update = self.app.prepare_present_frame(
                    size.width as usize,
                    size.height as usize,
                    &mut self.profiler,
                );
                let Some(frame) = self.app.present_frame() else {
                    return;
                };
                let upload_region = update.dirty_rect.map(|rect| UploadRegion {
                    x: rect.x as u32,
                    y: rect.y as u32,
                    width: rect.width as u32,
                    height: rect.height as u32,
                });
                let timings = match presenter.render(frame, upload_region) {
                    Ok(timings) => timings,
                    Err(error) => {
                        eprintln!("render failed: {error}");
                        event_loop.exit();
                        return;
                    }
                };
                self.profiler.record_present(timings);
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

        assert_eq!(app.document.active_color, ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff));
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
    fn panel_scroll_requests_surface_offset_change() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 120, &mut profiler);

        assert!(app.scroll_panel_surface(6));
        assert!(app.ui_shell.panel_scroll_offset() > 0);
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
}
