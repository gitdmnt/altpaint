use crate::painting::PaintInput;
use document_model::Document;
use raster::BitmapEdit;

use crate::build_paint_context;
use crate::ops::compute_bitmap_edits;

/// `Document` の読み取り状態から bitmap 差分を計算するペイントエンジンを表す。
///
/// R5 で `PaintPlugin` registry を撤去し、唯一の描画バックエンド
/// (`BUILTIN_BITMAP_BACKEND_ID`) の CPU op を直接呼ぶ。状態は持たない純計算機。
#[derive(Default)]
pub struct PaintEngine;

impl PaintEngine {
    pub fn new() -> Self {
        Self
    }

    /// 描画入力からレイヤーに適用するビットマップ差分を計算して返す (適用はしない)。
    ///
    /// コンテキスト解決に失敗した場合は `None` を返す。
    pub fn compute_paint_edits(
        &self,
        document: &Document,
        input: &PaintInput,
    ) -> Option<Vec<BitmapEdit>> {
        let resolved = build_paint_context(document, input)?;
        Some(compute_bitmap_edits(input, &resolved.context))
    }
}
