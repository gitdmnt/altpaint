//! `desktop` は最小のデスクトップエントリポイント。
//!
//! フェーズ2では、最小のラスタキャンバスにマウス入力で点を描けるようにする。

mod canvas_bridge;
mod wgpu_canvas;

use anyhow::Result;
use app_core::{Command, Document, ToolKind};
use canvas_bridge::{
    command_for_canvas_gesture, map_view_to_canvas, CanvasInputState, CanvasPointerEvent,
};
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use storage::{load_document_from_path, save_document_to_path};
use ui_shell::{SlintPanelItem, UiShell};
use wgpu_canvas::{install_wgpu_underlay, update_canvas_state_from_document, WgpuCanvasState};

slint::include_modules!();

const DEFAULT_PROJECT_PATH: &str = "altpaint-project.altp.json";

fn main() -> Result<()> {
    let app = DesktopApp::new(PathBuf::from(DEFAULT_PROJECT_PATH));
    app.run()
}

struct DesktopApp {
    document: Document,
    ui_shell: UiShell,
    project_path: PathBuf,
    canvas_input: CanvasInputState,
    wgpu_canvas: Option<Rc<RefCell<WgpuCanvasState>>>,
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
            wgpu_canvas: None,
        }
    }

    fn run(mut self) -> Result<()> {
        let window = AppWindow::new()?;
        self.wgpu_canvas = Some(install_wgpu_underlay(&window.window())?);
        let state = Rc::new(RefCell::new(self));

        {
            let mut app = state.borrow_mut();
            app.sync_window(&window);
        }

        let weak_window = window.as_weak();
        let state_for_callback = state.clone();
        window.on_panel_command(move |command_text| {
            if let Ok(mut app) = state_for_callback.try_borrow_mut() {
                app.handle_command_text(command_text.as_str());
                if let Some(window) = weak_window.upgrade() {
                    app.sync_window(&window);
                }
            }
        });

        let weak_window = window.as_weak();
        let state_for_canvas = state.clone();
        window.on_canvas_pointer(move |action, x, y, width, height| {
            if let Ok(mut app) = state_for_canvas.try_borrow_mut() {
                app.handle_canvas_pointer(action.as_str(), x, y, width, height);
                if let Some(window) = weak_window.upgrade() {
                    app.sync_window(&window);
                }
            }
        });

        window.run()?;
        Ok(())
    }

    fn sync_window(&mut self, window: &AppWindow) {
        self.ui_shell.update(&self.document);
        let frame = self.ui_shell.render_frame(&self.document);

        let panels = self
            .ui_shell
            .slint_panels()
            .into_iter()
            .map(|panel| PanelData {
                id: panel.id.into(),
                title: panel.title.into(),
                items: ModelRc::new(VecModel::from(
                    panel.items.into_iter().map(map_panel_item).collect::<Vec<_>>(),
                )),
            })
            .collect::<Vec<_>>();

        window.set_panels(ModelRc::new(VecModel::from(panels)));
    window.set_canvas_image(render_frame_to_slint_image(&frame));
        window.set_status_text(self.status_text().into());
        if let Some(wgpu_canvas) = &self.wgpu_canvas {
            let clear_color = match self.document.active_tool {
                ToolKind::Brush => wgpu::Color {
                    r: 0.20,
                    g: 0.20,
                    b: 0.22,
                    a: 1.0,
                },
                ToolKind::Eraser => wgpu::Color {
                    r: 0.22,
                    g: 0.18,
                    b: 0.18,
                    a: 1.0,
                },
            };
            update_canvas_state_from_document(
                wgpu_canvas,
                format!("{:?}", self.document.active_tool),
                clear_color,
                frame,
                self.document.view_transform,
            );
        }
        window.window().request_redraw();
    }

    fn handle_command_text(&mut self, command_text: &str) {
        if let Some(command) = command_from_text(command_text) {
            self.execute_command(command);
        }
    }

    fn handle_canvas_pointer(&mut self, action: &str, x: i32, y: i32, width: i32, height: i32) {
        let Some((canvas_x, canvas_y)) = self.canvas_position_from_view(x, y, width, height) else {
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

    fn canvas_position_from_view(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Option<(usize, usize)> {
        let frame = self.ui_shell.render_frame(&self.document);
        map_view_to_canvas(
            &frame,
            CanvasPointerEvent {
                x,
                y,
                width,
                height,
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

fn render_frame_to_slint_image(frame: &render::RenderFrame) -> Image {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        frame.pixels.as_slice(),
        frame.width as u32,
        frame.height as u32,
    );
    Image::from_rgba8(buffer)
}

fn map_panel_item(item: SlintPanelItem) -> PanelItem {
    match item {
        SlintPanelItem::Text { text } => PanelItem {
            kind: SharedString::from("text"),
            title: SharedString::default(),
            text: text.into(),
            command: SharedString::default(),
            active: false,
        },
        SlintPanelItem::Button {
            label,
            command,
            active,
            ..
        } => PanelItem {
            kind: SharedString::from("button"),
            title: SharedString::default(),
            text: label.into(),
            command: command_to_text(&command).into(),
            active,
        },
        SlintPanelItem::Section { title, text } => PanelItem {
            kind: SharedString::from("section"),
            title: title.into(),
            text: text.into(),
            command: SharedString::default(),
            active: false,
        },
    }
}

fn command_to_text(command: &Command) -> String {
    match command {
        Command::Noop => "noop".to_string(),
        Command::DrawPoint { x, y } => format!("draw-point:{x}:{y}"),
        Command::ErasePoint { x, y } => format!("erase-point:{x}:{y}"),
        Command::DrawStroke {
            from_x,
            from_y,
            to_x,
            to_y,
        } => format!("draw-stroke:{from_x}:{from_y}:{to_x}:{to_y}"),
        Command::EraseStroke {
            from_x,
            from_y,
            to_x,
            to_y,
        } => format!("erase-stroke:{from_x}:{from_y}:{to_x}:{to_y}"),
        Command::SetActiveTool { tool } => format!("set-tool:{tool:?}"),
        Command::NewDocument => "new-document".to_string(),
        Command::SaveProject => "save-project".to_string(),
        Command::LoadProject => "load-project".to_string(),
    }
}

fn command_from_text(text: &str) -> Option<Command> {
    match text {
        "noop" => Some(Command::Noop),
        "new-document" => Some(Command::NewDocument),
        "save-project" => Some(Command::SaveProject),
        "load-project" => Some(Command::LoadProject),
        _ if text.starts_with("set-tool:") => match text.trim_start_matches("set-tool:") {
            "Brush" => Some(Command::SetActiveTool {
                tool: ToolKind::Brush,
            }),
            "Eraser" => Some(Command::SetActiveTool {
                tool: ToolKind::Eraser,
            }),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_bridge::{command_for_canvas_gesture, map_view_to_canvas, CanvasPointerEvent};
    use render::RenderFrame;
    use wgpu_canvas::{update_canvas_state_from_document, WgpuCanvasState};

    #[test]
    fn render_frame_converts_into_slint_image() {
        let frame = RenderFrame {
            width: 2,
            height: 1,
            pixels: vec![0, 0, 0, 255, 255, 255, 255, 255],
        };

        let image = render_frame_to_slint_image(&frame);

        assert_eq!(image.size().width, 2);
        assert_eq!(image.size().height, 1);
    }

    #[test]
    fn command_text_roundtrip_for_tool_switch() {
        let command = Command::SetActiveTool {
            tool: ToolKind::Eraser,
        };

        let encoded = command_to_text(&command);
        let decoded = command_from_text(&encoded);

        assert_eq!(decoded, Some(command));
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
    fn wgpu_canvas_state_updates_from_document_tool() {
        let state = Rc::new(RefCell::new(WgpuCanvasState::new()));

        update_canvas_state_from_document(
            &state,
            "Eraser".to_string(),
            wgpu::Color {
                r: 0.22,
                g: 0.18,
                b: 0.18,
                a: 1.0,
            },
            RenderFrame {
                width: 64,
                height: 64,
                pixels: vec![255; 64 * 64 * 4],
            },
            app_core::CanvasViewTransform::default(),
        );

        let state = state.borrow();
        assert_eq!(state.active_tool_label, "Eraser");
        assert_eq!(state.clear_color.r, 0.22);
        assert_eq!(state.frame.as_ref().map(|frame| frame.width), Some(64));
        assert_eq!(state.transform.zoom, 1.0);
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

        app.handle_canvas_pointer("down", 10, 10, 640, 640);
        app.handle_canvas_pointer("drag", 30, 10, 640, 640);
        app.handle_canvas_pointer("up", 30, 10, 640, 640);

        let frame = app.ui_shell.render_frame(&app.document);
        let start = ((1 * frame.width) + 1) * 4;
        assert_eq!(&frame.pixels[start..start + 4], &[0, 0, 0, 255]);
    }
}
