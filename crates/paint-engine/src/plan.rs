//! ペイント計画 (`PaintPlan`): 入力解釈の純データ表現 (BL-130)。
//!
//! `plan_paint` は `PaintInput` を `resolve_paint_context` で解決し、画素を一切作らずに
//! 「何をどこに描くか」だけを `PaintPlan` として返す。CPU/GPU バックエンドはこの計画を
//! 受け取り、それぞれの方式で適用する (適用は後続チャンクの `PaintBackend`)。
//!
//! 計画段階で `dirty: PageDirtyRect` を確定させる:
//! - Stroke: スタンプ列の境界 (CPU 経路 `stroke_like_edit` と同一)。
//! - LassoFill: polygon の AABB (走査不要)。
//! - FloodFill: アクティブレイヤー全域 (保守的境界)。visited 走査はバックエンドが行うため
//!   計画では領域を確定させない。

use document_model::Document;
use editor_state::{ColorRgba8, StrokeMode, ToolDescriptor};
use geometry::{KomaLocalPoint, PageDirtyRect};

use crate::build_paint_context;
use crate::ops;
use crate::painting::PaintInput;

/// ストロークのスタンプ座標 (コマローカル)。
pub type StampPoint = KomaLocalPoint;

/// ペイント計画の本体 (画素を含まない)。
#[derive(Debug, Clone, PartialEq)]
pub enum PaintOp {
    /// ブラシ/消しゴムのストローク。
    Stroke {
        stamps: Vec<StampPoint>,
        /// 実効サイズ / 2。筆圧カーブは context 解決時に 1 回適用済み (BL-030)。
        radius: f32,
        color: ColorRgba8,
        mode: StrokeMode,
    },
    /// 連結領域塗りつぶし。visited 走査はバックエンドで行う。
    FloodFill {
        seed: KomaLocalPoint,
        color: ColorRgba8,
        target_layer: usize,
    },
    /// 投げ縄領域塗りつぶし。
    LassoFill {
        polygon: Vec<KomaLocalPoint>,
        color: ColorRgba8,
        target_layer: usize,
    },
}

/// ペイント計画。操作 (`op`) と計画段階で確定した dirty rect を持つ。
#[derive(Debug, Clone, PartialEq)]
pub struct PaintPlan {
    pub op: PaintOp,
    pub dirty: PageDirtyRect,
}

/// `PaintInput` を解釈し `PaintPlan` を生成する。画素は作らない。
///
/// アクティブコマ外・コンテキスト解決失敗・空ストロークの場合は `None`。
pub fn plan_paint(document: &Document, input: &PaintInput) -> Option<PaintPlan> {
    let resolved = build_paint_context(document, input)?;
    let context = &resolved.context;
    let target_layer = context.active_layer_index;

    match input {
        PaintInput::Stamp { at, .. } => {
            let stamps = vec![*at];
            let (stamp_w, stamp_h) = ops::stamp_dimensions(context);
            let dirty = ops::stroke_dirty_rect(&stamps, stamp_w, stamp_h)?;
            Some(PaintPlan {
                op: PaintOp::Stroke {
                    stamps,
                    radius: context.resolved_size as f32 * 0.5,
                    color: context.color,
                    mode: ToolDescriptor::for_kind(context.tool).blend_mode(),
                },
                dirty,
            })
        }
        PaintInput::StrokeSegment { from, to, .. } => {
            let stamps = ops::compute_stamp_positions(*from, *to, context);
            let (stamp_w, stamp_h) = ops::stamp_dimensions(context);
            let dirty = ops::stroke_dirty_rect(&stamps, stamp_w, stamp_h)?;
            Some(PaintPlan {
                op: PaintOp::Stroke {
                    stamps,
                    radius: context.resolved_size as f32 * 0.5,
                    color: context.color,
                    mode: ToolDescriptor::for_kind(context.tool).blend_mode(),
                },
                dirty,
            })
        }
        PaintInput::FloodFill { at } => {
            // visited 走査は backend (GPU) で行うため、計画段階では領域を確定させない。
            // 保守的にアクティブレイヤー全域を dirty とする。
            let dirty = PageDirtyRect::new(
                0,
                0,
                context.composited_bitmap.width,
                context.composited_bitmap.height,
            );
            Some(PaintPlan {
                op: PaintOp::FloodFill {
                    seed: *at,
                    color: context.color,
                    target_layer,
                },
                dirty,
            })
        }
        PaintInput::LassoFill { points } => {
            if points.len() < 3 {
                return None;
            }
            let min_x = points.iter().map(|p| p.x).min()?;
            let min_y = points.iter().map(|p| p.y).min()?;
            let max_x = points.iter().map(|p| p.x).max()?;
            let max_y = points.iter().map(|p| p.y).max()?;
            let dirty = PageDirtyRect::from_inclusive_points(min_x, min_y, max_x, max_y);
            Some(PaintPlan {
                op: PaintOp::LassoFill {
                    polygon: points.clone(),
                    color: context.color,
                    target_layer,
                },
                dirty,
            })
        }
    }
}
