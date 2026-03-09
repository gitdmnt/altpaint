//! ポインタ・キーボード・パネル入力の解釈を `DesktopApp` へ追加する。
//!
//! OS 由来の生イベントをドキュメント編集やパネル操作へ変換し、
//! ランタイム側が UI 詳細を知らずに済むようにする。

use app_core::Command;
use plugin_api::{HostAction, PanelEvent};

use super::{DesktopApp, PanelDragState};
use crate::canvas_bridge::{
    CanvasInputState, CanvasPointerEvent, map_view_to_canvas_with_transform,
};
use crate::frame::{brush_preview_rect, map_view_to_surface, map_view_to_surface_clamped};

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
            brush_preview_rect(layout, bitmap_width, bitmap_height, transform, position)
        }) {
            self.append_canvas_host_dirty_rect(previous);
        }
        if let Some(next) = next.and_then(|position| {
            brush_preview_rect(layout, bitmap_width, bitmap_height, transform, position)
        }) {
            self.append_canvas_host_dirty_rect(next);
        }
        true
    }

    /// ポインタ押下をキャンバスまたはパネル操作へ振り分ける。
    pub(crate) fn handle_pointer_pressed(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_position_from_window(x, y).is_some() {
            return self.handle_canvas_pointer("down", x, y);
        }

        self.begin_panel_interaction(x, y)
    }

    /// ポインタ解放を現在の操作状態に応じて処理する。
    pub(crate) fn handle_pointer_released(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("up", x, y);
        }
        if self.active_panel_drag.take().is_some() {
            return false;
        }
        self.handle_panel_pointer(x, y)
    }

    /// ポインタ移動をドラッグ中の対象へ配送する。
    pub(crate) fn handle_pointer_dragged(&mut self, x: i32, y: i32) -> bool {
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("drag", x, y);
        }

        if self.active_panel_drag.is_some() {
            return self.drag_panel_interaction(x, y);
        }

        false
    }

    /// パネル上の押下開始を解釈して、必要ならドラッグ状態を開始する。
    fn begin_panel_interaction(&mut self, x: i32, y: i32) -> bool {
        let Some(event) = self.panel_event_from_window(x, y) else {
            self.active_panel_drag = None;
            return false;
        };

        match &event {
            PanelEvent::SetValue {
                panel_id, node_id, ..
            } => {
                self.active_panel_drag = Some(PanelDragState {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                });
                self.dispatch_panel_event(event)
            }
            PanelEvent::Activate { .. }
            | PanelEvent::SetText { .. }
            | PanelEvent::Keyboard { .. } => false,
        }
    }

    /// スライダードラッグ中の移動イベントを現在ノードへ配送する。
    fn drag_panel_interaction(&mut self, x: i32, y: i32) -> bool {
        let Some(state) = self.active_panel_drag.clone() else {
            return false;
        };
        let Some(event) = self.panel_drag_event_from_window(&state, x, y) else {
            return false;
        };
        self.dispatch_panel_event(event)
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

        for action in self.ui_shell.handle_panel_event(&event) {
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
            return false;
        };
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
    pub(crate) fn handle_canvas_pointer(&mut self, action: &str, x: i32, y: i32) -> bool {
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

        match action {
            "down" => {
                self.canvas_input.is_drawing = true;
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
                self.execute_canvas_command(canvas_x, canvas_y, None)
            }
            "drag" if self.canvas_input.is_drawing => {
                let from = self.canvas_input.last_position;
                let changed = self.execute_canvas_command(canvas_x, canvas_y, from);
                self.canvas_input.last_position = Some((canvas_x, canvas_y));
                changed
            }
            "up" => {
                let from = self.canvas_input.last_position;
                let changed = if self.canvas_input.is_drawing && from != Some((canvas_x, canvas_y))
                {
                    self.execute_canvas_command(canvas_x, canvas_y, from)
                } else {
                    false
                };
                self.canvas_input.is_drawing = false;
                self.canvas_input.last_position = None;
                changed
            }
            _ => false,
        }
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
        let layout = self.layout.as_ref()?;
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = map_view_to_surface(
            panel_surface.width,
            panel_surface.height,
            layout.panel_surface_rect,
            x,
            y,
        )?;
        panel_surface.hit_test(surface_x, surface_y)
    }

    /// ドラッグ継続中のパネルノードへ値変更イベントを生成する。
    fn panel_drag_event_from_window(
        &self,
        state: &PanelDragState,
        x: i32,
        y: i32,
    ) -> Option<PanelEvent> {
        let layout = self.layout.as_ref()?;
        let panel_surface = self.panel_surface.as_ref()?;
        let (surface_x, surface_y) = map_view_to_surface_clamped(
            panel_surface.width,
            panel_surface.height,
            layout.panel_surface_rect,
            x,
            y,
        )?;
        panel_surface.drag_event(&state.panel_id, &state.node_id, surface_x, surface_y)
    }
}
