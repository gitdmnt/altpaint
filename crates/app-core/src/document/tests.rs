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
    koma.bitmap = super::layer_ops::composite_koma_bitmap(koma);
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
        document.work.pages[0].komas[0].bitmap.width,
        DEFAULT_PAGE_WIDTH
    );
    assert_eq!(
        document.work.pages[0].komas[0].bitmap.height,
        DEFAULT_PAGE_HEIGHT
    );
}

#[test]
fn draw_point_marks_target_pixel_black() {
    let mut document = Document::default();
    document.set_active_pen_size(1);

    let dirty = draw_point(&mut document, 3, 4).expect("koma should exist");

    let bitmap = &document.work.pages[0].komas[0].bitmap;
    let index = (4 * bitmap.width + 3) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    assert_eq!(dirty, PageDirtyRect::from_inclusive_points(3, 4, 3, 4));
}

#[test]
fn draw_stroke_draws_continuous_line() {
    let mut document = Document::default();
    document.set_active_pen_size(1);

    let dirty = draw_stroke(&mut document, 2, 2, 6, 2).expect("koma should exist");

    let bitmap = &document.work.pages[0].komas[0].bitmap;
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

    let bitmap = &document.work.pages[0].komas[0].bitmap;
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

    let bitmap = &document.work.pages[0].komas[0].bitmap;
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

    document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });

    assert_eq!(document.active_tool, ToolKind::Pen);
}

#[test]
fn apply_command_selects_registered_tool_by_id() {
    let mut document = Document::default();

    document.apply_command(&Command::SelectTool {
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

    document.apply_command(&Command::SetActivePenSize { size: 12 });

    assert_eq!(document.active_pen_size, 12);
}

#[test]
fn apply_command_switches_active_color() {
    let mut document = Document::default();

    document.apply_command(&Command::SetActiveColor {
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
    document.apply_command(&Command::SetActivePenSize { size: 1 });

    let dirty = draw_stroke(&mut document, 1, 1, 3, 1);

    assert_eq!(
        dirty,
        Some(PageDirtyRect::from_inclusive_points(1, 1, 3, 1))
    );
    let bitmap = &document.work.pages[0].komas[0].bitmap;
    let index = (bitmap.width + 2) * 4;
    assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
}

#[test]
fn pen_draws_wider_than_single_pixel_default_stroke() {
    let mut document = Document::default();
    document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });
    document.apply_command(&Command::SetActivePenSize { size: 5 });

    let dirty = draw_point(&mut document, 10, 10).expect("koma should exist");

    assert!(dirty.width >= 5);
    assert!(dirty.height >= 5);
    let bitmap = &document.work.pages[0].komas[0].bitmap;
    let center = (10 * bitmap.width + 10) * 4;
    let edge = (10 * bitmap.width + 8) * 4;
    assert_eq!(&bitmap.pixels[center..center + 4], &[0, 0, 0, 255]);
    assert_eq!(&bitmap.pixels[edge..edge + 4], &[0, 0, 0, 255]);
}

#[test]
fn wide_stroke_keeps_segment_core_filled() {
    let mut document = Document::new(128, 128);
    document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });
    document.apply_command(&Command::SetActivePenSize { size: 24 });

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
    document.apply_command(&Command::SetActiveTool {
        tool: ToolKind::Pen,
    });
    document.apply_command(&Command::SetActivePenSize { size: 18 });

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

    document.apply_command(&Command::NewDocumentSized {
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

    document.apply_command(&Command::AddRasterLayer);

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers.len(), 2);
    assert_eq!(koma.active_layer_index, 1);
    assert_eq!(koma.layers[1].name, "Layer 2");
}

#[test]
fn add_raster_layer_uses_created_layer_counter_for_names() {
    let mut document = Document::default();
    document.apply_command(&Command::AddRasterLayer);
    document.apply_command(&Command::AddRasterLayer);
    document.apply_command(&Command::RemoveActiveLayer);

    document.apply_command(&Command::AddRasterLayer);

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

    document.apply_command(&Command::RemoveActiveLayer);

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers.len(), 1);
    assert_eq!(koma.active_layer_index, 0);
}

