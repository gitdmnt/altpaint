use super::*;
use crate::{PageDirtyRect, ClampToCanvasBounds, MergeInSpace};

fn apply_layer_brush(
    document: &mut Document,
    paint: impl FnOnce(&mut CanvasBitmap, bool) -> PageDirtyRect,
) -> Option<PageDirtyRect> {
    let koma_bounds = document.active_koma_bounds()?;
    let (page_width, page_height) = document.active_page_dimensions();
    let koma = document.active_koma_mut()?;
    super::layer_ops::ensure_koma_layers(koma);
    let is_background = koma.active_layer_index == 0;
    let local_dirty = {
        let layer = &mut koma.layers[koma.active_layer_index];
        paint(&mut layer.bitmap, is_background)
    };
    koma.composite_cache = super::layer_ops::composite_koma_bitmap(koma);
    Some(
        PageDirtyRect {
            x: local_dirty.x.saturating_add(koma_bounds.x),
            y: local_dirty.y.saturating_add(koma_bounds.y),
            width: local_dirty.width,
            height: local_dirty.height,
        }
        .clamp_to_canvas_bounds(page_width.max(1), page_height.max(1)),
    )
}

fn draw_point(document: &mut Document, x: usize, y: usize) -> Option<PageDirtyRect> {
    let color = document.active_color.to_rgba8();
    let size = document.brush_size_for_pressure(1.0);
    let antialias = document
        .active_pen_preset()
        .map(|preset| preset.antialias)
        .unwrap_or(true);
    apply_layer_brush(document, |bitmap, _| {
        bitmap.draw_point_sized_rgba(x, y, color, size, antialias)
    })
}

fn draw_stroke(
    document: &mut Document,
    from_x: usize,
    from_y: usize,
    to_x: usize,
    to_y: usize,
) -> Option<PageDirtyRect> {
    let color = document.active_color.to_rgba8();
    let size = document.brush_size_for_pressure(1.0);
    let antialias = document
        .active_pen_preset()
        .map(|preset| preset.antialias)
        .unwrap_or(true);
    apply_layer_brush(document, |bitmap, _| {
        bitmap.draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size, antialias)
    })
}

fn erase_point(document: &mut Document, x: usize, y: usize) -> Option<PageDirtyRect> {
    let size = document.brush_size_for_pressure(1.0);
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

#[test]
fn default_document_has_single_page_single_koma_single_layer() {
    let document = Document::default();

    assert_eq!(document.work.title, "Untitled");
    assert_eq!(document.work.pages.len(), 1);
    assert_eq!(document.work.pages[0].komas.len(), 1);
    assert_eq!(document.work.pages[0].komas[0].layers[0].name, "Layer 1");
    assert_eq!(
        document.work.pages[0].komas[0].composite_cache.width,
        DEFAULT_PAGE_WIDTH
    );
    assert_eq!(
        document.work.pages[0].komas[0].composite_cache.height,
        DEFAULT_PAGE_HEIGHT
    );
}

#[test]
fn draw_point_marks_target_pixel_black() {
    let mut document = Document::default();
    document.set_active_pen_size(1);

    let dirty = draw_point(&mut document, 3, 4).expect("koma should exist");

    let bitmap = &document.work.pages[0].komas[0].composite_cache;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(dirty, PageDirtyRect::from_inclusive_points(3, 4, 3, 4));
}

#[test]
fn draw_stroke_draws_continuous_line() {
    let mut document = Document::default();
    document.set_active_pen_size(1);

    let dirty = draw_stroke(&mut document, 2, 2, 6, 2).expect("koma should exist");

    let bitmap = &document.work.pages[0].komas[0].composite_cache;
    for x in 2..=6 {
        let index = (2 * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
    assert_eq!(dirty, PageDirtyRect::from_inclusive_points(2, 2, 6, 2));
}

#[test]
fn erase_point_marks_target_pixel_white() {
    let mut document = Document::default();
    document.set_active_pen_size(1);
    let _ = draw_point(&mut document, 3, 4);

    let dirty = erase_point(&mut document, 3, 4).expect("koma should exist");

    let bitmap = &document.work.pages[0].komas[0].composite_cache;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[255, 255, 255, 255]);
    assert_eq!(dirty, PageDirtyRect::from_inclusive_points(3, 4, 3, 4));
}

#[test]
fn active_tool_defaults_to_pen() {
    let document = Document::default();

    assert_eq!(document.active_tool, ToolKind::Pen);
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

    let _ = draw_point(&mut document, 3, 4);

    let bitmap = &document.work.pages[0].komas[0].composite_cache;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0xe5, 0x39, 0x35, 0xff]);
}

