//! `winit` のイベントループと `DesktopApp` を接続するランタイム層。
//!
//! OS イベントをアプリ本体へ橋渡しし、`wgpu` 提示や IME 制御を含む
//! 実行時の副作用を一箇所へ閉じ込める。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use app_core::Command;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::DesktopApp;
use crate::config::{WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH};
use crate::profiler::DesktopProfiler;
use crate::wgpu_canvas::{
    CanvasLayer, FrameLayer, PresentScene, TextureSource, UploadRegion, WgpuPresenter,
};

/// `winit` アプリケーションとして振る舞う実行時コンテナを表す。
pub(crate) struct DesktopRuntime {
    app: DesktopApp,
    window: Option<Arc<Window>>,
    presenter: Option<WgpuPresenter>,
    last_cursor_position: Option<(i32, i32)>,
    last_cursor_position_f64: Option<(f64, f64)>,
    pending_wheel_pan: (f32, f32),
    pending_wheel_zoom_lines: f32,
    active_touch_id: Option<u64>,
    profiler: DesktopProfiler,
    modifiers: ModifiersState,
}

impl DesktopRuntime {
    const WHEEL_ANIMATION_BLEND: f32 = 0.45;
    const WHEEL_PAN_MIN_STEP: f32 = 0.5;
    const WHEEL_ZOOM_MIN_STEP_LINES: f32 = 0.02;

    fn wheel_delta_lines(delta: MouseScrollDelta) -> (f32, f32) {
        match delta {
            MouseScrollDelta::LineDelta(x, y) => (x, y),
            MouseScrollDelta::PixelDelta(position) => (
                position.x as f32 / ui_shell::text_line_height() as f32,
                position.y as f32 / ui_shell::text_line_height() as f32,
            ),
        }
    }

