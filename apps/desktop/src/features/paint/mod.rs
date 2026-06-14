//! paint feature スライス (BL-111 / D9)。
//!
//! ペイント実行 (`execute`)、編集履歴 (`history`)、ブラシプレビュー dirty rect
//! 演算 (`preview`) を所有する。
//!
//! 履歴 (`EditHistory` / `PaintPatch`) は `app-core` から移管され、
//! `document-model` / `gpu-paint` の具体型を直接保持する (BL-076)。
//! D9: `app/services/project_io.rs` のペイント実行+履歴部 (8 割) を
//! `execute` へ分離した。I/O 部 (2 割) は features/project へ。

mod backend;
mod execute;
mod history;
mod history_service;
mod preview;

pub(crate) use backend::{CpuPaintBackend, GpuPaintBackend};
pub(crate) use history::{BitmapPatch, EditHistory, GpuRegionPatch, PaintPatch};
pub(crate) use history_service::handle_history_service_request;
pub(crate) use preview::brush_preview_dirty_rect;

use crate::app::DesktopApp;

impl DesktopApp {
    /// 編集履歴を全消去する。スナップショット復元後など、履歴と文書の整合が
    /// 取れなくなる操作の後に呼ぶ。paint feature が `EditHistory` 正本を所有する。
    pub(crate) fn clear_edit_history(&mut self) {
        self.paint.history.clear();
    }
}