#[test]
fn dirty_rect_union_merges_bounds() {
    let left = PageDirtyRect::from_inclusive_points(2, 3, 4, 5);
    let right = PageDirtyRect::from_inclusive_points(6, 1, 7, 4);

    assert_eq!(
        left.merge(right),
        PageDirtyRect {
            x: 2,
            y: 1,
            width: 6,
            height: 5,
        }
    );
}

#[test]
fn canvas_defaults_to_white_background() {
    let bitmap = CanvasBitmap::default();

    assert_eq!(&bitmap.pixels[0..4], &[255, 255, 255, 255]);
}

#[test]
fn apply_command_switches_active_tool() {
    let mut document = Document::default();

    document.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Pen,
    });

    assert_eq!(document.active_tool, ToolKind::Pen);
}

#[test]
fn apply_command_selects_registered_tool_by_id() {
    let mut document = Document::default();

    document.apply_session_command(&SessionCommand::SelectTool {
        tool_id: "builtin.eraser".to_string(),
    });

    assert_eq!(document.active_tool, ToolKind::Eraser);
    assert_eq!(document.active_tool_id, "builtin.eraser");
}

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

#[test]
fn apply_command_updates_pen_size() {
    let mut document = Document::default();

    document.apply_session_command(&SessionCommand::SetActivePenSize { size: 12 });

    assert_eq!(document.active_pen_size, 12);
}

#[test]
fn apply_command_switches_active_color() {
    let mut document = Document::default();

    document.apply_session_command(&SessionCommand::SetActiveColor {
        color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
    });

    assert_eq!(
        document.active_color,
        ColorRgba8::new(0x43, 0xa0, 0x47, 0xff)
    );
}

#[test]
fn bitmap_edit_style_stroke_returns_dirty_rect() {
    let mut document = Document::default();
    document.apply_session_command(&SessionCommand::SetActivePenSize { size: 1 });

    let dirty = draw_stroke(&mut document, 1, 1, 3, 1);

    assert_eq!(
        dirty,
        Some(PageDirtyRect::from_inclusive_points(1, 1, 3, 1))
    );
    let bitmap = &document.work.pages[0].komas[0].composite_cache;
    let index = (bitmap.width + 2) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
}

#[test]
fn pen_draws_wider_than_single_pixel_default_stroke() {
    let mut document = Document::default();
    document.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Pen,
    });
    document.apply_session_command(&SessionCommand::SetActivePenSize { size: 5 });

    let dirty = draw_point(&mut document, 10, 10).expect("koma should exist");

    assert!(dirty.width >= 5);
    assert!(dirty.height >= 5);
    let bitmap = &document.work.pages[0].komas[0].composite_cache;
    let center = (10 * bitmap.width + 10) * 4;
    let edge = (10 * bitmap.width + 8) * 4;
    assert_eq!(&bitmap.pixels[center..center + 4], &[0, 0, 0, 255]);
    assert_eq!(&bitmap.pixels[edge..edge + 4], &[0, 0, 0, 255]);
}

