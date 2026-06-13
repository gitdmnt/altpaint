//! ペイント実行・履歴に関わる desktop 内モジュール群。
//!
//! 履歴 (`EditHistory` / `PaintPatch`) は `app-core` から移管され、
//! `document-model` / `gpu-paint` の具体型を直接保持する (BL-076)。
//! `features/paint` への最終配置は B7。

pub(crate) mod history;

pub(crate) use history::{BitmapPatch, EditHistory, GpuRegionPatch, PaintPatch};

use super::DesktopApp;

impl DesktopApp {
    /// 編集履歴を全消去する。スナップショット復元後など、履歴と文書の整合が
    /// 取れなくなる操作の後に呼ぶ。paint feature が `EditHistory` 正本を所有する。
    pub(crate) fn clear_edit_history(&mut self) {
        self.paint.history.clear();
    }
}
