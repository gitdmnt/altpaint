use app_core::{Document, ToolKind};
use serde_json::{Value, json};

/// 高価な JSON シリアライズ結果を再利用するためのキャッシュ。
///
/// ズーム/パンなど view のみが変わる操作では pen_presets / tool_catalog 等の
/// 再シリアライズをスキップし、build_host_snapshot のコストを大幅に削減する。
#[derive(Default)]
pub struct HostSnapshotCache {
    /// 初回呼び出しで必ず全フィールドを構築するためのフラグ。
    initialized: bool,

    // pen プリセット
    pen_count: usize,
    active_pen_index: usize,
    pen_presets_json: String,

    // ツールカタログ・設定
    active_tool_id: String,
    tool_catalog_json: String,
    child_tools_json: String,
    active_tool_settings_json: String,

    // レイヤー一覧
    layer_count: usize,
    active_layer_index: usize,
    layers_json: String,

    // パネル一覧
    page_panel_count: usize,
    active_panel_index: usize,
    panels_json: String,
}

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

/// キャッシュを利用してホストスナップショットを構築する。
///
/// 変化していないフィールドのシリアライズを再利用することで、
/// ズーム/パン操作時のコストを大幅に削減する。
pub fn build_host_snapshot_cached(
    document: &Document,
    can_undo: bool,
    can_redo: bool,
    active_jobs: usize,
    snapshot_count: usize,
    cache: &mut HostSnapshotCache,
) -> Value {
    let active_tool_definition = document.active_tool_definition().cloned();
    let active_page = document.active_page();
    let active_panel = document.active_panel();

    let force_rebuild = !cache.initialized;

    // ---- pen presets (変化しなければキャッシュを再利用) ----
    let pen_count = document.pen_presets.len();
    let active_pen_index = document.active_pen_index();
    if force_rebuild || cache.pen_count != pen_count || cache.active_pen_index != active_pen_index {
        cache.pen_presets_json =
            serde_json::to_string(&document.pen_presets).unwrap_or_else(|_| "[]".to_string());
        cache.pen_count = pen_count;
        cache.active_pen_index = active_pen_index;
    }

    // ---- ツールカタログ・設定 ----
    let active_tool_id = document.active_tool_id.as_str();
    if force_rebuild || cache.active_tool_id != active_tool_id {
        cache.tool_catalog_json =
            serde_json::to_string(&document.tool_catalog).unwrap_or_else(|_| "[]".to_string());
        cache.active_tool_settings_json =
            serde_json::to_string(document.active_tool_settings()).unwrap_or_else(|_| "[]".to_string());
        cache.child_tools_json = active_tool_definition
            .as_ref()
            .map(|t| serde_json::to_string(&t.children).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());
        cache.active_tool_id = active_tool_id.to_string();
    }

    // ---- レイヤー一覧 ----
    let layer_count = active_panel.map(|p| p.layers.len()).unwrap_or(1);
    let active_layer_index = active_panel.map(|p| p.active_layer_index).unwrap_or(0);
    if force_rebuild || cache.layer_count != layer_count || cache.active_layer_index != active_layer_index {
        let layers = active_panel
            .map(|panel| {
                // index 0 が最下層のため逆順で返す（UI の先頭 = 前面レイヤー）
                panel
                    .layers
                    .iter()
                    .rev()
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
        cache.layers_json =
            serde_json::to_string(&layers).unwrap_or_else(|_| "[]".to_string());
        cache.layer_count = layer_count;
        cache.active_layer_index = active_layer_index;
    }

    // ---- パネル一覧 ----
    let page_panel_count = active_page.map(|p| p.panels.len()).unwrap_or(1);
    let active_panel_index = document.active_panel_index();
    if force_rebuild
        || cache.page_panel_count != page_panel_count
        || cache.active_panel_index != active_panel_index
    {
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
        cache.panels_json =
            serde_json::to_string(&panels).unwrap_or_else(|_| "[]".to_string());
        cache.page_panel_count = page_panel_count;
        cache.active_panel_index = active_panel_index;
    }

    cache.initialized = true;

    // ---- 残りのフィールド（毎回計算するが軽量） ----
    let active_layer = active_panel.and_then(|p| p.layers.get(p.active_layer_index));
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
    let active_child_tool_id = &document.active_child_tool_id;
    let active_child_tool = document.active_child_tool_definition();
    let active_child_tool_label = active_child_tool
        .map(|c| c.name.clone())
        .unwrap_or_default();

    // UI インデックス: UI の先頭が前面なので実モデル index を逆変換する
    let active_layer_ui_index = if layer_count > 0 {
        layer_count.saturating_sub(1).saturating_sub(active_layer_index)
    } else {
        0
    };

    // layers / panels を JSON Value として埋め込む（キャッシュ文字列から再パース不要）
    let layers_value: Value =
        serde_json::from_str(&cache.layers_json).unwrap_or(Value::Array(vec![]));
    let panels_value: Value =
        serde_json::from_str(&cache.panels_json).unwrap_or(Value::Array(vec![]));

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
            "active_layer_index": active_layer_ui_index,
            "active_layer_blend_mode": active_layer.map(|layer| layer.blend_mode.as_str()).unwrap_or("normal"),
            "active_layer_visible": active_layer.map(|layer| layer.visible).unwrap_or(true),
            "active_layer_masked": active_layer.and_then(|layer| layer.mask.as_ref()).is_some(),
            "panels": panels_value,
            "panels_json": cache.panels_json,
            "layers": layers_value,
            "layers_json": cache.layers_json,
        },
        "tool": {
            "active": active_tool_name(document.active_tool),
            "active_id": &document.active_tool_id,
            "active_label": active_tool_definition
                .as_ref()
                .map(|tool| tool.name.clone())
                .unwrap_or_else(|| active_tool_name(document.active_tool).to_string()),
            "catalog_json": cache.tool_catalog_json,
            "active_settings_json": cache.active_tool_settings_json,
            "active_child_tool_id": active_child_tool_id,
            "active_child_tool_label": active_child_tool_label,
            "child_tools_json": cache.child_tools_json,
            "active_provider_plugin_id": document.active_tool_provider_plugin_id().unwrap_or_default(),
            "active_drawing_plugin_id": document.active_tool_drawing_plugin_id().unwrap_or_default(),
            "supports_size": document.active_tool_settings().iter().any(|setting| setting.key == "size"),
            "supports_pressure_enabled": document.active_tool_settings().iter().any(|setting| setting.key == "pressure_enabled"),
            "supports_antialias": document.active_tool_settings().iter().any(|setting| setting.key == "antialias"),
            "supports_stabilization": document.active_tool_settings().iter().any(|setting| setting.key == "stabilization"),
            "pen_name": active_pen.name,
            "pen_id": active_pen.id,
            "pen_presets_json": cache.pen_presets_json,
            "pen_index": active_pen_index,
            "pen_count": pen_count,
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
        "snapshot": {
            "count": snapshot_count,
            "storage_status": if snapshot_count == 0 { "empty" } else { "ok" },
        },
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

