//! `DesktopApp` の回帰テスト群を責務別に分割してまとめる。
//!
//! ダイアログ差し替えやパネルツリー探索など、複数テストで共有する補助をここへ置く。

mod bootstrap_tests;
mod command_router_tests;
mod commands;
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
    /// 次回の open ダイアログが返す単一パスを持つ実装を生成する。
    fn with_open_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(vec![path]),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// 次回の save ダイアログが返す単一パスを持つ実装を生成する。
    fn with_save_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(vec![path]),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// 次回の workspace preset 保存ダイアログが返す単一パスを持つ実装を生成する。
    fn with_workspace_save_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(vec![path]),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// 次回のペン読込ダイアログが返す単一パスを持つ実装を生成する。
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
    /// 仕込んだ open パスを一件返す。
    fn pick_open_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.open_paths.borrow_mut().pop()
    }

    /// 仕込んだ save パスを一件返す。
    fn pick_save_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.save_paths.borrow_mut().pop()
    }

    fn pick_save_workspace_preset_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.workspace_save_paths.borrow_mut().pop()
    }

    fn pick_open_pen_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.pen_open_paths.borrow_mut().pop()
    }

    /// 表示要求されたエラー内容を記録する。
    fn show_error(&self, title: &str, message: &str) {
        self.errors
            .borrow_mut()
            .push((title.to_string(), message.to_string()));
    }
}

/// 差し替えダイアログを使う `DesktopApp` を生成する。
fn test_app_with_dialogs(dialogs: TestDialogs) -> DesktopApp {
    DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-test.altp.json"),
        Box::new(dialogs),
        unique_test_path("session"),
        unique_test_path("workspace-presets"),
    )
}

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

pub(crate) fn unique_test_path(name: &str) -> PathBuf {
    let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("altpaint-{name}-{}-{id}.json", std::process::id()))
}

/// パネルツリー内に指定テキストが含まれるか再帰的に判定する。
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