    /// 既定プロジェクトパスからランタイムを初期化する。
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self {
            app: DesktopApp::new(project_path),
            window: None,
            presenter: None,
            last_cursor_position: None,
            last_cursor_position_f64: None,
            pending_wheel_pan: (0.0, 0.0),
            pending_wheel_zoom_lines: 0.0,
            active_touch_id: None,
            profiler: DesktopProfiler::new(),
            modifiers: ModifiersState::default(),
        }
    }

    /// `EventLoop` を生成して `DesktopRuntime` を起動する。
    pub(crate) fn run(project_path: PathBuf) -> anyhow::Result<()> {
        let event_loop = EventLoop::new().context("failed to create event loop")?;
        let mut runtime = Self::new(project_path);
        event_loop
            .run_app(&mut runtime)
            .context("failed to run desktop runtime")
    }

    /// 必要なら IME 設定を更新しつつ再描画を要求する。
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.set_ime_allowed(self.app.has_focused_panel_input());
            window.request_redraw();
        }
    }

    /// このランタイムが所有しているウィンドウ ID を返す。
    fn active_window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// マウス移動をドラッグ更新へ変換する。
    fn handle_mouse_cursor_moved(&mut self, x: i32, y: i32) -> bool {
        if self.active_touch_id.is_some() {
            return false;
        }

        let position = (x, y);
        self.last_cursor_position = Some(position);
        self.last_cursor_position_f64 = Some((x as f64, y as f64));
        self.profiler
            .record("canvas_input_window_event", std::time::Duration::ZERO);
        let hover_changed = self.app.update_canvas_hover(position.0, position.1);
        let changed = self.app.handle_pointer_dragged(position.0, position.1) || hover_changed;
        self.record_canvas_input_if_needed(changed)
    }

    /// raw mouse delta を使って描画中のサンプル密度を上げる。
    fn handle_raw_mouse_motion(&mut self, delta_x: f64, delta_y: f64) -> bool {
        if self.active_touch_id.is_some() || !self.app.is_canvas_interacting() {
            return false;
        }

        let Some((cursor_x, cursor_y)) = self.last_cursor_position_f64 else {
            return false;
        };

        let next_x = cursor_x + delta_x;
        let next_y = cursor_y + delta_y;
        self.last_cursor_position_f64 = Some((next_x, next_y));
        let next_position = (next_x.round() as i32, next_y.round() as i32);
        if Some(next_position) == self.last_cursor_position {
            return false;
        }

        self.last_cursor_position = Some(next_position);
        self.profiler
            .record("canvas_input_raw_event", std::time::Duration::ZERO);
        let changed = self
            .app
            .handle_pointer_dragged(next_position.0, next_position.1);
        self.record_canvas_input_if_needed(changed)
    }

    /// 左ボタン押下・解放をアプリ側ポインタ処理へ流す。
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

    /// キャンバス操作中のみ入力計測サンプルを記録する。
    fn record_canvas_input_if_needed(&mut self, changed: bool) -> bool {
        if changed && self.app.is_canvas_interacting() {
            self.profiler
                .record("canvas_input_dispatch", std::time::Duration::ZERO);
            self.profiler.record_canvas_input();
        }
        changed
    }

    fn has_pending_wheel_animation(&self) -> bool {
        self.pending_wheel_pan.0.abs() > f32::EPSILON
            || self.pending_wheel_pan.1.abs() > f32::EPSILON
            || self.pending_wheel_zoom_lines.abs() > f32::EPSILON
    }

    fn take_animated_step(pending: &mut f32, min_step: f32) -> f32 {
        if pending.abs() <= min_step {
            let step = *pending;
            *pending = 0.0;
            return step;
        }

        let mut step = *pending * Self::WHEEL_ANIMATION_BLEND;
        if step.abs() < min_step {
            step = pending.signum() * min_step;
        }
        *pending -= step;
        step
    }

    fn advance_wheel_animation(&mut self) -> bool {
        let pan_x =
            Self::take_animated_step(&mut self.pending_wheel_pan.0, Self::WHEEL_PAN_MIN_STEP);
        let pan_y =
            Self::take_animated_step(&mut self.pending_wheel_pan.1, Self::WHEEL_PAN_MIN_STEP);
        let zoom_lines = Self::take_animated_step(
            &mut self.pending_wheel_zoom_lines,
            Self::WHEEL_ZOOM_MIN_STEP_LINES,
        );

        let mut changed = false;
        if pan_x.abs() > f32::EPSILON || pan_y.abs() > f32::EPSILON {
            changed |= self.app.execute_command(Command::PanView {
                delta_x: pan_x,
                delta_y: pan_y,
            });
        }

        if zoom_lines.abs() > f32::EPSILON {
            let current = self.app.document.view_transform.zoom;
            let next_zoom = (current * 1.1_f32.powf(-zoom_lines)).clamp(0.25, 16.0);
            if (next_zoom - current).abs() > f32::EPSILON {
                changed |= self
                    .app
                    .execute_command(Command::SetViewZoom { zoom: next_zoom });
            } else {
                self.pending_wheel_zoom_lines = 0.0;
            }
        }

        changed
    }

    fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) -> bool {
        let Some((x, y)) = self.last_cursor_position else {
            return false;
        };
        let Some(layout) = self.app.layout.as_ref() else {
            return false;
        };
        let on_panel = layout.panel_host_rect.contains(x, y);
        let on_canvas = layout.canvas_host_rect.contains(x, y);
        let (delta_x_lines, delta_y_lines) = Self::wheel_delta_lines(delta);

        if on_panel {
            let delta_lines = -(delta_y_lines.round() as i32);
            if delta_lines == 0 {
                return false;
            }
            return self.app.scroll_panel_surface(delta_lines);
        }

        if !on_canvas {
            return false;
        }

        self.profiler
            .record("canvas_input_wheel_event", std::time::Duration::ZERO);

        if self.modifiers.control_key() {
            if delta_y_lines.abs() <= f32::EPSILON {
                return false;
            }
            self.pending_wheel_zoom_lines += delta_y_lines;
            return self.advance_wheel_animation();
        }

        let mut delta_x = -delta_x_lines * 32.0;
        let mut delta_y = -delta_y_lines * 32.0;
        if self.modifiers.shift_key() && delta_x.abs() <= f32::EPSILON {
            delta_x = delta_y;
            delta_y = 0.0;
        }
        if delta_x.abs() <= f32::EPSILON && delta_y.abs() <= f32::EPSILON {
            return false;
        }

        self.pending_wheel_pan.0 += delta_x;
        self.pending_wheel_pan.1 += delta_y;
        self.advance_wheel_animation()
    }

    /// タッチイベントを単一アクティブポインタとして処理する。
    fn handle_touch_phase(&mut self, touch_id: u64, phase: TouchPhase, x: i32, y: i32) -> bool {
        let position = (x, y);

        match phase {
            TouchPhase::Started => {
                if matches!(self.active_touch_id, Some(active_id) if active_id != touch_id) {
                    return false;
                }

                self.active_touch_id = Some(touch_id);
                self.last_cursor_position = Some(position);
                self.last_cursor_position_f64 = Some((position.0 as f64, position.1 as f64));
                let changed = self.app.handle_pointer_pressed(position.0, position.1);
                self.record_canvas_input_if_needed(changed)
            }
            TouchPhase::Moved => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                self.last_cursor_position = Some(position);
                self.last_cursor_position_f64 = Some((position.0 as f64, position.1 as f64));
                let changed = self.app.handle_pointer_dragged(position.0, position.1);
                self.record_canvas_input_if_needed(changed)
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                self.last_cursor_position = Some(position);
                self.last_cursor_position_f64 = Some((position.0 as f64, position.1 as f64));
                self.active_touch_id = None;
                self.app.handle_pointer_released(position.0, position.1)
            }
        }
    }

    fn normalized_shortcut(&self, key: &Key) -> Option<(String, String)> {
        let key_name = normalized_key_name(key)?;
        let mut parts = Vec::new();
        if self.modifiers.control_key() {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.alt_key() {
            parts.push("Alt".to_string());
        }
        if self.modifiers.super_key() {
            parts.push("Meta".to_string());
        }
        if self.modifiers.shift_key() {
            parts.push("Shift".to_string());
        }
        parts.push(key_name.clone());
        Some((parts.join("+"), key_name))
    }
}

