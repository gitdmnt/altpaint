//! `winit` のイベントループと `DesktopApp` を接続するランタイム層。
//!
//! OS イベントをアプリ本体へ橋渡しし、`wgpu` 提示や IME 制御を含む
//! 実行時の副作用を一箇所へ閉じ込める。

mod keyboard;
mod pointer;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use desktop_support::{DesktopProfiler, WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::DesktopApp;
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
    last_touch_pressure: f32,
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

    /// 既定プロジェクトパスからランタイムを初期化する。
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self {
            app: DesktopApp::new(project_path),
            window: None,
            presenter: None,
            last_cursor_position: None,
            last_cursor_position_f64: None,
            last_touch_pressure: 1.0,
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

    /// raw mouse input を描画継続用イベントとして受け取る。
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
                if self.handle_touch_phase(
                    touch.id,
                    touch.phase,
                    position.0,
                    position.1,
                    touch.force,
                ) {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.handle_mouse_wheel(delta) {
                    self.profiler.record_canvas_input();
                    self.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Ime(ime) => {
                if self.handle_ime_event(ime) {
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.handle_keyboard_input(&event) {
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
                if !self.app.is_canvas_interacting() && !self.has_pending_wheel_animation() {
                    let _ = self.app.flush_deferred_view_panel_sync();
                    let _ = self.app.flush_deferred_status_refresh();
                }
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
                let canvas_layer = self.app.canvas_frame().and_then(|bitmap| {
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
