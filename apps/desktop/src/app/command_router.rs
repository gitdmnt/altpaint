//! `DocumentCommand` / `SessionCommand` の `DesktopApp` への適用ルーティング (D10)。
//!
//! I/O を伴う操作 (保存・読込・preset 入出力・undo/redo・新規作成) は
//! `ServiceRequest` 経路 (`execute_service_request`) に一本化されており、
//! ここではドキュメント変異とエディタセッション変更のみを扱う。
//!
//! ルーティング (ここ = apply 入口) と、コマンド種別ごとの宣言的副作用表
//! (`command_effects`) を分離している。

use document_model::DocumentCommand;
use editor_state::SessionCommand;

use super::DesktopApp;

impl DesktopApp {
    /// 純粋なドキュメント変異コマンドを適用し、種別ごとの副作用を実行する。
    pub(crate) fn apply_document_command(&mut self, command: &DocumentCommand) -> bool {
        self.document.apply(command);
        self.document_command_effects(command)
    }

    /// エディタセッションコマンド (ツール/色/ペン/ビュー) を適用し、種別ごとの副作用を実行する。
    pub(crate) fn apply_session_command(&mut self, command: &SessionCommand) -> bool {
        let previous_transform = self.document.session.view_transform;
        self.document.apply_session_command(command);
        self.session_command_effects(command, previous_transform)
    }
}
