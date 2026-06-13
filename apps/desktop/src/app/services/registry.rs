//! service request の名前空間 registry (BL-111)。
//!
//! 各 feature が自分のサービスハンドラを 1 つ登録する。ハンドラは
//! `request.name` が自分の担当する wire 名であれば `Some(changed)` を返し、
//! 該当しなければ `None` を返して次のハンドラへ委譲する。
//!
//! 旧 `execute_service_request` の 10 連 if-let チェーンを置換する。新 feature の
//! 追加は registry へハンドラを 1 つ足すだけで済み、`execute_service_request` 本体の
//! 横断編集は不要になる。

use panel_runtime::ServiceRequest;

use super::DesktopApp;

/// feature が登録するサービスハンドラの型。
///
/// 担当 wire 名なら `Some(changed)`、非該当なら `None` を返す。
pub(crate) type ServiceHandler = fn(&mut DesktopApp, &ServiceRequest) -> Option<bool>;

/// 各 feature が登録するサービスハンドラの registry。
///
/// 登録順は探索順だが、wire 名の名前空間は互いに素なので順序に挙動依存はない。
pub(crate) const SERVICE_HANDLERS: &[ServiceHandler] = &[
    super::project_io::handle_project_service_request,
    super::workspace_io::handle_workspace_service_request,
    super::workspace_layout::handle_workspace_layout_service_request,
    super::tool_catalog::handle_tool_catalog_service_request,
    super::view::handle_view_service_request,
    super::koma_navigation::handle_koma_navigation_service_request,
    super::history::handle_history_service_request,
    super::snapshot::handle_snapshot_service_request,
    super::export::handle_export_service_request,
    super::text_render::handle_text_render_service_request,
];

impl DesktopApp {
    /// service request を registry のハンドラへ順に委譲する。
    ///
    /// 最初に `Some` を返したハンドラの結果を採用する。どのハンドラも担当しない
    /// (未登録 wire 名) 場合は `false`。
    pub(crate) fn execute_service_request(&mut self, request: ServiceRequest) -> bool {
        for handler in SERVICE_HANDLERS {
            if let Some(changed) = handler(self, &request) {
                return changed;
            }
        }
        false
    }
}
