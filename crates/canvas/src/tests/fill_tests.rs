use app_core::{ColorRgba8, Document, PaintInput, PanelLocalPoint};

use crate::CanvasRuntime;

use super::apply_input;

/// flood 塗りつぶし recolors matching 領域 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn flood_fill_recolors_matching_region() {
    let mut document = Document {
        active_color: ColorRgba8::new(0xff, 0x00, 0x00, 0xff),
        ..Document::default()
    };
    let runtime = CanvasRuntime::default();

    let dirty = apply_input(
        &mut document,
        &runtime,
        PaintInput::FloodFill {
            at: PanelLocalPoint::new(8, 8),
        },
    )
    .expect("dirty rect");

    assert!(dirty.width > 0);
    let bitmap = document.active_bitmap().expect("active bitmap");
    assert_eq!(bitmap.pixel_rgba(8, 8), Some([0xff, 0x00, 0x00, 0xff]));
}

/// 斜め辺を持つ三角形で lasso fill が正確に境界を判定することを検証する。
///
/// 修正前の `.abs()` バグでは upward-going な斜め辺の交点計算が誤るため、
/// 三角形内部のピクセルが外部扱いになっていた。
#[test]
fn lasso_fill_triangular_region_diagonal_edges() {
    let mut document = Document {
        active_color: ColorRgba8::new(0xff, 0x00, 0x00, 0xff),
        ..Document::default()
    };
    let runtime = CanvasRuntime::default();

    // 三角形: (0,0), (20,0), (10,20) — 斜め辺を含む
    let dirty = apply_input(
        &mut document,
        &runtime,
        PaintInput::LassoFill {
            points: vec![
                PanelLocalPoint::new(0, 0),
                PanelLocalPoint::new(20, 0),
                PanelLocalPoint::new(10, 20),
            ],
        },
    )
    .expect("dirty rect");

    assert!(dirty.width > 0);
    let bitmap = document.active_bitmap().expect("active bitmap");
    // 三角形の中心付近のピクセルは赤く塗られているはず
    assert_eq!(
        bitmap.pixel_rgba(10, 8),
        Some([0xff, 0x00, 0x00, 0xff]),
        "center of triangle should be filled"
    );
}

/// 投げ縄 塗りつぶし colors polygon area が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn lasso_fill_colors_polygon_area() {
    let mut document = Document {
        active_color: ColorRgba8::new(0x00, 0x00, 0xff, 0xff),
        ..Document::default()
    };
    let runtime = CanvasRuntime::default();

    let dirty = apply_input(
        &mut document,
        &runtime,
        PaintInput::LassoFill {
            points: vec![
                PanelLocalPoint::new(10, 10),
                PanelLocalPoint::new(30, 10),
                PanelLocalPoint::new(30, 30),
                PanelLocalPoint::new(10, 30),
            ],
        },
    )
    .expect("dirty rect");

    assert!(dirty.height > 0);
    let bitmap = document.active_bitmap().expect("active bitmap");
    assert_eq!(bitmap.pixel_rgba(20, 20), Some([0x00, 0x00, 0xff, 0xff]));
}
