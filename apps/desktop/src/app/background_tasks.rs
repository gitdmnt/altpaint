//! バックグラウンド保存タスクの起動と回収を扱う。

use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use storage::save_project_to_path;

use super::DesktopApp;

/// 非同期保存タスクの join handle を保持する。
#[derive(Debug)]
pub(crate) struct PendingSaveTask {
    pub(crate) handle: JoinHandle<Result<(), String>>,
}

impl DesktopApp {
    /// 現在の document / workspace 状態を非同期保存タスクへ積む。
    pub(super) fn enqueue_save_project(&mut self, path: PathBuf) -> bool {
        let document = self.document.clone();
        let workspace_layout = self.ui_shell.workspace_layout();
        let plugin_configs = self.ui_shell.persistent_panel_configs();
        let handle = thread::spawn(move || {
            save_project_to_path(&path, &document, &workspace_layout, &plugin_configs)
                .map_err(|error| error.to_string())
        });
        self.io_state
            .pending_save_tasks
            .push(PendingSaveTask { handle });
        self.mark_status_dirty();
        true
    }

    /// 完了した非同期保存タスクを回収し、エラーを UI へ通知する。
    pub(super) fn poll_background_tasks(&mut self) {
        let mut remaining = Vec::new();
        let mut completed_any = false;

        for task in self.io_state.pending_save_tasks.drain(..) {
            if task.handle.is_finished() {
                completed_any = true;
                match task.handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        eprintln!("failed to save project: {error}");
                        self.io_state.dialogs.show_error("Save failed", &error);
                    }
                    Err(_) => {
                        self.io_state
                            .dialogs
                            .show_error("Save failed", "background save task panicked");
                    }
                }
            } else {
                remaining.push(task);
            }
        }

        if completed_any {
            self.mark_status_dirty();
        }
        self.io_state.pending_save_tasks = remaining;
    }

    /// テスト用に全保存タスクの完了を待機する。
    #[cfg(test)]
    pub(crate) fn wait_for_pending_save_tasks(&mut self) {
        let mut remaining = Vec::new();
        std::mem::swap(&mut remaining, &mut self.io_state.pending_save_tasks);
        for task in remaining {
            match task.handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("background save failed: {error}"),
                Err(_) => panic!("background save task panicked"),
            }
        }
    }

    /// 未完了の保存タスク数を返す。
    #[cfg(test)]
    pub(crate) fn pending_save_task_count(&self) -> usize {
        self.io_state.pending_save_tasks.len()
    }
}
