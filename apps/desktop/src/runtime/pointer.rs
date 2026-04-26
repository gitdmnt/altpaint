//! ポインタ・ホイール・タッチ入力を `DesktopRuntime` へ追加する。
//!
//! 可能な限り座標変換や蓄積ロジックを小さな関数へ分け、
//! OS イベント処理とドキュメント更新の接点を読みやすく保つ。

use std::time::Instant;

use app_core::Command;
use winit::event::{ElementState, Force, MouseScrollDelta, TouchPhase};

use super::DesktopRuntime;

#[cfg(feature = "html-panel")]
#[derive(Clone, Copy)]
pub(super) enum HtmlPointerKind {
    Down,
    Up,
    Move,
}

impl DesktopRuntime {
    /// 入力や種別に応じて処理を振り分ける。
    fn wheel_delta_lines(delta: MouseScrollDelta) -> (f32, f32) {
        match delta {
            MouseScrollDelta::LineDelta(x, y) => (x, y),
            MouseScrollDelta::PixelDelta(position) => (
                position.x as f32 / ui_shell::text_line_height() as f32,
                position.y as f32 / ui_shell::text_line_height() as f32,
            ),
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_mouse_cursor_moved(&mut self, x: i32, y: i32) -> bool {
        if self.active_touch_id.is_some() {
            return false;
        }

        let position = (x, y);
        self.last_cursor_position = Some(position);
        self.last_cursor_position_f64 = Some((x as f64, y as f64));
        self.profiler
            .record("canvas_input_window_event", std::time::Duration::ZERO);

        // HTML パネル領域内なら Blitz に PointerMove を転送（:hover を動かすため）
        #[cfg(feature = "html-panel")]
        let html_changed = self.forward_html_pointer(x, y, HtmlPointerKind::Move);
        #[cfg(not(feature = "html-panel"))]
        let html_changed = false;

        let hover_changed = self.app.update_canvas_hover(position.0, position.1);
        let changed = self.app.handle_pointer_dragged(position.0, position.1)
            || hover_changed
            || html_changed;
        self.record_canvas_input_if_needed(changed)
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_raw_mouse_motion(&mut self, delta_x: f64, delta_y: f64) -> bool {
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

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_mouse_button(&mut self, state: ElementState) -> bool {
        if self.active_touch_id.is_some() {
            return false;
        }

        let Some((x, y)) = self.last_cursor_position else {
            return false;
        };

        // HTML パネル領域内なら Blitz に PointerDown/Up を転送（<details>/<button> click 経路）
        #[cfg(feature = "html-panel")]
        let html_kind = match state {
            ElementState::Pressed => HtmlPointerKind::Down,
            ElementState::Released => HtmlPointerKind::Up,
        };
        #[cfg(feature = "html-panel")]
        let html_changed = self.forward_html_pointer(x, y, html_kind);
        #[cfg(not(feature = "html-panel"))]
        let html_changed = false;

        let canvas_changed = match state {
            ElementState::Pressed => {
                let changed = self.app.handle_pointer_pressed_with_pressure(x, y, 1.0);
                self.record_canvas_input_if_needed(changed)
            }
            ElementState::Released => self.app.handle_pointer_released_with_pressure(x, y, 1.0),
        };
        canvas_changed || html_changed
    }

    /// Has pending ホイール animation かどうかを返す。
    pub(super) fn has_pending_wheel_animation(&self) -> bool {
        self.pending_wheel_pan.0.abs() > f32::EPSILON
            || self.pending_wheel_pan.1.abs() > f32::EPSILON
            || self.pending_wheel_zoom_lines.abs() > f32::EPSILON
    }

    /// Animated step を取り出して返す。
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

    /// ホイール animation を進行させる。
    pub(super) fn advance_wheel_animation(&mut self) -> bool {
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
            let t = Instant::now();
            changed |= self.app.execute_command(Command::PanView {
                delta_x: pan_x,
                delta_y: pan_y,
            });
            self.profiler.record("pan_step", t.elapsed());
        }

        if zoom_lines.abs() > f32::EPSILON {
            let current = self.app.document.view_transform.zoom;
            let next_zoom = (current * 1.1_f32.powf(zoom_lines)).clamp(0.25, 16.0);
            if (next_zoom - current).abs() > f32::EPSILON {
                let t = Instant::now();
                changed |= self
                    .app
                    .execute_command(Command::SetViewZoom { zoom: next_zoom });
                self.profiler.record("zoom_step", t.elapsed());
            } else {
                self.pending_wheel_zoom_lines = 0.0;
            }
        }

        changed
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) -> bool {
        let Some((x, y)) = self.last_cursor_position else {
            return false;
        };
        let Some(layout) = self.app.layout.as_ref() else {
            return false;
        };
        let on_panel = self.app.panel_is_hovered(x, y);
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

        let mut delta_x = delta_x_lines * 32.0;
        let mut delta_y = delta_y_lines * 32.0;
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

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_touch_phase(
        &mut self,
        touch_id: u64,
        phase: TouchPhase,
        x: i32,
        y: i32,
        force: Option<Force>,
    ) -> bool {
        let position = (x, y);

        match phase {
            TouchPhase::Started => {
                if matches!(self.active_touch_id, Some(active_id) if active_id != touch_id) {
                    return false;
                }

                let pressure = normalized_pressure(force, 1.0);
                self.active_touch_id = Some(touch_id);
                self.last_touch_pressure = pressure;
                self.last_cursor_position = Some(position);
                self.last_cursor_position_f64 = Some((position.0 as f64, position.1 as f64));
                let changed = self
                    .app
                    .handle_pointer_pressed_with_pressure(position.0, position.1, pressure);
                self.record_canvas_input_if_needed(changed)
            }
            TouchPhase::Moved => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                let pressure = normalized_pressure(force, self.last_touch_pressure);
                self.last_touch_pressure = pressure;
                self.last_cursor_position = Some(position);
                self.last_cursor_position_f64 = Some((position.0 as f64, position.1 as f64));
                let changed = self
                    .app
                    .handle_pointer_dragged_with_pressure(position.0, position.1, pressure);
                self.record_canvas_input_if_needed(changed)
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.active_touch_id != Some(touch_id) {
                    return false;
                }

                let pressure = normalized_pressure(force, 0.0);
                self.last_cursor_position = Some(position);
                self.last_cursor_position_f64 = Some((position.0 as f64, position.1 as f64));
                self.active_touch_id = None;
                self.last_touch_pressure = 1.0;
                self.app
                    .handle_pointer_released_with_pressure(position.0, position.1, pressure)
            }
        }
    }

    /// HTML パネル body 領域内なら、対応 Blitz `UiEvent` を Engine に転送する。
    /// 戻り値: 転送した場合 true（再描画トリガに使う）。
    #[cfg(feature = "html-panel")]
    pub(super) fn forward_html_pointer(
        &mut self,
        x: i32,
        y: i32,
        kind: HtmlPointerKind,
    ) -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        let Some((panel_id, local_x, local_y)) = self
            .app
            .panel_presentation
            .html_panel_at(x as usize, y as usize)
        else {
            return false;
        };
        use panel_html_experiment::blitz_traits::events::{
            BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons,
            PointerCoords, PointerDetails, UiEvent,
        };
        // local_x/y は chrome を含む panel 全体原点（screen_rect 基準）なので
        // body オフセット（chrome_height）を Engine 側で扱う。Blitz には panel-local 座標を渡す。
        let coords = PointerCoords {
            page_x: local_x as f32,
            page_y: local_y as f32,
            client_x: local_x as f32,
            client_y: local_y as f32,
            screen_x: x as f32,
            screen_y: y as f32,
        };
        let pointer = BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords,
            button: MouseEventButton::Main,
            buttons: match kind {
                HtmlPointerKind::Down => MouseEventButtons::Primary,
                _ => MouseEventButtons::empty(),
            },
            mods: keyboard_types::Modifiers::empty(),
            details: PointerDetails::default(),
        };
        let event = match kind {
            HtmlPointerKind::Down => UiEvent::PointerDown(pointer),
            HtmlPointerKind::Up => UiEvent::PointerUp(pointer),
            HtmlPointerKind::Move => UiEvent::PointerMove(pointer),
        };
        self.app.panel_runtime.forward_panel_input(&panel_id, event)
    }

    /// キャンバス 入力 if needed を記録する。
    pub(super) fn record_canvas_input_if_needed(&mut self, changed: bool) -> bool {
        if changed && self.app.is_canvas_interacting() {
            self.profiler
                .record("canvas_input_dispatch", std::time::Duration::ZERO);
            self.profiler.record_canvas_input();
        }
        changed
    }
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn normalized_pressure(force: Option<Force>, fallback: f32) -> f32 {
    match force {
        Some(Force::Calibrated {
            force,
            max_possible_force,
            ..
        }) if max_possible_force > f64::EPSILON => (force / max_possible_force) as f32,
        Some(Force::Normalized(value)) => value as f32,
        _ => fallback,
    }
    .clamp(0.0, 1.0)
}
