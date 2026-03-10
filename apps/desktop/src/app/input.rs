//! ポインタ・キーボード・パネル入力の解釈を `DesktopApp` へ追加する。
//!
//! OS 由来の生イベントをドキュメント編集やパネル操作へ変換し、
//! ランタイム側が UI 詳細を知らずに済むようにする。

use app_core::{Command, ToolKind};
use plugin_api::{HostAction, PanelEvent};

use super::{DesktopApp, PanelDragState};
use crate::app::state::PanelPressState;
use crate::canvas_bridge::{
    CanvasInputState, CanvasPointerEvent, map_view_to_canvas_with_transform,
};
use crate::frame::{Rect, brush_preview_rect, map_view_to_surface, map_view_to_surface_clamped};

impl DesktopApp {
    /// 現在のポインタ位置からキャンバスホバー状態を更新する。
    pub(crate) fn update_canvas_hover(&mut self, x: i32, y: i32) -> bool {
        let previous = self.hover_canvas_position;
        let next = self.canvas_position_from_window(x, y);
        if next == self.hover_canvas_position {
            return false;
        }
        self.hover_canvas_position = next;

        let Some(layout) = self.layout.as_ref().map(|layout| layout.canvas_host_rect) else {
            self.rebuild_present_frame();
            return true;
        };
        let Some((bitmap_width, bitmap_height)) = self
            .document
            .active_bitmap()
            .map(|bitmap| (bitmap.width, bitmap.height))
        else {
            self.rebuild_present_frame();
            return true;
        };

        let transform = self.document.view_transform;
        if let Some(previous) = previous.and_then(|position| {
            brush_preview_rect(
                layout,
                bitmap_width,
                bitmap_height,
                transform,
                position,
                self.brush_preview_size().unwrap_or(1),
            )
        }) {
            self.append_canvas_host_dirty_rect(previous);
        }
        if let Some(next) = next.and_then(|position| {
            brush_preview_rect(
                layout,
                bitmap_width,
                bitmap_height,
                transform,
                position,
                self.brush_preview_size().unwrap_or(1),
            )
        }) {
            self.append_canvas_host_dirty_rect(next);
        }
        true
    }

    /// ポインタ押下をキャンバスまたはパネル操作へ振り分ける。
    #[allow(dead_code)]
    pub(crate) fn handle_pointer_pressed(&mut self, x: i32, y: i32) -> bool {
        self.handle_pointer_pressed_with_pressure(x, y, 1.0)
    }

    pub(crate) fn handle_pointer_pressed_with_pressure(
        &mut self,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        if self.begin_panel_interaction(x, y) {
            return true;
        }

        if self.canvas_position_from_window(x, y).is_some() {
            return self.handle_canvas_pointer("down", x, y, pressure);
        }

        false
    }

    /// ポインタ解放を現在の操作状態に応じて処理する。
    #[allow(dead_code)]
    pub(crate) fn handle_pointer_released(&mut self, x: i32, y: i32) -> bool {
        self.handle_pointer_released_with_pressure(x, y, 1.0)
    }

