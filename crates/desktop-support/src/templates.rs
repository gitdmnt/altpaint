use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanvasTemplate {
    pub id: String,
    pub label: String,
    pub width: usize,
    pub height: usize,
}

impl CanvasTemplate {
    /// サイズ string 用の表示文字列を組み立てる。
    pub fn size_string(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    /// Dropdown オプション 用の表示文字列を組み立てる。
    pub fn dropdown_option(&self) -> String {
        format!("{}:{}", self.size_string(), self.label)
    }
}

/// 既定の キャンバス テンプレート パス を返す。
pub fn default_canvas_template_path() -> PathBuf {
    PathBuf::from("canvas-templates.json")
}

/// 既定の キャンバス templates を返す。
pub fn default_canvas_templates() -> Vec<CanvasTemplate> {
    vec![
        CanvasTemplate {
            id: "a4-350dpi".to_string(),
            label: "A4 350dpi (2894×4093)".to_string(),
            width: 2894,
            height: 4093,
        },
        CanvasTemplate {
            id: "a4-300dpi".to_string(),
            label: "A4 300dpi (2480×3508)".to_string(),
            width: 2480,
            height: 3508,
        },
        CanvasTemplate {
            id: "square-2048".to_string(),
            label: "Square 2048 (2048×2048)".to_string(),
            width: 2048,
            height: 2048,
        },
        CanvasTemplate {
            id: "hd-1080p".to_string(),
            label: "HD Landscape (1920×1080)".to_string(),
            width: 1920,
            height: 1080,
        },
    ]
}

/// 入力を解析して キャンバス templates に変換し、失敗時はエラーを返す。
pub fn load_canvas_templates(path: impl AsRef<Path>) -> Vec<CanvasTemplate> {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return default_canvas_templates(),
    };
    serde_json::from_slice::<Vec<CanvasTemplate>>(&bytes)
        .ok()
        .filter(|templates| !templates.is_empty())
        .unwrap_or_else(default_canvas_templates)
}

/// 現在の値を キャンバス templates へ変換する。
pub fn save_canvas_templates(
    path: impl AsRef<Path>,
    templates: &[CanvasTemplate],
) -> std::io::Result<()> {
    let serialized = serde_json::to_vec_pretty(templates)?;
    std::fs::write(path, serialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既定 templates include a4 350dpi が期待どおりに動作することを検証する。
    #[test]
    fn default_templates_include_a4_350dpi() {
        let templates = default_canvas_templates();
        assert!(templates.iter().any(|template| {
            template.id == "a4-350dpi" && template.width == 2894 && template.height == 4093
        }));
    }

    /// dropdown オプション embeds サイズ and ラベル が期待どおりに動作することを検証する。
    #[test]
    fn dropdown_option_embeds_size_and_label() {
        let template = CanvasTemplate {
            id: "demo".to_string(),
            label: "Demo".to_string(),
            width: 320,
            height: 240,
        };

        assert_eq!(template.dropdown_option(), "320x240:Demo");
    }
}
