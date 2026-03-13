use super::*;
use crate::{CanvasDirtyRect, ClampToCanvasBounds, MergeInSpace};

/// レイヤー ブラシ を更新し、必要な dirty 状態も記録する。
///
/// 必要に応じて dirty 状態も更新します。
fn apply_layer_brush(
    document: &mut Document,
    paint: impl FnOnce(&mut CanvasBitmap, bool) -> CanvasDirtyRect,
) -> Option<CanvasDirtyRect> {
    let panel_bounds = document.active_panel_bounds()?;
    let (page_width, page_height) = document.active_page_dimensions();
    let panel = document.active_panel_mut()?;
    super::layer_ops::ensure_panel_layers(panel);
    let is_background = panel.active_layer_index == 0;
    let local_dirty = {
        let layer = &mut panel.layers[panel.active_layer_index];
        paint(&mut layer.bitmap, is_background)
    };
    panel.bitmap = super::layer_ops::composite_panel_bitmap(panel);
    Some(
        CanvasDirtyRect {
            x: local_dirty.x.saturating_add(panel_bounds.x),
            y: local_dirty.y.saturating_add(panel_bounds.y),
            width: local_dirty.width,
            height: local_dirty.height,
        }
        .clamp_to_canvas_bounds(page_width.max(1), page_height.max(1)),
    )
}

/// 描画 点 に必要な差分領域だけを描画または合成する。
///
/// 値を生成できない場合は `None` を返します。
fn draw_point(document: &mut Document, x: usize, y: usize) -> Option<CanvasDirtyRect> {
    let color = document.active_color.to_rgba8();
    let size = document.resolved_paint_size_with_pressure(1.0);
    let antialias = document
        .active_pen_preset()
        .map(|preset| preset.antialias)
        .unwrap_or(true);
    apply_layer_brush(document, |bitmap, _| {
        bitmap.draw_point_sized_rgba(x, y, color, size, antialias)
    })
}

/// 描画 ストローク に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn draw_stroke(
    document: &mut Document,
    from_x: usize,
    from_y: usize,
    to_x: usize,
    to_y: usize,
) -> Option<CanvasDirtyRect> {
    let color = document.active_color.to_rgba8();
    let size = document.resolved_paint_size_with_pressure(1.0);
    let antialias = document
        .active_pen_preset()
        .map(|preset| preset.antialias)
        .unwrap_or(true);
    apply_layer_brush(document, |bitmap, _| {
        bitmap.draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size, antialias)
    })
}

/// Erase 点 に必要な差分領域だけを描画または合成する。
///
/// 値を生成できない場合は `None` を返します。
fn erase_point(document: &mut Document, x: usize, y: usize) -> Option<CanvasDirtyRect> {
    let size = document.resolved_paint_size_with_pressure(1.0);
    let antialias = document
        .active_pen_preset()
        .map(|preset| preset.antialias)
        .unwrap_or(true);
    apply_layer_brush(document, |bitmap, is_background| {
        if is_background {
            bitmap.erase_point_sized(x, y, size, antialias)
        } else {
            bitmap.draw_point_sized_rgba(x, y, [0, 0, 0, 0], size, antialias)
        }
    })
}

/// 既定 ドキュメント has single ページ single パネル single レイヤー が期待どおりに動作することを検証する。
#[test]
fn default_document_has_single_page_single_panel_single_layer() {
    let document = Document::default();

    assert_eq!(document.work.title, "Untitled");
    assert_eq!(document.work.pages.len(), 1);
    assert_eq!(document.work.pages[0].panels.len(), 1);
    assert_eq!(document.work.pages[0].panels[0].root_layer.name, "Layer 1");
    assert_eq!(
        document.work.pages[0].panels[0].bitmap.width,
        DEFAULT_DOCUMENT_WIDTH
    );
    assert_eq!(
        document.work.pages[0].panels[0].bitmap.height,
        DEFAULT_DOCUMENT_HEIGHT
    );
}

/// 描画 点 marks target ピクセル black が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn draw_point_marks_target_pixel_black() {
    let mut document = Document::default();
    document.set_active_pen_size(1);

    let dirty = draw_point(&mut document, 3, 4).expect("panel should exist");

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(dirty, CanvasDirtyRect::from_inclusive_points(3, 4, 3, 4));
}