    pub(crate) fn handle_pointer_released_with_pressure(
        &mut self,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("up", x, y, pressure);
        }
        if self.active_panel_drag.take().is_some() {
            self.pending_panel_press = None;
            self.persist_session_state();
            return false;
        }
        self.handle_panel_pointer(x, y)
    }

    /// ポインタ移動をドラッグ中の対象へ配送する。
    pub(crate) fn handle_pointer_dragged(&mut self, x: i32, y: i32) -> bool {
        self.handle_pointer_dragged_with_pressure(x, y, 1.0)
    }

    pub(crate) fn handle_pointer_dragged_with_pressure(
        &mut self,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("drag", x, y, pressure);
        }

        if self.active_panel_drag.is_some() {
            return self.drag_panel_interaction(x, y);
        }

        false
    }

    /// パネル上の押下開始を解釈して、必要ならドラッグ状態を開始する。
    fn begin_panel_interaction(&mut self, x: i32, y: i32) -> bool {
        self.pending_panel_press = None;
        if let Some(panel_id) = self.panel_move_hit_from_window(x, y) {
            let Some(panel_rect) = self.ui_shell.panel_rect(&panel_id) else {
                return false;
            };
            self.active_panel_drag = Some(PanelDragState::Move {
                panel_id,
                grab_offset_x: (x.max(0) as usize).saturating_sub(panel_rect.x),
                grab_offset_y: (y.max(0) as usize).saturating_sub(panel_rect.y),
            });
            return true;
        }

        let Some(event) = self.panel_event_from_window(x, y) else {
            self.active_panel_drag = None;
            return false;
        };

        match &event {
            PanelEvent::Activate { panel_id, node_id } => {
                let changed = self.ui_shell.focus_panel_node(panel_id, node_id);
                self.pending_panel_press = Some(PanelPressState {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                });
                self.refresh_panel_surface_if_changed(changed)
            }
            PanelEvent::SetValue {
                panel_id,
                node_id,
                value,
            } => {
                self.active_panel_drag = Some(PanelDragState::Control {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                    source_value: *value,
                });
                self.dispatch_panel_event(event)
            }
            PanelEvent::SetText {
                panel_id,
                node_id,
                ..
            } => {
                let drag_state = PanelDragState::Control {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                    source_value: 0,
                };
                self.active_panel_drag = Some(drag_state);
                self.dispatch_panel_event(event)
            }
            PanelEvent::DragValue { .. }
            | PanelEvent::Keyboard { .. } => false,
        }
    }

    /// スライダードラッグ中の移動イベントを現在ノードへ配送する。
    fn drag_panel_interaction(&mut self, x: i32, y: i32) -> bool {
        let Some(state) = self.active_panel_drag.clone() else {
            return false;
        };
        match state {
            PanelDragState::Control {
                ref panel_id,
                ref node_id,
                ..
            } => {
                let Some(event) = self.panel_drag_event_from_window(&state, x, y).or_else(|| {
                    let event = self.panel_event_from_window(x, y)?;
                    match &event {
                        PanelEvent::SetValue {
                            panel_id: event_panel_id,
                            node_id: event_node_id,
                            ..
                        }
                        | PanelEvent::SetText {
                            panel_id: event_panel_id,
                            node_id: event_node_id,
                            ..
                        } if *event_panel_id == *panel_id && *event_node_id == *node_id => Some(event),
                        _ => None,
                    }
                }) else {
                    return false;
                };
                let _changed = self.dispatch_panel_event(event.clone());
                self.advance_panel_drag_source(&event);
                true
            }
            PanelDragState::Move {
                panel_id,
                grab_offset_x,
                grab_offset_y,
            } => {
                let Some(layout) = self.layout.as_ref() else {
                    return false;
                };
                let window_x = x.max(0) as usize;
                let window_y = y.max(0) as usize;
                let changed = self.ui_shell.move_panel_to(
                    &panel_id,
                    window_x.saturating_sub(grab_offset_x),
                    window_y.saturating_sub(grab_offset_y),
                    layout.window_rect.width,
                    layout.window_rect.height,
                );
                if changed {
                    self.mark_panel_surface_dirty();
                }
                changed
            }
        }
    }

    pub(crate) fn advance_panel_drag_source(&mut self, event: &PanelEvent) {
        if let PanelEvent::DragValue { to, .. } = event
            && let Some(PanelDragState::Control { source_value, .. }) = self.active_panel_drag.as_mut()
        {
            *source_value = *to;
        }
    }

    /// パネルイベントを `UiShell` とホストアクションへ流す。
    pub(super) fn dispatch_panel_event(&mut self, event: PanelEvent) -> bool {
        let mut changed = false;
        let previous_configs = self.ui_shell.persistent_panel_configs();
        if let PanelEvent::Activate { panel_id, node_id } = &event {
            changed |= self.ui_shell.focus_panel_node(panel_id, node_id);
        }

        self.mark_panel_surface_dirty();
        let mut needs_redraw = true;
        let mut actions = self.ui_shell.handle_panel_event(&event);

        if let PanelEvent::SetText {
            panel_id,
            node_id,
            value,
        } = &event
            && panel_id == "builtin.workspace-presets"
            && node_id == "workspace.preset.selector"
        {
            let trimmed = value.trim();
            let already_dispatched = actions.iter().any(|action| {
                matches!(
                    action,
                    HostAction::DispatchCommand(Command::ApplyWorkspacePreset { preset_id })
                        if preset_id == trimmed
                )
            });
            if !trimmed.is_empty() && !already_dispatched {
                actions.push(HostAction::DispatchCommand(Command::ApplyWorkspacePreset {
                    preset_id: trimmed.to_string(),
                }));
            }
        }

        for action in actions {
            needs_redraw |= self.execute_host_action(action);
        }

        if self.ui_shell.persistent_panel_configs() != previous_configs {
            self.persist_session_state();
        }

        changed || needs_redraw
    }

    /// パネル上の単発ポインタイベントを処理する。
    fn handle_panel_pointer(&mut self, x: i32, y: i32) -> bool {
        let Some(event) = self.panel_event_from_window(x, y) else {
            self.pending_panel_press = None;
            return false;
        };
        let should_dispatch = matches!(
            (&self.pending_panel_press, &event),
            (
                Some(PanelPressState { panel_id, node_id }),
                PanelEvent::Activate {
                    panel_id: released_panel_id,
                    node_id: released_node_id,
                }
            ) if panel_id == released_panel_id && node_id == released_node_id
        );
        self.pending_panel_press = None;
        if !should_dispatch {
            return false;
        }
        self.dispatch_panel_event(event)
    }

    /// フォーカスを次のパネル操作対象へ進める。
    pub(crate) fn focus_next_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_next();
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカスを前のパネル操作対象へ戻す。
    pub(crate) fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.ui_shell.focus_previous();
        self.refresh_panel_surface_if_changed(changed)
    }

    /// 現在フォーカス中のパネル操作対象をアクティブ化する。
    pub(crate) fn activate_focused_panel_control(&mut self) -> Option<Command> {
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

    /// フォーカス中のテキスト入力へ文字列を挿入する。
    pub(crate) fn insert_text_into_focused_panel_input(&mut self, text: &str) -> bool {
        let changed = self.ui_shell.insert_text_into_focused_input(text);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力で後退削除を行う。
    pub(crate) fn backspace_focused_panel_input(&mut self) -> bool {
        let changed = self.ui_shell.backspace_focused_input();
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力で前方削除を行う。
    pub(crate) fn delete_focused_panel_input(&mut self) -> bool {
        let changed = self.ui_shell.delete_focused_input();
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力カーソルを相対移動する。
    pub(crate) fn move_focused_panel_input_cursor(&mut self, delta_chars: isize) -> bool {
        let changed = self.ui_shell.move_focused_input_cursor(delta_chars);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力カーソルを先頭へ移動する。
    pub(crate) fn move_focused_panel_input_cursor_to_start(&mut self) -> bool {
        let changed = self.ui_shell.move_focused_input_cursor_to_start();
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力カーソルを末尾へ移動する。
    pub(crate) fn move_focused_panel_input_cursor_to_end(&mut self) -> bool {
        let changed = self.ui_shell.move_focused_input_cursor_to_end();
        self.refresh_panel_surface_if_changed(changed)
    }

    /// IME の preedit 文字列をフォーカス中入力へ反映する。
    pub(crate) fn set_focused_panel_input_preedit(&mut self, preedit: Option<String>) -> bool {
        let changed = self.ui_shell.set_focused_input_preedit(preedit);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中の入力がテキスト入力かどうかを返す。
    pub(crate) fn has_focused_panel_input(&self) -> bool {
        self.ui_shell.has_focused_text_input()
    }

    /// パネル面を垂直スクロールする。
    pub(crate) fn scroll_panel_surface(&mut self, delta_lines: i32) -> bool {
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

    /// キャンバス上のポインタ操作を描画コマンドへ変換して適用する。
    pub(crate) fn handle_canvas_pointer(
        &mut self,
        action: &str,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        let canvas_position = self.canvas_position_from_window(x, y).or_else(|| {
            (action != "down" && self.canvas_input.is_drawing)
                .then(|| self.canvas_position_from_window_clamped(x, y))
                .flatten()
        });
        let Some((canvas_x, canvas_y)) = canvas_position else {
            if action == "up" {
                self.canvas_input = CanvasInputState::default();
            }
            return false;
        };

        let active_tool = self.document.active_tool;
        match action {
            "down" => {
                if active_tool == ToolKind::Bucket {
                    return self.execute_command(Command::FillRegion {
                        x: canvas_x,
                        y: canvas_y,
                    });
                }
                self.canvas_input.is_drawing = true;
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
                self.canvas_input.last_smoothed_position = Some((canvas_x as f32, canvas_y as f32));
                if active_tool == ToolKind::LassoBucket {
                    self.canvas_input.lasso_points.clear();
                    self.canvas_input.lasso_points.push((canvas_x, canvas_y));
                    if let Some(layout) = self.layout.as_ref() {
                        self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                    }
                    return false;
                }
                self.execute_canvas_command(canvas_x, canvas_y, None, pressure)
            }
            "drag" if self.canvas_input.is_drawing => {
                if active_tool == ToolKind::LassoBucket {
                    if self.canvas_input.lasso_points.last().copied() != Some((canvas_x, canvas_y)) {
                        self.canvas_input.lasso_points.push((canvas_x, canvas_y));
                        if let Some(layout) = self.layout.as_ref() {
                            self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                        }
                    }
                    self.canvas_input.last_position = Some((canvas_x, canvas_y));
                    return false;
                }
                let (next_x, next_y) = self.stabilized_canvas_position(canvas_x, canvas_y);
                let next_position = (next_x, next_y);
                let from = self.canvas_input.last_position;
                if from == Some(next_position) {
                    return false;
                }
                let changed = self.execute_canvas_command(next_x, next_y, from, pressure);
                self.canvas_input.last_position = Some(next_position);
                changed
            }
            "up" => {
                if active_tool == ToolKind::LassoBucket {
                    let changed = if self.canvas_input.lasso_points.len() >= 3 {
                        self.execute_command(Command::FillLasso {
                            points: self.canvas_input.lasso_points.clone(),
                        })
                    } else {
                        false
                    };
                    if let Some(layout) = self.layout.as_ref() {
                        self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                    }
                    self.canvas_input = CanvasInputState::default();
                    return changed;
                }
                let from = self.canvas_input.last_position;
                let changed = if self.canvas_input.is_drawing && from != Some((canvas_x, canvas_y))
                {
                    self.execute_canvas_command(canvas_x, canvas_y, from, pressure)
                } else {
                    false
                };
                self.canvas_input = CanvasInputState::default();
                changed
            }
            _ => false,
        }
    }

    fn stabilized_canvas_position(&mut self, x: usize, y: usize) -> (usize, usize) {
        if self.document.active_tool != ToolKind::Pen {
            self.canvas_input.last_smoothed_position = Some((x as f32, y as f32));
            return (x, y);
        }
        let stabilization = self
            .document
            .active_pen_preset()
            .map(|preset| preset.stabilization)
            .unwrap_or_default();
        if stabilization == 0 {
            self.canvas_input.last_smoothed_position = Some((x as f32, y as f32));
            return (x, y);
        }

        let blend = (1.0 / (1.0 + stabilization as f32 / 12.0)).clamp(0.05, 1.0);
        let previous = self
            .canvas_input
            .last_smoothed_position
            .unwrap_or((x as f32, y as f32));
        let next = (
            previous.0 + (x as f32 - previous.0) * blend,
            previous.1 + (y as f32 - previous.1) * blend,
        );
        self.canvas_input.last_smoothed_position = Some(next);
        (next.0.round().max(0.0) as usize, next.1.round().max(0.0) as usize)
    }

    /// ウィンドウ座標をキャンバスビットマップ座標へ変換する。
    fn canvas_position_from_window(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let layout = self.layout.as_ref()?;
        if !layout.canvas_host_rect.contains(x, y) {
            return None;
        }

        self.canvas_position_from_window_clamped(x, y)
    }

    /// ウィンドウ座標をキャンバスビットマップ座標へクランプ付きで変換する。
    fn canvas_position_from_window_clamped(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let layout = self.layout.as_ref()?;
        let clamped_x = x.clamp(
            layout.canvas_host_rect.x as i32,
            (layout.canvas_host_rect.x + layout.canvas_host_rect.width.saturating_sub(1)) as i32,
        );
        let clamped_y = y.clamp(
            layout.canvas_host_rect.y as i32,
            (layout.canvas_host_rect.y + layout.canvas_host_rect.height.saturating_sub(1)) as i32,
        );

        let bitmap = self.document.active_bitmap()?;
        map_view_to_canvas_with_transform(
            &render::RenderFrame {
                width: bitmap.width,
                height: bitmap.height,
                pixels: Vec::new(),
            },
            CanvasPointerEvent {
                x: clamped_x - layout.canvas_host_rect.x as i32,
                y: clamped_y - layout.canvas_host_rect.y as i32,
                width: layout.canvas_host_rect.width as i32,
                height: layout.canvas_host_rect.height as i32,
            },
            self.document.view_transform,
        )
    }

    /// ウィンドウ座標からパネルイベントを逆引きする。
    fn panel_event_from_window(&self, x: i32, y: i32) -> Option<PanelEvent> {
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = self.panel_surface_coordinates_from_window(x, y)?;
        panel_surface.hit_test(surface_x, surface_y)
    }

    pub(crate) fn panel_is_hovered(&self, x: i32, y: i32) -> bool {
        self.panel_move_hit_from_window(x, y).is_some() || self.panel_event_from_window(x, y).is_some()
    }

    fn panel_move_hit_from_window(&self, x: i32, y: i32) -> Option<String> {
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = self.panel_surface_coordinates_from_window(x, y)?;
        panel_surface.move_panel_hit_test(surface_x, surface_y)
    }

    fn panel_surface_coordinates_from_window(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let panel_surface = self.panel_surface.as_ref()?;
        map_view_to_surface(
            panel_surface.width,
            panel_surface.height,
            Rect {
                x: panel_surface.x,
                y: panel_surface.y,
                width: panel_surface.width,
                height: panel_surface.height,
            },
            x,
            y,
        )
    }

    /// ドラッグ継続中のパネルノードへ値変更イベントを生成する。
    fn panel_drag_event_from_window(
        &self,
        state: &PanelDragState,
        x: i32,
        y: i32,
    ) -> Option<PanelEvent> {
        let PanelDragState::Control {
            panel_id,
            node_id,
            source_value,
        } = state
        else {
            return None;
        };
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = map_view_to_surface_clamped(
            panel_surface.width,
            panel_surface.height,
            Rect {
                x: panel_surface.x,
                y: panel_surface.y,
                width: panel_surface.width,
                height: panel_surface.height,
            },
            x,
            y,
        )?;
        panel_surface.drag_event(
            panel_id,
            node_id,
            *source_value,
            surface_x,
            surface_y,
        )
    }
}
