//! パネル発のホストアクション/イベントのルーティング (D14)。
//!
//! `HostRequest` (Document/Session コマンド・サービス要求) と `PanelEvent`
//! (Activate 等) を受け取り、適切な適用経路へ振り分ける。パネル操作の
//! 幾何ステートマシン (drag/resize/press) は `features/panel_interaction` が担う。
//!
//! B7 で `app/panel_dispatch.rs` のルータ部をここへ分離した。

use geometry::WindowPoint;
use panel_runtime::{HostRequest, PanelEvent};

use super::DesktopApp;

impl DesktopApp {
    pub(crate) fn activate_panel_control(&mut self, panel_id: &str, node_id: &str) -> bool {
        self.dispatch_panel_event(PanelEvent::Activate {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        })
    }

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
        changed |= !runtime.changed_panel_ids.is_empty();
        for action in runtime.actions {
            changed |= self.execute_host_request(action);
        }
        self.request_panel_reconcile_if_changed(changed)
    }

    pub(crate) fn execute_host_request(&mut self, request: HostRequest) -> bool {
        // BL-065: バックグラウンドジョブの回収は prepare_present_frame に一本化する。
        // host request ごとの二重回収は廃止 (毎フレーム冒頭で 1 回だけ回収される)。
        //
        // P3: パネル可視性/並び替えは translator 経由で workspace_layout サービス
        // (`RequestService`) に一本化済み。専用 variant は廃止した。
        match request {
            HostRequest::DispatchDocumentCommand(command) => {
                self.apply_document_command(&command)
            }
            HostRequest::DispatchSessionCommand(command) => self.apply_session_command(&command),
            HostRequest::RequestService(request) => self.execute_service_request(request),
        }
    }

    pub(crate) fn dispatch_panel_event(&mut self, event: PanelEvent) -> bool {
        self.dispatch_panel_event_tracking_actions(event).0
    }

    /// パネルイベントを dispatch し、`(changed, produced_action)` を返す。
    /// `produced_action` は何らかの `HostRequest` が発行されたかを示す
    /// (`activate_focused_panel_control` の戻り値判定に使う)。
    fn dispatch_panel_event_tracking_actions(&mut self, event: PanelEvent) -> (bool, bool) {
        let mut changed = false;

        let mut needs_redraw = true;
        let mut produced_action = false;
        // Activate は focus を更新したうえで常にランタイムへ転送する。
        if let PanelEvent::Activate { panel_id, node_id } = &event {
            self.panel_workspace.focus_panel_node(panel_id, node_id);
        }

        // BL-097: config 変化検知は runtime の対象パネル単体比較 (`config_changed`) に
        // 一本化する。desktop 側の全パネル map 二重比較は廃止。
        // service 経由の config 変更 (`update_panel_config`) は変更箇所が自前で永続化する。
        let runtime = self.panel_runtime.dispatch_event(&event);
        if runtime.config_changed {
            self.persist_session_state();
        }
        changed |= !runtime.changed_panel_ids.is_empty();
        let actions = runtime.actions;

        for action in actions {
            produced_action = true;
            needs_redraw |= self.execute_host_request(action);
        }

        let changed = changed || needs_redraw;
        if changed {
            self.request_panel_reconcile();
        }
        (changed, produced_action)
    }

    pub(crate) fn handle_panel_pointer(&mut self, point: WindowPoint) -> bool {
        let Some(event) = self.panel_event_from_window(point) else {
            self.panel_interaction.pending_panel_press = None;
            return false;
        };
        let should_dispatch = matches!(
            (&self.panel_interaction.pending_panel_press, &event),
            (
                Some(press),
                PanelEvent::Activate {
                    panel_id: released_panel_id,
                    node_id: released_node_id,
                }
            ) if &press.panel_id == released_panel_id && &press.node_id == released_node_id
        );
        self.panel_interaction.pending_panel_press = None;
        if !should_dispatch {
            return false;
        }
        self.dispatch_panel_event(event)
    }

    pub(crate) fn focus_next_panel_control(&mut self) -> bool {
        let changed = self.panel_workspace.focus_next();
        self.request_panel_reconcile_if_changed(changed)
    }

    pub(crate) fn focus_previous_panel_control(&mut self) -> bool {
        let changed = self.panel_workspace.focus_previous();
        self.request_panel_reconcile_if_changed(changed)
    }

    /// フォーカス中のパネルコントロールを起動し、`HostRequest` が発行されたら `true` を返す。
    pub(crate) fn activate_focused_panel_control(&mut self) -> bool {
        let Some((panel_id, node_id)) = self.panel_workspace.activate_focused() else {
            return false;
        };
        let event = PanelEvent::Activate { panel_id, node_id };
        self.dispatch_panel_event_tracking_actions(event).1
    }
}
