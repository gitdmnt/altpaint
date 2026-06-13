use document_model::Document;
use editor_state::PenPreset;
use serde_json::{Value, json};

/// 高価な JSON シリアライズ結果を再利用するためのキャッシュ。
///
/// ズーム/パンなど view のみが変わる操作では pen_presets / tool_catalog 等の
/// 再シリアライズをスキップし、build_host_state のコストを大幅に削減する。
///
/// BL-031 暫定対応: 無効化キーは「件数 + active index」ではなく内容そのもの
/// (ペンプリセットはクローンの等値比較、レイヤー/コマは内容を毎回再構築) を使う。
/// revision ベースの完全なキャッシュ化は BL-093 (B6) で行う。
#[derive(Default)]
pub struct HostStateCache {
    /// 初回呼び出しで必ず全フィールドを構築するためのフラグ。
    initialized: bool,

    // pen プリセット (内容の等値比較で無効化。tip ビットマップを含む
    // 重いシリアライズを内容が変わらない限りスキップする)
    pen_presets: Vec<PenPreset>,
    pen_presets_json: String,

    // ツールカタログ・設定
    active_tool_id: String,
    tool_catalog_json: String,
    child_tools_json: String,
    active_tool_settings_json: String,
}

/// `build_host_state` 呼出側が事前に組み立てた workspace パネル一覧 JSON のデフォルト。
/// 未設定時は空配列を返す。
pub const EMPTY_WORKSPACE_PANELS_JSON: &str = "[]";

/// `Document` から導出できない host 側の付随状態 (BL-090)。
///
/// 履歴の undo/redo 可否・実行中ジョブ件数・スナップショット件数を 1 つの DTO に
/// まとめる。旧来 `build_host_state` / `HtmlWasmPanel::update` / `sync_dirty_panels`
/// が個別引数 (`can_undo` / `can_redo` / `active_jobs` / `snapshot_count`) で
/// 受けていたものを集約する。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub active_jobs: usize,
    pub snapshot_count: usize,
}