fn normalized_key_name(key: &Key) -> Option<String> {
    match key {
        Key::Character(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_uppercase())
            }
        }
        Key::Named(named) => match named {
            NamedKey::Space => Some("Space".to_string()),
            NamedKey::Enter => Some("Enter".to_string()),
            NamedKey::Tab => Some("Tab".to_string()),
            NamedKey::Backspace => Some("Backspace".to_string()),
            NamedKey::Delete => Some("Delete".to_string()),
            NamedKey::ArrowLeft => Some("ArrowLeft".to_string()),
            NamedKey::ArrowRight => Some("ArrowRight".to_string()),
            NamedKey::ArrowUp => Some("ArrowUp".to_string()),
            NamedKey::ArrowDown => Some("ArrowDown".to_string()),
            NamedKey::Home => Some("Home".to_string()),
            NamedKey::End => Some("End".to_string()),
            NamedKey::PageUp => Some("PageUp".to_string()),
            NamedKey::PageDown => Some("PageDown".to_string()),
            NamedKey::Escape => Some("Escape".to_string()),
            NamedKey::F1 => Some("F1".to_string()),
            NamedKey::F2 => Some("F2".to_string()),
            NamedKey::F3 => Some("F3".to_string()),
            NamedKey::F4 => Some("F4".to_string()),
            NamedKey::F5 => Some("F5".to_string()),
            NamedKey::F6 => Some("F6".to_string()),
            NamedKey::F7 => Some("F7".to_string()),
            NamedKey::F8 => Some("F8".to_string()),
            NamedKey::F9 => Some("F9".to_string()),
            NamedKey::F10 => Some("F10".to_string()),
            NamedKey::F11 => Some("F11".to_string()),
            NamedKey::F12 => Some("F12".to_string()),
            NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super => None,
            other => Some(format!("{other:?}")),
        },
        _ => None,
    }
}

impl ApplicationHandler for DesktopRuntime {
    /// 初回 resume 時にウィンドウと `wgpu` presenter を初期化する。
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

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.handle_raw_mouse_motion(delta.0, delta.1)
        {
            self.request_redraw();
        }
    }

    /// `winit` のウィンドウイベントをアプリ更新へ変換する。
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
                let changed = self.handle_mouse_wheel(delta);

                if changed {
                    self.profiler.record_canvas_input();
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

                if let Some((shortcut, key_name)) = self.normalized_shortcut(&event.logical_key)
                    && self
                        .app
                        .dispatch_keyboard_shortcut(&shortcut, &key_name, event.repeat)
                {
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
                let Some(window) = self.window.clone() else {
                    return;
                };
                let _ = self.advance_wheel_animation();
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
                let Some(base_frame) = self.app.base_frame() else {
                    return;
                };
                let Some(overlay_frame) = self.app.overlay_frame() else {
                    return;
                };
                let base_upload_region = update.base_dirty_rect.map(|rect| UploadRegion {
                    x: rect.x as u32,
                    y: rect.y as u32,
                    width: rect.width as u32,
                    height: rect.height as u32,
                });
                let overlay_upload_region = update.overlay_dirty_rect.map(|rect| UploadRegion {
                    x: rect.x as u32,
                    y: rect.y as u32,
                    width: rect.width as u32,
                    height: rect.height as u32,
                });
                let canvas_layer = self.app.document.active_bitmap().and_then(|bitmap| {
                    self.app.canvas_texture_quad().map(|quad| CanvasLayer {
                        source: TextureSource {
                            width: bitmap.width as u32,
                            height: bitmap.height as u32,
                            pixels: bitmap.pixels.as_slice(),
                        },
                        upload_region: update.canvas_dirty_rect.map(|rect| UploadRegion {
                            x: rect.x as u32,
                            y: rect.y as u32,
                            width: rect.width as u32,
                            height: rect.height as u32,
                        }),
                        quad,
                    })
                });
                let present_started = Instant::now();
                let timings = match presenter.render(PresentScene {
                    base_layer: FrameLayer {
                        source: TextureSource::from(base_frame),
                        upload_region: base_upload_region,
                    },
                    overlay_layer: FrameLayer {
                        source: TextureSource::from(overlay_frame),
                        upload_region: overlay_upload_region,
                    },
                    canvas_layer,
                }) {
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
                if self.app.is_canvas_interacting() || self.has_pending_wheel_animation() {
                    self.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
