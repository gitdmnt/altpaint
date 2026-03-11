//! パネル入力中継とホストアクション適用を集約する。

use app_core::WindowPoint;
use plugin_api::{HostAction, PanelEvent};

use super::DesktopApp;
/// スライダードラッグ中のパネルノード情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PanelDragState {
    Control {
        panel_id: String,
        node_id: String,
        source_value: usize,
    },
    Move {
        panel_id: String,
        grab_offset_x: usize,
        grab_offset_y: usize,
    },
}

/// ボタン系パネル操作の押下開始情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelPressState {
    pub(crate) panel_id: String,
    pub(crate) node_id: String,
}

/// パネル操作中の一時状態を保持する。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PanelInteractionState {
    pub(crate) active_panel_drag: Option<PanelDragState>,
    pub(crate) pending_panel_press: Option<PanelPressState>,
}

impl DesktopApp {
    /// パネル上の押下開始を解釈して、必要ならドラッグ状態を開始する。
    pub(super) fn begin_panel_interaction(&mut self, point: WindowPoint) -> bool {
        self.panel_interaction.pending_panel_press = None;
        if let Some(panel_id) = self.panel_move_hit_from_window(point) {
            let Some(panel_rect) = self.panel_presentation.panel_rect(&panel_id) else {
                return false;
            };
            self.panel_interaction.active_panel_drag = Some(PanelDragState::Move {
                panel_id,
                grab_offset_x: (point.x.max(0) as usize).saturating_sub(panel_rect.x),
                grab_offset_y: (point.y.max(0) as usize).saturating_sub(panel_rect.y),
            });
            return true;
        }

        let Some(event) = self.panel_event_from_window(point) else {
            self.panel_interaction.active_panel_drag = None;
            return false;
        };

        match &event {
            PanelEvent::Activate { panel_id, node_id } => {
                let changed = self
                    .panel_presentation
                    .focus_panel_node(&self.panel_runtime, panel_id, node_id);
                self.panel_interaction.pending_panel_press = Some(PanelPressState {
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
                self.panel_interaction.active_panel_drag = Some(PanelDragState::Control {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                    source_value: *value,
                });
                self.dispatch_panel_event(event)
            }
            PanelEvent::SetText {
                panel_id, node_id, ..
            } => {
                let drag_state = PanelDragState::Control {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                    source_value: 0,
                };
                self.panel_interaction.active_panel_drag = Some(drag_state);
                self.dispatch_panel_event(event)
            }
            PanelEvent::DragValue { .. } | PanelEvent::Keyboard { .. } => false,
        }
    }

    /// スライダードラッグ中の移動イベントを現在ノードへ配送する。
    pub(super) fn drag_panel_interaction(&mut self, point: WindowPoint) -> bool {
        let Some(state) = self.panel_interaction.active_panel_drag.clone() else {
            return false;
        };
        match state {
            PanelDragState::Control {
                ref panel_id,
                ref node_id,
                ..
            } => {
                let Some(event) = self
                    .panel_drag_event_from_window(&state, point)
                    .or_else(|| {
                        let event = self.panel_event_from_window(point)?;
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
                            } if *event_panel_id == *panel_id && *event_node_id == *node_id => {
                                Some(event)
                            }
                            _ => None,
                        }
                    })
                else {
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
                let window_x = point.x.max(0) as usize;
                let window_y = point.y.max(0) as usize;
                let changed = self.panel_presentation.move_panel_to(
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
            && let Some(PanelDragState::Control { source_value, .. }) =
                self.panel_interaction.active_panel_drag.as_mut()
        {
            *source_value = *to;
        }
    }

    /// 指定パネルノードを擬似的にアクティブ化する。
    pub(super) fn activate_panel_control(&mut self, panel_id: &str, node_id: &str) -> bool {
        self.dispatch_panel_event(PanelEvent::Activate {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        })
    }

    /// グローバルキーボードショートカットをパネルプラグインへ配送する。
    pub(crate) fn dispatch_keyboard_shortcut(
        &mut self,
        shortcut: &str,
        key: &str,
        repeat: bool,
    ) -> bool {
        let runtime = self.panel_runtime.dispatch_keyboard(shortcut, key, repeat);
        let handled = runtime.handled;
        let mut changed = handled;
        if runtime.config_changed {
            self.persist_session_state();
        }
        if !runtime.changed_panel_ids.is_empty() {
            self.panel_presentation
                .mark_runtime_panels_dirty(&runtime.changed_panel_ids);
            changed = true;
        }
        for action in runtime.actions {
            changed |= self.execute_host_action(action);
        }
        self.refresh_panel_surface_if_changed(changed)
    }

    /// パネルランタイムから返されたホストアクションを実行する。
    pub(crate) fn execute_host_action(&mut self, action: HostAction) -> bool {
        match action {
            HostAction::DispatchCommand(command) => self.execute_command(command),
            HostAction::InvokePanelHandler { .. } => false,
            HostAction::MovePanel {
                panel_id,
                direction,
            } => {
                let changed = self.panel_presentation.move_panel(&panel_id, direction);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                    self.persist_session_state();
                }
                changed
            }
            HostAction::SetPanelVisibility { panel_id, visible } => {
                let changed = self
                    .panel_presentation
                    .set_panel_visibility(&panel_id, visible);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                    self.persist_session_state();
                }
                changed
            }
        }
    }

    /// パネルイベントを `UiShell` とホストアクションへ流す。
    pub(super) fn dispatch_panel_event(&mut self, event: PanelEvent) -> bool {
        self.dispatch_panel_event_with_command(event).0
    }

    fn dispatch_panel_event_with_command(
        &mut self,
        event: PanelEvent,
    ) -> (bool, Option<app_core::Command>) {
        let mut changed = false;
        let previous_configs = self.panel_runtime.persistent_panel_configs();

        let mut needs_redraw = true;
        let mut first_command = None;
        let presentation = self
            .panel_presentation
            .handle_panel_event(&self.panel_runtime, &event);
        changed |= presentation.changed;
        let mut actions = presentation.actions;

        if presentation.forward_to_runtime {
            let runtime = self.panel_runtime.dispatch_event(&event);
            if runtime.config_changed || self.panel_runtime.persistent_panel_configs() != previous_configs {
                self.persist_session_state();
            }
            if !runtime.changed_panel_ids.is_empty() {
                self.panel_presentation
                    .mark_runtime_panels_dirty(&runtime.changed_panel_ids);
                changed = true;
            }
            actions.extend(runtime.actions);
        }

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
                    HostAction::DispatchCommand(app_core::Command::ApplyWorkspacePreset { preset_id })
                        if preset_id == trimmed
                )
            });
            if !trimmed.is_empty() && !already_dispatched {
                actions.push(HostAction::DispatchCommand(
                    app_core::Command::ApplyWorkspacePreset {
                        preset_id: trimmed.to_string(),
                    },
                ));
            }
        }

        for action in actions {
            if first_command.is_none()
                && let HostAction::DispatchCommand(command) = &action
            {
                first_command = Some(command.clone());
            }
            needs_redraw |= self.execute_host_action(action);
        }

        if self.panel_runtime.persistent_panel_configs() != previous_configs {
            self.persist_session_state();
        }

        let changed = changed || needs_redraw;
        if changed {
            self.mark_panel_surface_dirty();
        }
        (changed, first_command)
    }

    /// パネル上の単発ポインタイベントを処理する。
    pub(super) fn handle_panel_pointer(&mut self, point: WindowPoint) -> bool {
        let Some(event) = self.panel_event_from_window(point) else {
            self.panel_interaction.pending_panel_press = None;
            return false;
        };
        let should_dispatch = matches!(
            (&self.panel_interaction.pending_panel_press, &event),
            (
                Some(PanelPressState { panel_id, node_id }),
                PanelEvent::Activate {
                    panel_id: released_panel_id,
                    node_id: released_node_id,
                }
            ) if panel_id == released_panel_id && node_id == released_node_id
        );
        self.panel_interaction.pending_panel_press = None;
        if !should_dispatch {
            return false;
        }
        self.dispatch_panel_event(event)
    }

    /// フォーカスを次のパネル操作対象へ進める。
    pub(crate) fn focus_next_panel_control(&mut self) -> bool {
        let changed = self.panel_presentation.focus_next(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカスを前のパネル操作対象へ戻す。
    pub(crate) fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.panel_presentation.focus_previous(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// 現在フォーカス中のパネル操作対象をアクティブ化する。
    pub(crate) fn activate_focused_panel_control(&mut self) -> Option<app_core::Command> {
        let event = self.panel_presentation.activate_focused()?;
        self.dispatch_panel_event_with_command(event).1
    }

    /// フォーカス中のテキスト入力へ文字列を挿入する。
    pub(crate) fn insert_text_into_focused_panel_input(&mut self, text: &str) -> bool {
        let Some(event) = self
            .panel_presentation
            .insert_text_into_focused_input(&self.panel_runtime, text)
        else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    /// フォーカス中のテキスト入力で後退削除を行う。
    pub(crate) fn backspace_focused_panel_input(&mut self) -> bool {
        let Some(event) = self
            .panel_presentation
            .backspace_focused_input(&self.panel_runtime)
        else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    /// フォーカス中のテキスト入力で前方削除を行う。
    pub(crate) fn delete_focused_panel_input(&mut self) -> bool {
        let Some(event) = self
            .panel_presentation
            .delete_focused_input(&self.panel_runtime)
        else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    /// フォーカス中のテキスト入力カーソルを相対移動する。
    pub(crate) fn move_focused_panel_input_cursor(&mut self, delta_chars: isize) -> bool {
        let changed = self
            .panel_presentation
            .move_focused_input_cursor(&self.panel_runtime, delta_chars);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力カーソルを先頭へ移動する。
    pub(crate) fn move_focused_panel_input_cursor_to_start(&mut self) -> bool {
        let changed = self
            .panel_presentation
            .move_focused_input_cursor_to_start(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中のテキスト入力カーソルを末尾へ移動する。
    pub(crate) fn move_focused_panel_input_cursor_to_end(&mut self) -> bool {
        let changed = self
            .panel_presentation
            .move_focused_input_cursor_to_end(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// IME の preedit 文字列をフォーカス中入力へ反映する。
    pub(crate) fn set_focused_panel_input_preedit(&mut self, preedit: Option<String>) -> bool {
        let changed = self
            .panel_presentation
            .set_focused_input_preedit(&self.panel_runtime, preedit);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// フォーカス中の入力がテキスト入力かどうかを返す。
    pub(crate) fn has_focused_panel_input(&self) -> bool {
        self.panel_presentation
            .has_focused_text_input(&self.panel_runtime)
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

        let changed = self.panel_presentation.scroll_panels(delta_lines, viewport_height);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// ウィンドウ座標からパネルイベントを逆引きする。
    pub(super) fn panel_event_from_window(&self, point: WindowPoint) -> Option<PanelEvent> {
        let panel_surface = self.panel_surface.as_ref()?;
        let surface_point = self.panel_surface_coordinates_from_window(point)?;
        panel_surface.hit_test_at(surface_point)
    }

    pub(crate) fn panel_is_hovered(&self, x: i32, y: i32) -> bool {
        let point = WindowPoint::new(x, y);
        self.panel_move_hit_from_window(point).is_some()
            || self.panel_event_from_window(point).is_some()
    }

    pub(super) fn panel_move_hit_from_window(&self, point: WindowPoint) -> Option<String> {
        let panel_surface = self.panel_surface.as_ref()?;
        let surface_point = self.panel_surface_coordinates_from_window(point)?;
        panel_surface.move_panel_hit_test_at(surface_point)
    }

    pub(super) fn panel_surface_coordinates_from_window(
        &self,
        point: app_core::WindowPoint,
    ) -> Option<app_core::PanelSurfacePoint> {
        let panel_surface = self.panel_surface.as_ref()?;
        panel_surface.global_bounds().to_surface_point(point)
    }

    /// ドラッグ継続中のパネルノードへ値変更イベントを生成する。
    pub(super) fn panel_drag_event_from_window(
        &self,
        state: &PanelDragState,
        point: WindowPoint,
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
        let surface_point = panel_surface
            .global_bounds()
            .clamp_to_surface_point(point)?;
        panel_surface.drag_event_at(panel_id, node_id, *source_value, surface_point)
    }
}
