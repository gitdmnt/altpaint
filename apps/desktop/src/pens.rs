//! 外部ペンプリセットの読み込みを担当する。
//!
//! 現段階では `pens/` 配下の `*.altp-pen.json` を最小フォーマットとして扱い、
//! 将来の Photoshop / CSP importer はこの内部表現へ落とし込む前提にする。

use std::fs;
use std::path::{Path, PathBuf};

use app_core::PenPreset;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct PenPresetFile {
    #[serde(default = "default_format_version")]
    format_version: u32,
    id: String,
    name: String,
    #[serde(default = "default_pen_size")]
    size: u32,
    #[serde(default = "default_pen_min_size")]
    min_size: u32,
    #[serde(default = "default_pen_max_size")]
    max_size: u32,
}

fn default_format_version() -> u32 {
    1
}

fn default_pen_size() -> u32 {
    4
}

fn default_pen_min_size() -> u32 {
    1
}

fn default_pen_max_size() -> u32 {
    64
}

impl TryFrom<PenPresetFile> for PenPreset {
    type Error = String;

    fn try_from(value: PenPresetFile) -> Result<Self, Self::Error> {
        if value.format_version != 1 {
            return Err(format!("unsupported pen preset format version: {}", value.format_version));
        }
        if value.id.trim().is_empty() {
            return Err("pen preset id must not be empty".to_string());
        }
        if value.name.trim().is_empty() {
            return Err("pen preset name must not be empty".to_string());
        }
        let min_size = value.min_size.max(1);
        let max_size = value.max_size.max(min_size);
        Ok(PenPreset {
            id: value.id,
            name: value.name,
            size: value.size.clamp(min_size, max_size),
            min_size,
            max_size,
        })
    }
}

pub(crate) fn load_pen_directory(directory: impl AsRef<Path>) -> (Vec<PenPreset>, Vec<String>) {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_pen_files(directory.as_ref(), &mut files, &mut diagnostics);
    files.sort();

    let mut presets = Vec::new();
    for file_path in files {
        match load_pen_file(&file_path) {
            Ok(preset) => presets.push(preset),
            Err(error) => diagnostics.push(format!("{}: {error}", file_path.display())),
        }
    }

    (presets, diagnostics)
}

fn collect_pen_files(directory: &Path, files: &mut Vec<PathBuf>, diagnostics: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        if directory.exists() {
            diagnostics.push(format!("failed to read pen directory: {}", directory.display()));
        }
        return;
    };

    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.is_dir() {
                    collect_pen_files(&path, files, diagnostics);
                } else if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".altp-pen.json"))
                {
                    files.push(path);
                }
            }
            Err(error) => diagnostics.push(format!("failed to enumerate pen directory: {error}")),
        }
    }
}

fn load_pen_file(path: &Path) -> Result<PenPreset, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let file = serde_json::from_str::<PenPresetFile>(&text).map_err(|error| error.to_string())?;
    PenPreset::try_from(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "altpaint-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn load_pen_directory_reads_nested_presets() {
        let dir = unique_temp_dir("pens");
        let nested = dir.join("inktober");
        std::fs::create_dir_all(&nested).expect("nested dir");
        std::fs::write(
            nested.join("round.altp-pen.json"),
            r#"{
  "format_version": 1,
  "id": "round",
  "name": "Round",
  "size": 7,
  "min_size": 1,
  "max_size": 32
}"#,
        )
        .expect("write preset");

        let (presets, diagnostics) = load_pen_directory(&dir);

        assert!(diagnostics.is_empty());
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Round");
        assert_eq!(presets[0].size, 7);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_pen_directory_reports_invalid_files() {
        let dir = unique_temp_dir("invalid-pens");
        std::fs::write(
            dir.join("broken.altp-pen.json"),
            "{\"format_version\":2,\"id\":\"\",\"name\":\"\"}",
        )
        .expect("write broken preset");

        let (presets, diagnostics) = load_pen_directory(&dir);

        assert!(presets.is_empty());
        assert_eq!(diagnostics.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
