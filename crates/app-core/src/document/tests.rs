use super::*;

/// 最小ドキュメント構造がフェーズ0の前提を満たすことを確認する。
#[test]
fn default_document_has_single_page_single_panel_single_layer() {
    let document = Document::default();

    assert_eq!(document.work.title, "Untitled");
    assert_eq!(document.work.pages.len(), 1);
    assert_eq!(document.work.pages[0].panels.len(), 1);
    assert_eq!(document.work.pages[0].panels[0].root_layer.name, "Layer 1");
    assert_eq!(document.work.pages[0].panels[0].bitmap.width, 64);
    assert_eq!(document.work.pages[0].panels[0].bitmap.height, 64);
}

/// 点描画が対象ピクセルを黒に変えることを確認する。
#[test]
fn draw_point_marks_target_pixel_black() {
    let mut document = Document::default();

    let dirty = document.draw_point(3, 4).expect("panel should exist");

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(dirty, DirtyRect::from_inclusive_points(3, 4, 3, 4));
}

/// ストローク描画が始点と終点の間を連続的に塗ることを確認する。
#[test]
fn draw_stroke_draws_continuous_line() {
    let mut document = Document::default();

    let dirty = document.draw_stroke(2, 2, 6, 2).expect("panel should exist");

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    for x in 2..=6 {
        let index = (2 * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
    assert_eq!(dirty, DirtyRect::from_inclusive_points(2, 2, 6, 2));
}

#[test]
fn erase_point_marks_target_pixel_white() {
    let mut document = Document::default();
    let _ = document.draw_point(3, 4);

    let dirty = document.erase_point(3, 4).expect("panel should exist");

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[255, 255, 255, 255]);
    assert_eq!(dirty, DirtyRect::from_inclusive_points(3, 4, 3, 4));
}

#[test]
fn active_tool_defaults_to_brush() {
    let document = Document::default();

    assert_eq!(document.active_tool, ToolKind::Brush);
}

#[test]
fn active_color_defaults_to_black() {
    let document = Document::default();

    assert_eq!(document.active_color, ColorRgba8::new(0, 0, 0, 255));
}

#[test]
fn default_document_has_round_pen_preset() {
    let document = Document::default();

    assert_eq!(document.pen_presets.len(), 1);
    assert_eq!(document.active_pen_preset_id, "builtin.round-pen");
    assert_eq!(document.active_pen_size, 4);
}

#[test]
fn draw_point_uses_active_color() {
    let mut document = Document::default();
    document.set_active_color(ColorRgba8::new(0xe5, 0x39, 0x35, 0xff));

    let _ = document.draw_point(3, 4);

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0xe5, 0x39, 0x35, 0xff]);
}

/// dirty矩形のunionが両方を含む最小矩形になることを確認する。
#[test]
fn dirty_rect_union_merges_bounds() {
    let left = DirtyRect::from_inclusive_points(2, 3, 4, 5);
    let right = DirtyRect::from_inclusive_points(6, 1, 7, 4);

    assert_eq!(
        left.union(right),
        DirtyRect {
            x: 2,
            y: 1,
            width: 6,
            height: 5,
        }
    );
}

/// 初期キャンバスが白背景で塗られていることを確認する。
#[test]
fn canvas_defaults_to_white_background() {
    let bitmap = CanvasBitmap::default();

    assert_eq!(&bitmap.pixels[0..4], &[255, 255, 255, 255]);
}

#[test]
fn apply_command_switches_active_tool() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SetActiveTool { tool: ToolKind::Pen });

    assert_eq!(dirty, None);
    assert_eq!(document.active_tool, ToolKind::Pen);
}

#[test]
fn apply_command_updates_pen_size() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SetActivePenSize { size: 12 });

    assert_eq!(dirty, None);
    assert_eq!(document.active_pen_size, 12);
}

#[test]
fn apply_command_switches_active_color() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SetActiveColor {
        color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
    });

    assert_eq!(dirty, None);
    assert_eq!(document.active_color, ColorRgba8::new(0x43, 0xa0, 0x47, 0xff));
}

#[test]
fn apply_command_draw_stroke_returns_dirty_rect() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::DrawStroke {
        from_x: 1,
        from_y: 1,
        to_x: 3,
        to_y: 1,
        pressure: 1.0,
    });

    assert_eq!(dirty, Some(DirtyRect::from_inclusive_points(1, 1, 3, 1)));
    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (bitmap.width + 2) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
}

#[test]
fn pen_draws_wider_than_single_pixel_brush() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActiveTool { tool: ToolKind::Pen });
    let _ = document.apply_command(&Command::SetActivePenSize { size: 5 });

    let dirty = document.draw_point(10, 10).expect("panel should exist");

    assert!(dirty.width >= 5);
    assert!(dirty.height >= 5);
    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let center = (10 * bitmap.width + 10) * 4;
    let edge = (10 * bitmap.width + 8) * 4;
    assert_eq!(&bitmap.pixels[center..center + 4], &[0, 0, 0, 255]);
    assert_eq!(&bitmap.pixels[edge..edge + 4], &[0, 0, 0, 255]);
}

#[test]
fn cycling_pen_presets_updates_active_size() {
    let mut document = Document::default();
    document.replace_pen_presets(vec![
        PenPreset {
            id: "fine".to_string(),
            name: "Fine".to_string(),
            size: 2,
            pressure_enabled: true,
            antialias: true,
            stabilization: 0,
        },
        PenPreset {
            id: "bold".to_string(),
            name: "Bold".to_string(),
            size: 9,
            pressure_enabled: true,
            antialias: true,
            stabilization: 0,
        },
    ]);

    document.select_next_pen_preset();

    assert_eq!(document.active_pen_preset_id, "bold");
    assert_eq!(document.active_pen_size, 9);
}

