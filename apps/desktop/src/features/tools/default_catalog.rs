//! desktop 既定のツールカタログ (provisional)。
//!
//! `tools/` ディレクトリが存在しない・空である場合のフォールバックとして、
//! desktop 固有のプラグイン配置 (`provider_plugin_id`) を含むツール定義を提供する。
//! editor-state はプラグイン配置文字列を知らないため、この既定値は desktop が所有する。
//!
//! B7 で features/tools へ最終配置する予定。

use editor_state::{ToolDefinition, ToolKind, ToolSettingDefinition};

const DRAWING_PLUGIN_ID: &str = "builtin.bitmap";

/// desktop 既定のツールカタログを返す (provider_plugin_id を含む)。
pub(crate) fn desktop_default_tool_catalog() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            id: "builtin.pen".to_string(),
            name: "Pen".to_string(),
            kind: ToolKind::Pen,
            provider_plugin_id: "plugins/default-pens-plugin".to_string(),
            drawing_plugin_id: DRAWING_PLUGIN_ID.to_string(),
            settings: vec![
                ToolSettingDefinition::slider("size", "太さ", 1, 10_000),
                ToolSettingDefinition::checkbox("pressure_enabled", "筆圧"),
                ToolSettingDefinition::checkbox("antialias", "なめらか"),
                ToolSettingDefinition::slider("stabilization", "手ぶれ補正", 0, 100),
            ],
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.eraser".to_string(),
            name: "Eraser".to_string(),
            kind: ToolKind::Eraser,
            provider_plugin_id: "plugins/default-erasers-plugin".to_string(),
            drawing_plugin_id: DRAWING_PLUGIN_ID.to_string(),
            settings: vec![
                ToolSettingDefinition::slider("size", "太さ", 1, 10_000),
                ToolSettingDefinition::checkbox("antialias", "なめらか"),
                ToolSettingDefinition::slider("stabilization", "手ぶれ補正", 0, 100),
            ],
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.bucket".to_string(),
            name: "Bucket".to_string(),
            kind: ToolKind::Bucket,
            provider_plugin_id: "plugins/default-fill-tools-plugin".to_string(),
            drawing_plugin_id: DRAWING_PLUGIN_ID.to_string(),
            settings: Vec::new(),
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.lasso-bucket".to_string(),
            name: "Lasso Bucket".to_string(),
            kind: ToolKind::LassoBucket,
            provider_plugin_id: "plugins/default-fill-tools-plugin".to_string(),
            drawing_plugin_id: DRAWING_PLUGIN_ID.to_string(),
            settings: Vec::new(),
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.koma-rect".to_string(),
            name: "Koma Rect".to_string(),
            kind: ToolKind::KomaRect,
            provider_plugin_id: "plugins/default-koma-tools-plugin".to_string(),
            drawing_plugin_id: DRAWING_PLUGIN_ID.to_string(),
            settings: Vec::new(),
            children: Vec::new(),
        },
    ]
}
