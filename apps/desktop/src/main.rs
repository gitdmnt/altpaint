//! `desktop` は最小のデスクトップエントリポイント。
//!
//! `winit` がウィンドウと入力を受け持ち、`wgpu` が合成済みフレームを提示する。

mod canvas_bridge;
mod wgpu_canvas;

use anyhow::{Context, Result};
use app_core::{Command, Document};
use canvas_bridge::{
    CanvasInputState, CanvasPointerEvent, command_for_canvas_gesture, map_view_to_canvas,
};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use plugin_api::HostAction;
use std::path::PathBuf;
use std::sync::Arc;
use storage::{load_document_from_path, save_document_to_path};
use ui_shell::{PanelSurface, UiShell};
use wgpu_canvas::WgpuPresenter;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

const DEFAULT_PROJECT_PATH: &str = "altpaint-project.altp.json";
const PANEL_SURFACE_WIDTH: usize = 264;
const PANEL_SURFACE_HEIGHT: usize = 800;
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
const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 8;

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
        let panel_surface_rect =
            fit_rect(PANEL_SURFACE_WIDTH, PANEL_SURFACE_HEIGHT, panel_host_rect);

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

struct DesktopApp {
    document: Document,
    ui_shell: UiShell,
    project_path: PathBuf,
    canvas_input: CanvasInputState,
    panel_surface: Option<PanelSurface>,
    layout: Option<DesktopLayout>,
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
        }
    }

    fn prepare_present_frame(
        &mut self,
        window_width: usize,
        window_height: usize,
    ) -> render::RenderFrame {
        self.ui_shell.update(&self.document);
        let canvas_frame = self.ui_shell.render_frame(&self.document);
        let panel_surface = self
            .ui_shell
            .render_panel_surface(PANEL_SURFACE_WIDTH, PANEL_SURFACE_HEIGHT);
        let layout = DesktopLayout::new(
            window_width,
            window_height,
            canvas_frame.width,
            canvas_frame.height,
        );
        let present_frame = compose_desktop_frame(
            window_width,
            window_height,
            &layout,
            &panel_surface,
            &canvas_frame,
            &self.status_text(),
        );

        self.panel_surface = Some(panel_surface);
        self.layout = Some(layout);
        present_frame
    }

    fn handle_pointer_pressed(&mut self, x: i32, y: i32) {
        if self.canvas_position_from_window(x, y).is_some() {
            self.handle_canvas_pointer("down", x, y);
        }
    }

    fn handle_pointer_released(&mut self, x: i32, y: i32) {
        if self.canvas_input.is_drawing {
            self.handle_canvas_pointer("up", x, y);
            return;
        }
        self.handle_panel_pointer(x, y);
    }

    fn handle_pointer_dragged(&mut self, x: i32, y: i32) {
        if self.canvas_input.is_drawing {
            self.handle_canvas_pointer("drag", x, y);
        }
    }

    fn handle_panel_pointer(&mut self, x: i32, y: i32) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let Some(panel_surface) = self.panel_surface.as_ref() else {
            return;
        };

        let Some((surface_x, surface_y)) = map_view_to_surface(
            panel_surface.width,
            panel_surface.height,
            layout.panel_surface_rect,
            x,
            y,
        ) else {
            return;
        };

        let Some(event) = panel_surface.hit_test(surface_x, surface_y) else {
            return;
        };

        for action in self.ui_shell.handle_panel_event(&event) {
            self.execute_host_action(action);
        }
    }

    fn handle_canvas_pointer(&mut self, action: &str, x: i32, y: i32) {
        let Some((canvas_x, canvas_y)) = self.canvas_position_from_window(x, y) else {
            if action == "up" {
                self.canvas_input = CanvasInputState::default();
            }
            return;
        };

        match action {
            "down" => {
                self.canvas_input.is_drawing = true;
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
                self.execute_canvas_command(canvas_x, canvas_y, None);
            }
            "drag" if self.canvas_input.is_drawing => {
                let from = self.canvas_input.last_position;
                self.execute_canvas_command(canvas_x, canvas_y, from);
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
            }
            "up" => {
                self.canvas_input.is_drawing = false;
                self.canvas_input.last_position = None;
            }
            _ => {}
        }
    }

    fn execute_canvas_command(&mut self, x: usize, y: usize, from: Option<(usize, usize)>) {
        let command = command_for_canvas_gesture(self.document.active_tool, (x, y), from);
        self.execute_command(command);
    }

    fn canvas_position_from_window(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let layout = self.layout.as_ref()?;
        if !layout.canvas_display_rect.contains(x, y) {
            return None;
        }

        let frame = self.ui_shell.render_frame(&self.document);
        map_view_to_canvas(
            &frame,
            CanvasPointerEvent {
                x: x - layout.canvas_display_rect.x as i32,
                y: y - layout.canvas_display_rect.y as i32,
                width: layout.canvas_display_rect.width as i32,
                height: layout.canvas_display_rect.height as i32,
            },
        )
    }

    fn execute_command(&mut self, command: Command) {
        match command {
            Command::SaveProject => {
                if let Err(error) = save_document_to_path(&self.project_path, &self.document) {
                    eprintln!("failed to save project: {error}");
                }
            }
            Command::LoadProject => match load_document_from_path(&self.project_path) {
                Ok(document) => self.document = document,
                Err(error) => eprintln!("failed to load project: {error}"),
            },
            other => {
                let _ = self.document.apply_command(&other);
            }
        }
    }

    fn execute_host_action(&mut self, action: HostAction) {
        match action {
            HostAction::DispatchCommand(command) => self.execute_command(command),
        }
    }

    fn status_text(&self) -> String {
        format!(
            "tool={:?} / pages={} / panels={}",
            self.document.active_tool,
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
}

impl DesktopRuntime {
    fn new(project_path: PathBuf) -> Self {
        Self {
            app: DesktopApp::new(project_path),
            window: None,
            presenter: None,
            last_cursor_position: None,
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

        let _ = self
            .app
            .prepare_present_frame(size.width as usize, size.height as usize);
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
                let _ = self
                    .app
                    .prepare_present_frame(size.width as usize, size.height as usize);
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = (position.x as i32, position.y as i32);
                self.last_cursor_position = Some(position);
                self.app.handle_pointer_dragged(position.0, position.1);
                self.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if let Some((x, y)) = self.last_cursor_position {
                    match state {
                        ElementState::Pressed => self.app.handle_pointer_pressed(x, y),
                        ElementState::Released => self.app.handle_pointer_released(x, y),
                    }
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
                let frame = self
                    .app
                    .prepare_present_frame(size.width as usize, size.height as usize);
                if let Err(error) = presenter.render(&frame) {
                    eprintln!("render failed: {error}");
                    event_loop.exit();
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

fn compose_desktop_frame(
    width: usize,
    height: usize,
    layout: &DesktopLayout,
    panel_surface: &PanelSurface,
    canvas_frame: &render::RenderFrame,
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
        canvas_frame.width,
        canvas_frame.height,
        canvas_frame.pixels.as_slice(),
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
    for (index, ch) in text.chars().enumerate() {
        draw_glyph(frame, x + index * FONT_WIDTH, y, ch, color);
    }
}

fn draw_glyph(frame: &mut render::RenderFrame, x: usize, y: usize, ch: char, color: [u8; 4]) {
    let glyph = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?'));
    let Some(glyph) = glyph else {
        return;
    };

    for (row, bits) in glyph.iter().enumerate().take(FONT_HEIGHT) {
        for col in 0..FONT_WIDTH {
            if ((bits >> col) & 1) == 1 {
                write_pixel(frame, x + col, y + row, color);
            }
        }
    }
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
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return;
    }

    for dst_y in 0..destination.height {
        let src_y = (((dst_y as f32 / destination.height as f32) * source_height as f32).floor()
            as usize)
            .min(source_height - 1);
        for dst_x in 0..destination.width {
            let src_x = (((dst_x as f32 / destination.width as f32) * source_width as f32).floor()
                as usize)
                .min(source_width - 1);
            let src_index = (src_y * source_width + src_x) * 4;
            write_pixel(
                frame,
                destination.x + dst_x,
                destination.y + dst_y,
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
    use app_core::ToolKind;
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
    fn execute_command_updates_document_tool() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        app.execute_command(Command::SetActiveTool {
            tool: ToolKind::Eraser,
        });

        assert_eq!(app.document.active_tool, ToolKind::Eraser);
    }

    #[test]
    fn execute_command_new_document_resets_tool_to_default() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));
        app.document.set_active_tool(ToolKind::Eraser);

        app.execute_command(Command::NewDocument);

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
        let _ = app.prepare_present_frame(1280, 800);
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
    fn host_action_dispatches_tool_switch_command() {
        let mut app = DesktopApp::new(PathBuf::from("/tmp/altpaint-test.altp.json"));

        app.execute_host_action(HostAction::DispatchCommand(Command::SetActiveTool {
            tool: ToolKind::Eraser,
        }));

        assert_eq!(app.document.active_tool, ToolKind::Eraser);
    }

    #[test]
    fn compose_desktop_frame_writes_panel_and_canvas_regions() {
        let layout = DesktopLayout::new(640, 480, 64, 64);
        let shell = UiShell::new();
        let panel_surface = shell.render_panel_surface(264, 800);
        let canvas_frame = RenderFrame {
            width: 2,
            height: 2,
            pixels: vec![16; 16],
        };

        let frame =
            compose_desktop_frame(640, 480, &layout, &panel_surface, &canvas_frame, "status");

        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [16, 16, 16, 16])
        );
    }
}
