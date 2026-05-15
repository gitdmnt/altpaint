//! `workspace_layout.*` サービス要求を処理する。
//!
//! Phase 12 (ADR 014): builtin.workspace-layout パネルが
//! チェックボックス操作経由で `workspace_layout.set_panel_visibility`
//! を呼び、panel_presentation の visibility を切り替える。

use std::collections::BTreeMap;

use panel_api::{ServiceRequest, services::names};
use serde_json::json;

use super::DesktopApp;

impl DesktopApp {
    /// `workspace_layout.*` サービス要求を振り分ける。該当しない場合は `None`。
    pub(super) fn handle_workspace_layout_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::WORKSPACE_LAYOUT_SET_PANEL_VISIBILITY => {
                let panel_id = request.string("panel_id")?;
                let visible = request
                    .payload
                    .get("visible")
                    .and_then(|value| value.as_bool())?;
                self.set_panel_visibility_from_workspace_layout(panel_id, visible)
            }
            _ => return None,
        };
        Some(changed)
    }

    /// 指定パネルの可視性を切り替え、関連 dirty フラグと永続化を発火する。
    fn set_panel_visibility_from_workspace_layout(&mut self, panel_id: &str, visible: bool) -> bool {
        if !self
            .panel_presentation
            .set_panel_visibility(panel_id, visible)
        {
            return false;
        }
        self.panel_runtime
            .mark_dirty("builtin.workspace-layout");
        self.mark_panel_surface_dirty();
        self.mark_status_dirty();
        self.persist_session_state();
        true
    }

    /// ワークスペース登録パネル一覧 (id / title / visible) を JSON 化する。
    /// builtin.workspace-layout が host snapshot 経由で参照する。
    /// `workspace-layout` 自身も含めて返し、UI 側でフィルタする。
    pub(crate) fn build_workspace_panels_json(&self) -> String {
        let titles: BTreeMap<String, String> =
            self.panel_runtime.panel_id_titles().into_iter().collect();
        let workspace_layout = self.panel_presentation.workspace_layout();
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
