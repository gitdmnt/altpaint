//! `tools/` 配下の描画ツール定義を再帰ロードする。

use std::fs;
use std::path::{Path, PathBuf};

use app_core::ToolDefinition;

/// 入力や種別に応じて処理を振り分ける。
pub fn load_tool_directory(directory: impl AsRef<Path>) -> (Vec<ToolDefinition>, Vec<String>) {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_tool_files(directory.as_ref(), &mut files, &mut diagnostics);
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

/// 入力や種別に応じて処理を振り分ける。
fn collect_tool_files(directory: &Path, files: &mut Vec<PathBuf>, diagnostics: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        if directory.exists() {
            diagnostics.push(format!(
                "failed to read tool directory: {}",
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
                    collect_tool_files(&path, files, diagnostics);
                } else if is_supported_tool_file(&path) {
                    files.push(path);
                }
            }
            Err(error) => diagnostics.push(format!("failed to enumerate tool directory: {error}")),
        }
    }
}

/// Is supported ツール file かどうかを返す。
fn is_supported_tool_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".altp-tool.json"))
}

/// 入力を解析して ツール file に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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

    /// Unique temp dir 用の表示文字列を組み立てる。
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

    /// 読込 ツール directory reads nested ツール definitions が期待どおりに動作することを検証する。
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

    /// 読込 ツール directory reports invalid files が期待どおりに動作することを検証する。
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
