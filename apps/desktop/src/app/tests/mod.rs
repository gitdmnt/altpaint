//! `DesktopApp` の回帰テスト群を責務別に分割してまとめる。
//!
//! ダイアログ差し替えやパネルツリー探索など、複数テストで共有する補助をここへ置く。

mod bootstrap_tests;
mod command_router_tests;
mod commands;
#[cfg(feature = "gpu")]
mod gpu_tests;
mod interaction;
mod panel_dispatch_tests;
mod persistence;
mod service_dispatch_tests;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use desktop_support::DesktopDialogs;

use super::DesktopApp;

static TEST_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// テストごとに返却パスと表示エラーを制御できるダイアログ実装を表す。
#[derive(Default)]
pub(crate) struct TestDialogs {
    open_paths: RefCell<Vec<PathBuf>>,
    save_paths: RefCell<Vec<PathBuf>>,
    workspace_save_paths: RefCell<Vec<PathBuf>>,
    pen_open_paths: RefCell<Vec<PathBuf>>,
    errors: RefCell<Vec<(String, String)>>,
}

impl TestDialogs {
    /// 入力値を束ねた新しいインスタンスを生成する。
    fn with_open_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(vec![path]),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    fn with_save_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(vec![path]),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    fn with_workspace_save_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(vec![path]),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    fn with_pen_open_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(vec![path]),
            errors: RefCell::new(Vec::new()),
        }
    }
}

impl DesktopDialogs for TestDialogs {
    /// 現在の pick 開く プロジェクト パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_open_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.open_paths.borrow_mut().pop()
    }

    /// 現在の pick 保存 プロジェクト パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_save_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.save_paths.borrow_mut().pop()
    }

    /// 現在の pick 保存 ワークスペース preset パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_save_workspace_preset_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.workspace_save_paths.borrow_mut().pop()
    }

    /// 現在の pick 開く ペン パス を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn pick_open_pen_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.pen_open_paths.borrow_mut().pop()
    }

    /// エラー を表示できるよう状態を更新する。
    fn show_error(&self, title: &str, message: &str) {
        self.errors
            .borrow_mut()
            .push((title.to_string(), message.to_string()));
    }
}

/// test アプリ with dialogs を計算して返す。
fn test_app_with_dialogs(dialogs: TestDialogs) -> DesktopApp {
    DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(dialogs),
        unique_test_path("session"),
        unique_test_path("workspace-presets"),
    )
}

/// 現在の test アプリ with dialogs and セッション パス を返す。
fn test_app_with_dialogs_and_session_path(
    dialogs: TestDialogs,
    session_path: PathBuf,
) -> DesktopApp {
    DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(dialogs),
        session_path,
        unique_test_path("workspace-presets"),
    )
}

/// 現在の test アプリ with dialogs and ワークスペース preset パス を返す。
fn test_app_with_dialogs_and_workspace_preset_path(
    dialogs: TestDialogs,
    workspace_preset_path: PathBuf,
) -> DesktopApp {
    DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(dialogs),
        unique_test_path("session"),
        workspace_preset_path,
    )
}

/// 現在の unique test パス を返す。
pub(crate) fn unique_test_path(name: &str) -> PathBuf {
    let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("altpaint-{name}-{}-{id}.json", std::process::id()))
}

/// 入力や種別に応じて処理を振り分ける。
fn tree_contains_text(nodes: &[panel_api::PanelNode], target: &str) -> bool {
    nodes.iter().any(|node| match node {
        panel_api::PanelNode::Text { text, .. } => text == target,
        panel_api::PanelNode::Column { children, .. }
        | panel_api::PanelNode::Row { children, .. }
        | panel_api::PanelNode::Section { children, .. } => tree_contains_text(children, target),
        panel_api::PanelNode::ColorPreview { .. }
        | panel_api::PanelNode::ColorWheel { .. }
        | panel_api::PanelNode::Button { .. }
        | panel_api::PanelNode::Slider { .. }
        | panel_api::PanelNode::TextInput { .. }
        | panel_api::PanelNode::Dropdown { .. }
        | panel_api::PanelNode::LayerList { .. } => false,
    })
}

/// 入力や種別に応じて処理を振り分ける。
fn tree_contains_button_id(nodes: &[panel_api::PanelNode], target: &str) -> bool {
    nodes.iter().any(|node| match node {
        panel_api::PanelNode::Button { id, .. } => id == target,
        panel_api::PanelNode::Column { children, .. }
        | panel_api::PanelNode::Row { children, .. }
        | panel_api::PanelNode::Section { children, .. } => {
            tree_contains_button_id(children, target)
        }
        panel_api::PanelNode::Text { .. }
        | panel_api::PanelNode::Slider { .. }
        | panel_api::PanelNode::TextInput { .. }
        | panel_api::PanelNode::Dropdown { .. }
        | panel_api::PanelNode::LayerList { .. }
        | panel_api::PanelNode::ColorPreview { .. }
        | panel_api::PanelNode::ColorWheel { .. } => false,
    })
}
