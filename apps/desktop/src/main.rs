//! `desktop` は最小のデスクトップエントリポイント。
//!
//! `winit` がウィンドウと入力を受け持ち、`wgpu` が合成済みフレームを提示する。

mod canvas_bridge;
mod frame;
mod profiler;
mod wgpu_canvas;

use anyhow::{Context, Result};
use app_core::{Command, DirtyRect, Document};
use canvas_bridge::{
    CanvasInputState, CanvasPointerEvent, command_for_canvas_gesture, map_view_to_canvas,
};
use frame::{
    CanvasCompositeSource, DesktopLayout, Rect, blit_scaled_rgba_region,
    compose_desktop_frame, compose_panel_host_region, compose_status_region, map_canvas_dirty_to_display,
    map_view_to_surface, map_view_to_surface_clamped, status_text_rect,
};
use plugin_api::{HostAction, PanelEvent};
use profiler::DesktopProfiler;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{load_project_from_path, save_project_to_path};
use ui_shell::{PanelSurface, UiShell};
use wgpu_canvas::{UploadRegion, WgpuPresenter};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
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
#[cfg(test)]
const MAX_DOCUMENT_DIMENSION: usize = 8192;
#[cfg(test)]
const MAX_DOCUMENT_PIXELS: usize = 16_777_216;

fn default_panel_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("plugins")
}

trait DesktopDialogs {
    fn pick_open_project_path(&self, current_path: &Path) -> Option<PathBuf>;
    fn pick_save_project_path(&self, current_path: &Path) -> Option<PathBuf>;
    fn show_error(&self, title: &str, message: &str);
}

struct NativeDesktopDialogs;

impl DesktopDialogs for NativeDesktopDialogs {
    fn pick_open_project_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::open_file_dialog(
            "Open Project",
            &current_path.to_string_lossy(),
            Some((&["*.altp.json", "*.json"], "altpaint project")),
        )
        .map(PathBuf::from)
    }

    fn pick_save_project_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::save_file_dialog_with_filter(
            "Save Project",
            &current_path.to_string_lossy(),
            &["*.altp.json", "*.json"],
            "altpaint project",
        )
        .map(PathBuf::from)
    }

    fn show_error(&self, title: &str, message: &str) {
        tinyfiledialogs::message_box_ok(title, message, tinyfiledialogs::MessageBoxIcon::Error);
    }
}

fn normalize_project_path(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension("altp.json")
    }
}

