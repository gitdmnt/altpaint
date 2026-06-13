//! ペイント実行・履歴に関わる desktop 内モジュール群。
//!
//! 履歴 (`EditHistory` / `PaintPatch`) は `app-core` から移管され、
//! `document-model` / `gpu-paint` の具体型を直接保持する (BL-076)。
//! `features/paint` への最終配置は B7。

pub(crate) mod history;

pub(crate) use history::{BitmapPatch, EditHistory, GpuRegionPatch, PaintPatch};
