use app_core::{Document, PaintInput, PaintPluginContext};

use crate::ResolvedPaintContext;

/// 解決済みの サイズ for 入力 を返す。
pub fn resolved_size_for_input(document: &Document, input: &PaintInput) -> u32 {
    match input {
        PaintInput::Stamp { pressure, .. } | PaintInput::StrokeSegment { pressure, .. } => {
            document.resolved_paint_size_with_pressure(*pressure)
        }
        PaintInput::FloodFill { .. } | PaintInput::LassoFill { .. } => {
            document.active_pen_size.max(1)
        }
    }
}

/// Paint コンテキスト を構築する。
pub fn build_paint_context<'a>(
    document: &'a Document,
    input: &PaintInput,
) -> Option<ResolvedPaintContext<'a>> {
    if !points_inside_active_panel(document, input) {
        return None;
    }

    let resolved_size = resolved_size_for_input(document, input);
    let active_tool = document.active_tool_definition()?;
    let active_pen = document.active_pen_preset()?;
    let active_panel = document.active_panel()?;
    let active_layer_bitmap = document.active_layer_bitmap()?;
    let composited_bitmap = document.active_bitmap()?;

    Some(ResolvedPaintContext {
        plugin_id: active_tool.drawing_plugin_id.as_str(),
        context: PaintPluginContext {
            tool: active_tool.kind,
            tool_id: active_tool.id.as_str(),
            provider_plugin_id: active_tool.provider_plugin_id.as_str(),
            drawing_plugin_id: active_tool.drawing_plugin_id.as_str(),
            tool_settings: active_tool.settings.as_slice(),
            color: document.active_color,
            pen: active_pen,
            resolved_size,
            active_layer_bitmap,
            composited_bitmap,
            active_layer_is_background: document.active_layer_is_background().unwrap_or(false),
            active_layer_index: active_panel.active_layer_index,
            layer_count: active_panel.layers.len(),
        },
    })
}

/// 入力や種別に応じて処理を振り分ける。
fn points_inside_active_panel(document: &Document, input: &PaintInput) -> bool {
    match input {
        PaintInput::Stamp { at, .. } | PaintInput::FloodFill { at } => {
            document.active_panel_contains_local_point(*at)
        }
        PaintInput::StrokeSegment { from, to, .. } => {
            document.active_panel_contains_local_point(*from)
                || document.active_panel_contains_local_point(*to)
        }
        PaintInput::LassoFill { points } => points
            .iter()
            .any(|point| document.active_panel_contains_local_point(*point)),
    }
}
