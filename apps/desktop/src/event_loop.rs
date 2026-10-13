//! `winit` のイベントループと `DesktopApp` を接続するイベントループ層。
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
use frame_profiler::FrameProfiler;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::DesktopApp;
use crate::presenter::WgpuPresenter;
use crate::presenter::theme::{FOOTER_HEIGHT, WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH};

/// `winit` アプリケーションとして振る舞うイベントループホストを表す。
pub(crate) struct DesktopEventLoop {
    app: DesktopApp,
    window: Option<Arc<Window>>,
    presenter: Option<WgpuPresenter>,
    last_cursor_position: Option<(i32, i32)>,
    last_cursor_position_f64: Option<(f64, f64)>,
    last_touch_pressure: f32,
    pending_wheel_pan: (f32, f32),
    pending_wheel_zoom_lines: f32,
    active_touch_id: Option<u64>,
    profiler: FrameProfiler,
    modifiers: ModifiersState,
}

impl DesktopEventLoop {
    const WHEEL_ANIMATION_BLEND: f32 = 0.45;
    /// BL-064: pan アニメーションの最小ステップ (line 単位)。
    /// 従来の 0.5px を 32px/line で割った値 (挙動を維持する)。
    const WHEEL_PAN_MIN_STEP: f32 = 0.5 / editor_state::view_policy::PAN_PIXELS_PER_LINE;
    const WHEEL_ZOOM_MIN_STEP_LINES: f32 = 0.02;

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
            profiler: FrameProfiler::new(),
            modifiers: ModifiersState::default(),
        }
    }

    pub(crate) fn run(project_path: PathBuf) -> anyhow::Result<()> {
        let event_loop = EventLoop::new().context("failed to create event loop")?;
        let mut handler = Self::new(project_path);
        event_loop
            .run_app(&mut handler)
            .context("failed to run desktop event loop")
    }

    /// ADR 014 でテキスト入力は HTML パネル内部完結に統一済みのため、IME 許可は常に false。
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.set_ime_allowed(false);
            window.request_redraw();
        }
    }

    fn active_window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// 1 フレームを提示する (BL-115)。
    ///
    /// OS 由来の前処理 (ホイールアニメーション前進・遅延同期のフラッシュ) を行い、
    /// `app.compose_frame()` で提示フレームを所有形に組み立て、`presenter.render()`
    /// へ渡す。`PresentFrame` の構成データ取得・借用順序はすべて `compose_frame`
    /// 内部に閉じ込められている。
    fn render_frame(&mut self, window: &Window) -> FrameOutcome {
        let wheel_t = Instant::now();
        let _ = self.advance_wheel_animation();
        self.profiler.record("wheel_animation", wheel_t.elapsed());
        if !self.app.is_canvas_interacting() && !self.has_pending_wheel_animation() {
            let sync_t = Instant::now();
            let _ = self.app.flush_deferred_view_panel_sync();
            let _ = self.app.flush_deferred_status_refresh();
            self.profiler.record("deferred_view_sync", sync_t.elapsed());
        }
        if self.presenter.is_none() {
            return FrameOutcome::Continue;
        }

        let size = window.inner_size();
        let frame_started = Instant::now();
        let composed = self.app.compose_frame(
            size.width,
            size.height,
            FOOTER_HEIGHT as u32,
            &mut self.profiler,
        );

        // presenter (&mut) と app.layer_texture_store() (&) は disjoint フィールドのため
        // 借用を分割して同時に渡せる。
        let present_started = Instant::now();
        let presenter = self.presenter.as_mut().expect("presenter checked above");
        let timings = match presenter.render(composed.present_frame(), self.app.layer_texture_store())
        {
            Ok(timings) => timings,
            Err(error) => {
                eprintln!("render failed: {error}");
                return FrameOutcome::Exit;
            }
        };
        self.profiler
            .record_stage(frame_profiler::FrameStage::PresentTotal, present_started.elapsed());
        self.profiler.record_present(timings);
        if composed.canvas_updated {
            self.profiler.record_canvas_present();
        }
        if let Some(report) = self.profiler.finish_frame(frame_started.elapsed()) {
            crate::profiling::print_frame_report(&report);
        }
        window.set_title(&crate::profiling::window_title(self.profiler.latest_snapshot()));
        if self.app.is_canvas_interacting() || self.has_pending_wheel_animation() {
            self.request_redraw();
        }
        FrameOutcome::Continue
    }
}

/// `render_frame` の結果。提示失敗時はイベントループ終了を要求する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameOutcome {
    Continue,
    Exit,
}

impl ApplicationHandler for DesktopEventLoop {
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

        self.app.install_gpu_resources(
            presenter.device(),
            presenter.queue(),
        );

        self.app
            .install_panel_gpu_context(presenter.device(), presenter.queue());
        self.app.mark_all_panels_dirty();

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
                if self.render_frame(&window) == FrameOutcome::Exit {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}
