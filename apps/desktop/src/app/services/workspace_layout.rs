//! `workspace_layout.*` サービス要求を処理する。
//!
//! Phase 12 (ADR 014): builtin.workspace-layout パネルが
//! チェックボックス操作経由で `workspace_layout.set_panel_visibility`
//! を呼び、panel_workspace の visibility を切り替える。

use std::collections::BTreeMap;

use panel_runtime::{ServiceRequest, services::names};
use panel_workspace::PanelMoveDirection;
use serde_json::json;

use super::DesktopApp;

/// `workspace_layout.*` サービス要求を振り分ける。該当しない場合は `None`。
pub(crate) fn handle_workspace_layout_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY => {
            let panel_id = request.string("panel_id")?;
            let visible = request
                .payload
                .get("visible")
                .and_then(|value| value.as_bool())?;
            app.set_panel_visibility_from_workspace_layout(panel_id, visible)
        }
        names::WORKSPACE_LAYOUT_MOVE_PANEL => {
            let panel_id = request.string("panel_id")?;
            let direction = match request.string("direction")? {
                "up" => PanelMoveDirection::Up,
                "down" => PanelMoveDirection::Down,
                _ => return None,
            };
            app.move_panel_from_workspace_layout(panel_id, direction)
        }
        _ => return None,
    };
    Some(changed)
}

impl DesktopApp {
    /// 指定パネルの可視性を切り替え、関連 dirty フラグと永続化を発火する。
    fn set_panel_visibility_from_workspace_layout(&mut self, panel_id: &str, visible: bool) -> bool {
        let previous_rect = self.panel_rect_in_window(panel_id);
        if !self
            .panel_workspace
            .set_panel_visibility(panel_id, visible)
        {
            return false;
        }
        // BL-095: workspace セクション購読パネル (= パネル管理) を dirty にする。
        // ビルトイン ID 直書きを subscribes 解決へ置換。
        self.sync_ui_from_section("workspace");
        self.mark_status_dirty();
        self.persist_session_state();
        if let Some(rect) = previous_rect {
            self.append_ui_panel_dirty_rect(rect);
        }
        true
    }

    /// 指定パネルを並び順で移動し、関連 dirty フラグと永続化を発火する。
    fn move_panel_from_workspace_layout(
        &mut self,
        panel_id: &str,
        direction: PanelMoveDirection,
    ) -> bool {
        let previous_rect = self.panel_rect_in_window(panel_id);
        if !self.panel_workspace.move_panel(panel_id, direction) {
            return false;
        }
        // BL-095: workspace セクション購読パネル (= パネル管理) を dirty にする。
        self.sync_ui_from_section("workspace");
        self.mark_status_dirty();
        self.persist_session_state();
        if let Some(rect) = previous_rect {
            self.append_ui_panel_dirty_rect(rect);
        }
        true
    }

    /// ワークスペース登録パネル一覧 (id / title / visible) を JSON 化する。
    /// builtin.workspace-layout が host state 経由で参照する。
    /// `workspace-layout` 自身も含めて返し、UI 側でフィルタする。
    pub(crate) fn build_workspace_panels_json(&self) -> String {
        let titles: BTreeMap<String, String> =
            self.panel_runtime.panel_id_titles().into_iter().collect();
        let workspace_layout = self.panel_workspace.workspace_layout();
        let entries: Vec<_> = workspace_layout
            .panels
            .iter()
            .filter_map(|entry| {
                titles.get(entry.id.as_str()).map(|title| {
                    json!({
                        "id": entry.id,
                        "title": title,
                        "visible": entry.visible,
                    })
                })
            })
            .collect();
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    }
}
