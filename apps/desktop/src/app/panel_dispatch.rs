//! パネル入力中継とホストアクション適用を集約する。

use app_core::WindowPoint;
use panel_api::{HostAction, PanelEvent, ResizeEdge};
use render_types::PixelRect;

use super::DesktopApp;
/// パネル移動ドラッグ中の被操作パネル情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelDragState {
    pub panel_id: String,
    pub grab_offset_x: usize,
    pub grab_offset_y: usize,
}

/// Phase 11: パネルリサイズドラッグ中の状態。
/// `start_rect` は pointer down 時のパネル矩形 (絶対 screen 座標)。
/// pointer move のたびに `start_pointer` からの差分でリサイズ後の矩形を再計算する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelResizeState {
    pub(crate) panel_id: String,
    pub(crate) edge: ResizeEdge,
    pub(crate) start_rect: PixelRect,
    pub(crate) start_pointer: (i32, i32),
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
    /// Phase 11: リサイズハンドルドラッグ中の状態。`active_panel_drag` と排他。
    pub(crate) active_panel_resize: Option<PanelResizeState>,
    pub(crate) pending_panel_press: Option<PanelPressState>,
}

impl DesktopApp {
    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn begin_panel_interaction(&mut self, point: WindowPoint) -> bool {
        self.panel_interaction.pending_panel_press = None;

        // Phase 11: リサイズハンドルを最優先で評価。タイトルバー上端 6px (= N edge) も
        // 移動より優先される。
        if let Some((panel_id, edge)) = self.panel_resize_hit_from_window(point) {
            let viewport = self
                .layout
                .as_ref()
                .map(|l| (l.window_rect.width, l.window_rect.height))
                .unwrap_or((usize::MAX, usize::MAX));
            let Some(start_rect) = self
                .panel_presentation
                .panel_rect_in_viewport(&panel_id, viewport.0, viewport.1)
            else {
                return false;
            };
            self.panel_interaction.active_panel_resize = Some(PanelResizeState {
                panel_id,
                edge,
                start_rect,
                start_pointer: (point.x, point.y),
            });
            self.panel_interaction.active_panel_drag = None;
            return true;
        }

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
        // Phase 11: リサイズが active なら先に処理する。
        if let Some(resize_state) = self.panel_interaction.active_panel_resize.clone() {
            return self.drag_resize_interaction(point, &resize_state);
        }

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

    /// Phase 11: リサイズドラッグ 1 フレームの処理。
    /// edge に応じて new_rect を算出し、最小/最大クランプ → workspace に書き戻し →
    /// engine の measured_size に即時反映する。
    fn drag_resize_interaction(
        &mut self,
        point: WindowPoint,
        state: &PanelResizeState,
    ) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let (win_w, win_h) = (layout.window_rect.width, layout.window_rect.height);
        let constraints = self
            .panel_runtime
            .panel_size_constraints(&state.panel_id)
            .unwrap_or_default();
        let new_rect = compute_resized_rect(
            state,
            (point.x, point.y),
            (win_w as u32, win_h as u32),
            constraints,
        );
        let previous_rect = self.panel_presentation.panel_rect(&state.panel_id);

        let applied = self.panel_presentation.resize_panel_keeping_anchor(
            &state.panel_id,
            new_rect,
            (win_w, win_h),
        );
        let Some(applied_rect) = applied else {
            return false;
        };
        // engine の measured_size をフレーム内追従させる
        let _ = self.panel_runtime.restore_panel_size(
            &state.panel_id,
            (
                applied_rect.width.max(1) as u32,
                applied_rect.height.max(1) as u32,
            ),
        );
        self.mark_panel_surface_dirty();
        if let Some(rect) = previous_rect {
            self.append_ui_panel_dirty_rect(rect);
        }
        true
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

    /// Phase 11: パネルリサイズハンドル hit from ウィンドウ。
    /// HTML パネル full rect のいずれかの 8 ハンドルにヒットした場合 `(panel_id, edge)`。
    pub(crate) fn panel_resize_hit_from_window(
        &self,
        point: WindowPoint,
    ) -> Option<(String, ResizeEdge)> {
        if point.x < 0 || point.y < 0 {
            return None;
        }
        self.panel_presentation
            .panel_resize_hit_at(point.x as usize, point.y as usize)
    }
}

/// Phase 11 デフォルトの絶対最小サイズ (CSS 制約がない場合のフォールバック)。
const ABS_MIN_WIDTH: i32 = 80;
const ABS_MIN_HEIGHT: i32 = 60;

/// Phase 11: リサイズドラッグ中の現在 pointer 位置から、edge に応じた new_rect を算出する。
/// CSS の `min/max-width/height` 制約 (`constraints`) を尊重し、viewport 内に
/// クランプ済みの矩形を返す。`constraints` が `None` の場合は CSS 制約なしと
/// みなしてデフォルト (80x60 min / viewport max) を適用する。
pub(crate) fn compute_resized_rect(
    state: &PanelResizeState,
    pointer: (i32, i32),
    viewport: (u32, u32),
    constraints: panel_runtime::PanelSizeConstraints,
) -> PixelRect {
    let (sx, sy) = state.start_pointer;
    let (px, py) = pointer;
    let dx = px - sx;
    let dy = py - sy;

    let start_left = state.start_rect.x as i32;
    let start_top = state.start_rect.y as i32;
    let start_right = start_left + state.start_rect.width as i32;
    let start_bottom = start_top + state.start_rect.height as i32;

    // CSS 制約と絶対最小を合成
    let min_w = constraints
        .min_width
        .map(|v| (v as i32).max(ABS_MIN_WIDTH))
        .unwrap_or(ABS_MIN_WIDTH);
    let min_h = constraints
        .min_height
        .map(|v| (v as i32).max(ABS_MIN_HEIGHT))
        .unwrap_or(ABS_MIN_HEIGHT);
    let max_w = constraints
        .max_width
        .map(|v| (v as i32).max(min_w))
        .unwrap_or(i32::MAX);
    let max_h = constraints
        .max_height
        .map(|v| (v as i32).max(min_h))
        .unwrap_or(i32::MAX);

    // edge 別に left/top/right/bottom を更新
    let mut new_left = start_left;
    let mut new_top = start_top;
    let mut new_right = start_right;
    let mut new_bottom = start_bottom;

    if state.edge.touches_left() {
        // width = right - left を [min_w, max_w] に収めるための left の許容範囲
        let left_min = start_right.saturating_sub(max_w).max(0);
        let left_max = start_right.saturating_sub(min_w);
        new_left = start_left.saturating_add(dx).clamp(left_min, left_max);
    }
    if state.edge.touches_right() {
        let right_min = start_left.saturating_add(min_w);
        let right_max = start_left.saturating_add(max_w).min(viewport.0 as i32);
        new_right = start_right.saturating_add(dx).clamp(right_min, right_max);
    }
    if state.edge.touches_top() {
        let top_min = start_bottom.saturating_sub(max_h).max(0);
        let top_max = start_bottom.saturating_sub(min_h);
        new_top = start_top.saturating_add(dy).clamp(top_min, top_max);
    }
    if state.edge.touches_bottom() {
        let bottom_min = start_top.saturating_add(min_h);
        let bottom_max = start_top.saturating_add(max_h).min(viewport.1 as i32);
        new_bottom = start_bottom.saturating_add(dy).clamp(bottom_min, bottom_max);
    }

    // 念のため幅・高さを [min, max] にクランプ
    let width = (new_right - new_left).clamp(min_w, max_w);
    let height = (new_bottom - new_top).clamp(min_h, max_h);
    let max_x = (viewport.0 as i32).saturating_sub(width).max(0);
    let max_y = (viewport.1 as i32).saturating_sub(height).max(0);
    let final_x = new_left.clamp(0, max_x);
    let final_y = new_top.clamp(0, max_y);

    PixelRect {
        x: final_x as usize,
        y: final_y as usize,
        width: width as usize,
        height: height as usize,
    }
}

#[cfg(test)]
mod resize_drag_tests {
    use super::*;
    use panel_runtime::PanelSizeConstraints;

