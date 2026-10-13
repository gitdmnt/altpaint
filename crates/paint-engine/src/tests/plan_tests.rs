//! `plan_paint` の計画生成テスト (BL-130)。
//!
//! 計画は純データ (画素なし)。stamp 列・dirty rect・mode・color・target_layer の
//! 期待値を検証する。CPU ops の `compute_paint_edits` と同じ dirty rect になることを
//! 確認し、後続チャンクの backend 等価テストの基準とする。

use crate::painting::PaintInput;
use crate::plan::{PaintOp, plan_paint};
use document_model::Document;
use editor_state::{ColorRgba8, SessionCommand, StrokeMode, ToolKind};
use geometry::KomaLocalPoint;

use crate::PaintEngine;

/// Stamp 入力は単一スタンプの Stroke 計画になり、dirty rect は CPU 経路と一致する。
#[test]
fn stamp_plans_single_stroke_with_matching_dirty() {
    let mut document = Document::default();
    document.session.set_active_pen_size(10);
    document
        .session
        .set_active_color(ColorRgba8::new(0x12, 0x34, 0x56, 0xff));
    let engine = PaintEngine::new();

    let input = PaintInput::Stamp {
        at: KomaLocalPoint::new(64, 64),
        pressure: 1.0,
    };

    let plan = plan_paint(&document, &input).expect("plan");
    let edits = engine
        .compute_paint_edits(&document, &input)
        .expect("edits");

    // dirty rect は CPU 経路 (compute_paint_edits) と完全一致する。
    assert_eq!(plan.dirty, edits[0].dirty_rect);

    match plan.op {
        PaintOp::Stroke {
            stamps,
            radius,
            color,
            mode,
        } => {
            assert_eq!(stamps, vec![KomaLocalPoint::new(64, 64)]);
            // radius = 実効サイズ / 2。pressure=1.0, size=10 → resolved_size=10 → radius=5.0
            assert_eq!(radius, 5.0);
            assert_eq!(color, ColorRgba8::new(0x12, 0x34, 0x56, 0xff));
            assert_eq!(mode, StrokeMode::Paint);
        }
        other => panic!("expected Stroke, got {other:?}"),
    }
}

/// StrokeSegment は複数スタンプ列の Stroke 計画になり、stamps は
/// `compute_stamp_positions` と一致し、dirty rect は CPU 経路と一致する。
#[test]
fn stroke_segment_plans_stamp_positions_with_matching_dirty() {
    let document = Document::default();
    let engine = PaintEngine::new();

    let from = KomaLocalPoint::new(32, 32);
    let to = KomaLocalPoint::new(64, 32);
    let input = PaintInput::StrokeSegment {
        from,
        to,
        pressure: 1.0,
    };

    let plan = plan_paint(&document, &input).expect("plan");
    let edits = engine
        .compute_paint_edits(&document, &input)
        .expect("edits");
    assert_eq!(plan.dirty, edits[0].dirty_rect);

    let expected_positions = {
        let resolved = crate::build_paint_context(&document, &input).expect("context");
        crate::ops::compute_stamp_positions(from, to, &resolved.context)
    };

    match plan.op {
        PaintOp::Stroke { stamps, mode, .. } => {
            assert!(stamps.len() > 1, "segment should span multiple stamps");
            assert_eq!(stamps, expected_positions);
            assert_eq!(mode, StrokeMode::Paint);
        }
        other => panic!("expected Stroke, got {other:?}"),
    }
}

/// 消しゴムツールの Stroke 計画は `StrokeMode::Erase` になる。
#[test]
fn eraser_stroke_plan_uses_erase_mode() {
    let mut document = Document::default();
    document.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Eraser,
    });

    let input = PaintInput::Stamp {
        at: KomaLocalPoint::new(48, 48),
        pressure: 1.0,
    };
    let plan = plan_paint(&document, &input).expect("plan");
    match plan.op {
        PaintOp::Stroke { mode, .. } => assert_eq!(mode, StrokeMode::Erase),
        other => panic!("expected Stroke, got {other:?}"),
    }
}

/// FloodFill 計画は seed・color・target_layer を保持し、dirty は
/// アクティブレイヤー全域 (保守的境界)。visited 走査は計画段階で行わない。
#[test]
fn flood_fill_plan_carries_seed_and_full_bounds() {
    let mut document = Document::default();
    document
        .session
        .set_active_color(ColorRgba8::new(0xff, 0x00, 0x00, 0xff));

    let seed = KomaLocalPoint::new(8, 8);
    let input = PaintInput::FloodFill { at: seed };
    let plan = plan_paint(&document, &input).expect("plan");

    let bitmap = document.active_bitmap().expect("active bitmap");
    let active_layer = document.active_koma().unwrap().active_layer_index;

    assert_eq!(
        plan.dirty,
        geometry::PageDirtyRect::new(0, 0, bitmap.width, bitmap.height),
        "flood fill の計画段階 dirty はレイヤー全域 (保守的)"
    );
    match plan.op {
        PaintOp::FloodFill {
            seed: plan_seed,
            color,
            target_layer,
        } => {
            assert_eq!(plan_seed, seed);
            assert_eq!(color, ColorRgba8::new(0xff, 0x00, 0x00, 0xff));
            assert_eq!(target_layer, active_layer);
        }
        other => panic!("expected FloodFill, got {other:?}"),
    }
}

/// LassoFill 計画は polygon を保持し、dirty は polygon の AABB (走査不要で確定)。
#[test]
fn lasso_fill_plan_carries_polygon_and_aabb() {
    let mut document = Document::default();
    document
        .session
        .set_active_color(ColorRgba8::new(0x00, 0x00, 0xff, 0xff));

    let polygon = vec![
        KomaLocalPoint::new(10, 10),
        KomaLocalPoint::new(30, 10),
        KomaLocalPoint::new(30, 30),
        KomaLocalPoint::new(10, 30),
    ];
    let input = PaintInput::LassoFill {
        points: polygon.clone(),
    };
    let plan = plan_paint(&document, &input).expect("plan");
    let active_layer = document.active_koma().unwrap().active_layer_index;

    // AABB = (10,10) 〜 (30,30) inclusive → x=10,y=10,w=21,h=21
    assert_eq!(plan.dirty, geometry::PageDirtyRect::new(10, 10, 21, 21));
    match plan.op {
        PaintOp::LassoFill {
            polygon: plan_polygon,
            color,
            target_layer,
        } => {
            assert_eq!(plan_polygon, polygon);
            assert_eq!(color, ColorRgba8::new(0x00, 0x00, 0xff, 0xff));
            assert_eq!(target_layer, active_layer);
        }
        other => panic!("expected LassoFill, got {other:?}"),
    }
}

/// アクティブコマ外の入力は計画を生成しない (context 解決失敗)。
#[test]
fn out_of_bounds_input_yields_no_plan() {
    let document = Document::default();
    let input = PaintInput::Stamp {
        at: KomaLocalPoint::new(100_000, 100_000),
        pressure: 1.0,
    };
    assert!(plan_paint(&document, &input).is_none());
}
