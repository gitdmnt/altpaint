use app_core::{Command, Document, PaintInput, PanelLocalPoint, ToolKind};

use crate::CanvasRuntime;

use super::apply_input;

/// ストローク segment paints multiple pixels が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn stroke_segment_paints_multiple_pixels() {
    let mut document = Document::default();
    let runtime = CanvasRuntime::default();

    let dirty = apply_input(
        &mut document,
        &runtime,
        PaintInput::StrokeSegment {
            from: PanelLocalPoint::new(32, 32),
            to: PanelLocalPoint::new(64, 32),
            pressure: 1.0,
        },
    )
    .expect("dirty rect");

    assert!(dirty.width >= 16);
    let bitmap = document.active_bitmap().expect("active bitmap");
    assert!(
        bitmap
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 0, 255])
    );
}

/// 消しゴム uses runtime composite to clear pixels が期待どおりに動作することを検証する。
#[test]
fn eraser_uses_runtime_composite_to_clear_pixels() {
    let mut document = Document::default();
    let runtime = CanvasRuntime::default();

    let _ = apply_input(
        &mut document,
        &runtime,
        PaintInput::Stamp {
            at: PanelLocalPoint::new(48, 48),
            pressure: 1.0,
        },
    );
    let _ = document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Eraser,
    });
    let _ = apply_input(
        &mut document,
        &runtime,
        PaintInput::Stamp {
            at: PanelLocalPoint::new(48, 48),
            pressure: 1.0,
        },
    );

    let bitmap = document.active_bitmap().expect("active bitmap");
    let center = bitmap.pixel_rgba(48, 48).expect("pixel");
    assert_eq!(center, [255, 255, 255, 255]);
}