/// 描画 ストローク draws continuous line が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn draw_stroke_draws_continuous_line() {
    let mut document = Document::default();
    document.set_active_pen_size(1);

    let dirty = draw_stroke(&mut document, 2, 2, 6, 2).expect("panel should exist");

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    for x in 2..=6 {
        let index = (2 * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
    assert_eq!(dirty, CanvasDirtyRect::from_inclusive_points(2, 2, 6, 2));
}

/// erase 点 marks target ピクセル white が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn erase_point_marks_target_pixel_white() {
    let mut document = Document::default();
    document.set_active_pen_size(1);
    let _ = draw_point(&mut document, 3, 4);

    let dirty = erase_point(&mut document, 3, 4).expect("panel should exist");

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[255, 255, 255, 255]);
    assert_eq!(dirty, CanvasDirtyRect::from_inclusive_points(3, 4, 3, 4));
}

/// アクティブ ツール defaults to ペン が期待どおりに動作することを検証する。
#[test]
fn active_tool_defaults_to_pen() {
    let document = Document::default();

    assert_eq!(document.active_tool, ToolKind::Pen);
}

/// アクティブ 色 defaults to black が期待どおりに動作することを検証する。
#[test]
fn active_color_defaults_to_black() {
    let document = Document::default();

    assert_eq!(document.active_color, ColorRgba8::new(0, 0, 0, 255));
}

/// 既定 ドキュメント has round ペン preset が期待どおりに動作することを検証する。
#[test]
fn default_document_has_round_pen_preset() {
    let document = Document::default();

    assert_eq!(document.pen_presets.len(), 1);
    assert_eq!(document.active_pen_preset_id, "builtin.round-pen");
    assert_eq!(document.active_pen_size, 4);
}

/// 描画 点 uses アクティブ 色 が期待どおりに動作することを検証する。
#[test]
fn draw_point_uses_active_color() {
    let mut document = Document::default();
    document.set_active_color(ColorRgba8::new(0xe5, 0x39, 0x35, 0xff));

    let _ = draw_point(&mut document, 3, 4);

    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0xe5, 0x39, 0x35, 0xff]);
}

/// 差分 矩形 union merges 範囲 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn dirty_rect_union_merges_bounds() {
    let left = CanvasDirtyRect::from_inclusive_points(2, 3, 4, 5);
    let right = CanvasDirtyRect::from_inclusive_points(6, 1, 7, 4);

    assert_eq!(
        left.merge(right),
        CanvasDirtyRect {
            x: 2,
            y: 1,
            width: 6,
            height: 5,
        }
    );
}

/// キャンバス defaults to white 背景 が期待どおりに動作することを検証する。
#[test]
fn canvas_defaults_to_white_background() {
    let bitmap = CanvasBitmap::default();

    assert_eq!(&bitmap.pixels[0..4], &[255, 255, 255, 255]);
}

/// 適用 コマンド switches アクティブ ツール が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn apply_command_switches_active_tool() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });

    assert_eq!(dirty, None);
    assert_eq!(document.active_tool, ToolKind::Pen);
}

/// 適用 コマンド selects registered ツール by ID が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn apply_command_selects_registered_tool_by_id() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SelectTool {
        tool_id: "builtin.eraser".to_string(),
    });

    assert_eq!(dirty, None);
    assert_eq!(document.active_tool, ToolKind::Eraser);
    assert_eq!(document.active_tool_id, "builtin.eraser");
}

/// アクティブ ツール definition uses registered ツール metadata が期待どおりに動作することを検証する。
#[test]
fn active_tool_definition_uses_registered_tool_metadata() {
    let mut document = Document::default();
    assert!(document.set_active_tool_by_id("builtin.eraser"));

    let tool = document
        .active_tool_definition()
        .expect("active tool definition");

    assert_eq!(tool.kind, ToolKind::Eraser);
    assert_eq!(tool.id, "builtin.eraser");
    assert_eq!(tool.provider_plugin_id, "plugins/default-erasers-plugin");
    assert_eq!(tool.drawing_plugin_id, "builtin.bitmap");
    assert!(tool.settings.iter().any(|setting| setting.key == "size"));
}

/// 適用 コマンド updates ペン サイズ が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn apply_command_updates_pen_size() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SetActivePenSize { size: 12 });

    assert_eq!(dirty, None);
    assert_eq!(document.active_pen_size, 12);
}

/// 適用 コマンド switches アクティブ 色 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn apply_command_switches_active_color() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::SetActiveColor {
        color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
    });

    assert_eq!(dirty, None);
    assert_eq!(
        document.active_color,
        ColorRgba8::new(0x43, 0xa0, 0x47, 0xff)
    );
}

