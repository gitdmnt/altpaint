//! `panel.meta.json` の deserialize 型。
//!
//! Phase 11: `default_size` を必須フィールドとして導入。
//! BuiltinPanelPlugin / HtmlPanelPlugin の双方で共有する。

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PanelMeta {
    pub id: String,
    pub title: String,
    pub default_size: PanelSizeMeta,
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
