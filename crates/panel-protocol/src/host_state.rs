//! host state セクションの型付き DTO (BL-142)。
//!
//! host state はホスト→パネルへ配る状態 JSON。トップレベルキー
//! (`document` / `tool` / `color` / `view` / `history` / `jobs` / `snapshot` /
//! `workspace`) ごとに 1 セクションを持つ。本モジュールは各セクションを
//! serde `Deserialize` 構造体として型定義し、**セクション JSON を 1 回取得して
//! serde で構造体へ落とす**経路を提供する。
//!
//! これにより、従来の「1 値 1 往復の文字列 path ABI」(`host_get_string("document.title")`
//! を値の数だけ呼ぶ) と「個別 getter と `layers_json` の二重供給」を解消する。
//! パネルは購読セクションを 1 度デシリアライズして必要な値をまとめて読む。
//!
//! セクションキーは [`section`] モジュールの定数で一元管理する。
//! 構造体のフィールド名 (serde キー) はホスト (`panel-runtime` の host state 構築) が
//! 出力する JSON キーと一致させる契約点である。

use serde::Deserialize;

/// host state のトップレベルセクションキー。
///
/// ホストの section registry とパネルの購読宣言 (meta.json) が同じキーを参照する
/// ための単一定義点。
pub mod section {
    /// 作品/コマ/レイヤー構造 ([`super::DocumentState`])。
    pub const DOCUMENT: &str = "document";
    /// ツール/ペン状態 ([`super::ToolState`])。
    pub const TOOL: &str = "tool";
    /// アクティブ色 ([`super::ColorState`])。
    pub const COLOR: &str = "color";
    /// ビュー変換 ([`super::ViewState`])。
    pub const VIEW: &str = "view";
    /// 編集履歴 (undo/redo 可否) ([`super::HistoryState`])。
    pub const HISTORY: &str = "history";
    /// 実行中/待機ジョブ件数 ([`super::JobsState`])。
    pub const JOBS: &str = "jobs";
    /// スナップショット件数/状態 ([`super::SnapshotState`])。
    pub const SNAPSHOT: &str = "snapshot";
    /// ワークスペースの UI パネル一覧 ([`super::WorkspaceState`])。
    pub const WORKSPACE: &str = "workspace";

    /// 全セクションキー (登録順)。
    pub const ALL: [&str; 8] = [DOCUMENT, TOOL, COLOR, VIEW, HISTORY, JOBS, SNAPSHOT, WORKSPACE];
}

/// 1 レイヤーの状態 (`document.layers_json` の要素)。
///
/// 並べ替えで意味が変わる index ではなく安定 id (`id`) で選択・操作を指定するため、
/// レイヤー id を含める (BL-148)。`id` はホストが供給するまでの間 `None` でも
/// デシリアライズできるよう `Option` とする。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LayerState {
    /// 安定レイヤー id (`RasterLayer.id`)。
    #[serde(default)]
    pub id: Option<u64>,
    pub name: String,
    pub blend_mode: String,
    pub visible: bool,
    pub masked: bool,
}

/// 1 コマの bounds (`document.komas_json` の要素、生データ)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct KomaState {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// `document` セクションの型付き DTO。
///
/// `layers` / `komas` はホストが JSON 文字列 (`layers_json` / `komas_json`) として
/// 配るため、[`DocumentState::layers`] / [`DocumentState::komas`] で 2 段目を
/// パースして取り出す。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DocumentState {
    pub title: String,
    pub page_count: i64,
    pub koma_count: i64,
    pub active_page_number: i64,
    pub active_page_koma_count: i64,
    pub active_koma_index: i64,
    pub active_koma_number: i64,
    pub active_koma_x: i64,
    pub active_koma_y: i64,
    pub active_koma_width: i64,
    pub active_koma_height: i64,
    pub active_layer_name: String,
    pub layer_count: i64,
    pub active_layer_index: i64,
    pub active_layer_blend_mode: String,
    pub active_layer_visible: bool,
    pub active_layer_masked: bool,
    /// レイヤー一覧 (UI 順 = 先頭が前面) の JSON 文字列。
    pub layers_json: String,
    /// コマ一覧の JSON 文字列 (生 bounds)。
    pub komas_json: String,
}