#[cfg(test)]
fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized
        .split(|ch: char| ch == 'x' || ch.is_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }

    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0
        || height == 0
        || width > MAX_DOCUMENT_DIMENSION
        || height > MAX_DOCUMENT_DIMENSION
        || width.saturating_mul(height) > MAX_DOCUMENT_PIXELS
    {
        return None;
    }

    Some((width, height))
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    let mut runtime = DesktopRuntime::new(PathBuf::from(DEFAULT_PROJECT_PATH));
    event_loop
        .run_app(&mut runtime)
        .context("failed to run desktop runtime")
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PresentFrameUpdate {
    dirty_rect: Option<Rect>,
    canvas_updated: bool,
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
    dialogs: Box<dyn DesktopDialogs>,
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
        Self::new_with_dialogs(project_path, Box::new(NativeDesktopDialogs))
    }

    fn new_with_dialogs(project_path: PathBuf, dialogs: Box<dyn DesktopDialogs>) -> Self {
        let loaded_project = load_project_from_path(&project_path).ok();
        let document = loaded_project
            .as_ref()
            .map(|project| project.document.clone())
            .unwrap_or_default();
        let mut ui_shell = UiShell::new();
        let _ = ui_shell.load_panel_directory(default_panel_dir());
        if let Some(project) = loaded_project {
            ui_shell.set_workspace_layout(project.workspace_layout);
        }
        ui_shell.update(&document);

        Self {
            document,
            ui_shell,
            project_path,
            dialogs,
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

    fn mark_panel_surface_dirty(&mut self) {
        self.needs_panel_surface_refresh = true;
    }

    fn mark_status_dirty(&mut self) {
        self.needs_status_refresh = true;
    }

    fn sync_ui_from_document(&mut self) {
        self.needs_ui_sync = true;
        self.mark_panel_surface_dirty();
    }

    fn rebuild_present_frame(&mut self) {
        self.needs_full_present_rebuild = true;
    }

    fn reset_active_interactions(&mut self) {
        self.canvas_input = CanvasInputState::default();
        self.pending_canvas_dirty_rect = None;
        self.active_panel_drag = None;
    }

    fn refresh_panel_surface_if_changed(&mut self, changed: bool) -> bool {
        if changed {
            self.mark_panel_surface_dirty();
        }
        changed
    }

    fn append_canvas_dirty_rect(&mut self, dirty: DirtyRect) -> bool {
        self.pending_canvas_dirty_rect = Some(
            self.pending_canvas_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    fn execute_document_command(&mut self, command: Command) -> bool {
        let dirty = self.document.apply_command(&command);
        match command {
            Command::DrawPoint { .. }
            | Command::DrawStroke { .. }
            | Command::ErasePoint { .. }
            | Command::EraseStroke { .. } => dirty.is_some_and(|dirty| self.append_canvas_dirty_rect(dirty)),
            Command::SetActiveTool { .. } | Command::SetActiveColor { .. } => {
                self.sync_ui_from_document();
                self.mark_status_dirty();
                true
            }
            Command::NewDocumentSized { .. } => {
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::Noop
            | Command::NewDocument
            | Command::SaveProject
            | Command::SaveProjectAs
            | Command::SaveProjectToPath { .. }
            | Command::LoadProject
            | Command::LoadProjectFromPath { .. } => false,
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
            self.mark_panel_surface_dirty();
            self.rebuild_present_frame();
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
            self.rebuild_present_frame();
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
                self.rebuild_present_frame();
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
            PanelEvent::Activate { .. } | PanelEvent::SetText { .. } => false,
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

        self.mark_panel_surface_dirty();
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
        self.refresh_panel_surface_if_changed(changed)
    }

    fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_previous();
        self.refresh_panel_surface_if_changed(changed)
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

    fn insert_text_into_focused_panel_input(&mut self, text: &str) -> bool {
        let changed = self.ui_shell.insert_text_into_focused_input(text);
        self.refresh_panel_surface_if_changed(changed)
    }

    fn backspace_focused_panel_input(&mut self) -> bool {
        let changed = self.ui_shell.backspace_focused_input();
        self.refresh_panel_surface_if_changed(changed)
    }

    fn delete_focused_panel_input(&mut self) -> bool {
        let changed = self.ui_shell.delete_focused_input();
        self.refresh_panel_surface_if_changed(changed)
    }

    fn move_focused_panel_input_cursor(&mut self, delta_chars: isize) -> bool {
        let changed = self.ui_shell.move_focused_input_cursor(delta_chars);
        self.refresh_panel_surface_if_changed(changed)
    }

    fn move_focused_panel_input_cursor_to_start(&mut self) -> bool {
        let changed = self.ui_shell.move_focused_input_cursor_to_start();
        self.refresh_panel_surface_if_changed(changed)
    }

    fn move_focused_panel_input_cursor_to_end(&mut self) -> bool {
        let changed = self.ui_shell.move_focused_input_cursor_to_end();
        self.refresh_panel_surface_if_changed(changed)
    }

    fn set_focused_panel_input_preedit(&mut self, preedit: Option<String>) -> bool {
        let changed = self.ui_shell.set_focused_input_preedit(preedit);
        self.refresh_panel_surface_if_changed(changed)
    }

    fn has_focused_panel_input(&self) -> bool {
        self.ui_shell.has_focused_text_input()
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
        self.refresh_panel_surface_if_changed(changed)
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

    fn save_project_to_current_path(&mut self) -> bool {
        match save_project_to_path(
            &self.project_path,
            &self.document,
            &self.ui_shell.workspace_layout(),
        ) {
            Ok(()) => true,
            Err(error) => {
                let message = format!("failed to save project: {error}");
                eprintln!("{message}");
                self.dialogs.show_error("Save failed", &message);
                false
            }
        }
    }

    fn save_project_as(&mut self) -> bool {
        let Some(path) = self.dialogs.pick_save_project_path(&self.project_path) else {
            return false;
        };
        self.save_project_to_path(path)
    }

    fn save_project_to_path(&mut self, path: PathBuf) -> bool {
        self.project_path = normalize_project_path(path);
        self.needs_status_refresh = true;
        self.save_project_to_current_path()
    }

    fn open_project(&mut self) -> bool {
        let Some(path) = self.dialogs.pick_open_project_path(&self.project_path) else {
            return false;
        };
        self.load_project(path)
    }

    fn load_project(&mut self, path: PathBuf) -> bool {
        let path = normalize_project_path(path);
        match load_project_from_path(&path) {
            Ok(project) => {
                self.project_path = path;
                self.document = project.document;
                self.ui_shell.set_workspace_layout(project.workspace_layout);
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Err(error) => {
                let message = format!("failed to load project: {error}");
                eprintln!("{message}");
                self.dialogs.show_error("Open failed", &message);
                false
            }
        }
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
            Command::NewDocument => self.activate_panel_control("builtin.app-actions", "app.new"),
            Command::SaveProject => self.save_project_to_current_path(),
            Command::SaveProjectAs => self.save_project_as(),
            Command::SaveProjectToPath { path } => self.save_project_to_path(PathBuf::from(path)),
            Command::LoadProject => self.open_project(),
            Command::LoadProjectFromPath { path } => self.load_project(PathBuf::from(path)),
            other => self.execute_document_command(other),
        }
    }

    fn activate_panel_control(&mut self, panel_id: &str, node_id: &str) -> bool {
        self.dispatch_panel_event(PanelEvent::Activate {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        })
    }

    fn execute_host_action(&mut self, action: HostAction) -> bool {
        match action {
            HostAction::DispatchCommand(command) => self.execute_command(command),
            HostAction::InvokePanelHandler { .. } => false,
            HostAction::MovePanel {
                panel_id,
                direction,
            } => {
                let changed = self.ui_shell.move_panel(&panel_id, direction);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                }
                changed
            }
            HostAction::SetPanelVisibility { panel_id, visible } => {
                let changed = self.ui_shell.set_panel_visibility(&panel_id, visible);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                }
                changed
            }
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
        let file_name = self
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_PATH);
        let hidden_panels = self
            .ui_shell
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / color={} / pages={} / panels={} / hidden={}",
            file_name,
            self.document.active_tool,
            self.document.active_color.hex_rgb(),
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.panels.len())
                .sum::<usize>(),
            hidden_panels,
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
            window.set_ime_allowed(self.app.has_focused_panel_input());
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
        self.record_canvas_input_if_needed(changed)
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
                self.record_canvas_input_if_needed(changed)
            }
            ElementState::Released => self.app.handle_pointer_released(x, y),
        }
    }

    fn record_canvas_input_if_needed(&mut self, changed: bool) -> bool {
        if changed && self.app.is_canvas_interacting() {
            self.profiler.record_canvas_input();
        }
        changed
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
                self.record_canvas_input_if_needed(changed)
            }
            TouchPhase::Moved => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                self.last_cursor_position = Some(position);
                let changed = self.app.handle_pointer_dragged(position.0, position.1);
                self.record_canvas_input_if_needed(changed)
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
            WindowEvent::Ime(ime) => {
                let changed = match ime {
                    Ime::Commit(text) => {
                        self.app.set_focused_panel_input_preedit(None);
                        self.app.insert_text_into_focused_panel_input(text.as_ref())
                    }
                    Ime::Preedit(text, _) => self
                        .app
                        .set_focused_panel_input_preedit(Some(text.to_string())),
                    Ime::Enabled | Ime::Disabled => false,
                };
                if changed {
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let editing_repeat = self.app.has_focused_panel_input()
                    && matches!(
                        &event.logical_key,
                        Key::Named(
                            NamedKey::Backspace
                                | NamedKey::Delete
                                | NamedKey::ArrowLeft
                                | NamedKey::ArrowRight
                                | NamedKey::Home
                                | NamedKey::End
                                | NamedKey::Space
                        ) | Key::Character(_)
                    );
                if event.state != ElementState::Pressed || (event.repeat && !editing_repeat) {
                    return;
                }

                let edited_text = if !self.modifiers.control_key() && !self.modifiers.alt_key() {
                    match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => self.app.backspace_focused_panel_input(),
                        Key::Named(NamedKey::Delete) => self.app.delete_focused_panel_input(),
                        Key::Named(NamedKey::ArrowLeft) => {
                            self.app.move_focused_panel_input_cursor(-1)
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            self.app.move_focused_panel_input_cursor(1)
                        }
                        Key::Named(NamedKey::Home) => {
                            self.app.move_focused_panel_input_cursor_to_start()
                        }
                        Key::Named(NamedKey::End) => {
                            self.app.move_focused_panel_input_cursor_to_end()
                        }
                        Key::Named(NamedKey::Space) => {
                            self.app.insert_text_into_focused_panel_input(" ")
                        }
                        Key::Character(text) => self.app.insert_text_into_focused_panel_input(text),
                        _ => false,
                    }
                } else {
                    false
                };
                if edited_text {
                    self.request_redraw();
                    return;
                }

                let changed = match &event.logical_key {
                    Key::Character(text)
                        if self.modifiers.control_key()
                            && self.modifiers.shift_key()
                            && text.eq_ignore_ascii_case("s") =>
                    {
                        self.app.execute_command(Command::SaveProjectAs)
                    }
                    Key::Character(text)
                        if self.modifiers.control_key() && text.eq_ignore_ascii_case("s") =>
                    {
                        self.app.execute_command(Command::SaveProject)
                    }
                    Key::Character(text)
                        if self.modifiers.control_key() && text.eq_ignore_ascii_case("o") =>
                    {
                        self.app.execute_command(Command::LoadProject)
                    }
                    Key::Character(text)
                        if self.modifiers.control_key() && text.eq_ignore_ascii_case("n") =>
                    {
                        self.app.execute_command(Command::NewDocument)
                    }
                    Key::Named(NamedKey::Tab) if self.modifiers.shift_key() => {
                        self.app.focus_previous_panel_control()
                    }
                    Key::Named(NamedKey::Tab) => self.app.focus_next_panel_control(),
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space)
                        if !self.app.has_focused_panel_input() =>
                    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_bridge::{
        CanvasPointerEvent, command_for_canvas_gesture, map_view_to_canvas,
    };
    use app_core::{ColorRgba8, ToolKind};
    use render::RenderFrame;
    use std::cell::RefCell;
    use winit::event::TouchPhase;

    #[derive(Default)]
    struct TestDialogs {
        open_paths: RefCell<Vec<PathBuf>>,
        save_paths: RefCell<Vec<PathBuf>>,
        errors: RefCell<Vec<(String, String)>>,
    }

    impl TestDialogs {
        fn with_open_path(path: PathBuf) -> Self {
            Self {
                open_paths: RefCell::new(vec![path]),
                save_paths: RefCell::new(Vec::new()),
                errors: RefCell::new(Vec::new()),
            }
        }

        fn with_save_path(path: PathBuf) -> Self {
            Self {
                open_paths: RefCell::new(Vec::new()),
                save_paths: RefCell::new(vec![path]),
                errors: RefCell::new(Vec::new()),
            }
        }
    }

    impl DesktopDialogs for TestDialogs {
        fn pick_open_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
            self.open_paths.borrow_mut().pop()
        }

        fn pick_save_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
            self.save_paths.borrow_mut().pop()
        }

        fn show_error(&self, title: &str, message: &str) {
            self.errors
                .borrow_mut()
                .push((title.to_string(), message.to_string()));
        }
    }

    fn test_app_with_dialogs(dialogs: TestDialogs) -> DesktopApp {
        DesktopApp::new_with_dialogs(
            PathBuf::from("/tmp/altpaint-test.altp.json"),
            Box::new(dialogs),
        )
    }

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
        let mut app = test_app_with_dialogs(TestDialogs::default());
        app.document.set_active_tool(ToolKind::Eraser);

        let _ = app.execute_command(Command::NewDocumentSized {
            width: 64,
            height: 64,
        });

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
        let mut app = test_app_with_dialogs(TestDialogs::default());
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 200, &mut profiler);

        assert!(
            app.ui_shell
                .focus_panel_node("builtin.app-actions", "app.save")
        );
        assert_eq!(
            app.activate_focused_panel_control(),
            Some(Command::SaveProject)
        );
    }

    #[test]
    fn parse_document_size_accepts_common_formats() {
        assert_eq!(parse_document_size("64x64"), Some((64, 64)));
        assert_eq!(parse_document_size("320 240"), Some((320, 240)));
        assert_eq!(parse_document_size("800,600"), Some((800, 600)));
        assert_eq!(parse_document_size("0x600"), None);
    }

    #[test]
    fn execute_command_new_document_opens_inline_form() {
        let mut app = test_app_with_dialogs(TestDialogs::default());

        assert!(app.execute_command(Command::NewDocument));
    }

    #[test]
    fn execute_command_load_project_uses_native_dialog_path() {
        let path = std::env::temp_dir().join("altpaint-open-dialog-test.altp.json");
        let mut source_app = test_app_with_dialogs(TestDialogs::default());
        assert!(
            source_app.execute_host_action(HostAction::SetPanelVisibility {
                panel_id: "builtin.tool-palette".to_string(),
                visible: false,
            })
        );
        save_project_to_path(
            &path,
            &source_app.document,
            &source_app.ui_shell.workspace_layout(),
        )
        .expect("project save should succeed");

        let mut app = test_app_with_dialogs(TestDialogs::with_open_path(path.clone()));
        assert!(app.execute_command(Command::LoadProject));
        assert_eq!(app.project_path, path);
        assert!(
            !app.ui_shell
                .panel_trees()
                .iter()
                .any(|panel| panel.id == "builtin.tool-palette")
        );

        let _ = std::fs::remove_file(app.project_path.clone());
    }

    #[test]
    fn execute_command_new_document_sized_replaces_bitmap() {
        let mut app = test_app_with_dialogs(TestDialogs::default());

        assert!(app.execute_command(Command::NewDocumentSized {
            width: 320,
            height: 240,
        }));

        let bitmap = app.document.active_bitmap().expect("bitmap exists");
        assert_eq!((bitmap.width, bitmap.height), (320, 240));
    }

    #[test]
    fn save_project_as_updates_project_path_and_persists_workspace_layout() {
        let path = std::env::temp_dir().join("altpaint-save-as-test.altp.json");
        let mut app = test_app_with_dialogs(TestDialogs::with_save_path(path.clone()));

        assert!(app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        }));
        assert!(app.execute_command(Command::SaveProjectAs));

        let loaded = load_project_from_path(&path).expect("saved project should load");
        assert_eq!(app.project_path, path);
        assert!(
            loaded
                .workspace_layout
                .panels
                .iter()
                .any(|entry| entry.id == "builtin.tool-palette" && !entry.visible)
        );

        let _ = std::fs::remove_file(app.project_path.clone());
    }

    #[test]
    fn load_project_restores_workspace_layout() {
        let path = std::env::temp_dir().join("altpaint-load-test.altp.json");
        let mut source_app = test_app_with_dialogs(TestDialogs::default());
        let before_ids = source_app
            .ui_shell
            .panel_trees()
            .iter()
            .map(|panel| panel.id)
            .collect::<Vec<_>>();
        let before_index = before_ids
            .iter()
            .position(|panel_id| *panel_id == "builtin.layers-panel")
            .expect("layers panel visible");
        assert!(source_app.execute_host_action(HostAction::MovePanel {
            panel_id: "builtin.layers-panel".to_string(),
            direction: plugin_api::PanelMoveDirection::Up,
        }));
        assert!(source_app.execute_host_action(HostAction::MovePanel {
            panel_id: "builtin.layers-panel".to_string(),
            direction: plugin_api::PanelMoveDirection::Up,
        }));
        assert!(
            source_app.execute_host_action(HostAction::SetPanelVisibility {
                panel_id: "builtin.tool-palette".to_string(),
                visible: false,
            })
        );
        save_project_to_path(
            &path,
            &source_app.document,
            &source_app.ui_shell.workspace_layout(),
        )
        .expect("project save should succeed");

        let mut app = test_app_with_dialogs(TestDialogs::default());
        assert!(app.execute_command(Command::LoadProjectFromPath {
            path: path.to_string_lossy().to_string(),
        }));

        let panels = app.ui_shell.panel_trees();
        assert!(
            !panels
                .iter()
                .any(|panel| panel.id == "builtin.tool-palette")
        );
        let visible_ids = panels.iter().map(|panel| panel.id).collect::<Vec<_>>();
        let layers_index = visible_ids
            .iter()
            .position(|panel_id| *panel_id == "builtin.layers-panel")
            .expect("layers panel visible");
        assert!(layers_index < before_index);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn move_panel_host_action_updates_status_without_full_recompose() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 200, &mut profiler);
        profiler.stats.clear();
        let layout = app.layout.clone().expect("layout exists");

        assert!(app.execute_host_action(HostAction::MovePanel {
            panel_id: "builtin.layers-panel".to_string(),
            direction: plugin_api::PanelMoveDirection::Up,
        }));
        let update = app.prepare_present_frame(1280, 200, &mut profiler);

        assert!(!profiler.stats.contains_key("ui_update"));
        assert!(!profiler.stats.contains_key("compose_full_frame"));
        assert!(profiler.stats.contains_key("compose_dirty_panel"));
        assert!(profiler.stats.contains_key("compose_dirty_status"));
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
    fn set_panel_visibility_updates_status_without_full_recompose() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        let mut profiler = DesktopProfiler::new();
        let _ = app.prepare_present_frame(1280, 200, &mut profiler);
        profiler.stats.clear();
        let layout = app.layout.clone().expect("layout exists");

        assert!(app.execute_host_action(HostAction::SetPanelVisibility {
            panel_id: "builtin.tool-palette".to_string(),
            visible: false,
        }));
        let update = app.prepare_present_frame(1280, 200, &mut profiler);

        assert!(!profiler.stats.contains_key("ui_update"));
        assert!(!profiler.stats.contains_key("compose_full_frame"));
        assert!(profiler.stats.contains_key("compose_dirty_panel"));
        assert!(profiler.stats.contains_key("compose_dirty_status"));
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
    fn desktop_app_loads_phase6_sample_panel_from_default_ui_directory() {
        let app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        assert!(
            default_panel_dir()
                .join("phase6-sample")
                .join("panel.altp-panel")
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
        let title = profiler::PerformanceSnapshot {
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

        let snapshot = profiler.latest_snapshot().expect("snapshot exists");
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

        let snapshot = profiler.latest_snapshot().expect("snapshot exists");
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
            | plugin_api::PanelNode::Slider { .. }
            | plugin_api::PanelNode::TextInput { .. } => false,
        })
    }
}
