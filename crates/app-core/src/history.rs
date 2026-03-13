//! Undo/Redo の履歴基盤。
//!
//! `CommandHistory` は操作記録（`HistoryEntry`）のスタックを管理する。
//! undo 方式は操作パラメータ保持の replay 方式であり、
//! 前状態の再構築は呼び出し元（canvas runtime）が担う。

use crate::BitmapEditRecord;

/// 履歴スタックのデフォルト容量。
pub const DEFAULT_HISTORY_CAPACITY: usize = 50;

/// 履歴エントリ。現在は描画操作のみ。
///
/// 将来的にレイヤー追加・削除などのドキュメントコマンドを追加できるよう
/// enum として定義する。
#[derive(Debug, Clone)]
pub enum HistoryEntry {
    BitmapOp(BitmapEditRecord),
}

/// Undo/Redo スタック。
///
/// `push` で過去スタックへ追加し、`undo` / `redo` で移動する。
/// 容量超過時は最も古いエントリを破棄する。
pub struct CommandHistory {
    past: Vec<HistoryEntry>,
    future: Vec<HistoryEntry>,
    capacity: usize,
}

impl CommandHistory {
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

    /// 過去スタックの全エントリへの参照を返す（replay 用）。
    pub fn past_entries(&self) -> &[HistoryEntry] {
        &self.past
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BitmapEditOperation, BitmapEditRecord, ColorRgba8, PanelId, PanelLocalPoint};

    fn make_record(x: f32) -> BitmapEditRecord {
        BitmapEditRecord {
            panel_id: PanelId(1),
            layer_index: 0,
            operation: BitmapEditOperation::Stamp {
                at: PanelLocalPoint { x, y: 0.0 },
                pressure: 1.0,
            },
            pen_snapshot: crate::PenPreset::default(),
            color_snapshot: ColorRgba8 {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            tool_id: "pen".to_string(),
        }
    }

    /// push した後に undo すると past から取り出せることを確認する。
    #[test]
    fn push_and_undo_round_trip() {
        let mut history = CommandHistory::new();
        history.push(HistoryEntry::BitmapOp(make_record(1.0)));
        assert!(history.can_undo());
        let entry = history.undo();
        assert!(entry.is_some());
        assert!(!history.can_undo());
    }

    /// undo 後に redo すると future から取り出せることを確認する。
    #[test]
    fn undo_then_redo() {
        let mut history = CommandHistory::new();
        history.push(HistoryEntry::BitmapOp(make_record(1.0)));
        history.undo();
        assert!(history.can_redo());
        history.redo();
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    /// 新規 push で future がクリアされることを確認する。
    #[test]
    fn push_clears_future() {
        let mut history = CommandHistory::new();
        history.push(HistoryEntry::BitmapOp(make_record(1.0)));
        history.undo();
        assert!(history.can_redo());
        history.push(HistoryEntry::BitmapOp(make_record(2.0)));
        assert!(!history.can_redo());
    }

    /// 容量超過時に最古エントリが破棄されることを確認する。
    #[test]
    fn capacity_evicts_oldest() {
        let mut history = CommandHistory::with_capacity(2);
        history.push(HistoryEntry::BitmapOp(make_record(1.0)));
        history.push(HistoryEntry::BitmapOp(make_record(2.0)));
        history.push(HistoryEntry::BitmapOp(make_record(3.0)));
        assert_eq!(history.past.len(), 2);
        // 最新の 2 つが残っていることを確認する
        if let HistoryEntry::BitmapOp(r) = &history.past[0] {
            if let BitmapEditOperation::Stamp { at, .. } = r.operation {
                assert!((at.x - 2.0).abs() < f32::EPSILON);
            }
        }
    }

    /// clear で past/future が空になることを確認する。
    #[test]
    fn clear_empties_stacks() {
        let mut history = CommandHistory::new();
        history.push(HistoryEntry::BitmapOp(make_record(1.0)));
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
