//! OS ダイアログとパス正規化を担当する補助モジュール。
//!
//! デスクトップ本体からネイティブダイアログ依存を切り離し、
//! テストでは差し替え可能な境界として扱う。

use std::path::{Path, PathBuf};

/// プロジェクトの開閉に必要なダイアログ操作を抽象化する。
pub trait DesktopDialogs {
    /// 現在の pick 開く プロジェクト パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_open_project_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 現在の pick 保存 プロジェクト パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_save_project_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 現在の pick 保存 ワークスペース preset パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_save_workspace_preset_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 現在の pick 開く ペン パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_open_pen_path(&self, current_path: &Path) -> Option<PathBuf>;
    /// 画像書き出し先パスを選択するダイアログを表示する。
    ///
    /// 値を生成できない場合は None を返します。
    fn pick_save_image_path(&self, _current_path: &Path) -> Option<PathBuf> {
        None
    }
    /// エラー を表示できるよう状態を更新する。
    fn show_error(&self, title: &str, message: &str);
}

/// 実行環境でネイティブダイアログを使う既定実装を表す。
pub struct NativeDesktopDialogs;

impl DesktopDialogs for NativeDesktopDialogs {
    /// 現在の pick 開く プロジェクト パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_open_project_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::open_file_dialog(
            "Open Project",
            &current_path.to_string_lossy(),
            Some((&["*.altp.json", "*.json"], "altpaint project")),
        )
        .map(PathBuf::from)
    }

    /// 現在の pick 保存 プロジェクト パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_save_project_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::save_file_dialog_with_filter(
            "Save Project",
            &current_path.to_string_lossy(),
            &["*.altp.json", "*.json"],
            "altpaint project",
        )
        .map(PathBuf::from)
    }

    /// 現在の pick 保存 ワークスペース preset パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// 現在の pick 開く ペン パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// 画像書き出し先パスを選択するダイアログを表示する。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_save_image_path(&self, current_path: &Path) -> Option<PathBuf> {
        tinyfiledialogs::save_file_dialog_with_filter(
            "Export Image",
            &current_path.to_string_lossy(),
            &["*.png"],
            "PNG image",
        )
        .map(PathBuf::from)
    }

    /// エラー を表示できるよう状態を更新する。
    fn show_error(&self, title: &str, message: &str) {
        tinyfiledialogs::message_box_ok(title, message, tinyfiledialogs::MessageBoxIcon::Error);
    }
}

/// 現在の normalize プロジェクト パス を返す。
pub fn normalize_project_path(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension("altp.json")
    }
}

/// 現在の normalize ワークスペース preset パス を返す。
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

    /// normalize プロジェクト パス adds 既定 extension が期待どおりに動作することを検証する。
    #[test]
    fn normalize_project_path_adds_default_extension() {
        assert_eq!(
            normalize_project_path(PathBuf::from("sample")),
            PathBuf::from("sample.altp.json")
        );
    }

    /// normalize プロジェクト パス preserves existing extension が期待どおりに動作することを検証する。
    #[test]
    fn normalize_project_path_preserves_existing_extension() {
        assert_eq!(
            normalize_project_path(PathBuf::from("sample.json")),
            PathBuf::from("sample.json")
        );
    }

    /// normalize ワークスペース preset パス adds 既定 extension が期待どおりに動作することを検証する。
    #[test]
    fn normalize_workspace_preset_path_adds_default_extension() {
        assert_eq!(
            normalize_workspace_preset_path(PathBuf::from("workspace-sample")),
            PathBuf::from("workspace-sample.altp-workspace.json")
        );
    }
}