    fn state(edge: ResizeEdge, start_rect: PixelRect, start_pointer: (i32, i32)) -> PanelResizeState {
        PanelResizeState {
            panel_id: "test.panel".to_string(),
            edge,
            start_rect,
            start_pointer,
        }
    }

    fn rect(x: usize, y: usize, w: usize, h: usize) -> PixelRect {
        PixelRect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn no_constraints() -> PanelSizeConstraints {
        PanelSizeConstraints::default()
    }

    #[test]
    fn south_east_grows_width_and_height_only() {
        let st = state(ResizeEdge::SouthEast, rect(100, 100, 200, 150), (300, 250));
        let result = compute_resized_rect(&st, (350, 290), (1000, 800), no_constraints());
        assert_eq!(result.x, 100);
        assert_eq!(result.y, 100);
        assert_eq!(result.width, 250); // 200 + 50
        assert_eq!(result.height, 190); // 150 + 40
    }

    #[test]
    fn north_west_moves_origin_and_shrinks_size() {
        let st = state(ResizeEdge::NorthWest, rect(100, 100, 200, 150), (100, 100));
        let result = compute_resized_rect(&st, (140, 130), (1000, 800), no_constraints());
        assert_eq!(result.x, 140);
        assert_eq!(result.y, 130);
        assert_eq!(result.width, 160); // (100+200)-140
        assert_eq!(result.height, 120); // (100+150)-130
    }

    #[test]
    fn min_size_clamps_to_80x60_when_no_css_constraints() {
        let st = state(ResizeEdge::SouthEast, rect(100, 100, 200, 150), (300, 250));
        // 大きく内側にドラッグ
        let result = compute_resized_rect(&st, (50, 50), (1000, 800), no_constraints());
        assert_eq!(result.width, 80);
        assert_eq!(result.height, 60);
    }

    #[test]
    fn west_drag_clamps_to_min_width_keeping_right_edge() {
        let st = state(ResizeEdge::West, rect(100, 100, 200, 150), (100, 175));
        // 右辺 (x=300) を超えるほど内側にドラッグ
        let result = compute_resized_rect(&st, (400, 175), (1000, 800), no_constraints());
        assert_eq!(result.width, 80);
        // x = right - min_width = 300 - 80 = 220
        assert_eq!(result.x, 220);
    }

    #[test]
    fn east_drag_clamps_to_viewport() {
        let st = state(ResizeEdge::East, rect(100, 100, 200, 150), (300, 175));
        // viewport の外までドラッグ
        let result = compute_resized_rect(&st, (5000, 175), (800, 600), no_constraints());
        // 右辺は viewport (800) でクランプされる
        assert_eq!(result.x, 100);
        assert_eq!(result.width, 700); // 800 - 100
    }

    #[test]
    fn north_drag_only_changes_y_and_height() {
        let st = state(ResizeEdge::North, rect(100, 100, 200, 150), (200, 100));
        let result = compute_resized_rect(&st, (200, 50), (1000, 800), no_constraints());
        assert_eq!(result.x, 100);
        assert_eq!(result.width, 200);
        assert_eq!(result.y, 50);
        assert_eq!(result.height, 200); // 150 + 50
    }

    /// CSS の min-width が絶対最小 80 より大きい場合、CSS 制約が優先される。
    #[test]
    fn css_min_width_overrides_absolute_minimum() {
        let st = state(ResizeEdge::SouthEast, rect(100, 100, 300, 200), (400, 300));
        let c = PanelSizeConstraints {
            min_width: Some(240),
            ..Default::default()
        };
        // 大きく内側にドラッグしても width は 240 でクランプ
        let result = compute_resized_rect(&st, (50, 50), (1000, 800), c);
        assert_eq!(result.width, 240);
        assert_eq!(result.height, 60); // height は CSS 制約なし → 絶対 min 60
    }

    /// CSS の max-width 指定があれば、それ以上に広がらない。
    #[test]
    fn css_max_width_caps_expansion() {
        let st = state(ResizeEdge::East, rect(100, 100, 200, 150), (300, 175));
        let c = PanelSizeConstraints {
            max_width: Some(400),
            ..Default::default()
        };
        // 大きく外側にドラッグしても width は 400 でクランプ
        let result = compute_resized_rect(&st, (5000, 175), (2000, 800), c);
        assert_eq!(result.width, 400);
        assert_eq!(result.x, 100);
    }

    /// CSS min-height / max-height も同様に効く。
    #[test]
    fn css_min_max_height_constraints_apply() {
        let st = state(ResizeEdge::South, rect(100, 100, 200, 150), (200, 250));
        let c = PanelSizeConstraints {
            min_height: Some(120),
            max_height: Some(300),
            ..Default::default()
        };
        // 縮める → min_height で止まる
        let small = compute_resized_rect(&st, (200, 50), (1000, 800), c);
        assert_eq!(small.height, 120);
        // 広げる → max_height で止まる
        let large = compute_resized_rect(&st, (200, 5000), (1000, 800), c);
        assert_eq!(large.height, 300);
    }

    /// W ハンドル: CSS min-width で右辺が固定されたまま左辺が動く。
    #[test]
    fn css_min_width_constraint_keeps_right_edge_fixed_on_west_drag() {
        let st = state(ResizeEdge::West, rect(100, 100, 300, 200), (100, 200));
        let c = PanelSizeConstraints {
            min_width: Some(240),
            ..Default::default()
        };
        // 右へドラッグ → min_width 240 でクランプ。x = right - 240 = 400 - 240 = 160
        let result = compute_resized_rect(&st, (300, 200), (1000, 800), c);
        assert_eq!(result.width, 240);
        assert_eq!(result.x, 160);
    }
}