#[test]
fn remove_active_layer_selects_remaining_layer() {
    let mut document = Document::default();
    document.apply_command(&Command::AddRasterLayer);
    document.apply_command(&Command::AddRasterLayer);

    document.apply_command(&Command::RemoveActiveLayer);

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers.len(), 2);
    assert_eq!(koma.active_layer_index, 1);
    assert_eq!(koma.layers[1].name, "Layer 2");
}

#[test]
fn move_layer_reorders_layers_and_tracks_active_selection() {
    let mut document = Document::default();
    document.apply_command(&Command::AddRasterLayer);
    document.apply_command(&Command::AddRasterLayer);

    document.apply_command(&Command::MoveLayer {
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
    document.apply_command(&Command::AddRasterLayer);

    document.apply_command(&Command::RenameActiveLayer {
        name: "Ink".to_string(),
    });

    let koma = &document.work.pages[0].komas[0];
    assert_eq!(koma.layers[1].name, "Ink");
}

#[test]
fn set_active_layer_blend_mode_sets_requested_mode() {
    let mut document = Document::default();
    document.apply_command(&Command::SetActiveLayerBlendMode {
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

/// BlendMode::gpu_code は GPU shader の switch コードと 1:1 対応する。
#[test]
fn gpu_code_matches_shader_switch_codes() {
    assert_eq!(BlendMode::Normal.gpu_code(), 0);
    assert_eq!(BlendMode::Multiply.gpu_code(), 1);
    assert_eq!(BlendMode::Screen.gpu_code(), 2);
    assert_eq!(BlendMode::Add.gpu_code(), 3);
}

#[test]
fn toggle_active_layer_visibility_reveals_underlying_layer() {
    let mut document = Document::default();
    document.apply_command(&Command::AddRasterLayer);
    let _ = draw_point(&mut document, 5, 5);

    let visible_bitmap = document.active_bitmap().expect("bitmap exists").clone();
    document.apply_command(&Command::ToggleActiveLayerVisibility);
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

    document.apply_command(&Command::CreateKoma {
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
    assert_eq!((koma.bitmap.width, koma.bitmap.height), (120, 80));
}

#[test]
fn koma_local_draw_returns_page_space_dirty_rect() {
    let mut document = Document::new(320, 240);
    document.apply_command(&Command::CreateKoma {
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

    document.apply_command(&Command::AddKoma);

    assert_eq!(document.active_page_koma_count(), 2);
    assert_eq!(document.active_koma_index(), 1);
    let active_koma = document.active_koma().expect("active koma exists");
    assert!(active_koma.bounds.width > 0);
    assert!(active_koma.bounds.height > 0);
}

#[test]
fn koma_selection_switches_edit_target() {
    let mut document = Document::new(128, 128);
    document.apply_command(&Command::AddKoma);
    document.apply_command(&Command::SelectKoma { index: 1 });
    document.set_active_pen_size(1);

    let _ = draw_point(&mut document, 2, 3);

    let first_koma = &document.work.pages[0].komas[0];
    let second_koma = &document.work.pages[0].komas[1];
    let first_index = (3 * first_koma.bitmap.width + 2) * 4;
    let second_index = (3 * second_koma.bitmap.width + 2) * 4;
    assert_eq!(
        &first_koma.bitmap.pixels[first_index..first_index + 4],
        &[255, 255, 255, 255]
    );
    assert_eq!(
        &second_koma.bitmap.pixels[second_index..second_index + 4],
        &[0, 0, 0, 255]
    );
}

#[test]
fn select_previous_koma_wraps_to_last_koma() {
    let mut document = Document::new(256, 256);
    document.apply_command(&Command::AddKoma);
    document.apply_command(&Command::SelectKoma { index: 0 });

    document.apply_command(&Command::SelectPreviousKoma);

    assert_eq!(document.active_koma_index(), 1);
}

#[test]
fn remove_active_koma_keeps_single_koma_minimum() {
    let mut document = Document::new(256, 256);
    document.apply_command(&Command::RemoveActiveKoma);

    assert_eq!(document.active_page_koma_count(), 1);

    document.apply_command(&Command::AddKoma);
    document.apply_command(&Command::RemoveActiveKoma);

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

    document.apply_command(&Command::FocusActiveKoma);

    assert_eq!(document.view_transform, CanvasViewTransform::default());
}