#[test]
fn wide_stroke_keeps_segment_core_filled() {
    let mut document = Document::new(128, 128);
    document.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Pen,
    });
    document.apply_session_command(&SessionCommand::SetActivePenSize { size: 24 });

    let dirty = draw_stroke(&mut document, 20, 64, 108, 64).expect("koma should exist");

    assert!(dirty.width >= 88);
    assert!(dirty.height >= 24);
    let bitmap = document.active_bitmap().expect("bitmap exists");
    for x in [20usize, 44, 64, 84, 108] {
        let index = (64 * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
}

#[test]
fn wide_diagonal_stroke_marks_midpoint_pixels() {
    let mut document = Document::new(128, 128);
    document.apply_session_command(&SessionCommand::SetActiveTool {
        tool: ToolKind::Pen,
    });
    document.apply_session_command(&SessionCommand::SetActivePenSize { size: 18 });

    let dirty = draw_stroke(&mut document, 16, 16, 112, 112).expect("koma should exist");

    assert!(dirty.width >= 96);
    assert!(dirty.height >= 96);
    let bitmap = document.active_bitmap().expect("bitmap exists");
    for (x, y) in [(16usize, 16usize), (64, 64), (112, 112)] {
        let index = (y * bitmap.width + x) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }
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

#[test]
fn document_new_uses_requested_canvas_size() {
    let document = Document::new(320, 240);

    let bitmap = document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (320, 240));
}

#[test]
fn apply_command_new_document_sized_replaces_bitmap_dimensions() {
    let mut document = Document::default();

    document.apply(&DocumentCommand::NewDocumentSized {
        width: 512,
        height: 384,
    });

    let bitmap = document.active_bitmap().expect("bitmap exists");
    assert_eq!((bitmap.width, bitmap.height), (512, 384));
}

#[test]
fn dirty_rect_clamps_to_bitmap_bounds() {
    let rect = PageDirtyRect {
        x: 60,
        y: 62,
        width: 10,
        height: 10,
    };

    assert_eq!(
        rect.clamp_to_canvas_bounds(64, 64),
        PageDirtyRect {
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

    document.apply(&DocumentCommand::AddRasterLayer);

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers.len(), 2);
    assert_eq!(koma.active_layer_index, 1);
    assert_eq!(koma.layers[1].name, "Layer 2");
}

#[test]
fn add_raster_layer_uses_created_layer_counter_for_names() {
    let mut document = Document::default();
    document.apply(&DocumentCommand::AddRasterLayer);
    document.apply(&DocumentCommand::AddRasterLayer);
    document.apply(&DocumentCommand::RemoveActiveLayer);

    document.apply(&DocumentCommand::AddRasterLayer);

    let koma = &document.work.pages[0].komas[0];
    let names = koma
        .layers
        .iter()
        .map(|layer| layer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Layer 1", "Layer 2", "Layer 4"]);
    assert_eq!(koma.created_layer_count, 4);
}

#[test]
fn remove_active_layer_keeps_at_least_one_layer() {
    let mut document = Document::default();

    document.apply(&DocumentCommand::RemoveActiveLayer);

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers.len(), 1);
    assert_eq!(koma.active_layer_index, 0);
}

#[test]
fn remove_active_layer_selects_remaining_layer() {
    let mut document = Document::default();
    document.apply(&DocumentCommand::AddRasterLayer);
    document.apply(&DocumentCommand::AddRasterLayer);

    document.apply(&DocumentCommand::RemoveActiveLayer);

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers.len(), 2);
    assert_eq!(koma.active_layer_index, 1);
    assert_eq!(koma.layers[1].name, "Layer 2");
}

#[test]
fn move_layer_reorders_layers_and_tracks_active_selection() {
    let mut document = Document::default();
    document.apply(&DocumentCommand::AddRasterLayer);
    document.apply(&DocumentCommand::AddRasterLayer);

    document.apply(&DocumentCommand::MoveLayer {
        from_index: 2,
        to_index: 0,
    });

    let koma = &document.work.pages[0].komas[0];
    let names = koma
        .layers
        .iter()
        .map(|layer| layer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Layer 3", "Layer 1", "Layer 2"]);
    assert_eq!(koma.active_layer_index, 0);
}

#[test]
fn rename_active_layer_updates_selected_layer_name() {
    let mut document = Document::default();
    document.apply(&DocumentCommand::AddRasterLayer);

    document.apply(&DocumentCommand::RenameActiveLayer {
        name: "Ink".to_string(),
    });

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers[1].name, "Ink");
}

#[test]
fn set_active_layer_blend_mode_sets_requested_mode() {
    let mut document = Document::default();
    document.apply(&DocumentCommand::SetActiveLayerBlendMode {
        mode: BlendMode::Screen,
    });

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers[0].blend_mode, BlendMode::Screen);
}

/// 未知のブレンドモード名は後方互換として Normal にフォールバックする。
#[test]
fn parse_name_falls_back_to_normal_for_unknown_strings() {
    assert_eq!(BlendMode::parse_name("max(src, dst)"), Some(BlendMode::Normal));
    assert_eq!(BlendMode::parse_name("unknown_mode"), Some(BlendMode::Normal));
    assert_eq!(BlendMode::parse_name(""), None);
    assert_eq!(BlendMode::parse_name("   "), None);
}

