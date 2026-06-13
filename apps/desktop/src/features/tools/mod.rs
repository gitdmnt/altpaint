//! tools feature スライス (provisional)。
//!
//! ツールカタログのディレクトリロードを所有する (BL-119)。ペン import 報告・tool 状態の
//! 完全移行は B7-part2。B7 で `project-store::tool_catalog` から移管した。

use std::fs;
use std::path::{Path, PathBuf};

use editor_state::ToolDefinition;

/// `tools/` 配下の描画ツール定義を再帰ロードする。
pub(crate) fn load_tool_directory(
    directory: impl AsRef<Path>,
) -> (Vec<ToolDefinition>, Vec<String>) {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_files(
        directory.as_ref(),
        "tool",
        &is_supported_tool_file,
        &mut files,
        &mut diagnostics,
    );
    files.sort();

    let mut tools = Vec::new();
    for file_path in files {
        match load_tool_file(&file_path) {
            Ok(tool) => tools.push(tool),
            Err(error) => diagnostics.push(format!("{}: {error}", file_path.display())),
        }
    }

    (tools, diagnostics)
}

/// `directory` を再帰的に走査し、`is_supported` を満たすファイルを `files` へ収集する。
///
/// 読み取り失敗や列挙失敗は `diagnostics` へ記録する (`label` は診断メッセージに埋め込む
/// 種別名)。ディレクトリが存在しない場合は無診断で何もしない。
fn collect_files(
    directory: &Path,
    label: &str,
    is_supported: &dyn Fn(&Path) -> bool,
    files: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<String>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        if directory.exists() {
            diagnostics.push(format!(
                "failed to read {label} directory: {}",
                directory.display()
            ));
        }
        return;
    };

    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.is_dir() {
                    collect_files(&path, label, is_supported, files, diagnostics);
                } else if is_supported(&path) {
                    files.push(path);
                }
            }
            Err(error) => {
                diagnostics.push(format!("failed to enumerate {label} directory: {error}"))
            }
        }
    }
}

fn is_supported_tool_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".altp-tool.json"))
}

fn load_tool_file(path: &Path) -> Result<ToolDefinition, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let tool =
        serde_json::from_str::<ToolDefinition>(&content).map_err(|error| error.to_string())?;
    if tool.id.trim().is_empty() {
        return Err("tool id must not be empty".to_string());
    }
    if tool.name.trim().is_empty() {
        return Err("tool name must not be empty".to_string());
    }
    if tool.provider_plugin_id.trim().is_empty() {
        return Err("provider_plugin_id must not be empty".to_string());
    }
    if tool.drawing_plugin_id.trim().is_empty() {
        return Err("drawing_plugin_id must not be empty".to_string());
    }
    Ok(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "altpaint-{name}-{}-{}",
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
    fn load_tool_directory_reads_nested_tool_definitions() {
        let dir = unique_temp_dir("tools");
        let nested = dir.join("builtin").join("pens");
        std::fs::create_dir_all(&nested).expect("nested dir");
        std::fs::write(
            nested.join("ink.altp-tool.json"),
            r#"{
  "id": "builtin.ink",
  "name": "Ink",
  "kind": "Pen",
  "provider_plugin_id": "plugins/default-pens-plugin",
  "drawing_plugin_id": "builtin.bitmap",
  "settings": [
    { "key": "size", "label": "太さ", "control": "slider", "min": 1, "max": 1000 }
  ]
}"#,
        )
        .expect("write tool");

        let (tools, diagnostics) = load_tool_directory(&dir);

        assert!(diagnostics.is_empty());
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "builtin.ink");
        assert_eq!(tools[0].name, "Ink");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_tool_directory_reports_invalid_files() {
        let dir = unique_temp_dir("invalid-tools");
        std::fs::write(
            dir.join("broken.altp-tool.json"),
            r#"{ "id": "", "name": "", "kind": "Pen", "provider_plugin_id": "", "drawing_plugin_id": "" }"#,
        )
        .expect("write broken tool");

        let (tools, diagnostics) = load_tool_directory(&dir);

        assert!(tools.is_empty());
        assert_eq!(diagnostics.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
