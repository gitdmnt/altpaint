use app_core::{ColorRgba8, Document, PaintInput, PanelLocalPoint};

use crate::CanvasRuntime;

use super::apply_input;

#[test]
fn stamp_input_paints_selected_color() {
    let mut document = Document {
        active_color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
        ..Document::default()
    };
    let runtime = CanvasRuntime::default();

    let dirty = apply_input(
        &mut document,
        &runtime,
        PaintInput::Stamp {
            at: PanelLocalPoint::new(64, 64),
            pressure: 1.0,
        },
    )
    .expect("dirty rect");

    assert!(dirty.width > 0);
    let bitmap = document.active_bitmap().expect("active bitmap");
    assert!(bitmap
        .pixels
        .chunks_exact(4)
        .any(|pixel| pixel == [0x43, 0xa0, 0x47, 0xff]));
}
