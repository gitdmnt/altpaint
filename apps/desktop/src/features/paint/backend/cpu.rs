//! CPU 参照バックエンド (BL-131 / BL-136)。
//!
//! `paint-engine` の CPU ops (`compute_paint_edits`) を呼び、アクティブレイヤーの
//! ビットマップへ直接書き込む。GPU 不在時の唯一の描画経路であり、`CpuCanvasSnapshot`
//! 表示フォールバックへ画素を供給する (BL-136: GPU 必須化はしない)。
//!
//! ストローク (Stamp / StrokeSegment) は開始時にレイヤー全体を `before_layer` として
//! 保存し、commit 時に前後スナップショットから `BitmapPatch` を生成する。

use geometry::{MergeInSpace, PageDirtyRect};
use paint_engine::{PaintEngine, PaintInput, PaintPlan};
use raster::{BitmapEdit, RgbaBitmap};

use super::super::{BitmapPatch, PaintPatch};
use super::{AppliedPaint, PaintBackend, PaintTarget};

/// ストローク中の CPU 差分追跡状態。
struct CpuPendingStroke {
    /// ストローク開始前のレイヤービットマップ全体。
    before_layer: RgbaBitmap,
    /// ストローク中に蓄積したコマローカル dirty rect の合計。
    dirty: Option<PageDirtyRect>,
}

/// CPU 参照バックエンド。
pub(crate) struct CpuPaintBackend {
    engine: PaintEngine,
    pending: Option<CpuPendingStroke>,
}

impl CpuPaintBackend {
    pub(crate) fn new() -> Self {
        Self {
            engine: PaintEngine::new(),
            pending: None,
        }
    }
}

/// ビットマップ編集列の dirty rect を 1 つの矩形へ畳み込む。編集が空なら `None`。
fn merged_dirty(edits: &[BitmapEdit]) -> Option<PageDirtyRect> {
    edits.iter().fold(None::<PageDirtyRect>, |acc, edit| {
        Some(match acc {
            Some(existing) => existing.merge(edit.dirty_rect),
            None => edit.dirty_rect,
        })
    })
}

impl PaintBackend for CpuPaintBackend {
    fn apply(
        &mut self,
        _plan: &PaintPlan,
        input: &PaintInput,
        target: &mut PaintTarget<'_>,
    ) -> AppliedPaint {
        let Some(edits) = self.engine.compute_paint_edits(target.document, input) else {
            return AppliedPaint::default();
        };
        let edit_dirty = merged_dirty(&edits);

        let is_stroke = matches!(
            input,
            PaintInput::Stamp { .. } | PaintInput::StrokeSegment { .. }
        );

        if is_stroke {
            // ストローク: dirty を蓄積し commit_stroke で patch 化する。
            if let (Some(pending), Some(edit_dirty)) = (self.pending.as_mut(), edit_dirty) {
                pending.dirty = Some(match pending.dirty {
                    Some(existing) => existing.merge(edit_dirty),
                    None => edit_dirty,
                });
            }
            let changed = target
                .document
                .apply_bitmap_edits_to_active_layer(&edits)
                .is_some();
            AppliedPaint {
                changed,
                dirty: edit_dirty,
                koma_id: Some(target.koma_id),
                patch: None,
            }
        } else {
            // 即時操作 (FloodFill / LassoFill): その場で BitmapPatch を確定する。
            let before = edit_dirty.and_then(|dirty| {
                target
                    .document
                    .capture_koma_layer_region(target.koma_id, target.layer_index, dirty)
            });
            let changed = target
                .document
                .apply_bitmap_edits_to_active_layer(&edits)
                .is_some();
            let patch = match (edit_dirty, before) {
                (Some(dirty), Some(before)) => target
                    .document
                    .capture_koma_layer_region(target.koma_id, target.layer_index, dirty)
                    .map(|after| {
                        PaintPatch::Cpu(BitmapPatch {
                            koma_id: target.koma_id,
                            layer_index: target.layer_index,
                            dirty,
                            before,
                            after,
                        })
                    }),
                _ => None,
            };
            AppliedPaint {
                changed,
                dirty: edit_dirty,
                koma_id: Some(target.koma_id),
                patch,
            }
        }
    }

    fn begin_stroke(&mut self, target: &PaintTarget<'_>) {
        if self.pending.is_some() {
            return;
        }
        if let Some(before_layer) = target
            .document
            .clone_koma_layer_bitmap(target.koma_id, target.layer_index)
        {
            self.pending = Some(CpuPendingStroke {
                before_layer,
                dirty: None,
            });
        }
    }

    fn commit_stroke(&mut self, target: &PaintTarget<'_>) -> Option<PaintPatch> {
        let pending = self.pending.take()?;
        let dirty = pending.dirty?;
        let before = pending
            .before_layer
            .extract_region(dirty.x, dirty.y, dirty.width, dirty.height)?;
        let after =
            target
                .document
                .capture_koma_layer_region(target.koma_id, target.layer_index, dirty)?;
        Some(PaintPatch::Cpu(BitmapPatch {
            koma_id: target.koma_id,
            layer_index: target.layer_index,
            dirty,
            before,
            after,
        }))
    }
}
