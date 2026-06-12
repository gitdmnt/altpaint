//! Undo/Redo の履歴基盤。
//!
//! `EditHistory` は操作記録（`HistoryEntry`）のスタックを管理する。
//! undo 方式はビットマップ前後スナップショット（`BitmapPatch`）の保存・復元方式。

use crate::{CanvasBitmap, KomaId};
use geometry::PageDirtyRect;

/// 履歴スタックのデフォルト容量。
pub const DEFAULT_HISTORY_CAPACITY: usize = 50;

/// GPU テクスチャスナップショットへの型消去ラッパー。
///
/// `app-core` は wgpu に依存しないため、desktop 層で定義した GPU スナップショット型を
/// `Arc<dyn Any + Send + Sync>` として保持する。desktop 層で `downcast_ref` して使う。
#[derive(Clone)]
pub struct OpaqueGpuData(pub std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl std::fmt::Debug for OpaqueGpuData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpaqueGpuData")
    }
}

/// 履歴エントリ。
///
/// 将来的にレイヤー追加・削除などのドキュメントコマンドを追加できるよう
/// enum として定義する。
#[derive(Debug, Clone)]
pub enum HistoryEntry {
    /// ビットマップ前後スナップショット方式。
    BitmapPatch {
        koma_id: KomaId,
        layer_index: usize,
        /// コマローカル座標系の変更領域。
        dirty: PageDirtyRect,
        /// 操作前のビットマップ領域。
        before: CanvasBitmap,
        /// 操作後のビットマップ領域。
        after: CanvasBitmap,
    },
    /// GPU テクスチャスナップショット方式（`gpu` feature 有効時のストローク用）。
    GpuBitmapPatch {
        koma_id: KomaId,
        layer_index: usize,
        /// コマローカル座標系の変更領域。
        dirty: PageDirtyRect,
        /// desktop 層で定義した `GpuPatchSnapshot` を保持する型消去ラッパー。
        gpu_data: OpaqueGpuData,
    },
}

/// Undo/Redo スタック。
///
/// `push` で過去スタックへ追加し、`undo` / `redo` で移動する。
/// 容量超過時は最も古いエントリを破棄する。
pub struct EditHistory {
    past: Vec<HistoryEntry>,
    future: Vec<HistoryEntry>,
    capacity: usize,
}

impl EditHistory {
    /// デフォルト容量（50）で生成する。
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_HISTORY_CAPACITY)
    }

    /// 指定容量で生成する。
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            capacity,
        }
    }

    /// 操作を記録する。
    ///
    /// 記録時に future スタックをクリアする（新操作後は redo 不可）。
    /// 容量超過時は最も古いエントリを破棄する。
    pub fn push(&mut self, entry: HistoryEntry) {
        self.future.clear();
        if self.past.len() == self.capacity {
            self.past.remove(0);
        }
        self.past.push(entry);
    }

    /// 直前の操作を取り出す。past → future へ移動する。
    pub fn undo(&mut self) -> Option<HistoryEntry> {
        let entry = self.past.pop()?;
        self.future.push(entry.clone());
        Some(entry)
    }

    /// やり直し操作を取り出す。future → past へ移動する。
    pub fn redo(&mut self) -> Option<HistoryEntry> {
        let entry = self.future.pop()?;
        self.past.push(entry.clone());
        Some(entry)
    }

    /// undo 可能かどうかを返す。
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// redo 可能かどうかを返す。
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// 履歴を全消去する。
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_patch(x: usize) -> HistoryEntry {
        HistoryEntry::BitmapPatch {
            koma_id: KomaId(1),
            layer_index: 0,
            dirty: PageDirtyRect {
                x,
                y: 0,
                width: 1,
                height: 1,
            },
            before: CanvasBitmap::transparent(1, 1),
            after: CanvasBitmap::transparent(1, 1),
        }
    }

    /// push した後に undo すると past から取り出せることを確認する。
    #[test]
    fn push_and_undo_round_trip() {
        let mut history = EditHistory::new();
        history.push(make_patch(1));
        assert!(history.can_undo());
        let entry = history.undo();
        assert!(entry.is_some());
        assert!(!history.can_undo());
    }

    /// undo 後に redo すると future から取り出せることを確認する。
    #[test]
    fn undo_then_redo() {
        let mut history = EditHistory::new();
        history.push(make_patch(1));
        history.undo();
        assert!(history.can_redo());
        history.redo();
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    /// 新規 push で future がクリアされることを確認する。
    #[test]
    fn push_clears_future() {
        let mut history = EditHistory::new();
        history.push(make_patch(1));
        history.undo();
        assert!(history.can_redo());
        history.push(make_patch(2));
        assert!(!history.can_redo());
    }

    /// 容量超過時に最古エントリが破棄されることを確認する。
    #[test]
    fn capacity_evicts_oldest() {
        let mut history = EditHistory::with_capacity(2);
        history.push(make_patch(1));
        history.push(make_patch(2));
        history.push(make_patch(3));
        assert_eq!(history.past.len(), 2);
        // 最新の 2 つが残っていることを確認する
        let HistoryEntry::BitmapPatch { dirty, .. } = &history.past[0] else {
            panic!("BitmapPatch expected");
        };
        assert_eq!(dirty.x, 2);
    }

    /// clear で past/future が空になることを確認する。
    #[test]
    fn clear_empties_stacks() {
        let mut history = EditHistory::new();
        history.push(make_patch(1));
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
