//! `DesktopApp` の回帰テスト群を責務別に分割してまとめる。
//!
//! ダイアログ差し替えやパネルツリー探索など、複数テストで共有する補助をここへ置く。

mod bootstrap_tests;
mod command_router_tests;
mod commands;
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
    fn with_open_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(vec![path]),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    fn with_save_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(vec![path]),
            workspace_save_paths: RefCell::new(Vec::new()),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    fn with_workspace_save_path(path: PathBuf) -> Self {
        Self {
            open_paths: RefCell::new(Vec::new()),
            save_paths: RefCell::new(Vec::new()),
            workspace_save_paths: RefCell::new(vec![path]),
            pen_open_paths: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

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
    fn pick_open_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.open_paths.borrow_mut().pop()
    }

    fn pick_save_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.save_paths.borrow_mut().pop()
    }

    fn pick_save_workspace_preset_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.workspace_save_paths.borrow_mut().pop()
    }

    fn pick_open_pen_path(&self, _current_path: &Path) -> Option<PathBuf> {
        self.pen_open_paths.borrow_mut().pop()
    }

    fn show_error(&self, title: &str, message: &str) {
        self.errors
            .borrow_mut()
            .push((title.to_string(), message.to_string()));
    }
}

/// project / session / workspace preset の全パスをテストごとに一意化する。
/// 共有パスは並列テスト間の状態汚染 (一方の persist を他方の bootstrap が読む) を
/// 引き起こすため使用しない (ADR 015)。
fn test_app_with_dialogs(dialogs: TestDialogs) -> DesktopApp {
    DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        unique_test_project_path(),
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
        unique_test_project_path(),
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
        unique_test_project_path(),
        Box::new(dialogs),
        unique_test_path("session"),
        workspace_preset_path,
    )
}

pub(crate) fn unique_test_path(name: &str) -> PathBuf {
    let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("altpaint-{name}-{}-{id}.json", std::process::id()))
}

/// テストごとに一意なプロジェクトパスを返す。
pub(crate) fn unique_test_project_path() -> PathBuf {
    let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "altpaint-project-{}-{id}.altp.json",
        std::process::id()
    ))
}

