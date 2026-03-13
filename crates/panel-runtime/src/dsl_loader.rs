use std::fs;
use std::path::{Path, PathBuf};

/// collect パネル files recursive に必要な処理を行う。
pub(crate) fn collect_panel_files_recursive(
    directory: &Path,
    panel_files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_panel_files_recursive(&path, panel_files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("altp-panel") {
            panel_files.push(path);
        }
    }
    Ok(())
}
