//! GPU バックエンド (BL-131 / BL-133 準備)。
//!
//! `paint-engine` が生成した [`PaintPlan`] を `gpu-paint` の `BrushPipeline` /
//! `FillPipeline` へ機械的に変換し dispatch する。編集中に CPU 画素を一切生成しない:
//! ストロークは `plan.stamps` をそのまま GPU へ渡し、flood fill は GPU shader 側の
//! ピンポンマスクで連結成分を解決する (CPU の visited 配列を持たない)。
//!
//! 履歴は GPU テクスチャ前後スナップショット方式 (`GpuRegionPatch`)。`before` は
//! ストローク前の CPU bitmap (Paint Runtime は GPU 経路でも CPU bitmap を変更しない)
//! を 1 回 GPU へアップロードし、`after` は GPU-to-GPU コピーで取得する。

use editor_state::ColorRgba8;
use geometry::{KomaLocalPoint, MergeInSpace, PageDirtyRect};
use paint_engine::{PaintInput, PaintOp, PaintPlan};

use super::super::{GpuRegionPatch, PaintPatch};
use super::{AppliedPaint, PaintBackend, PaintTarget};

/// ストローク中の GPU 差分追跡状態。
struct GpuPendingStroke {
    /// ストローク中に蓄積したコマローカル dirty rect の合計。
    dirty: Option<PageDirtyRect>,
}

/// GPU バックエンド。
pub(crate) struct GpuPaintBackend {
    pending: Option<GpuPendingStroke>,
}

impl GpuPaintBackend {
    pub(crate) fn new() -> Self {
        Self { pending: None }
    }
}

/// `ColorRgba8` を GPU シェーダー用の正規化 RGBA float 配列へ変換する。
fn color_to_rgba_f32(color: ColorRgba8) -> [f32; 4] {
    [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    ]
}

/// GPU テクスチャ前後スナップショットから `GpuRegionPatch` を構築する。
///
/// `before` はストローク/塗りつぶし前の CPU bitmap 領域を 1 回 GPU へアップロードした
/// テクスチャ、`after` は dispatch 後のレイヤーテクスチャの GPU-to-GPU コピー。
fn build_region_patch(
    target: &PaintTarget<'_>,
    dirty: PageDirtyRect,
    before_pixels: &raster::RgbaBitmap,
) -> Option<PaintPatch> {
    let gpu = target.gpu.as_ref()?;
    let koma_key = gpu_paint::KomaTextureId(target.koma_id.0);
    let after = gpu
        .pool
        .snapshot_region(koma_key, target.layer_index, dirty)?;
    let before = gpu.pool.create_snapshot_texture(
        dirty.width as u32,
        dirty.height as u32,
        &before_pixels.pixels,
    );
    Some(PaintPatch::Gpu(GpuRegionPatch {
        koma_id: target.koma_id,
        layer_index: target.layer_index,
        dirty,
        before,
        after,
    }))
}

