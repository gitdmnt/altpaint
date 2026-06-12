use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanvasSizePreset {
    pub id: String,
    pub label: String,
    pub width: usize,
    pub height: usize,
}

impl CanvasSizePreset {
    pub fn size_string(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    pub fn dropdown_option(&self) -> String {
        format!("{}:{}", self.size_string(), self.label)
    }
}

pub fn default_canvas_size_preset_path() -> PathBuf {
    PathBuf::from("canvas-templates.json")
}

pub fn default_canvas_size_presets() -> Vec<CanvasSizePreset> {
    vec![
        CanvasSizePreset {
            id: "a4-350dpi".to_string(),
            label: "A4 350dpi (2894×4093)".to_string(),
            width: 2894,
            height: 4093,
        },
        CanvasSizePreset {
            id: "a4-300dpi".to_string(),
            label: "A4 300dpi (2480×3508)".to_string(),
            width: 2480,
            height: 3508,
        },
        CanvasSizePreset {
            id: "square-2048".to_string(),
            label: "Square 2048 (2048×2048)".to_string(),
            width: 2048,
            height: 2048,
        },
        CanvasSizePreset {
            id: "hd-1080p".to_string(),
            label: "HD Landscape (1920×1080)".to_string(),
            width: 1920,
            height: 1080,
        },
    ]
}

pub fn load_canvas_size_presets(path: impl AsRef<Path>) -> Vec<CanvasSizePreset> {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return default_canvas_size_presets(),
    };
    serde_json::from_slice::<Vec<CanvasSizePreset>>(&bytes)
        .ok()
        .filter(|presets| !presets.is_empty())
        .unwrap_or_else(default_canvas_size_presets)
}

pub fn save_canvas_size_presets(
    path: impl AsRef<Path>,
    presets: &[CanvasSizePreset],
) -> std::io::Result<()> {
    let serialized = serde_json::to_vec_pretty(presets)?;
    std::fs::write(path, serialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_size_presets_include_a4_350dpi() {
        let presets = default_canvas_size_presets();
        assert!(presets.iter().any(|preset| {
            preset.id == "a4-350dpi" && preset.width == 2894 && preset.height == 4093
        }));
    }

    #[test]
    fn dropdown_option_embeds_size_and_label() {
        let preset = CanvasSizePreset {
            id: "demo".to_string(),
            label: "Demo".to_string(),
            width: 320,
            height: 240,
        };

        assert_eq!(preset.dropdown_option(), "320x240:Demo");
    }
}
