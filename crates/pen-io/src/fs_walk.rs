//! カタログロード用のディレクトリ再帰走査ヘルパ。

use std::fs;
use std::path::{Path, PathBuf};

/// `directory` を再帰的に走査し、`is_supported` を満たすファイルを `files` へ収集する。
///
/// 読み取り失敗や列挙失敗は `diagnostics` へ記録する (`label` は "tool" / "pen" 等の
/// 種別名で、診断メッセージに埋め込まれる)。ディレクトリが存在しない場合は無診断で何もしない。
pub(crate) fn collect_files(
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