/// ビットマップ 編集 style ストローク returns 差分 矩形 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn bitmap_edit_style_stroke_returns_dirty_rect() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActivePenSize { size: 1 });

    let dirty = draw_stroke(&mut document, 1, 1, 3, 1);

    assert_eq!(
        dirty,
        Some(CanvasDirtyRect::from_inclusive_points(1, 1, 3, 1))
    );
    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let index = (bitmap.width + 2) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
}

/// ペン draws wider than single ピクセル 既定 ストローク が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn pen_draws_wider_than_single_pixel_default_stroke() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });
    let _ = document.apply_command(&Command::SetActivePenSize { size: 5 });

    let dirty = draw_point(&mut document, 10, 10).expect("panel should exist");

    assert!(dirty.width >= 5);
    assert!(dirty.height >= 5);
    let bitmap = &document.work.pages[0].panels[0].bitmap;
    let center = (10 * bitmap.width + 10) * 4;
    let edge = (10 * bitmap.width + 8) * 4;
    assert_eq!(&bitmap.pixels[center..center + 4], &[0, 0, 0, 255]);
    assert_eq!(&bitmap.pixels[edge..edge + 4], &[0, 0, 0, 255]);
}

/// wide ストローク keeps segment core filled が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn wide_stroke_keeps_segment_core_filled() {
    let mut document = Document::new(128, 128);
    let _ = document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });
    let _ = document.apply_command(&Command::SetActivePenSize { size: 24 });

    let dirty = draw_stroke(&mut document, 20, 64, 108, 64).expect("panel should exist");

    assert!(dirty.width >= 88);
    assert!(dirty.height >= 24);
    let bitmap = document.active_bitmap().expect("bitmap exists");
    for x in [20usize, 44, 64, 84, 108] {
        let index = (64 * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
}

/// wide diagonal ストローク marks midpoint pixels が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn wide_diagonal_stroke_marks_midpoint_pixels() {
    let mut document = Document::new(128, 128);
    let _ = document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });
    let _ = document.apply_command(&Command::SetActivePenSize { size: 18 });

    let dirty = draw_stroke(&mut document, 16, 16, 112, 112).expect("panel should exist");

    assert!(dirty.width >= 96);
    assert!(dirty.height >= 96);
    let bitmap = document.active_bitmap().expect("bitmap exists");
    for (x, y) in [(16usize, 16usize), (64, 64), (112, 112)] {
        let index = (y * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
}

/// cycling ペン presets updates アクティブ サイズ が期待どおりに動作することを検証する。
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
            ..PenPreset::default()
        },
        PenPreset {
            id: "bold".to_string(),
            name: "Bold".to_string(),
            size: 9,
            pressure_enabled: true,
            antialias: true,
            stabilization: 0,
            ..PenPreset::default()
        },
    ]);

    document.select_next_pen_preset();

    assert_eq!(document.active_pen_preset_id, "bold");
    assert_eq!(document.active_pen_size, 9);
}

/// ドキュメント 新規 uses requested キャンバス サイズ が期待どおりに動作することを検証する。
#[test]
fn document_new_uses_requested_canvas_size() {
    let document = Document::new(320, 240);

    let bitmap = document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (320, 240));
}

/// 適用 コマンド 新規 ドキュメント sized replaces ビットマップ dimensions が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn apply_command_new_document_sized_replaces_bitmap_dimensions() {
    let mut document = Document::default();

    let dirty = document.apply_command(&Command::NewDocumentSized {
        width: 512,
        height: 384,
    });

    assert_eq!(dirty, None);
    let bitmap = document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (512, 384));
}

/// 差分 矩形 clamps to ビットマップ 範囲 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn dirty_rect_clamps_to_bitmap_bounds() {
    let rect = CanvasDirtyRect {
        x: 60,
        y: 62,
        width: 10,
        height: 10,
    };

    assert_eq!(
        rect.clamp_to_canvas_bounds(64, 64),
        CanvasDirtyRect {
            x: 60,
            y: 62,
            width: 4,
            height: 2,
        }
    );
}

/// ドキュメント stores キャンバス ビュー 変換 が期待どおりに動作することを検証する。
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

/// 追加 raster レイヤー selects 新規 レイヤー が期待どおりに動作することを検証する。
#[test]
fn add_raster_layer_selects_new_layer() {
    let mut document = Document::default();

    let _ = document.apply_command(&Command::AddRasterLayer);

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers.len(), 2);
    assert_eq!(panel.active_layer_index, 1);
    assert_eq!(panel.layers[1].name, "Layer 2");
}