#[test]
fn document_new_uses_requested_canvas_size() {
    let document = Document::new(320, 240);

    let bitmap = document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (320, 240));
}

#[test]
fn apply_command_new_document_sized_replaces_bitmap_dimensions() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::NewDocumentSized { width: 512, height: 384 });

    assert_eq!(dirty, None);
    let bitmap = document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (512, 384));
}

#[test]
fn dirty_rect_clamps_to_bitmap_bounds() {
    let rect = DirtyRect {
        x: 60,
        y: 62,
        width: 10,
        height: 10,
    };

    assert_eq!(
        rect.clamp_to_bitmap(64, 64),
        DirtyRect {
            x: 60,
            y: 62,
            width: 4,
            height: 2,
        }
    );
}

#[test]
fn document_stores_canvas_view_transform() {
    let mut document = Document::default();
    let transform = CanvasViewTransform {
        zoom: 2.0,
        rotation_degrees: 12.5,
        pan_x: 18.0,
        pan_y: -6.0,
        flip_x: false,
        flip_y: false,
    };

    document.set_view_transform(transform);

    assert_eq!(document.view_transform, transform);
}

#[test]
fn add_raster_layer_selects_new_layer() {
    let mut document = Document::default();

    let _ = document.apply_command(&Command::AddRasterLayer);

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers.len(), 2);
    assert_eq!(panel.active_layer_index, 1);
    assert_eq!(panel.layers[1].name, "Layer 2");
}

#[test]
fn add_raster_layer_uses_created_layer_counter_for_names() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.apply_command(&Command::RemoveActiveLayer);

    let _ = document.apply_command(&Command::AddRasterLayer);

    let panel = &document.work.pages[0].panels[0];
    let names = panel.layers.iter().map(|layer| layer.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, vec!["Layer 1", "Layer 2", "Layer 4"]);
    assert_eq!(panel.created_layer_count, 4);
}

#[test]
fn remove_active_layer_keeps_at_least_one_layer() {
    let mut document = Document::default();

    let _ = document.apply_command(&Command::RemoveActiveLayer);

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers.len(), 1);
    assert_eq!(panel.active_layer_index, 0);
}

#[test]
fn remove_active_layer_selects_remaining_layer() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.apply_command(&Command::AddRasterLayer);

    let _ = document.apply_command(&Command::RemoveActiveLayer);

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers.len(), 2);
    assert_eq!(panel.active_layer_index, 1);
    assert_eq!(panel.layers[1].name, "Layer 2");
}

#[test]
fn move_layer_reorders_layers_and_tracks_active_selection() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.apply_command(&Command::AddRasterLayer);

    let _ = document.apply_command(&Command::MoveLayer {
        from_index: 2,
        to_index: 0,
    });

    let panel = &document.work.pages[0].panels[0];
    let names = panel.layers.iter().map(|layer| layer.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, vec!["Layer 3", "Layer 1", "Layer 2"]);
    assert_eq!(panel.active_layer_index, 0);
}

#[test]
fn rename_active_layer_updates_selected_layer_name() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);

    let _ = document.apply_command(&Command::RenameActiveLayer {
        name: "Ink".to_string(),
    });

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers[1].name, "Ink");
    assert_eq!(panel.root_layer.name, "Ink");
}

#[test]
fn set_active_layer_blend_mode_sets_requested_mode() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
        mode: BlendMode::Screen,
    });

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers[0].blend_mode, BlendMode::Screen);
}

#[test]
fn set_active_layer_blend_mode_accepts_custom_formula_string() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
        mode: BlendMode::parse_name("max(src, dst)").expect("custom mode"),
    });

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers[0].blend_mode.as_str(), "max(src, dst)");
}

#[test]
fn custom_blend_formula_is_applied_during_layer_composition() {
    let mut document = Document::default();
    let panel = &mut document.work.pages[0].panels[0];
    panel.layers[0].bitmap.draw_point_rgba(0, 0, [64, 64, 64, 255]);

    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.draw_point(0, 0);
    let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
        mode: BlendMode::parse_name("max(src, dst)").expect("custom mode"),
    });

    let pixel = &document.active_bitmap().expect("bitmap exists").pixels[0..4];
    assert_eq!(pixel, &[64, 64, 64, 255]);
}

#[test]
fn toggle_active_layer_visibility_reveals_underlying_layer() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.draw_point(5, 5);

    let visible_bitmap = document.active_bitmap().expect("bitmap exists").clone();
    let _ = document.apply_command(&Command::ToggleActiveLayerVisibility);
    let hidden_bitmap = document.active_bitmap().expect("bitmap exists");

    let index = (5 * visible_bitmap.width + 5) * 4;
    assert_eq!(&visible_bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(&hidden_bitmap.pixels[index..index + 4], &[255, 255, 255, 255]);
}

#[test]
fn toggle_active_layer_mask_applies_demo_mask() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.draw_point(1, 1);

    let before_mask = document.active_bitmap().expect("bitmap exists").clone();
    let _ = document.apply_command(&Command::ToggleActiveLayerMask);
    let after_mask = document.active_bitmap().expect("bitmap exists");

    let index = (before_mask.width + 1) * 4;
    assert_eq!(&before_mask.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(&after_mask.pixels[index..index + 4], &[255, 255, 255, 255]);
}
