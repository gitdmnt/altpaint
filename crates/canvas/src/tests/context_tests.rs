use app_core::{Document, PaintInput, PanelLocalPoint};

use crate::{build_paint_context, resolved_size_for_input};

#[test]
fn context_builder_resolves_active_tool_and_layer_metadata() {
    let document = Document::default();
    let input = PaintInput::Stamp {
        at: PanelLocalPoint::new(32, 32),
        pressure: 1.0,
    };

    let resolved = build_paint_context(&document, &input).expect("paint context");

    assert_eq!(resolved.plugin_id, "builtin.bitmap");
    assert_eq!(resolved.context.tool_id, "builtin.pen");
    assert_eq!(resolved.context.active_layer_index, 0);
    assert_eq!(resolved.context.layer_count, 1);
}

#[test]
fn context_builder_rejects_points_outside_active_panel() {
    let document = Document::default();
    let input = PaintInput::Stamp {
        at: PanelLocalPoint::new(10_000, 10_000),
        pressure: 1.0,
    };

    assert!(build_paint_context(&document, &input).is_none());
}

#[test]
fn resolved_size_uses_pressure_for_stamp_inputs() {
    let document = Document::default();
    let full = resolved_size_for_input(
        &document,
        &PaintInput::Stamp {
            at: PanelLocalPoint::new(16, 16),
            pressure: 1.0,
        },
    );
    let light = resolved_size_for_input(
        &document,
        &PaintInput::Stamp {
            at: PanelLocalPoint::new(16, 16),
            pressure: 0.2,
        },
    );

    assert!(full >= light);
}
