use app_core::{ColorRgba8, Document, PaintInput};
use geometry::KomaLocalPoint;

use crate::PaintEngine;

use super::apply_input;

/// BL-030: 筆圧カーブは context 解決時 (`brush_size_for_pressure`) の 1 回だけ適用される。
///
/// stamp 側で再適用 (二重適用) があるとスタンプ径が
/// `round(round(base*f)*f)` に縮み、GPU 経路 (1 回適用) と線幅が乖離する。
#[test]
fn stamp_diameter_applies_pressure_curve_exactly_once() {
    let mut document = Document::default();
    document.set_active_pen_size(10);
    let engine = PaintEngine::default();

    // カーブ 1 回適用の期待値: round(10 * (0.2 + 0.5 * 0.8)) = 6
    let pressure = 0.5_f32;
    let expected = document.brush_size_for_pressure(pressure) as usize;
    assert_eq!(expected, 6);

    let input = PaintInput::Stamp {
        at: KomaLocalPoint::new(64, 64),
        pressure,
    };
    let edits = engine
        .compute_paint_edits(&document, &input)
        .expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(
        (edits[0].dirty_rect.width, edits[0].dirty_rect.height),
        (expected, expected),
        "スタンプ径はカーブ 1 回適用の実効サイズと一致する"
    );
}

#[test]
fn stamp_input_paints_selected_color() {
    let mut document = Document {
        active_color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
        ..Document::default()
    };
    let engine = PaintEngine::default();

    let dirty = apply_input(
        &mut document,
        &engine,
        PaintInput::Stamp {
            at: KomaLocalPoint::new(64, 64),
            pressure: 1.0,
        },
    )
    .expect("dirty rect");

    assert!(dirty.width > 0);
    let bitmap = document.active_bitmap().expect("active bitmap");
    assert!(
        bitmap
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0x43, 0xa0, 0x47, 0xff])
    );
}
