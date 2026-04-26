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
    CanvasLayer, CanvasLayerSource, FrameLayer, PresentScene, TextureSource, UploadRegion,
    WgpuPresenter,
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

    /// 既定値を使って新しいインスタンスを生成する。
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

    /// イベントループを開始し、デスクトップ実行を継続する。
    ///
    /// 失敗時はエラーを返します。
    pub(crate) fn run(project_path: PathBuf) -> anyhow::Result<()> {
        let event_loop = EventLoop::new().context("failed to create event loop")?;
        let mut runtime = Self::new(project_path);
        event_loop
            .run_app(&mut runtime)
            .context("failed to run desktop runtime")
    }

    /// 次のフレームで再描画が行われるよう要求する。
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.set_ime_allowed(self.app.has_focused_panel_input());
            window.request_redraw();
        }
    }

    /// アクティブな ウィンドウ ID を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn active_window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }
}

impl ApplicationHandler for DesktopRuntime {
    /// 入力や種別に応じて処理を振り分ける。
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

        #[cfg(feature = "html-panel")]
        {
            self.app
                .panel_runtime
                .install_gpu_context(presenter.device(), presenter.queue());
            self.app.panel_runtime.mark_all_dirty();
        }

        let _ = self.app.prepare_present_frame(
            size.width as usize,
            size.height as usize,
            &mut self.profiler,
        );
        self.presenter = Some(presenter);
        self.window = Some(window);
        self.request_redraw();
    }

    /// device イベント に必要な処理を行う。
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

    /// 入力や種別に応じて処理を振り分ける。
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
                let wheel_t = Instant::now();
                let _ = self.advance_wheel_animation();
                self.profiler.record("wheel_animation", wheel_t.elapsed());
                if !self.app.is_canvas_interacting() && !self.has_pending_wheel_animation() {
                    let sync_t = Instant::now();
                    let _ = self.app.flush_deferred_view_panel_sync();
                    let _ = self.app.flush_deferred_status_refresh();
                    self.profiler.record("deferred_view_sync", sync_t.elapsed());
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
                // canvas_texture_quad は &mut self を必要とするため frame 参照の取得より先に呼ぶ
                let quad_t = Instant::now();
                let canvas_quad = self.app.canvas_texture_quad();
                self.profiler.record("canvas_texture_quad", quad_t.elapsed());

                // HTML パネル描画も先に処理（&mut panel_runtime を必要とするため、
                // 続く &self.app 借用と衝突しない順序で実施）。
                #[cfg(feature = "html-panel")]
                struct HtmlQuadEntry {
                    panel_id: String,
                    texture_ptr: *const wgpu::Texture,
                    screen_rect: render_types::PixelRect,
                }
                #[cfg(feature = "html-panel")]
                let html_quad_entries: Vec<HtmlQuadEntry> = {
                    const HTML_CHROME_HEIGHT: u32 = 24;
                    // 9E-3: DSL/HTML 両方の GPU 対応パネルを統一的に扱う
                    let all_panel_ids = self.app.panel_runtime.panel_ids_with_gpu();
                    let (panel_ids, hidden_ids): (Vec<String>, Vec<String>) = all_panel_ids
                        .into_iter()
                        .partition(|id| self.app.panel_presentation.is_panel_visible(id));
                    // 不可視になったパネルの hit / move handle 情報は掃除する
                    for id in &hidden_ids {
                        self.app.panel_presentation.remove_html_panel_hits(id);
                        self.app.panel_presentation.remove_html_panel_move_handle(id);
                    }
                    if panel_ids.is_empty() {
                        Vec::new()
                    } else {
                        // GPU パネルのサイズは Engine が保有する measured_size が権威。
                        // 位置は workspace_layout の position（panel_rect_in_viewport の位置部分）から取る。
                        // viewport は GPU テクスチャの上限としてそのまま渡し、Engine 側でクランプさせる。
                        let measured = self.app.panel_runtime.panel_measured_sizes();
                        let mut sized: Vec<(String, u32, u32)> = Vec::with_capacity(panel_ids.len());
                        let mut panel_rects: Vec<render_types::PixelRect> =
                            Vec::with_capacity(panel_ids.len());
                        for id in &panel_ids {
                            // measured_size を取得
                            let (mw, mh) = measured
                                .iter()
                                .find(|(pid, _, _)| pid == id)
                                .map(|(_, w, h)| (*w, *h))
                                .unwrap_or((1, 1));
                            // 位置は workspace_layout の position を使う（サイズは measured で上書き）
                            let position_rect = self
                                .app
                                .panel_presentation
                                .panel_rect_in_viewport(
                                    id,
                                    size.width as usize,
                                    size.height as usize,
                                )
                                .unwrap_or(render_types::PixelRect {
                                    x: 0,
                                    y: 0,
                                    width: mw as usize,
                                    height: mh as usize,
                                });
                            let panel_rect = render_types::PixelRect {
                                x: position_rect.x,
                                y: position_rect.y,
                                width: mw as usize,
                                height: mh as usize,
                            };
                            // viewport を Engine に渡す（クランプ用）
                            sized.push((id.clone(), size.width, size.height));
                            panel_rects.push(panel_rect);
                        }
                        let frames = self.app.panel_runtime.render_panels(
                            &sized,
                            1.0,
                            HTML_CHROME_HEIGHT,
                        );
                        // 描画されたパネルの hit/move 情報と quad entry を一気に組み立てる
                        // (frames は &mut panel_runtime に紐付くため、この間 panel_runtime は再借用しない)
                        type FrameMeta = (
                            String,
                            *const wgpu::Texture,
                            render_types::PixelRect, // パネル全体 (chrome 含む)
                            render_types::PixelRect, // body 部分 (hit 領域)
                            render_types::PixelRect, // chrome 部分 (move handle)
                            Vec<(String, render_types::PixelRect)>,
                        );
                        let mut frame_meta: Vec<FrameMeta> = Vec::with_capacity(frames.len());
                        for frame in frames.iter() {
                            // panel_id に対応する panel_rect を取得
                            let panel_rect = panel_ids
                                .iter()
                                .position(|id| id == &frame.panel_id)
                                .map(|i| panel_rects[i])
                                .unwrap_or(render_types::PixelRect {
                                    x: 0,
                                    y: 0,
                                    width: frame.width as usize,
                                    height: frame.height as usize,
                                });
                            let chrome_h = HTML_CHROME_HEIGHT as usize;
                            let body_screen_rect = render_types::PixelRect {
                                x: panel_rect.x,
                                y: panel_rect.y + chrome_h,
                                width: panel_rect.width,
                                height: panel_rect.height.saturating_sub(chrome_h),
                            };
                            let chrome_screen_rect = render_types::PixelRect {
                                x: panel_rect.x,
                                y: panel_rect.y,
                                width: panel_rect.width,
                                height: chrome_h,
                            };
                            let hits: Vec<(String, render_types::PixelRect)> = frame
                                .hit_regions
                                .iter()
                                .filter_map(|hit| {
                                    let element_id = hit.element_id.clone()?;
                                    Some((
                                        element_id,
                                        render_types::PixelRect {
                                            x: hit.rect.x as usize,
                                            y: hit.rect.y as usize,
                                            width: hit.rect.width as usize,
                                            height: hit.rect.height as usize,
                                        },
                                    ))
                                })
                                .collect();
                            frame_meta.push((
                                frame.panel_id.clone(),
                                frame.texture as *const wgpu::Texture,
                                panel_rect,
                                body_screen_rect,
                                chrome_screen_rect,
                                hits,
                            ));
                        }
                        // 描画ループ完了後にまとめて hit/move テーブルを更新する
                        let mut entries = Vec::with_capacity(frame_meta.len());
                        for (
                            panel_id,
                            texture_ptr,
                            panel_rect,
                            body_screen_rect,
                            chrome_screen_rect,
                            hits,
                        ) in frame_meta
                        {
                            self.app.panel_presentation.update_html_panel_hits(
                                &panel_id,
                                body_screen_rect,
                                hits,
                            );
                            self.app
                                .panel_presentation
                                .update_html_panel_move_handle(&panel_id, chrome_screen_rect);
                            entries.push(HtmlQuadEntry {
                                panel_id,
                                texture_ptr,
                                screen_rect: panel_rect,
                            });
                        }
                        // measured_size 変化を workspace_layout に反映（永続化に流す）
                        let size_changes = self.app.panel_runtime.take_panel_size_changes();
                        for (panel_id, (new_w, new_h)) in size_changes {
                            self.app.panel_presentation.set_panel_size(
                                &panel_id,
                                new_w as usize,
                                new_h as usize,
                            );
                        }
                        entries
                    }
                };

                let Some(background_frame) = self.app.background_frame() else {
                    return;
                };
                let Some(ui_panel_frame) = self.app.ui_panel_frame() else {
                    return;
                };
                let base_upload_region = update.background_dirty_rect.map(|rect| UploadRegion {
                    x: rect.x as u32,
                    y: rect.y as u32,
                    width: rect.width as u32,
                    height: rect.height as u32,
                });
                let ui_panel_upload_region =
                    update.ui_panel_dirty_rect.map(|rect| UploadRegion {
                        x: rect.x as u32,
                        y: rect.y as u32,
                        width: rect.width as u32,
                        height: rect.height as u32,
                    });
                let gpu_source_spec: Option<(
                    String,
                    crate::app::GpuCanvasSourceKind,
                    u32,
                    u32,
                )> = self.app.canvas_layer_source_kind().and_then(|kind| {
                    let panel = self.app.document.active_panel()?;
                    let (w, h) = match kind {
                        crate::app::GpuCanvasSourceKind::Single => panel
                            .layers
                            .first()
                            .map(|l| (l.bitmap.width as u32, l.bitmap.height as u32))?,
                        crate::app::GpuCanvasSourceKind::Composite => {
                            (panel.bitmap.width as u32, panel.bitmap.height as u32)
                        }
                    };
                    Some((panel.id.0.to_string(), kind, w, h))
                });

                let canvas_layer = if let Some((ref panel_id, kind, w, h)) = gpu_source_spec {
                    canvas_quad.map(|quad| CanvasLayer {
                        source: match kind {
                            crate::app::GpuCanvasSourceKind::Single => {
                                CanvasLayerSource::Gpu {
                                    panel_id: panel_id.as_str(),
                                    layer_index: 0,
                                    width: w,
                                    height: h,
                                }
                            }
                            crate::app::GpuCanvasSourceKind::Composite => {
                                CanvasLayerSource::GpuComposite {
                                    panel_id: panel_id.as_str(),
                                    width: w,
                                    height: h,
                                }
                            }
                        },
                        upload_region: None,
                        quad,
                    })
                } else {
                    self.app.canvas_frame().and_then(|bitmap| {
                        canvas_quad.map(|quad| CanvasLayer {
                            source: CanvasLayerSource::Cpu(TextureSource {
                                width: bitmap.width as u32,
                                height: bitmap.height as u32,
                                pixels: bitmap.pixels.as_slice(),
                            }),
                            upload_region: update.canvas_dirty_rect.map(|rect| UploadRegion {
                                x: rect.x as u32,
                                y: rect.y as u32,
                                width: rect.width as u32,
                                height: rect.height as u32,
                            }),
                            quad,
                        })
                    })
                };
                let present_started = Instant::now();

                // 上で組み立てた html_quad_entries を `GpuPanelQuad<'_>` に変換する。
                // SAFETY: texture_ptr は self.app.panel_runtime 所有の Box<HtmlPanelPlugin>::target.texture
                // を指す。Box は heap に固定されており、本フレームの間 panel_runtime に変更を加えないため
                // 寿命が保たれる。html_quad_entries 自体は本ブロックスコープで保持されている。
                #[cfg(feature = "html-panel")]
                let html_panel_quads_owned: Vec<crate::wgpu_canvas::GpuPanelQuad<'_>> =
                    html_quad_entries
                        .iter()
                        .map(|e| crate::wgpu_canvas::GpuPanelQuad {
                            panel_id: e.panel_id.as_str(),
                            texture: unsafe { &*e.texture_ptr },
                            screen_rect: e.screen_rect,
                        })
                        .collect();
                #[cfg(feature = "html-panel")]
                let html_panel_quads_slice: &[crate::wgpu_canvas::GpuPanelQuad<'_>] =
                    &html_panel_quads_owned;
                #[cfg(not(feature = "html-panel"))]
                let html_panel_quads_slice: &[crate::wgpu_canvas::GpuPanelQuad<'_>] = &[];

                let background_solid_quads = self.app.background_solid_quads();
                let foreground_solid_quads = self.app.foreground_solid_quads();
                let (overlay_solid_quads, overlay_circle_quads, overlay_line_quads) =
                    self.app.overlay_quads(size.width as usize, size.height as usize);

                let timings = match presenter.render(
                    PresentScene {
                        background_quads: &background_solid_quads,
                        base_layer: FrameLayer {
                            source: TextureSource::from(background_frame),
                            upload_region: base_upload_region,
                        },
                        canvas_layer,
                        overlay_solid_quads: &overlay_solid_quads,
                        overlay_circle_quads: &overlay_circle_quads,
                        overlay_line_quads: &overlay_line_quads,
                        ui_panel_layer: FrameLayer {
                            source: TextureSource::from(ui_panel_frame),
                            upload_region: ui_panel_upload_region,
                        },
                        html_panel_quads: html_panel_quads_slice,
                        foreground_quads: &foreground_solid_quads,
                    },
                    self.app.gpu_canvas_pool(),
                ) {
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