impl DocumentState {
    /// `layers_json` を [`LayerState`] 列へパースする。不正な JSON は空列。
    pub fn layers(&self) -> Vec<LayerState> {
        serde_json::from_str(&self.layers_json).unwrap_or_default()
    }

    /// `komas_json` を [`KomaState`] 列へパースする。不正な JSON は空列。
    pub fn komas(&self) -> Vec<KomaState> {
        serde_json::from_str(&self.komas_json).unwrap_or_default()
    }
}

/// `tool` セクションの型付き DTO。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ToolState {
    pub active: String,
    pub active_id: String,
    pub active_label: String,
    pub catalog_json: String,
    pub active_settings_json: String,
    pub active_child_tool_id: String,
    pub active_child_tool_label: String,
    pub child_tools_json: String,
    pub active_provider_plugin_id: String,
    pub active_drawing_plugin_id: String,
    pub supports_size: bool,
    pub supports_pressure_enabled: bool,
    pub supports_antialias: bool,
    pub supports_stabilization: bool,
    pub pen_name: String,
    pub pen_id: String,
    pub pen_presets_json: String,
    pub pen_index: i64,
    pub pen_count: i64,
    pub pen_size: i64,
    pub pen_pressure_enabled: bool,
    pub pen_antialias: bool,
    pub pen_stabilization: i64,
}

/// `color` セクションの型付き DTO。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ColorState {
    /// アクティブ色の `#RRGGBB` 表現。
    pub active: String,
    pub red: i64,
    pub green: i64,
    pub blue: i64,
}

/// `view` セクションの型付き DTO。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ViewState {
    pub zoom: f64,
    pub zoom_milli: i64,
    pub pan_x: i64,
    pub pan_y: i64,
    pub rotation_degrees: i64,
    pub quarter_turns: i64,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// `history` セクションの型付き DTO。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
}

/// `jobs` セクションの型付き DTO。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct JobsState {
    pub active: i64,
    pub queued: i64,
}

/// `snapshot` セクションの型付き DTO。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SnapshotState {
    pub count: i64,
    pub storage_status: String,
}

/// `workspace` セクションの型付き DTO。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkspaceState {
    /// `[{"id","title","visible"}, ...]` 形式の JSON 文字列。
    pub panels_json: String,
}

/// `workspace.panels_json` の 1 要素。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkspacePanelState {
    pub id: String,
    pub title: String,
    pub visible: bool,
}

impl WorkspaceState {
    /// `panels_json` を [`WorkspacePanelState`] 列へパースする。不正な JSON は空列。
    pub fn panels(&self) -> Vec<WorkspacePanelState> {
        serde_json::from_str(&self.panels_json).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn section_keys_are_unique_and_stable() {
        use std::collections::BTreeSet;
        let unique: BTreeSet<&str> = section::ALL.iter().copied().collect();
        assert_eq!(unique.len(), section::ALL.len(), "section キーが重複している");
        assert_eq!(
            section::ALL,
            ["document", "tool", "color", "view", "history", "jobs", "snapshot", "workspace"]
        );
    }

    #[test]
    fn document_state_deserializes_host_shape() {
        // panel-runtime の DocumentSection::build が出力する形 (要約)。
        let value = json!({
            "title": "Untitled",
            "page_count": 1,
            "koma_count": 1,
            "active_page_number": 1,
            "active_page_koma_count": 1,
            "active_koma_index": 0,
            "active_koma_number": 1,
            "active_koma_x": 0,
            "active_koma_y": 0,
            "active_koma_width": 320,
            "active_koma_height": 240,
            "active_layer_name": "Layer 1",
            "layer_count": 1,
            "active_layer_index": 0,
            "active_layer_blend_mode": "normal",
            "active_layer_visible": true,
            "active_layer_masked": false,
            "komas_json": "[{\"x\":0,\"y\":0,\"width\":320,\"height\":240}]",
            "layers_json": "[{\"name\":\"Layer 1\",\"blend_mode\":\"normal\",\"visible\":true,\"masked\":false}]",
        });

        let state: DocumentState = serde_json::from_value(value).expect("document state parses");
        assert_eq!(state.title, "Untitled");
        assert_eq!(state.active_koma_width, 320);

        // 2 段目のパース。
        let layers = state.layers();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].name, "Layer 1");
        assert!(layers[0].visible);
        // host が id をまだ供給していない場合は None。
        assert_eq!(layers[0].id, None);