/// キャッシュを利用してhost state を構築する。
///
/// 変化していないフィールドのシリアライズを再利用することで、
/// ズーム/パン操作時のコストを大幅に削減する。
///
/// `workspace_panels_json` には呼出元が事前に組み立てた
/// `[{"id","title","visible"}, ...]` 形式の JSON 文字列を渡す。
/// `workspace.panels_json` キーに格納され、`host::workspace::panels_json()` から参照される。
pub fn build_host_state(
    document: &Document,
    host_state: HostState,
    cache: &mut HostStateCache,
    workspace_panels_json: &str,
) -> Value {
    let HostState {
        can_undo,
        can_redo,
        active_jobs,
        snapshot_count,
    } = host_state;
    let active_tool_definition = document.session.active_tool_definition().cloned();
    let active_page = document.active_page();
    let active_koma = document.active_koma();

    let force_rebuild = !cache.initialized;

    // ---- pen presets (内容が変化しなければキャッシュを再利用) ----
    // BL-031: 件数 + active index ではプリセット内容の編集を検知できないため、
    // 内容の等値比較で無効化する。
    let pen_count = document.session.pen_presets.len();
    let active_pen_index = document.session.active_pen_index();
    if force_rebuild || cache.pen_presets != document.session.pen_presets {
        cache.pen_presets_json =
            serde_json::to_string(&document.session.pen_presets).unwrap_or_else(|_| "[]".to_string());
        cache.pen_presets = document.session.pen_presets.clone();
    }

    // ---- ツールカタログ・設定 ----
    let active_tool_id = document.session.active_tool_id.as_str();
    if force_rebuild || cache.active_tool_id != active_tool_id {
        cache.tool_catalog_json =
            serde_json::to_string(&document.session.tool_catalog).unwrap_or_else(|_| "[]".to_string());
        cache.active_tool_settings_json =
            serde_json::to_string(document.session.active_tool_settings()).unwrap_or_else(|_| "[]".to_string());
        cache.child_tools_json = active_tool_definition
            .as_ref()
            .map(|t| serde_json::to_string(&t.children).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());
        cache.active_tool_id = active_tool_id.to_string();
    }

    // ---- レイヤー一覧 ----
    // BL-031: 件数 + active index では名前・visible・blend_mode・mask の変更を
    // 検知できないため、内容 (name, visible, blend_mode, masked) を毎回構築する。
    let layer_count = active_koma.map(|p| p.layers.len()).unwrap_or(1);
    let active_layer_index = active_koma.map(|p| p.active_layer_index).unwrap_or(0);
    let layers = active_koma
        .map(|koma| {
            // index 0 が最下層のため逆順で返す（UI の先頭 = 前面レイヤー）
            koma
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
    let layers_json = serde_json::to_string(&layers).unwrap_or_else(|_| "[]".to_string());

    // ---- コマ一覧 ----
    // BL-031: 件数 + active index では bounds の変更を検知できないため、
    // 内容 (bounds 由来の detail を含む) を毎回構築する。
    let komas = active_page
        .map(|page| {
            page.komas
                .iter()
                .enumerate()
                .map(|(index, koma)| {
                    json!({
                        "name": format!("コマ {}", index + 1),
                        "detail": format!(
                            "{}×{} / ({}, {})",
                            koma.bounds.width,
                            koma.bounds.height,
                            koma.bounds.x,
                            koma.bounds.y,
                        ),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![json!({ "name": "コマ 1", "detail": "0×0 / (0, 0)" })]);
    let komas_json = serde_json::to_string(&komas).unwrap_or_else(|_| "[]".to_string());

    cache.initialized = true;

    // ---- 残りのフィールド（毎回計算するが軽量） ----
    let active_layer = active_koma.and_then(|p| p.layers.get(p.active_layer_index));
    let page_count = document.work.pages.len();
    let active_page_number = document.active_page_index() + 1;
    let active_koma_number = document.active_koma_index() + 1;
    let active_page_koma_count = document.active_page_koma_count();
    let koma_count = document
        .work
        .pages
        .iter()
        .map(|page| page.komas.len())
        .sum::<usize>();
    let active_layer_name = active_layer
        .map(|layer| layer.name.clone())
        .unwrap_or_else(|| "<no layer>".to_string());
    let active_koma_label = format!(
        "ページ {} / コマ {}",
        active_page_number, active_koma_number
    );
    let active_koma_bounds = active_koma
        .map(|koma| {
            format!(
                "({}, {}) {}×{}",
                koma.bounds.x, koma.bounds.y, koma.bounds.width, koma.bounds.height,
            )
        })
        .unwrap_or_else(|| "(0, 0) 0×0".to_string());
    let active_pen = document.session.active_pen_preset().cloned().unwrap_or_default();
    let active_child_tool_id = &document.session.active_child_tool_id;
    let active_child_tool = document.session.active_child_tool_definition();
    let active_child_tool_label = active_child_tool
        .map(|c| c.name.clone())
        .unwrap_or_default();

    // UI インデックス: UI の先頭が前面なので実モデル index を逆変換する
    let active_layer_ui_index = if layer_count > 0 {
        layer_count.saturating_sub(1).saturating_sub(active_layer_index)
    } else {
        0
    };

    json!({
        "document": {
            "title": document.work.title,
            "page_count": page_count,
            "koma_count": koma_count,
            "active_page_number": active_page_number,
            "active_page_koma_count": active_page_koma_count,
            "active_koma_index": document.active_koma_index(),
            "active_koma_number": active_koma_number,
            "active_koma_label": active_koma_label,
            "active_koma_bounds": active_koma_bounds,
            "active_layer_name": active_layer_name,
            "layer_count": layer_count,
            "active_layer_index": active_layer_ui_index,
            "active_layer_blend_mode": active_layer.map(|layer| layer.blend_mode.as_str()).unwrap_or("normal"),
            "active_layer_visible": active_layer.map(|layer| layer.visible).unwrap_or(true),
            "active_layer_masked": active_layer.and_then(|layer| layer.mask.as_ref()).is_some(),
            "komas_json": komas_json,
            "layers_json": layers_json,
        },
        "tool": {
            "active": document.session.active_tool().as_str(),
            "active_id": &document.session.active_tool_id,
            "active_label": active_tool_definition
                .as_ref()
                .map(|tool| tool.name.clone())
                .unwrap_or_else(|| document.session.active_tool().as_str().to_string()),
            "catalog_json": cache.tool_catalog_json,
            "active_settings_json": cache.active_tool_settings_json,
            "active_child_tool_id": active_child_tool_id,
            "active_child_tool_label": active_child_tool_label,
            "child_tools_json": cache.child_tools_json,
            "active_provider_plugin_id": document.session.active_tool_provider_plugin_id().unwrap_or_default(),
            "active_drawing_plugin_id": document.session.active_tool_drawing_plugin_id().unwrap_or_default(),
            "supports_size": document.session.active_tool_settings().iter().any(|setting| setting.key == "size"),
            "supports_pressure_enabled": document.session.active_tool_settings().iter().any(|setting| setting.key == "pressure_enabled"),
            "supports_antialias": document.session.active_tool_settings().iter().any(|setting| setting.key == "antialias"),
            "supports_stabilization": document.session.active_tool_settings().iter().any(|setting| setting.key == "stabilization"),
            "pen_name": active_pen.name,
            "pen_id": active_pen.id,
            "pen_presets_json": cache.pen_presets_json,
            "pen_index": active_pen_index,
            "pen_count": pen_count,
            "pen_size": document.session.active_pen_size,
            "pen_pressure_enabled": active_pen.pressure_enabled,
            "pen_antialias": active_pen.antialias,
            "pen_stabilization": active_pen.stabilization,
        },
        "color": {
            "active": document.session.active_color.hex_rgb(),
            "red": document.session.active_color.r,
            "green": document.session.active_color.g,
            "blue": document.session.active_color.b,
        },
        "history": { "can_undo": can_undo, "can_redo": can_redo },
        "jobs": { "active": active_jobs, "queued": 0, "status": if active_jobs == 0 { format!("idle / work={}", document.work.title) } else { format!("{active_jobs} job(s) running") } },
        "snapshot": {
            "count": snapshot_count,
            "storage_status": if snapshot_count == 0 { "empty" } else { "ok" },
        },
        "view": {
            "zoom": document.session.view_transform.zoom,
            "zoom_milli": (document.session.view_transform.zoom * 1000.0).round() as i32,
            "pan_x": document.session.view_transform.pan_x.round() as i32,
            "pan_y": document.session.view_transform.pan_y.round() as i32,
            "rotation_degrees": document.session.view_transform.rotation_degrees.round() as i32,
            "quarter_turns": ((document.session.view_transform.rotation_degrees / 90.0).round() as i32).rem_euclid(4),
            "flip_x": document.session.view_transform.flip_x,
            "flip_y": document.session.view_transform.flip_y,
        },
        "workspace": {
            "panels_json": workspace_panels_json,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_model::{Document, KomaBounds};
    use raster::BlendMode;

    fn build(document: &Document, cache: &mut HostStateCache) -> Value {
        build_host_state(
            document,
            HostState::default(),
            cache,
            EMPTY_WORKSPACE_PANELS_JSON,
        )
    }

    /// BL-031: レイヤー名変更が次回 build の layers_json に反映される (stale 配信回帰)。
    #[test]
    fn layer_rename_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut cache = HostStateCache::default();
        let _ = build(&document, &mut cache);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .layers[0]
            .name = "Renamed Layer".to_string();

        let second = build(&document, &mut cache);
        let layers_json = second["document"]["layers_json"]
            .as_str()
            .expect("layers_json is string");
        assert!(
            layers_json.contains("Renamed Layer"),
            "layers_json must reflect the renamed layer: {layers_json}"
        );
    }

    /// BL-031: レイヤー visible 変更が次回 build の layers_json に反映される。
    #[test]
    fn layer_visibility_change_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut cache = HostStateCache::default();
        let _ = build(&document, &mut cache);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .layers[0]
            .visible = false;

        let second = build(&document, &mut cache);
        let layers_json = second["document"]["layers_json"]
            .as_str()
            .expect("layers_json is string");
        assert!(
            layers_json.contains(r#""visible":false"#),
            "layers_json must reflect visibility change: {layers_json}"
        );
    }

    /// BL-031: レイヤー blend_mode 変更が次回 build の layers_json に反映される。
    #[test]
    fn layer_blend_mode_change_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut cache = HostStateCache::default();
        let _ = build(&document, &mut cache);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .layers[0]
            .blend_mode = BlendMode::Multiply;

        let second = build(&document, &mut cache);
        let layers_json = second["document"]["layers_json"]
            .as_str()
            .expect("layers_json is string");
        assert!(
            layers_json.contains("multiply"),
            "layers_json must reflect blend mode change: {layers_json}"
        );
    }

    /// BL-031: コマ bounds 変更が次回 build の komas_json に反映される。
    #[test]
    fn koma_bounds_change_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut cache = HostStateCache::default();
        let _ = build(&document, &mut cache);

        document
            .active_koma_mut()
            .expect("active koma exists")
            .bounds = KomaBounds {
            x: 7,
            y: 9,
            width: 123,
            height: 45,
        };

        let second = build(&document, &mut cache);
        let komas_json = second["document"]["komas_json"]
            .as_str()
            .expect("komas_json is string");
        assert!(
            komas_json.contains("123×45 / (7, 9)"),
            "komas_json must reflect bounds change: {komas_json}"
        );
    }

    /// BL-031: ペンプリセットの内容編集 (件数・active index 不変) が次回 build の
    /// pen_presets_json に反映される。
    #[test]
    fn pen_preset_content_edit_is_reflected_in_next_host_state() {
        let mut document = Document::default();
        let mut cache = HostStateCache::default();
        let _ = build(&document, &mut cache);

        document.session.pen_presets[0].name = "Edited Pen".to_string();

        let second = build(&document, &mut cache);
        let pen_presets_json = second["tool"]["pen_presets_json"]
            .as_str()
            .expect("pen_presets_json is string");
        assert!(
            pen_presets_json.contains("Edited Pen"),
            "pen_presets_json must reflect preset content edit: {pen_presets_json}"
        );
    }

    /// build_host_state が `workspace.panels_json` を登録順 + visible 反映で出力する。
    #[test]
    fn host_state_emits_workspace_panels_json_in_registered_order() {
        let document = Document::default();
        let mut cache = HostStateCache::default();
        let workspace_panels_json = r#"[{"id":"builtin.foo","title":"Foo","visible":true},{"id":"builtin.bar","title":"Bar","visible":false}]"#;

        let host_state = build_host_state(
            &document,
            HostState::default(),
            &mut cache,
            workspace_panels_json,
        );

        let emitted = host_state
            .get("workspace")
            .and_then(|v| v.get("panels_json"))
            .and_then(|v| v.as_str())
            .expect("workspace.panels_json must be present");
        assert_eq!(emitted, workspace_panels_json);
    }

    /// `workspace_panels_json` が空 (デフォルト) の場合は空配列文字列がそのまま出る。
    #[test]
    fn host_state_emits_empty_workspace_panels_json_when_absent() {
        let document = Document::default();
        let mut cache = HostStateCache::default();

        let host_state = build_host_state(
            &document,
            HostState::default(),
            &mut cache,
            EMPTY_WORKSPACE_PANELS_JSON,
        );

        let emitted = host_state
            .get("workspace")
            .and_then(|v| v.get("panels_json"))
            .and_then(|v| v.as_str())
            .expect("workspace.panels_json must be present");
        assert_eq!(emitted, "[]");
    }
}