#[test]
fn toggle_active_layer_visibility_reveals_underlying_layer() {
    let mut document = Document::default();
    document.apply(&DocumentCommand::AddRasterLayer);
    let _ = draw_point(&mut document, 5, 5);

    let visible_bitmap = document.active_bitmap().expect("bitmap exists").clone();
    document.apply(&DocumentCommand::ToggleActiveLayerVisibility);
    let hidden_bitmap = document.active_bitmap().expect("bitmap exists");

    let index = (5 * visible_bitmap.width + 5) * 4;
    assert_eq!(&visible_bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(
        &hidden_bitmap.pixels[index..index + 4],
        &[255, 255, 255, 255]
    );
}

#[test]
fn create_koma_command_adds_rectangular_koma_without_relayout() {
    let mut document = Document::new(320, 240);

    document.apply(&DocumentCommand::CreateKoma {
        x: 40,
        y: 32,
        width: 120,
        height: 80,
    });

    assert_eq!(document.active_page_koma_count(), 2);
    let koma = document.active_koma().expect("active koma exists");
    assert_eq!(
        koma.bounds,
        KomaBounds {
            x: 40,
            y: 32,
            width: 120,
            height: 80,
        }
    );
    assert_eq!((koma.composite_cache.width, koma.composite_cache.height), (120, 80));
}

#[test]
fn koma_local_draw_returns_page_space_dirty_rect() {
    let mut document = Document::new(320, 240);
    document.apply(&DocumentCommand::CreateKoma {
        x: 40,
        y: 32,
        width: 120,
        height: 80,
    });
    document.set_active_pen_size(1);

    let dirty = draw_point(&mut document, 2, 3).expect("dirty rect exists");

    assert_eq!(
        dirty,
        PageDirtyRect::from_inclusive_points(42, 35, 42, 35)
    );
}

#[test]
fn add_koma_selects_new_active_koma() {
    let mut document = Document::new(320, 240);

    document.apply(&DocumentCommand::AddKoma);

    assert_eq!(document.active_page_koma_count(), 2);
    assert_eq!(document.active_koma_index(), 1);
    let active_koma = document.active_koma().expect("active koma exists");
    assert!(active_koma.bounds.width > 0);
    assert!(active_koma.bounds.height > 0);
}

#[test]
fn koma_selection_switches_edit_target() {
    let mut document = Document::new(128, 128);
    document.apply(&DocumentCommand::AddKoma);
    document.apply(&DocumentCommand::SelectKoma { index: 1 });
    document.set_active_pen_size(1);

    let _ = draw_point(&mut document, 2, 3);

    let first_koma = &document.work.pages[0].komas[0];
    let second_koma = &document.work.pages[0].komas[1];
    let first_index = (3 * first_koma.composite_cache.width + 2) * 4;
    let second_index = (3 * second_koma.composite_cache.width + 2) * 4;
    assert_eq!(
        &first_koma.composite_cache.pixels[first_index..first_index + 4],
        &[255, 255, 255, 255]
    );
    assert_eq!(
        &second_koma.composite_cache.pixels[second_index..second_index + 4],
        &[0, 0, 0, 255]
    );
}

#[test]
fn select_previous_koma_wraps_to_last_koma() {
    let mut document = Document::new(256, 256);
    document.apply(&DocumentCommand::AddKoma);
    document.apply(&DocumentCommand::SelectKoma { index: 0 });

    document.apply(&DocumentCommand::SelectPreviousKoma);

    assert_eq!(document.active_koma_index(), 1);
}

#[test]
fn remove_active_koma_keeps_single_koma_minimum() {
    let mut document = Document::new(256, 256);
    document.apply(&DocumentCommand::RemoveActiveKoma);

    assert_eq!(document.active_page_koma_count(), 1);

    document.apply(&DocumentCommand::AddKoma);
    document.apply(&DocumentCommand::RemoveActiveKoma);

    assert_eq!(document.active_page_koma_count(), 1);
    assert_eq!(document.active_koma_index(), 0);
}

#[test]
fn focus_active_koma_resets_view_transform() {
    let mut document = Document::new(256, 256);
    document.set_view_transform(CanvasViewTransform {
        zoom: 2.5,
        rotation_degrees: 33.0,
        pan_x: 40.0,
        pan_y: -20.0,
        flip_x: true,
        flip_y: false,
    });

    document.apply(&DocumentCommand::FocusActiveKoma);

    assert_eq!(document.view_transform, CanvasViewTransform::default());
}

// --- BL-032 ゴールデンテスト: CPU 合成の現挙動を固定する ---
//
// ブレンド実装の統合 (単一ブレンドモジュール化) の等価性確認に使う。
// dst 側は opaque / 半透明 / 透明 / 高アルファの 4 画素を共通で用いる。

/// ゴールデン共通の dst (下レイヤー) 画素列。
const GOLDEN_DST_PIXELS: [[u8; 4]; 4] = [
    [255, 0, 0, 255],
    [0, 255, 0, 128],
    [0, 0, 0, 0],
    [100, 100, 100, 200],
];

fn golden_row_bitmap(pixels: &[[u8; 4]]) -> CanvasBitmap {
    let mut bitmap = CanvasBitmap::transparent(pixels.len(), 1);
    for (x, px) in pixels.iter().enumerate() {
        let _ = bitmap.set_pixel_rgba(x, 0, *px);
    }
    bitmap
}

/// 下 (Normal) + 上 (指定 mode/mask) の 2 レイヤー koma を組み立てる。
fn golden_two_layer_koma(
    mode: BlendMode,
    top_pixels: [[u8; 4]; 4],
    mask: Option<LayerMask>,
) -> Koma {
    Koma {
        id: KomaId(1),
        bounds: KomaBounds::full_page(4, 1),
        composite_cache: CanvasBitmap::transparent(4, 1),
        layers: vec![
            RasterLayer {
                id: LayerNodeId(1),
                name: "bottom".into(),
                visible: true,
                blend_mode: BlendMode::Normal,
                bitmap: golden_row_bitmap(&GOLDEN_DST_PIXELS),
                mask: None,
            },
            RasterLayer {
                id: LayerNodeId(2),
                name: "top".into(),
                visible: true,
                blend_mode: mode,
                bitmap: golden_row_bitmap(&top_pixels),
                mask,
            },
        ],
        active_layer_index: 0,
        created_layer_count: 2,
    }
}

/// BL-032 ゴールデン: 代表 BlendMode × アルファ組合せのレイヤー合成結果を固定する。
#[test]
fn composite_golden_layer_blend_modes_with_semi_alpha_src() {
    let semi_src = [[50u8, 80, 200, 128]; 4];
    let cases: [(BlendMode, [u8; 16]); 4] = [
        (
            BlendMode::Normal,
            [152, 40, 100, 255, 25, 104, 100, 192, 25, 40, 100, 128, 64, 79, 139, 228],
        ),
        (
            BlendMode::Multiply,
            [152, 0, 0, 255, 0, 84, 0, 192, 0, 0, 0, 128, 47, 51, 70, 228],
        ),
        (
            BlendMode::Screen,
            [255, 40, 100, 255, 25, 148, 100, 192, 25, 40, 100, 128, 95, 106, 148, 228],
        ),
        (
            BlendMode::Add,
            [255, 40, 100, 255, 25, 168, 100, 192, 25, 40, 100, 128, 103, 118, 167, 228],
        ),
    ];

    for (mode, expected) in cases {
        let koma = golden_two_layer_koma(mode.clone(), semi_src, None);
        let composite = super::layer_ops::composite_koma_bitmap(&koma);
        assert_eq!(composite.pixels, expected, "mode: {mode:?}");
    }
}

/// BL-032 ゴールデン: 不透明 src は dst を完全に置換し、alpha 0 の src は dst を保持する。
#[test]
fn composite_golden_layer_normal_opaque_and_zero_alpha_src() {
    let koma = golden_two_layer_koma(BlendMode::Normal, [[50, 80, 200, 255]; 4], None);
    assert_eq!(
        super::layer_ops::composite_koma_bitmap(&koma).pixels,
        [50, 80, 200, 255, 50, 80, 200, 255, 50, 80, 200, 255, 50, 80, 200, 255],
    );

    // alpha 0 の src は no-op。透明地への半透明 dst 合成 (0,255,0,128 → 0,128,0,128) も固定する。
    let koma = golden_two_layer_koma(BlendMode::Normal, [[1, 2, 3, 0]; 4], None);
    assert_eq!(
        super::layer_ops::composite_koma_bitmap(&koma).pixels,
        [255, 0, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0, 78, 78, 78, 200],
    );
}

/// BL-032 ゴールデン: レイヤーマスク (alpha 128) は src alpha を整数演算で半減させる。
#[test]
fn composite_golden_layer_mask_halves_src_alpha() {
    let koma = golden_two_layer_koma(
        BlendMode::Multiply,
        [[50, 80, 200, 128]; 4],
        Some(LayerMask {
            width: 4,
            height: 1,
            alpha: vec![128; 4],
        }),
    );
    assert_eq!(
        super::layer_ops::composite_koma_bitmap(&koma).pixels,
        [204, 0, 0, 255, 0, 106, 0, 160, 0, 0, 0, 64, 62, 65, 74, 214],
    );
}

/// BL-032 ゴールデン: ブラシ被覆ブレンド (AA ディスク) の結果を固定する。
///
/// 透明地では dst 色に引きずられず src 色が保持され (straight alpha)、
/// 不透明地 (白) では被覆率に応じて補間されることを固定する。
#[test]
fn brush_blend_golden_on_transparent_and_opaque_bitmap() {
    let mut transparent = CanvasBitmap::transparent(5, 5);
    let _ = transparent.draw_point_sized_rgba(2, 2, [200, 40, 40, 128], 3, true);
    assert_eq!(
        transparent.pixels,
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
            0, 0, 0, 0, 200, 40, 40, 75, 200, 40, 40, 128, 200, 40, 40, 75, 0, 0, 0, 0, //
            0, 0, 0, 0, 200, 40, 40, 128, 200, 40, 40, 128, 200, 40, 40, 128, 0, 0, 0, 0, //
            0, 0, 0, 0, 200, 40, 40, 75, 200, 40, 40, 128, 200, 40, 40, 75, 0, 0, 0, 0, //
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
    );

    let mut white = CanvasBitmap::new(5, 5);
    let _ = white.draw_point_sized_rgba(2, 2, [200, 40, 40, 128], 3, true);
    assert_eq!(
        white.pixels,
        [
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, //
            255, 255, 255, 255, 239, 192, 192, 255, 227, 147, 147, 255, 239, 192, 192, 255, 255,
            255, 255, 255, //
            255, 255, 255, 255, 227, 147, 147, 255, 227, 147, 147, 255, 227, 147, 147, 255, 255,
            255, 255, 255, //
            255, 255, 255, 255, 239, 192, 192, 255, 227, 147, 147, 255, 239, 192, 192, 255, 255,
            255, 255, 255, //
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255,
        ],
    );
}

/// BL-032 ゴールデン: `BitmapEdit` 合成 (`BitmapComposite`) の結果を固定する。
#[test]
fn bitmap_composite_golden_source_over_and_multiply() {
    let incoming = golden_row_bitmap(&[
        [50, 80, 200, 128],
        [50, 80, 200, 255],
        [1, 2, 3, 0],
        [200, 200, 200, 128],
    ]);
    let previous = golden_row_bitmap(&GOLDEN_DST_PIXELS);

    assert_eq!(
        crate::BitmapComposite::SourceOver
            .compose(&incoming, &previous)
            .pixels,
        [152, 40, 100, 255, 50, 80, 200, 255, 0, 0, 0, 0, 150, 150, 150, 228],
    );
    assert_eq!(
        crate::BitmapComposite::Multiply
            .compose(&incoming, &previous)
            .pixels,
        [152, 0, 0, 255, 0, 80, 0, 255, 0, 0, 0, 0, 89, 89, 89, 228],
    );
}

#[test]
fn parse_document_size_accepts_common_formats() {
    assert_eq!(parse_document_size("64x64"), Some((64, 64)));
    assert_eq!(parse_document_size("2894x4093"), Some((2894, 4093)));
    assert_eq!(parse_document_size("320 240"), Some((320, 240)));
    assert_eq!(parse_document_size("800,600"), Some((800, 600)));
}

#[test]
fn parse_document_size_rejects_invalid_dimensions() {
    assert_eq!(parse_document_size("0x600"), None);
    assert_eq!(parse_document_size("99999x1"), None);
    assert_eq!(parse_document_size("foo"), None);
}

#[test]
fn tool_kind_wire_name_round_trips() {
    for tool in [
        ToolKind::Pen,
        ToolKind::Eraser,
        ToolKind::Bucket,
        ToolKind::LassoBucket,
        ToolKind::KomaRect,
    ] {
        assert_eq!(ToolKind::from_wire(tool.as_str()), Some(tool));
    }
    assert_eq!(ToolKind::from_wire("unknown"), None);
}

#[test]
fn tool_kind_display_labels_are_pascal_case() {
    assert_eq!(ToolKind::Pen.display_label(), "Pen");
    assert_eq!(ToolKind::LassoBucket.display_label(), "LassoBucket");
    assert_eq!(ToolKind::KomaRect.display_label(), "KomaRect");
}
