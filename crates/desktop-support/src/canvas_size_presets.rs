use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::json_store::{JsonLoad, load_json};

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
    // Missing / Corrupt はいずれも既定値で動作する。Corrupt は load_json が
    // 診断を出力済みで、元ファイルはこの層では温存される (書き込まない)。
    match load_json::<Vec<CanvasSizePreset>>(path, "canvas size presets") {
        JsonLoad::Loaded(presets) if !presets.is_empty() => presets,
        _ => default_canvas_size_presets(),
    }
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
    fn size_string_formats_width_by_height() {
        let preset = CanvasSizePreset {
            id: "demo".to_string(),
            label: "Demo".to_string(),
            width: 320,
            height: 240,
        };

        assert_eq!(preset.size_string(), "320x240");
    }

    fn unique_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "altpaint-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults_without_overwriting() {
        let path = unique_path("corrupt-canvas-presets");
        let raw = b"{ broken json that the user was hand-editing";
        std::fs::write(&path, raw).expect("write corrupt");

        // 破損ファイルでも既定値で動作する。
        let presets = load_canvas_size_presets(&path);
        assert_eq!(presets, default_canvas_size_presets());

        // ただし破損ファイルは温存される (黙って既定値で上書きしない)。
        let after = std::fs::read(&path).expect("read back");
        assert_eq!(after, raw);
        let _ = std::fs::remove_file(path);
    }
}