impl PaintBackend for GpuPaintBackend {
    fn apply(
        &mut self,
        plan: &PaintPlan,
        _input: &PaintInput,
        target: &mut PaintTarget<'_>,
        encoder: Option<&mut wgpu::CommandEncoder>,
    ) -> AppliedPaint {
        let Some(gpu) = target.gpu.as_ref() else {
            return AppliedPaint::default();
        };
        let koma_key = gpu_paint::KomaTextureId(target.koma_id.0);

        match &plan.op {
            PaintOp::Stroke {
                stamps,
                radius,
                color,
                mode,
            } => {
                let Some(encoder) = encoder else {
                    return AppliedPaint::default();
                };
                let Some(texture) = gpu.pool.get(koma_key, target.layer_index) else {
                    return AppliedPaint::default();
                };
                // ペン不透明度/アンチエイリアスは現コンテキストから取得する。
                let (opacity, antialias) = target
                    .document
                    .session
                    .active_pen_preset()
                    .map(|pen| (pen.opacity, pen.antialias))
                    .unwrap_or((1.0, true));
                let params = gpu_paint::BrushStrokeParams {
                    color_rgba: color_to_rgba_f32(*color),
                    radius: *radius,
                    opacity,
                    antialias,
                    mode: *mode,
                };
                // brush pass を呼び出し側 encoder に積む (submit しない、BL-133)。
                // ストローク区間の composite と 1 submit にまとめる。
                gpu.brush.dispatch_stroke(encoder, texture, stamps, &params);

                // ストローク中の dirty を蓄積する (commit_stroke でスナップショット)。
                if let Some(pending) = self.pending.as_mut() {
                    pending.dirty = Some(match pending.dirty {
                        Some(existing) => existing.merge(plan.dirty),
                        None => plan.dirty,
                    });
                }

                AppliedPaint {
                    changed: true,
                    dirty: Some(plan.dirty),
                    koma_id: Some(target.koma_id),
                    patch: None,
                }
            }
            PaintOp::FloodFill { seed, color, .. } => {
                // flood fill は ping-pong 反復で内部 submit を持つ即時操作。
                // flood fill の dirty はレイヤー全域 (plan.dirty が保守的境界)。
                self.apply_fill(target, koma_key, plan.dirty, |gpu, source, dst| {
                    gpu.fill.dispatch_flood_fill(
                        source,
                        dst,
                        *seed,
                        color_to_rgba_f32(*color),
                    );
                })
            }
            PaintOp::LassoFill {
                polygon, color, ..
            } => {
                if polygon.len() < 3 {
                    return AppliedPaint::default();
                }
                let poly: Vec<(f32, f32)> =
                    polygon.iter().map(|p| (p.x as f32, p.y as f32)).collect();
                let aabb = lasso_aabb(polygon);
                // lasso は即時操作。mark + apply pass を専用 encoder にまとめ 1 submit
                // する (BL-133)。dirty は polygon AABB (plan.dirty と同一)。
                self.apply_fill(target, koma_key, plan.dirty, move |gpu, _source, dst| {
                    let mut enc = gpu.pool.create_paint_encoder("lasso-fill-encoder");
                    gpu.fill.dispatch_lasso_fill(
                        &mut enc,
                        dst,
                        &poly,
                        aabb,
                        color_to_rgba_f32(*color),
                    );
                    gpu.pool.submit(enc);
                })
            }
        }
    }

    fn begin_stroke(&mut self, _target: &PaintTarget<'_>) {
        if self.pending.is_none() {
            self.pending = Some(GpuPendingStroke { dirty: None });
        }
    }

    fn commit_stroke(&mut self, target: &PaintTarget<'_>) -> Option<PaintPatch> {
        let pending = self.pending.take()?;
        let dirty = pending.dirty?;
        // before: ストローク前の CPU bitmap (GPU 経路は CPU bitmap を変更しない)。
        let before_pixels =
            target
                .document
                .capture_koma_layer_region(target.koma_id, target.layer_index, dirty)?;
        build_region_patch(target, dirty, &before_pixels)
    }
}

impl GpuPaintBackend {
    /// FloodFill / LassoFill 共通の即時適用 + GpuRegionPatch 確定。
    ///
    /// `dispatch` は塗りつぶし dispatch (source/target テクスチャを受ける) を行う。
    fn apply_fill(
        &mut self,
        target: &mut PaintTarget<'_>,
        koma_key: gpu_paint::KomaTextureId,
        dirty: PageDirtyRect,
        dispatch: impl FnOnce(
            &super::GpuPaintResources<'_>,
            &gpu_paint::GpuRgbaTexture,
            &gpu_paint::GpuRgbaTexture,
        ),
    ) -> AppliedPaint {
        // before スナップショット (塗りつぶし前の CPU bitmap の dirty 領域)。
        let before =
            target
                .document
                .capture_koma_layer_region(target.koma_id, target.layer_index, dirty);

        let Some(gpu) = target.gpu.as_ref() else {
            return AppliedPaint::default();
        };
        let Some(dst) = gpu.pool.get(koma_key, target.layer_index) else {
            return AppliedPaint::default();
        };
        // source は composite があればそれ、無ければ active layer 自身。
        let source: &gpu_paint::GpuRgbaTexture =
            gpu.pool.get_composite(koma_key).unwrap_or(dst);
        dispatch(gpu, source, dst);

        // after スナップショットを撮り GpuRegionPatch を作る。
        let patch = before.and_then(|before| build_region_patch(target, dirty, &before));
        AppliedPaint {
            changed: true,
            dirty: Some(dirty),
            koma_id: Some(target.koma_id),
            patch,
        }
    }
}

/// ポリゴンの包括 AABB を半開矩形 `PageDirtyRect` として返す。
fn lasso_aabb(polygon: &[KomaLocalPoint]) -> PageDirtyRect {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for p in polygon {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    if x0 == usize::MAX {
        return PageDirtyRect::new(0, 0, 0, 0);
    }
    PageDirtyRect::from_inclusive_points(x0, y0, x1, y1)
}
