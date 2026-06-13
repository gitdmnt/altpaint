//! `panel.meta.json` の deserialize 型。
//!
//! Phase 11: `default_size` を必須フィールドとして導入。
//! HtmlWasmPanel と同梱パネル loader の双方で共有する。

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PanelMeta {
    pub id: String,
    pub title: String,
    pub default_size: PanelSizeMeta,
    /// 購読する host state セクションキー (BL-093)。
    ///
    /// `["document", "tool", ...]` のように宣言すると、いずれかの購読セクションの
    /// revision が変化した時のみ再 render される。省略 (空) の場合は全セクションを
    /// 購読しているとみなし、何らかの変化があれば再 render される。
    #[serde(default)]
    pub subscribes: Vec<String>,
    /// 既定ワークスペース配置 (BL-095)。
    ///
    /// パネルがどの画面隅に・どの初期位置に・どのサイズで・表示/非表示で配置されるかを
    /// パネル自身が宣言する。panel-workspace / desktop の水平層はこの宣言を購読して
    /// 配置を解決し、ビルトイン ID をハードコードしない。
    #[serde(default)]
    pub layout: PanelLayoutMeta,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PanelLayoutMeta {
    /// 配置基準の画面隅 (`top-left` / `top-right` / `bottom-left` / `bottom-right`)。
    /// 省略時は `None` (フォールバックの index ベース配置に委ねる)。
    #[serde(default)]
    pub anchor: Option<String>,
    /// アンカー基準のオフセット。省略時は `None` (index ベースフォールバック)。
    #[serde(default)]
    pub position: Option<PanelPositionMeta>,
    /// 起動直後に非表示にするか (旧 `HIDDEN_BY_DEFAULT_PANEL_IDS`)。
    #[serde(default)]
    pub hidden_by_default: bool,
    /// 常に表示し、ユーザーが非表示にできないか (旧 `WORKSPACE_PANEL_ID` 特別扱い)。
    #[serde(default)]
    pub always_visible: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PanelPositionMeta {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PanelSizeMeta {
    pub width: u32,
    pub height: u32,
}

impl PanelSizeMeta {
    pub fn as_tuple(self) -> (u32, u32) {
        (self.width.max(1), self.height.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_meta() {
        let raw = r#"{
            "id": "builtin.test",
            "title": "Test",
            "default_size": { "width": 280, "height": 320 }
        }"#;
        let meta: PanelMeta = serde_json::from_str(raw).expect("should parse");
        assert_eq!(meta.id, "builtin.test");
        assert_eq!(meta.title, "Test");
        assert_eq!(meta.default_size.as_tuple(), (280, 320));
        // subscribes は任意 (省略時は空 = 全セクション購読)。
        assert!(meta.subscribes.is_empty());
    }

    #[test]
    fn parses_subscribes() {
        let raw = r#"{
            "id": "builtin.test",
            "title": "Test",
            "default_size": { "width": 280, "height": 320 },
            "subscribes": ["document", "tool"]
        }"#;
        let meta: PanelMeta = serde_json::from_str(raw).expect("should parse");
        assert_eq!(meta.subscribes, vec!["document", "tool"]);
    }

    #[test]
    fn layout_defaults_to_empty_when_missing() {
        let raw = r#"{
            "id": "builtin.test",
            "title": "Test",
            "default_size": { "width": 280, "height": 320 }
        }"#;
        let meta: PanelMeta = serde_json::from_str(raw).expect("should parse");
        assert!(meta.layout.anchor.is_none());
        assert!(meta.layout.position.is_none());
        assert!(!meta.layout.hidden_by_default);
        assert!(!meta.layout.always_visible);
    }

    #[test]
    fn parses_layout_defaults() {
        let raw = r#"{
            "id": "builtin.test",
            "title": "Test",
            "default_size": { "width": 280, "height": 320 },
            "layout": {
                "anchor": "top-right",
                "position": { "x": 24, "y": 72 },
                "hidden_by_default": true,
                "always_visible": false
            }
        }"#;
        let meta: PanelMeta = serde_json::from_str(raw).expect("should parse");
        assert_eq!(meta.layout.anchor.as_deref(), Some("top-right"));
        let pos = meta.layout.position.expect("position present");
        assert_eq!((pos.x, pos.y), (24, 72));
        assert!(meta.layout.hidden_by_default);
        assert!(!meta.layout.always_visible);
    }

    #[test]
    fn fails_without_default_size() {
        let raw = r#"{ "id": "builtin.test", "title": "Test" }"#;
        let result: Result<PanelMeta, _> = serde_json::from_str(raw);
        assert!(
            result.is_err(),
            "default_size must be required, got: {result:?}"
        );
    }

    #[test]
    fn fails_with_partial_size() {
        let raw = r#"{
            "id": "builtin.test",
            "title": "Test",
            "default_size": { "width": 280 }
        }"#;
        let result: Result<PanelMeta, _> = serde_json::from_str(raw);
        assert!(result.is_err(), "height is required");
    }
}
