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
