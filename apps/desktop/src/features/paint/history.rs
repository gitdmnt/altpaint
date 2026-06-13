//! Undo/Redo の履歴基盤。
//!
//! `EditHistory` は操作記録 (`PaintPatch`) のスタックを管理する。
//! undo 方式はビットマップ / GPU テクスチャの前後スナップショット保存・復元方式。
//!
//! desktop 層に置くことで、`document-model` / `gpu-paint` の具体型を直接保持できる。
//! 旧 `app-core::history` の `OpaqueGpuData` (`Arc<dyn Any>`) + downcast を全廃し、
//! GPU パッチは `wgpu::Texture` を直接持つ型付き表現とした。

use document_model::KomaId;
use geometry::PageDirtyRect;
use raster::RgbaBitmap;

/// 履歴スタックのデフォルト容量。
pub(crate) const DEFAULT_HISTORY_CAPACITY: usize = 50;

/// CPU ビットマップ前後スナップショット方式のパッチ。
#[derive(Debug, Clone)]
pub(crate) struct BitmapPatch {
    pub(crate) koma_id: KomaId,
    pub(crate) layer_index: usize,
    /// コマローカル座標系の変更領域。
    pub(crate) dirty: PageDirtyRect,
    /// 操作前のビットマップ領域。
    pub(crate) before: RgbaBitmap,
    /// 操作後のビットマップ領域。
    pub(crate) after: RgbaBitmap,
}

/// GPU テクスチャ前後スナップショット方式のパッチ。
///
/// dirty 領域サイズの小テクスチャを `before` / `after` に保持する。
/// `LayerTextureStore::restore_region` に直接渡せる `wgpu::Texture` を保持する。
#[derive(Clone)]
pub(crate) struct GpuRegionPatch {
    pub(crate) koma_id: KomaId,
    pub(crate) layer_index: usize,
    /// コマローカル座標系の変更領域。
    pub(crate) dirty: PageDirtyRect,
    /// 操作前のテクスチャ領域。
    pub(crate) before: wgpu::Texture,
    /// 操作後のテクスチャ領域。
    pub(crate) after: wgpu::Texture,
}

impl std::fmt::Debug for GpuRegionPatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuRegionPatch")
            .field("koma_id", &self.koma_id)
            .field("layer_index", &self.layer_index)
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

/// 履歴エントリ。
///
/// CPU パッチと GPU パッチを型付きで区別する。`Arc<dyn Any>` と downcast を使わない。
#[derive(Debug, Clone)]
pub(crate) enum PaintPatch {
    /// CPU ビットマップ前後スナップショット方式。
    Cpu(BitmapPatch),
    /// GPU テクスチャスナップショット方式 (`gpu` 有効時のストローク / 塗りつぶし用)。
    Gpu(GpuRegionPatch),
}

/// Undo/Redo スタック。
///
/// `push` で過去スタックへ追加し、`undo` / `redo` で移動する。
/// 容量超過時は最も古いエントリを破棄する。
pub(crate) struct EditHistory {
    past: Vec<PaintPatch>,
    future: Vec<PaintPatch>,
    capacity: usize,
}

impl EditHistory {
    /// デフォルト容量 (50) で生成する。
    pub(crate) fn new() -> Self {
        Self::with_capacity(DEFAULT_HISTORY_CAPACITY)
    }

    /// 指定容量で生成する。
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            capacity,
        }
    }

    /// 操作を記録する。
    ///
    /// 記録時に future スタックをクリアする (新操作後は redo 不可)。
    /// 容量超過時は最も古いエントリを破棄する。
    pub(crate) fn push(&mut self, entry: PaintPatch) {
        self.future.clear();
        if self.past.len() == self.capacity {
            self.past.remove(0);
        }
        self.past.push(entry);
    }

    /// 直前の操作を取り出す。past → future へ移動する。
    pub(crate) fn undo(&mut self) -> Option<PaintPatch> {
        let entry = self.past.pop()?;
        self.future.push(entry.clone());
        Some(entry)
    }

    /// やり直し操作を取り出す。future → past へ移動する。
    pub(crate) fn redo(&mut self) -> Option<PaintPatch> {
        let entry = self.future.pop()?;
        self.past.push(entry.clone());
        Some(entry)
    }

    /// undo 可能かどうかを返す。
    pub(crate) fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// redo 可能かどうかを返す。
    pub(crate) fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// 履歴を全消去する。
    pub(crate) fn clear(&mut self) {
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

    fn make_patch(x: usize) -> PaintPatch {
        PaintPatch::Cpu(BitmapPatch {
            koma_id: KomaId(1),
            layer_index: 0,
            dirty: PageDirtyRect {
                x,
                y: 0,
                width: 1,
                height: 1,
            },
            before: RgbaBitmap::transparent(1, 1),
            after: RgbaBitmap::transparent(1, 1),
        })
    }

    /// push した後に undo すると past から取り出せ、CPU パッチが往復することを確認する。
    #[test]
    fn push_and_undo_round_trip() {
        let mut history = EditHistory::new();
        history.push(make_patch(1));
        assert!(history.can_undo());
        let entry = history.undo();
        let Some(PaintPatch::Cpu(patch)) = entry else {
            panic!("Cpu patch expected");
        };
        assert_eq!(patch.dirty.x, 1);
        assert!(!history.can_undo());
    }

    /// undo 後に redo すると future から取り出せることを確認する。
    #[test]
    fn undo_then_redo() {
        let mut history = EditHistory::new();
        history.push(make_patch(1));
        history.undo();
        assert!(history.can_redo());
        let entry = history.redo();
        let Some(PaintPatch::Cpu(patch)) = entry else {
            panic!("Cpu patch expected");
        };
        assert_eq!(patch.dirty.x, 1);
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
        let PaintPatch::Cpu(patch) = &history.past[0] else {
            panic!("Cpu patch expected");
        };
        assert_eq!(patch.dirty.x, 2);
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