/// 追加 raster レイヤー uses created レイヤー counter for names が期待どおりに動作することを検証する。
#[test]
fn add_raster_layer_uses_created_layer_counter_for_names() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = document.apply_command(&Command::RemoveActiveLayer);

    let _ = document.apply_command(&Command::AddRasterLayer);

    let panel = &document.work.pages[0].panels[0];
    let names = panel
        .layers
        .iter()
        .map(|layer| layer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Layer 1", "Layer 2", "Layer 4"]);
    assert_eq!(panel.created_layer_count, 4);
}

/// 削除 アクティブ レイヤー keeps at least one レイヤー が期待どおりに動作することを検証する。
#[test]
fn remove_active_layer_keeps_at_least_one_layer() {
    let mut document = Document::default();

    let _ = document.apply_command(&Command::RemoveActiveLayer);

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers.len(), 1);
    assert_eq!(panel.active_layer_index, 0);
}

/// 削除 アクティブ レイヤー selects remaining レイヤー が期待どおりに動作することを検証する。
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

/// move レイヤー reorders layers and tracks アクティブ selection が期待どおりに動作することを検証する。
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
    let names = panel
        .layers
        .iter()
        .map(|layer| layer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Layer 3", "Layer 1", "Layer 2"]);
    assert_eq!(panel.active_layer_index, 0);
}

/// rename アクティブ レイヤー updates 選択中 レイヤー 名前 が期待どおりに動作することを検証する。
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

/// 設定 アクティブ レイヤー ブレンド モード sets requested モード が期待どおりに動作することを検証する。
#[test]
fn set_active_layer_blend_mode_sets_requested_mode() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
        mode: BlendMode::Screen,
    });

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers[0].blend_mode, BlendMode::Screen);
}

/// 設定 アクティブ レイヤー ブレンド モード accepts custom formula string が期待どおりに動作することを検証する。
#[test]
fn set_active_layer_blend_mode_accepts_custom_formula_string() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
        mode: BlendMode::parse_name("max(src, dst)").expect("custom mode"),
    });

    let panel = &document.work.pages[0].panels[0];
    assert_eq!(panel.layers[0].blend_mode.as_str(), "max(src, dst)");
}

/// custom ブレンド formula is applied during レイヤー composition が期待どおりに動作することを検証する。
#[test]
fn custom_blend_formula_is_applied_during_layer_composition() {
    let mut document = Document::default();
    let panel = &mut document.work.pages[0].panels[0];
    panel.layers[0]
        .bitmap
        .draw_point_rgba(0, 0, [64, 64, 64, 255]);

    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = draw_point(&mut document, 0, 0);
    let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
        mode: BlendMode::parse_name("max(src, dst)").expect("custom mode"),
    });

    let pixel = &document.active_bitmap().expect("bitmap exists").pixels[0..4];
    assert_eq!(pixel, &[64, 64, 64, 255]);
}

/// 切替 アクティブ レイヤー visibility reveals underlying レイヤー が期待どおりに動作することを検証する。
#[test]
fn toggle_active_layer_visibility_reveals_underlying_layer() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = draw_point(&mut document, 5, 5);

    let visible_bitmap = document.active_bitmap().expect("bitmap exists").clone();
    let _ = document.apply_command(&Command::ToggleActiveLayerVisibility);
    let hidden_bitmap = document.active_bitmap().expect("bitmap exists");

    let index = (5 * visible_bitmap.width + 5) * 4;
    assert_eq!(&visible_bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(
        &hidden_bitmap.pixels[index..index + 4],
        &[255, 255, 255, 255]
    );
}

