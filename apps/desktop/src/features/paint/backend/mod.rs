//! ペイントバックエンド境界 (BL-131 / R5)。
//!
//! `PaintBackend` trait は `paint-engine` が生成した [`PaintPlan`] を受け取り、
//! それぞれの方式 (CPU ビットマップ / GPU compute dispatch) で適用し、履歴用の
//! [`PaintPatch`] を生成する。CPU/GPU 二重実装 (旧 `execute_paint_input` 3 分岐 /
//! `execute_gpu_fill` / `commit_stroke_to_history`) を 1 つの境界へ閉じる。
//!
//! - [`CpuPaintBackend`]: `paint-engine` の CPU ops を呼ぶ参照実装。GPU 不在時の
//!   唯一の描画経路 (BL-136: GPU 必須化はしない)。
//! - [`GpuPaintBackend`]: `gpu-paint` の `BrushPipeline` / `FillPipeline` へ機械的に
//!   変換し dispatch する。編集中に CPU 画素を一切生成しない (flood fill の visited
//!   配列も持たない)。
//!
//! 同一 `PaintPlan` を両バックエンドへ適用すると同一結果になることを
//! ゴールデン等価テスト (`tests`) で担保する。

mod cpu;
mod gpu;
#[cfg(test)]
mod tests;

pub(crate) use cpu::CpuPaintBackend;
pub(crate) use gpu::GpuPaintBackend;

use document_model::{Document, KomaId};
use geometry::PageDirtyRect;
use paint_engine::{PaintInput, PaintPlan};

use super::PaintPatch;

/// ペイント適用の結果 (純データ)。
///
/// バックエンドは画素書き込み / GPU dispatch を内部で完結させ、呼び出し側
/// (`DesktopApp`) が dirty 蓄積・再合成・履歴積みに使う副作用情報を返す。
#[derive(Debug, Default)]
pub(crate) struct AppliedPaint {
    /// 文書または GPU テクスチャが変化したか。
    pub(crate) changed: bool,
    /// 変化したコマローカル dirty rect (なければ `None`)。
    pub(crate) dirty: Option<PageDirtyRect>,
    /// 変化したコマ。再合成対象。
    pub(crate) koma_id: Option<KomaId>,
    /// 即時操作 (FloodFill / LassoFill) で確定した履歴パッチ。
    ///
    /// ストローク (Stamp / StrokeSegment) はバッチされ `commit_stroke` で確定するため
    /// `apply` 時点では `None`。
    pub(crate) patch: Option<PaintPatch>,
}

/// GPU バックエンドが dispatch / スナップショットに使うリソース参照束。
///
/// `DesktopApp.gpu` の各フィールド (互いに素) を借用する。`document` (= `&mut`) とは
/// 別フィールドのため借用は両立する。
pub(crate) struct GpuPaintResources<'a> {
    pub(crate) pool: &'a gpu_paint::LayerTextureStore,
    pub(crate) brush: &'a gpu_paint::BrushPipeline,
    pub(crate) fill: &'a gpu_paint::FillPipeline,
}

/// バックエンドが適用対象とするドキュメント・コマ・レイヤーの参照束。
///
/// CPU バックエンドは `document` を可変参照して画素を書き込み、`gpu` を無視する。
/// GPU バックエンドは `document` を不変参照 (スナップショット用) し、画素は
/// `gpu` のテクスチャへ書く。
pub(crate) struct PaintTarget<'a> {
    pub(crate) document: &'a mut Document,
    pub(crate) koma_id: KomaId,
    pub(crate) layer_index: usize,
    /// GPU リソース (GPU バックエンド時のみ `Some`)。
    pub(crate) gpu: Option<GpuPaintResources<'a>>,
}

/// ペイント計画を適用し、履歴パッチを生成するバックエンド契約。
pub(crate) trait PaintBackend {
    /// ストローク以外 (FloodFill / LassoFill) の即時操作、または 1 ストローク
    /// セグメントを適用する。`input` は CPU 参照実装がコンテキスト (ペン先など) を
    /// 再解決するために使う。`plan` は GPU 経路が画素なしで dispatch するために使う。
    ///
    /// `encoder` は GPU バックエンドが compute pass を積む先 (BL-133)。GPU 経路では
    /// 呼び出し側が brush/fill + composite を 1 encoder にまとめ 1 submit する。
    /// CPU バックエンドは `encoder` を無視する (`None` でよい)。
    fn apply(
        &mut self,
        plan: &PaintPlan,
        input: &PaintInput,
        target: &mut PaintTarget<'_>,
        encoder: Option<&mut wgpu::CommandEncoder>,
    ) -> AppliedPaint;

    /// ストローク開始時に before スナップショットを準備する。
    fn begin_stroke(&mut self, target: &PaintTarget<'_>);

    /// ストロークを確定し履歴パッチを生成する。ポインタ Up 後に呼ぶ。
    ///
    /// ストローク中の編集が無ければ `None`。
    fn commit_stroke(&mut self, target: &PaintTarget<'_>) -> Option<PaintPatch>;
}
