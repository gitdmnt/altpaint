//! OS ダイアログとパス正規化を担当する補助モジュール。
//!
//! デスクトップ本体からネイティブダイアログ依存を切り離し、
//! テストでは差し替え可能な境界として扱う。

use std::path::{Path, PathBuf};

/// プロジェクトの開閉に必要なダイアログ操作を抽象化する。
pub trait DesktopDialogs {
    /// 開く対象のプロジェクトパスを選択する。
    fn pick_open_project_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 保存先のプロジェクトパスを選択する。
    fn pick_save_project_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 書き出し先の workspace preset パスを選択する。
    fn pick_save_workspace_preset_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 読み込む外部ペンファイルのパスを選択する。
    fn pick_open_pen_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// ユーザーへエラー内容を通知する。
    fn show_error(&self, title: &str, message: &str);
}

/// 実行環境でネイティブダイアログを使う既定実装を表す。
pub struct NativeDesktopDialogs;

impl DesktopDialogs for NativeDesktopDialogs {
    /// 既定のファイルオープンダイアログを表示する。
    fn pick_open_project_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::open_file_dialog(
            "Open Project",
            &current_path.to_string_lossy(),
            Some((&["*.altp.json", "*.json"], "altpaint project")),
        )
        .map(PathBuf::from)
    }

    /// 既定のファイル保存ダイアログを表示する。
    fn pick_save_project_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::save_file_dialog_with_filter(
            "Save Project",
            &current_path.to_string_lossy(),
            &["*.altp.json", "*.json"],
            "altpaint project",
        )
        .map(PathBuf::from)
    }

    /// 既定の workspace preset 保存ダイアログを表示する。
    fn pick_save_workspace_preset_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::save_file_dialog_with_filter(
            "Export Workspace Preset",
            &current_path.to_string_lossy(),
            &["*.altp-workspace.json", "*.json"],
            "altpaint workspace preset",
        )
        .map(PathBuf::from)
        .map(normalize_workspace_preset_path)
    }

    /// 既定の外部ペン読込ダイアログを表示する。
    fn pick_open_pen_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::open_file_dialog(
            "Import Pen Preset",
            &current_path.to_string_lossy(),
            Some((
                &["*.altp-pen.json", "*.abr", "*.sut", "*.gbr", "*.json"],
                "altpaint / Photoshop / Clip Studio / GIMP pen",
            )),
        )
        .map(PathBuf::from)
    }

    /// ネイティブのエラーダイアログを表示する。
    fn show_error(&self, title: &str, message: &str) {
        tinyfiledialogs::message_box_ok(title, message, tinyfiledialogs::MessageBoxIcon::Error);
    }
}

/// 拡張子が省略された保存先へ既定拡張子を補う。
pub fn normalize_project_path(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension("altp.json")
    }
}

/// 拡張子が省略された workspace preset 保存先へ既定拡張子を補う。
pub fn normalize_workspace_preset_path(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension("altp-workspace.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_project_path_adds_default_extension() {
        assert_eq!(
            normalize_project_path(PathBuf::from("sample")),
            PathBuf::from("sample.altp.json")
        );
    }

    #[test]
    fn normalize_project_path_preserves_existing_extension() {
        assert_eq!(
            normalize_project_path(PathBuf::from("sample.json")),
            PathBuf::from("sample.json")
        );
    }

    #[test]
    fn normalize_workspace_preset_path_adds_default_extension() {
        assert_eq!(
            normalize_workspace_preset_path(PathBuf::from("workspace-sample")),
            PathBuf::from("workspace-sample.altp-workspace.json")
        );
    }
}
