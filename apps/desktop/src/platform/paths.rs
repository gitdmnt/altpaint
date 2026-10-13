//! 永続化パスと同梱アセットディレクトリの解決を担う (BL-113)。
//!
//! 以前は CWD 相対 (セッション/プリセット) と `CARGO_MANIFEST_DIR` 相対
//! (同梱アセット) で解決しており、配布バイナリでは書き込み不能/不在の
//! ディレクトリを指して破綻していた。これをプラットフォーム標準の
//! ユーザーデータディレクトリ (`dirs`) ベースへ変更する。
//!
//! OS 差異は `dirs` クレートが吸収する (OS 固有 cfg は書かない)。開発時は
//! ソースツリー相対のフォールバックを許容する (同梱アセットが実行ファイル
//! 隣接に無い場合)。

use std::path::{Path, PathBuf};

/// 既定のプロジェクトファイル名。
pub(crate) const DEFAULT_PROJECT_FILE_NAME: &str = "altpaint-project.altp.json";
/// ユーザーデータディレクトリ配下のアプリ専用サブディレクトリ名。
const APP_DIR: &str = "altpaint";
/// セッション永続化ファイル名。
const SESSION_FILE_NAME: &str = "altpaint-session.json";
/// キャンバスサイズプリセットファイル名。
const CANVAS_SIZE_PRESET_FILE_NAME: &str = "canvas-templates.json";
/// ワークスペースプリセットファイル名。
const WORKSPACE_PRESET_FILE_NAME: &str = "workspace-presets.json";

/// 永続化ファイルの配置先ユーザーデータディレクトリを返す。
///
/// `dirs::data_dir()/altpaint`。取得不能な (極めて稀な) 環境では CWD に
/// フォールバックする。
pub(crate) fn user_data_dir() -> PathBuf {
    resolve_user_data_dir(dirs::data_dir())
}

/// 既定プロジェクトパスを返す。
pub(crate) fn default_project_path() -> PathBuf {
    user_data_dir().join(DEFAULT_PROJECT_FILE_NAME)
}

/// セッション永続化パスを返す。
pub(crate) fn default_session_path() -> PathBuf {
    user_data_dir().join(SESSION_FILE_NAME)
}

/// キャンバスサイズプリセットパスを返す。
pub(crate) fn default_canvas_size_preset_path() -> PathBuf {
    user_data_dir().join(CANVAS_SIZE_PRESET_FILE_NAME)
}

/// ワークスペースプリセットパスを返す。
pub(crate) fn default_workspace_preset_path() -> PathBuf {
    user_data_dir().join(WORKSPACE_PRESET_FILE_NAME)
}

/// 同梱ビルトインパネルディレクトリを返す。
pub(crate) fn builtin_panels_dir() -> PathBuf {
    asset_dir("builtin-panels", &source_tree_builtin_panels_dir())
}

/// 同梱ペンディレクトリを返す。
pub(crate) fn pen_dir() -> PathBuf {
    asset_dir("pens", &source_tree_dir("pens"))
}

/// 同梱ツールディレクトリを返す。
pub(crate) fn tool_dir() -> PathBuf {
    asset_dir("tools", &source_tree_dir("tools"))
}

/// `data_dir` が取得できればその配下に `altpaint` を、できなければ CWD を返す
/// (純関数。テスト対象)。
fn resolve_user_data_dir(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir
        .map(|dir| dir.join(APP_DIR))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 同梱アセットディレクトリを解決する。実行ファイル隣接に存在すればそれを
/// (配布形態)、無ければソースツリー相対 (開発時) を使う。
fn asset_dir(asset_name: &str, source_relative: &Path) -> PathBuf {
    resolve_asset_dir(executable_dir().as_deref(), asset_name, source_relative)
}

/// 実行ファイル隣接候補 → ソースツリー相対の優先順でアセットディレクトリを
/// 解決する (純関数。テスト対象)。
fn resolve_asset_dir(
    exe_dir: Option<&Path>,
    asset_name: &str,
    source_relative: &Path,
) -> PathBuf {
    if let Some(exe_dir) = exe_dir {
        let candidate = exe_dir.join(asset_name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    source_relative.to_path_buf()
}

fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

fn source_tree_root() -> PathBuf {
    // apps/desktop/Cargo.toml からワークスペースルートへ 2 段上がる。
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn source_tree_dir(name: &str) -> PathBuf {
    source_tree_root().join(name)
}

fn source_tree_builtin_panels_dir() -> PathBuf {
    source_tree_root().join("crates").join("builtin-panels")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_dir_joins_app_subdir_under_data_dir() {
        let base = PathBuf::from("/home/user/.local/share");
        assert_eq!(
            resolve_user_data_dir(Some(base.clone())),
            base.join("altpaint")
        );
    }

    #[test]
    fn user_data_dir_falls_back_to_cwd_when_data_dir_missing() {
        assert_eq!(resolve_user_data_dir(None), PathBuf::from("."));
    }

    #[test]
    fn default_persistence_files_live_under_user_data_dir() {
        let base = PathBuf::from("/data/altpaint");
        // 各既定ファイルは同一データディレクトリ配下に固有名で並ぶ。
        let names = [
            (SESSION_FILE_NAME, "altpaint-session.json"),
            (CANVAS_SIZE_PRESET_FILE_NAME, "canvas-templates.json"),
            (WORKSPACE_PRESET_FILE_NAME, "workspace-presets.json"),
            (DEFAULT_PROJECT_FILE_NAME, "altpaint-project.altp.json"),
        ];
        for (constant, expected) in names {
            assert_eq!(constant, expected);
            assert_eq!(base.join(constant), base.join(expected));
        }
    }

    #[test]
    fn asset_dir_prefers_executable_adjacent_when_present() {
        // 実行ファイル隣接に asset ディレクトリが存在する場合 (配布形態) は
        // それを使う。実在ディレクトリでテストするため temp dir を使う。
        let exe_dir = std::env::temp_dir().join(format!(
            "altpaint-asset-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ));
        let asset = exe_dir.join("pens");
        std::fs::create_dir_all(&asset).expect("create asset dir");

        let source_relative = PathBuf::from("/nonexistent/source/pens");
        let resolved = resolve_asset_dir(Some(&exe_dir), "pens", &source_relative);
        assert_eq!(resolved, asset);

        let _ = std::fs::remove_dir_all(&exe_dir);
    }

    #[test]
    fn asset_dir_falls_back_to_source_tree_when_not_executable_adjacent() {
        // 実行ファイル隣接に asset ディレクトリが無い場合 (開発時) は
        // ソースツリー相対へフォールバックする。
        let exe_dir = std::env::temp_dir().join(format!(
            "altpaint-asset-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ));
        let source_relative = PathBuf::from("/source/tree/pens");
        let resolved = resolve_asset_dir(Some(&exe_dir), "pens", &source_relative);
        assert_eq!(resolved, source_relative);
    }

    #[test]
    fn asset_dir_uses_source_tree_when_executable_dir_unknown() {
        let source_relative = PathBuf::from("/source/tree/tools");
        let resolved = resolve_asset_dir(None, "tools", &source_relative);
        assert_eq!(resolved, source_relative);
    }
}
