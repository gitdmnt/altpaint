//! ペイント入力の適用・ストローク確定 (D9 / BL-131)。
//!
//! `plan_paint` で計画 (`PaintPlan`) を生成し、`PaintBackend` (Cpu/Gpu) が適用する。
//! GPU 有効時は `GpuPaintBackend` が compute shader で GPU テクスチャへ直接描画し、
//! CPU 画素を一切作らない。GPU 不在時は `CpuPaintBackend` がアクティブレイヤーの
//! ビットマップへ書き込む。dirty 蓄積・再合成・履歴積みの配線のみがここに残る。

use paint_engine::{PaintInput, plan_paint};

use super::PaintPatch;
use super::backend::{AppliedPaint, GpuPaintResources, PaintBackend, PaintTarget};
use crate::app::DesktopApp;

impl DesktopApp {
    /// 描画入力を計画し、選択したバックエンドで適用して操作を履歴へ積む。
    ///
    /// Stamp/StrokeSegment はストローク単位でバッチし `commit_stroke_to_history` で確定する。
    /// FloodFill/LassoFill は即座に `PaintPatch` として確定する。
    pub(crate) fn apply_paint_input(&mut self, input: PaintInput) -> bool {
        let Some(plan) = plan_paint(&self.document, &input) else {
            return false;
        };
        let Some((koma_id, layer_index)) = self
            .document
            .active_koma()
            .map(|koma| (koma.id, koma.active_layer_index))
        else {
            return false;
        };

        let is_stroke = matches!(
            input,
            PaintInput::Stamp { .. } | PaintInput::StrokeSegment { .. }
        );
        let use_gpu = self.gpu.is_some();

        // バックエンド適用は disjoint なフィールド借用 (document / paint / gpu) を
        // 同時に握るため、self のメソッドを呼ばずローカル参照で分割する。結果
        // (AppliedPaint) を取り出してから dirty/再合成の配線を行う。
        let applied: AppliedPaint = {
            let document = &mut self.document;
            let gpu_resources = self.gpu.as_ref().map(|gpu| GpuPaintResources {
                pool: &gpu.pool,
                brush: &gpu.brush,
                fill: &gpu.fill,
            });
            let mut target = PaintTarget {
                document,
                koma_id,
                layer_index,
                gpu: gpu_resources,
            };

            if use_gpu {
                let backend = &mut self.paint.gpu_backend;
                if is_stroke {
                    backend.begin_stroke(&target);
                }
                backend.apply(&plan, &input, &mut target)
            } else {
                let backend = &mut self.paint.cpu_backend;
                if is_stroke {
                    backend.begin_stroke(&target);
                }
                backend.apply(&plan, &input, &mut target)
            }
        };

        if !applied.changed {
            return false;
        }

        // 即時操作 (FloodFill / LassoFill) で確定した patch を履歴へ積む。
        if let Some(patch) = applied.patch {
            self.paint.history.push(patch);
        }

        if let Some(dirty) = applied.dirty {
            self.append_canvas_dirty_rect(dirty);
            if let Some(koma_id) = applied.koma_id {
                // GPU 経路では composite テクスチャを再合成する。CPU 経路では
                // recomposite_koma は GPU 不在で no-op となり、CpuCanvasSnapshot 側が
                // 表示を担う (BL-136)。
                self.recomposite_koma(koma_id, Some(dirty));
            }
        }
        true
    }

    /// ストロークを確定して履歴へ積む。ポインタ Up 後に呼び出す。
    pub(crate) fn commit_stroke_to_history(&mut self) {
        let patch: Option<PaintPatch> = {
            let document = &mut self.document;
            let gpu_resources = self.gpu.as_ref().map(|gpu| GpuPaintResources {
                pool: &gpu.pool,
                brush: &gpu.brush,
                fill: &gpu.fill,
            });
            // commit に必要な koma/layer は backend 内 pending には含まれないため、
            // 現在のアクティブコマから取り直す (ストローク中はコマ切替が起きない)。
            let Some((koma_id, layer_index)) = document
                .active_koma()
                .map(|koma| (koma.id, koma.active_layer_index))
            else {
                return;
            };
            let target = PaintTarget {
                document,
                koma_id,
                layer_index,
                gpu: gpu_resources,
            };
            if self.gpu.is_some() {
                self.paint.gpu_backend.commit_stroke(&target)
            } else {
                self.paint.cpu_backend.commit_stroke(&target)
            }
        };

        if let Some(patch) = patch {
            self.paint.history.push(patch);
        }
        self.sync_ui_from_document();
    }
}

#[cfg(test)]
mod tests {
    use document_model::Document;
    use geometry::KomaLocalPoint;
    use paint_engine::{PaintInput, plan_paint};

    /// BL-030 回帰: 同一 PaintInput に対し、CPU 経路のスタンプ半径
    /// (dirty rect 幅 / 2) と PaintPlan の `Stroke.radius` が一致する。
    /// 筆圧カーブが片側で二重適用されると線幅が乖離する (GPU 実行は不要)。
    #[test]
    fn cpu_stamp_radius_matches_plan_radius() {
        use paint_engine::{PaintEngine, PaintOp};

        let mut document = Document::default();
        document.session.set_active_pen_size(10);
        let engine = PaintEngine::new();

        for pressure in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let input = PaintInput::Stamp {
                at: KomaLocalPoint::new(64, 64),
                pressure,
            };
            let plan = plan_paint(&document, &input).expect("plan");
            let PaintOp::Stroke { radius, .. } = plan.op else {
                panic!("expected Stroke plan");
            };
            let edits = engine
                .compute_paint_edits(&document, &input)
                .expect("edits");
            assert!(!edits.is_empty());
            let cpu_radius = edits[0].dirty_rect.width as f32 * 0.5;
            assert_eq!(
                cpu_radius, radius,
                "pressure={pressure}: CPU スタンプ半径と PaintPlan.radius が乖離"
            );
        }
    }
}
