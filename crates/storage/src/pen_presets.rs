//! 外部ペンプリセットの読み込みを担当する。
//!
//! 現段階では `pens/` 配下の `*.altp-pen.json` を最小フォーマットとして扱い、
//! 将来の Photoshop / CSP importer はこの内部表現へ落とし込む前提にする。

use std::fs;
use std::path::{Path, PathBuf};

use app_core::PenPreset;

use crate::parse_pen_file;

/// 入力や種別に応じて処理を振り分ける。
pub fn load_pen_directory(directory: impl AsRef<Path>) -> (Vec<PenPreset>, Vec<String>) {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_pen_files(directory.as_ref(), &mut files, &mut diagnostics);
    files.sort();

    let mut presets = Vec::new();
    for file_path in files {
        match load_pen_file(&file_path) {
            Ok(mut loaded_presets) => presets.append(&mut loaded_presets),
            Err(error) => diagnostics.push(format!("{}: {error}", file_path.display())),
        }
    }

    (presets, diagnostics)
}

/// 入力や種別に応じて処理を振り分ける。
fn collect_pen_files(directory: &Path, files: &mut Vec<PathBuf>, diagnostics: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        if directory.exists() {
            diagnostics.push(format!(
                "failed to read pen directory: {}",
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
                    collect_pen_files(&path, files, diagnostics);
                } else if is_supported_pen_file(&path) {
                    files.push(path);
                }
            }
            Err(error) => diagnostics.push(format!("failed to enumerate pen directory: {error}")),
        }
    }
}

/// Is supported ペン file かどうかを返す。
fn is_supported_pen_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name.ends_with(".altp-pen.json") {
        return true;
    }

    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("abr" | "sut" | "gbr")
    )
}

/// 現在の値を ペン file へ変換する。
///
/// 失敗時はエラーを返します。
fn load_pen_file(path: &Path) -> Result<Vec<PenPreset>, String> {
    let imported = parse_pen_file(path).map_err(|error| error.to_string())?;
    let mut diagnostics = imported
        .report
        .issues
        .into_iter()
        .map(|issue| {
            format!(
                "{} [{}] {}",
                issue.severity.severity_label(),
                issue.code,
                issue.message
            )
        })
        .collect::<Vec<_>>();
    let presets = imported
        .pens
        .into_iter()
        .map(|pen| pen.to_runtime_preset())
        .collect::<Vec<_>>();

    if presets.is_empty() {
        if diagnostics.is_empty() {
            diagnostics.push("no importable pens were found".to_string());
        }
        return Err(diagnostics.join("; "));
    }

    Ok(presets)
}

trait PenImportSeverityLabel {
    /// 現在の severity ラベル を返す。
    fn severity_label(&self) -> &'static str;
}

impl PenImportSeverityLabel for crate::PenImportIssueSeverity {
    /// 現在の severity ラベル を返す。
    fn severity_label(&self) -> &'static str {
        match self {
            crate::PenImportIssueSeverity::Info => "info",
            crate::PenImportIssueSeverity::Warning => "warning",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 現在の ワークスペース ペン パス を返す。
    fn workspace_pen_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(relative)
    }

    /// Unique temp dir 用の表示文字列を組み立てる。
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

    /// 読込 ペン directory reads nested presets が期待どおりに動作することを検証する。
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

    /// 読込 ペン directory reports invalid files が期待どおりに動作することを検証する。
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

    /// 読込 ペン directory imports supported external ペン files が期待どおりに動作することを検証する。
    #[test]
    fn load_pen_directory_imports_supported_external_pen_files() {
        let source_abr = workspace_pen_path("pens/abr/manga.abr");
        let source_sut = workspace_pen_path("pens/sut/しげペン改[WEB用].sut");
        if !source_abr.exists() || !source_sut.exists() {
            return;
        }

        let dir = unique_temp_dir("external-pens");
        std::fs::copy(&source_abr, dir.join("manga.abr")).expect("copy abr");
        std::fs::copy(&source_sut, dir.join("しげペン改[WEB用].sut")).expect("copy sut");

        let (presets, diagnostics) = load_pen_directory(&dir);

        assert!(
            presets.iter().any(|preset| preset.id.starts_with("abr.")),
            "expected imported ABR presets, got {}",
            presets.len()
        );
        assert!(
            presets.iter().any(|preset| preset.id.starts_with("sut.")),
            "expected imported SUT presets, got {}",
            presets.len()
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.contains("no importable pens")),
            "unexpected diagnostics: {diagnostics:?}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