/// 切替 アクティブ レイヤー マスク applies demo マスク が期待どおりに動作することを検証する。
#[test]
fn toggle_active_layer_mask_applies_demo_mask() {
    let mut document = Document::default();
    let _ = document.apply_command(&Command::AddRasterLayer);
    let _ = draw_point(&mut document, 1, 1);

    let before_mask = document.active_bitmap().expect("bitmap exists").clone();
    let _ = document.apply_command(&Command::ToggleActiveLayerMask);
    let after_mask = document.active_bitmap().expect("bitmap exists");

    let index = (before_mask.width + 1) * 4;
    assert_eq!(&before_mask.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(&after_mask.pixels[index..index + 4], &[255, 255, 255, 255]);
}

/// 生成 パネル コマンド adds rectangular パネル without relayout が期待どおりに動作することを検証する。
#[test]
fn create_panel_command_adds_rectangular_panel_without_relayout() {
    let mut document = Document::new(320, 240);

    let _ = document.apply_command(&Command::CreatePanel {
        x: 40,
        y: 32,
        width: 120,
        height: 80,
    });

    assert_eq!(document.active_page_panel_count(), 2);
    let panel = document.active_panel().expect("active panel exists");
    assert_eq!(
        panel.bounds,
        PanelBounds {
            x: 40,
            y: 32,
            width: 120,
            height: 80,
        }
    );
    assert_eq!((panel.bitmap.width, panel.bitmap.height), (120, 80));
}

/// パネル local 描画 returns ページ space 差分 矩形 が期待どおりに動作することを検証する。
///
/// 必要に応じて dirty 状態も更新します。
#[test]
fn panel_local_draw_returns_page_space_dirty_rect() {
    let mut document = Document::new(320, 240);
    let _ = document.apply_command(&Command::CreatePanel {
        x: 40,
        y: 32,
        width: 120,
        height: 80,
    });
    document.set_active_pen_size(1);

    let dirty = draw_point(&mut document, 2, 3).expect("dirty rect exists");

    assert_eq!(
        dirty,
        CanvasDirtyRect::from_inclusive_points(42, 35, 42, 35)
    );
}

/// 追加 パネル selects 新規 アクティブ パネル が期待どおりに動作することを検証する。
#[test]
fn add_panel_selects_new_active_panel() {
    let mut document = Document::new(320, 240);

    let _ = document.apply_command(&Command::AddPanel);

    assert_eq!(document.active_page_panel_count(), 2);
    assert_eq!(document.active_panel_index(), 1);
    let active_panel = document.active_panel().expect("active panel exists");
    assert!(active_panel.bounds.width > 0);
    assert!(active_panel.bounds.height > 0);
}

/// パネル selection switches 編集 target が期待どおりに動作することを検証する。
#[test]
fn panel_selection_switches_edit_target() {
    let mut document = Document::new(128, 128);
    let _ = document.apply_command(&Command::AddPanel);
    let _ = document.apply_command(&Command::SelectPanel { index: 1 });
    document.set_active_pen_size(1);

    let _ = draw_point(&mut document, 2, 3);

    let first_panel = &document.work.pages[0].panels[0];
    let second_panel = &document.work.pages[0].panels[1];
    let first_index = (3 * first_panel.bitmap.width + 2) * 4;
    let second_index = (3 * second_panel.bitmap.width + 2) * 4;
    assert_eq!(
        &first_panel.bitmap.pixels[first_index..first_index + 4],
        &[255, 255, 255, 255]
    );
    assert_eq!(
        &second_panel.bitmap.pixels[second_index..second_index + 4],
        &[0, 0, 0, 255]
    );
}

/// 選択 前 パネル wraps to last パネル が期待どおりに動作することを検証する。
#[test]
fn select_previous_panel_wraps_to_last_panel() {
    let mut document = Document::new(256, 256);
    let _ = document.apply_command(&Command::AddPanel);
    let _ = document.apply_command(&Command::SelectPanel { index: 0 });

    let _ = document.apply_command(&Command::SelectPreviousPanel);

    assert_eq!(document.active_panel_index(), 1);
}

/// 削除 アクティブ パネル keeps single パネル minimum が期待どおりに動作することを検証する。
#[test]
fn remove_active_panel_keeps_single_panel_minimum() {
    let mut document = Document::new(256, 256);
    let _ = document.apply_command(&Command::RemoveActivePanel);

    assert_eq!(document.active_page_panel_count(), 1);

    let _ = document.apply_command(&Command::AddPanel);
    let _ = document.apply_command(&Command::RemoveActivePanel);

    assert_eq!(document.active_page_panel_count(), 1);
    assert_eq!(document.active_panel_index(), 0);
}

/// フォーカス アクティブ パネル resets ビュー 変換 が期待どおりに動作することを検証する。
#[test]
fn focus_active_panel_resets_view_transform() {
    let mut document = Document::new(256, 256);
    document.set_view_transform(CanvasViewTransform {
        zoom: 2.5,
        rotation_degrees: 33.0,
        pan_x: 40.0,
        pan_y: -20.0,
        flip_x: true,
        flip_y: false,
    });

    let _ = document.apply_command(&Command::FocusActivePanel);

    assert_eq!(document.view_transform, CanvasViewTransform::default());
}
