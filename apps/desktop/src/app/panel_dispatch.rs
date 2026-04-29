//! パネル入力中継とホストアクション適用を集約する。

use app_core::WindowPoint;
use panel_api::{HostAction, PanelEvent};

use super::DesktopApp;
/// パネル移動ドラッグ中の被操作パネル情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelDragState {
    pub panel_id: String,
    pub grab_offset_x: usize,
    pub grab_offset_y: usize,
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
    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn begin_panel_interaction(&mut self, point: WindowPoint) -> bool {
        self.panel_interaction.pending_panel_press = None;
        if let Some(panel_id) = self.panel_move_hit_from_window(point) {
            let viewport = self
                .layout
                .as_ref()
                .map(|l| (l.window_rect.width, l.window_rect.height))
                .unwrap_or((usize::MAX, usize::MAX));
            let Some(panel_rect) = self
                .panel_presentation
                .panel_rect_in_viewport(&panel_id, viewport.0, viewport.1)
            else {
                return false;
            };
            self.panel_interaction.active_panel_drag = Some(PanelDragState {
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

        // Phase 9F: panel_event_from_window は HTML hit-table のみを参照するため
        // 返り値は常に PanelEvent::Activate になる。SetValue / SetText / DragValue は
        // Wasm パネル handler が dispatch_panel_event 経由で直接発行する経路に統一。
        let PanelEvent::Activate { panel_id, node_id } = &event else {
            return false;
        };
        let changed = self
            .panel_presentation
            .focus_panel_node(&self.panel_runtime, panel_id, node_id);
        self.panel_interaction.pending_panel_press = Some(PanelPressState {
            panel_id: panel_id.clone(),
            node_id: node_id.clone(),
        });
        self.refresh_panel_surface_if_changed(changed);
        // パネルボタンにヒットした場合は常に処理済みとしてキャンバスへのフォールスルーを防ぐ
        true
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn drag_panel_interaction(&mut self, point: WindowPoint) -> bool {
        let Some(state) = self.panel_interaction.active_panel_drag.clone() else {
            return false;
        };
        let PanelDragState {
            panel_id,
            grab_offset_x,
            grab_offset_y,
        } = state;
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let (win_w, win_h) = (layout.window_rect.width, layout.window_rect.height);
        let window_x = point.x.max(0) as usize;
        let window_y = point.y.max(0) as usize;
        let previous_rect = self.panel_presentation.panel_rect(&panel_id);
        let changed = self.panel_presentation.move_panel_to(
            &panel_id,
            window_x.saturating_sub(grab_offset_x),
            window_y.saturating_sub(grab_offset_y),
            win_w,
            win_h,
        );
        if changed {
            self.mark_panel_surface_dirty();
            if let Some(rect) = previous_rect {
                self.append_ui_panel_dirty_rect(rect);
            }
        }
        changed
    }

    /// パネル control をアクティブ化する。
    pub(super) fn activate_panel_control(&mut self, panel_id: &str, node_id: &str) -> bool {
        self.dispatch_panel_event(PanelEvent::Activate {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        })
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
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

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(crate) fn execute_host_action(&mut self, action: HostAction) -> bool {
        match action {
            HostAction::DispatchCommand(command) => self.execute_command(command),
            HostAction::RequestService(request) => self.execute_service_request(request),
            HostAction::InvokePanelHandler { .. } => false,
            HostAction::MovePanel {
                panel_id,
                direction,
            } => {
                let previous_rect = self.panel_presentation.panel_rect(&panel_id);
                let changed = self.panel_presentation.move_panel(&panel_id, direction);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                    self.persist_session_state();
                    if let Some(rect) = previous_rect {
                        self.append_ui_panel_dirty_rect(rect);
                    }
                }
                changed
            }
            HostAction::SetPanelVisibility { panel_id, visible } => {
                let previous_rect = self.panel_presentation.panel_rect(&panel_id);
                let changed = self
                    .panel_presentation
                    .set_panel_visibility(&panel_id, visible);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                    self.persist_session_state();
                    if let Some(rect) = previous_rect {
                        self.append_ui_panel_dirty_rect(rect);
                    }
                }
                changed
            }
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn dispatch_panel_event(&mut self, event: PanelEvent) -> bool {
        self.dispatch_panel_event_with_command(event).0
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
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
            if runtime.config_changed
                || self.panel_runtime.persistent_panel_configs() != previous_configs
            {
                self.persist_session_state();
            }
            if !runtime.changed_panel_ids.is_empty() {
                self.panel_presentation
                    .mark_runtime_panels_dirty(&runtime.changed_panel_ids);
                changed = true;
            }
            actions.extend(runtime.actions);
        }

        for action in actions {
            if first_command.is_none() {
                match &action {
                    HostAction::DispatchCommand(command) => {
                        first_command = Some(command.clone());
                    }
                    HostAction::RequestService(_) => {
                        first_command = Some(app_core::Command::Noop);
                    }
                    HostAction::InvokePanelHandler { .. }
                    | HostAction::MovePanel { .. }
                    | HostAction::SetPanelVisibility { .. } => {}
                }
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

    /// 入力や種別に応じて処理を振り分ける。
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

    /// 次 パネル control へフォーカスを移す。
    pub(crate) fn focus_next_panel_control(&mut self) -> bool {
        let changed = self.panel_presentation.focus_next(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// 前 パネル control へフォーカスを移す。
    pub(crate) fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.panel_presentation.focus_previous(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// Focused パネル control をアクティブ化する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub(crate) fn activate_focused_panel_control(&mut self) -> Option<app_core::Command> {
        let event = self.panel_presentation.activate_focused()?;
        self.dispatch_panel_event_with_command(event).1
    }

    /// insert テキスト into focused パネル 入力 を計算して返す。
    pub(crate) fn insert_text_into_focused_panel_input(&mut self, text: &str) -> bool {
        let Some(event) = self
            .panel_presentation
            .insert_text_into_focused_input(&self.panel_runtime, text)
        else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    /// backspace focused パネル 入力 を計算して返す。
    pub(crate) fn backspace_focused_panel_input(&mut self) -> bool {
        let Some(event) = self
            .panel_presentation
            .backspace_focused_input(&self.panel_runtime)
        else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    /// delete focused パネル 入力 を計算して返す。
    pub(crate) fn delete_focused_panel_input(&mut self) -> bool {
        let Some(event) = self
            .panel_presentation
            .delete_focused_input(&self.panel_runtime)
        else {
            return false;
        };
        self.dispatch_panel_event(event)
    }

    /// move focused パネル 入力 cursor を計算して返す。
    pub(crate) fn move_focused_panel_input_cursor(&mut self, delta_chars: isize) -> bool {
        let changed = self
            .panel_presentation
            .move_focused_input_cursor(&self.panel_runtime, delta_chars);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// move focused パネル 入力 cursor to start を計算して返す。
    pub(crate) fn move_focused_panel_input_cursor_to_start(&mut self) -> bool {
        let changed = self
            .panel_presentation
            .move_focused_input_cursor_to_start(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// move focused パネル 入力 cursor to end を計算して返す。
    pub(crate) fn move_focused_panel_input_cursor_to_end(&mut self) -> bool {
        let changed = self
            .panel_presentation
            .move_focused_input_cursor_to_end(&self.panel_runtime);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// Focused パネル 入力 preedit を設定する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub(crate) fn set_focused_panel_input_preedit(&mut self, preedit: Option<String>) -> bool {
        let changed = self
            .panel_presentation
            .set_focused_input_preedit(&self.panel_runtime, preedit);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// Has focused パネル 入力 かどうかを返す。
    pub(crate) fn has_focused_panel_input(&self) -> bool {
        self.panel_presentation
            .has_focused_text_input(&self.panel_runtime)
    }

    /// スクロール パネル サーフェス に必要な描画内容を組み立てる。
    pub(crate) fn scroll_panel_surface(&mut self, delta_lines: i32) -> bool {
        let viewport_height = self
            .layout
            .as_ref()
            .map(|layout| layout.panel_surface_rect.height)
            .unwrap_or(0);
        if viewport_height == 0 {
            return false;
        }

        let changed = self
            .panel_presentation
            .scroll_panels(delta_lines, viewport_height);
        self.refresh_panel_surface_if_changed(changed)
    }

    /// パネル イベント from ウィンドウ を計算して返す。
    ///
    /// HTML パネル hit テーブルだけを参照する。Phase 9F で DSL surface 側の hit-test 経路は
    /// 削除済みのため、ここに来るのは HTML パネルのみ。
    /// 値を生成できない場合は `None` を返します。
    pub(super) fn panel_event_from_window(&self, point: WindowPoint) -> Option<PanelEvent> {
        if point.x < 0 || point.y < 0 {
            return None;
        }
        let (panel_id, node_id) = self
            .panel_presentation
            .html_panel_hit_at(point.x as usize, point.y as usize)?;
        Some(PanelEvent::Activate { panel_id, node_id })
    }

    /// パネル is hovered を計算して返す。
    pub(crate) fn panel_is_hovered(&self, x: i32, y: i32) -> bool {
        let point = WindowPoint::new(x, y);
        self.panel_move_hit_from_window(point).is_some()
            || self.panel_event_from_window(point).is_some()
    }

    /// パネル move hit from ウィンドウ を計算して返す。
    ///
    /// HTML パネルの move handle (タイトルバー) のみを確認する。
    /// 値を生成できない場合は `None` を返します。
    pub(super) fn panel_move_hit_from_window(&self, point: WindowPoint) -> Option<String> {
        if point.x < 0 || point.y < 0 {
            return None;
        }
        self.panel_presentation
            .html_panel_move_handle_at(point.x as usize, point.y as usize)
    }
}