        let komas = state.komas();
        assert_eq!(komas, vec![KomaState { x: 0, y: 0, width: 320, height: 240 }]);
    }

    #[test]
    fn layer_state_carries_stable_id_when_present() {
        // BL-148: 安定 id が供給されれば取り込む。
        let layers: Vec<LayerState> = serde_json::from_str(
            r#"[{"id":7,"name":"Ink","blend_mode":"multiply","visible":false,"masked":true}]"#,
        )
        .expect("layers parse");
        assert_eq!(layers[0].id, Some(7));
        assert_eq!(layers[0].blend_mode, "multiply");
        assert!(!layers[0].visible);
        assert!(layers[0].masked);
    }

    #[test]
    fn tool_state_deserializes_host_shape() {
        let value = json!({
            "active": "pen",
            "active_id": "builtin.pen",
            "active_label": "Pen",
            "catalog_json": "[]",
            "active_settings_json": "[]",
            "active_child_tool_id": "",
            "active_child_tool_label": "",
            "child_tools_json": "[]",
            "active_provider_plugin_id": "builtin.bitmap",
            "active_drawing_plugin_id": "builtin.bitmap",
            "supports_size": true,
            "supports_pressure_enabled": true,
            "supports_antialias": false,
            "supports_stabilization": false,
            "pen_name": "Round",
            "pen_id": "round",
            "pen_presets_json": "[]",
            "pen_index": 0,
            "pen_count": 3,
            "pen_size": 12,
            "pen_pressure_enabled": true,
            "pen_antialias": false,
            "pen_stabilization": 0,
        });
        let state: ToolState = serde_json::from_value(value).expect("tool state parses");
        assert_eq!(state.active, "pen");
        assert_eq!(state.pen_size, 12);
        assert!(state.supports_size);
    }

    #[test]
    fn small_sections_deserialize() {
        let color: ColorState =
            serde_json::from_value(json!({"active":"#0C2238","red":12,"green":34,"blue":56}))
                .expect("color parses");
        assert_eq!(color.active, "#0C2238");
        assert_eq!(color.blue, 56);

        let view: ViewState = serde_json::from_value(json!({
            "zoom": 1.5, "zoom_milli": 1500, "pan_x": 10, "pan_y": -5,
            "rotation_degrees": 90, "quarter_turns": 1, "flip_x": true, "flip_y": false,
        }))
        .expect("view parses");
        assert_eq!(view.zoom_milli, 1500);
        assert!(view.flip_x);

        let history: HistoryState =
            serde_json::from_value(json!({"can_undo": true, "can_redo": false}))
                .expect("history parses");
        assert!(history.can_undo);
        assert!(!history.can_redo);

        let jobs: JobsState =
            serde_json::from_value(json!({"active": 2, "queued": 0})).expect("jobs parses");
        assert_eq!(jobs.active, 2);

        let snapshot: SnapshotState =
            serde_json::from_value(json!({"count": 0, "storage_status": "empty"}))
                .expect("snapshot parses");
        assert_eq!(snapshot.storage_status, "empty");
    }

    #[test]
    fn workspace_state_parses_panel_list() {
        let state: WorkspaceState = serde_json::from_value(json!({
            "panels_json": "[{\"id\":\"builtin.layers\",\"title\":\"Layers\",\"visible\":true}]",
        }))
        .expect("workspace parses");
        let panels = state.panels();
        assert_eq!(
            panels,
            vec![WorkspacePanelState {
                id: "builtin.layers".to_string(),
                title: "Layers".to_string(),
                visible: true,
            }]
        );
    }
}
