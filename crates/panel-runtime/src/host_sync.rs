use app_core::{ColorRgba8, Document, ToolKind};
use serde_json::{Value, json};

pub(crate) const MAX_DOCUMENT_DIMENSION: usize = 8192;
pub(crate) const MAX_DOCUMENT_PIXELS: usize = 16_777_216;

/// アクティブな ツール 名前 を返す。
pub(crate) fn active_tool_name(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::Pen => "pen",
        ToolKind::Eraser => "eraser",
        ToolKind::Bucket => "bucket",
        ToolKind::LassoBucket => "lasso_bucket",
        ToolKind::PanelRect => "panel_rect",
    }
}

/// ホスト スナップショット を構築する。
pub(crate) fn build_host_snapshot(
    document: &Document,
    can_undo: bool,
    can_redo: bool,
    active_jobs: usize,
) -> Value {
    let active_tool_definition = document.active_tool_definition().cloned();
    let active_page = document.active_page();
    let active_panel = document.active_panel();
    let layers = active_panel
        .map(|panel| {
            panel
                .layers
                .iter()
                .map(|layer| {
                    json!({
                        "name": layer.name,
                        "blend_mode": layer.blend_mode.as_str(),
                        "visible": layer.visible,
                        "masked": layer.mask.is_some(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![json!({ "name": "Layer 1", "blend_mode": "normal", "visible": true, "masked": false })]
        });
    let layers_json = serde_json::to_string(&layers).unwrap_or_else(|_| "[]".to_string());
    let panels = active_page
        .map(|page| {
            page.panels
                .iter()
                .enumerate()
                .map(|(index, panel)| {
                    json!({
                        "name": format!("コマ {}", index + 1),
                        "detail": format!(
                            "{}×{} / ({}, {})",
                            panel.bounds.width,
                            panel.bounds.height,
                            panel.bounds.x,
                            panel.bounds.y,
                        ),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![json!({ "name": "コマ 1", "detail": "0×0 / (0, 0)" })]);
    let panels_json = serde_json::to_string(&panels).unwrap_or_else(|_| "[]".to_string());
    let layer_count = active_panel.map(|panel| panel.layers.len()).unwrap_or(1);
    let active_layer_index = active_panel
        .map(|panel| panel.active_layer_index)
        .unwrap_or(0);
    let active_layer = active_panel.and_then(|panel| panel.layers.get(panel.active_layer_index));
    let page_count = document.work.pages.len();
    let active_page_number = document.active_page_index() + 1;
    let active_panel_number = document.active_panel_index() + 1;
    let active_page_panel_count = document.active_page_panel_count();
    let panel_count = document
        .work
        .pages
        .iter()
        .map(|page| page.panels.len())
        .sum::<usize>();
    let active_layer_name = active_layer
        .map(|layer| layer.name.clone())
        .unwrap_or_else(|| "<no layer>".to_string());
    let active_panel_label = format!(
        "ページ {} / コマ {}",
        active_page_number, active_panel_number
    );
    let active_panel_bounds = active_panel
        .map(|panel| {
            format!(
                "({}, {}) {}×{}",
                panel.bounds.x, panel.bounds.y, panel.bounds.width, panel.bounds.height,
            )
        })
        .unwrap_or_else(|| "(0, 0) 0×0".to_string());
    let active_pen = document.active_pen_preset().cloned().unwrap_or_default();
    let tool_catalog_json =
        serde_json::to_string(&document.tool_catalog).unwrap_or_else(|_| "[]".to_string());
    let active_tool_settings_json =
        serde_json::to_string(document.active_tool_settings()).unwrap_or_else(|_| "[]".to_string());

    json!({
        "document": {
            "title": document.work.title,
            "page_count": page_count,
            "panel_count": panel_count,
            "active_page_number": active_page_number,
            "active_page_panel_count": active_page_panel_count,
            "active_panel_index": document.active_panel_index(),
            "active_panel_number": active_panel_number,
            "active_panel_label": active_panel_label,
            "active_panel_bounds": active_panel_bounds,
            "active_layer_name": active_layer_name,
            "layer_count": layer_count,
            "active_layer_index": active_layer_index,
            "active_layer_blend_mode": active_layer.map(|layer| layer.blend_mode.as_str()).unwrap_or("normal"),
            "active_layer_visible": active_layer.map(|layer| layer.visible).unwrap_or(true),
            "active_layer_masked": active_layer.and_then(|layer| layer.mask.as_ref()).is_some(),
            "panels": panels,
            "panels_json": panels_json,
            "layers": layers,
            "layers_json": layers_json,
        },
        "tool": {
            "active": active_tool_name(document.active_tool),
            "active_id": document.active_tool_id,
            "active_label": active_tool_definition
                .as_ref()
                .map(|tool| tool.name.clone())
                .unwrap_or_else(|| active_tool_name(document.active_tool).to_string()),
            "catalog_json": tool_catalog_json,
            "active_settings_json": active_tool_settings_json,
            "active_provider_plugin_id": document.active_tool_provider_plugin_id().unwrap_or_default(),
            "active_drawing_plugin_id": document.active_tool_drawing_plugin_id().unwrap_or_default(),
            "supports_size": document.active_tool_settings().iter().any(|setting| setting.key == "size"),
            "supports_pressure_enabled": document.active_tool_settings().iter().any(|setting| setting.key == "pressure_enabled"),
            "supports_antialias": document.active_tool_settings().iter().any(|setting| setting.key == "antialias"),
            "supports_stabilization": document.active_tool_settings().iter().any(|setting| setting.key == "stabilization"),
            "pen_name": active_pen.name,
            "pen_id": active_pen.id,
            "pen_presets_json": serde_json::to_string(&document.pen_presets).unwrap_or_else(|_| "[]".to_string()),
            "pen_index": document.active_pen_index(),
            "pen_count": document.pen_presets.len(),
            "pen_size": document.active_pen_size,
            "pen_pressure_enabled": active_pen.pressure_enabled,
            "pen_antialias": active_pen.antialias,
            "pen_stabilization": active_pen.stabilization,
        },
        "color": {
            "active": document.active_color.hex_rgb(),
            "red": document.active_color.r,
            "green": document.active_color.g,
            "blue": document.active_color.b,
        },
        "history": { "can_undo": can_undo, "can_redo": can_redo },
        "jobs": { "active": active_jobs, "queued": 0, "status": if active_jobs == 0 { format!("idle / work={}", document.work.title) } else { format!("{active_jobs} job(s) running") } },
        "snapshot": { "storage_status": "pending" },
        "view": {
            "zoom": document.view_transform.zoom,
            "zoom_milli": (document.view_transform.zoom * 1000.0).round() as i32,
            "pan_x": document.view_transform.pan_x.round() as i32,
            "pan_y": document.view_transform.pan_y.round() as i32,
            "rotation_degrees": document.view_transform.rotation_degrees.round() as i32,
            "quarter_turns": ((document.view_transform.rotation_degrees / 90.0).round() as i32).rem_euclid(4),
            "flip_x": document.view_transform.flip_x,
            "flip_y": document.view_transform.flip_y,
        },
    })
}

/// 入力を解析して 16進文字列 色 に変換する。
///
/// 値を生成できない場合は `None` を返します。
pub(crate) fn parse_hex_color(input: &str) -> Option<ColorRgba8> {
    let hex = input.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(ColorRgba8::new(r, g, b, 0xff))
}

/// 入力を解析して ドキュメント サイズ に変換する。
///
/// 値を生成できない場合は `None` を返します。
pub(crate) fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized
        .split(|ch: char| ch == 'x' || ch.is_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0
        || height == 0
        || width > MAX_DOCUMENT_DIMENSION
        || height > MAX_DOCUMENT_DIMENSION
        || width.saturating_mul(height) > MAX_DOCUMENT_PIXELS
    {
        return None;
    }
    Some((width, height))
}
