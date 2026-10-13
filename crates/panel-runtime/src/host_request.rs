//! パネル (Wasm) ↔ ホスト間のイベント/要求型 (旧 `panel-api`、C9 で panel-runtime へ移設)。

use document_model::DocumentCommand;
use editor_state::SessionCommand;

use crate::services::ServiceRequest;

/// パネル (Wasm) → ホストへの要求。
///
/// P3: パネル可視性/並び替えは workspace_layout サービス経路 (`RequestService`)
/// に一本化済みのため、専用 variant (`MovePanel` / `SetPanelVisibility`) は削除した。
/// translator が `RequestDescriptor` を 3 経路へ振り分けた結果を搬送する。
#[derive(Debug, Clone, PartialEq)]
pub enum HostRequest {
    /// 純粋なドキュメント変異コマンドを適用する。
    DispatchDocumentCommand(DocumentCommand),
    /// エディタセッション (ツール/色/ペン/ビュー) を変更する。
    /// translator が namespace から振り分けるセッション経路。
    DispatchSessionCommand(SessionCommand),
    /// I/O を伴うホストサービス要求。
    RequestService(ServiceRequest),
}

/// ホスト → パネルへの UI イベント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    Activate {
        panel_id: String,
        node_id: String,
    },
    SetValue {
        panel_id: String,
        node_id: String,
        value: i32,
    },
    DragValue {
        panel_id: String,
        node_id: String,
        from: i32,
        to: i32,
    },
    SetText {
        panel_id: String,
        node_id: String,
        value: String,
    },
    Keyboard {
        panel_id: String,
        shortcut: String,
        key: String,
        repeat: bool,
    },
}
