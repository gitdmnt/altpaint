//! バックグラウンドジョブの起動・監視・回収を扱う。

use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use storage::save_project_to_path;

use super::DesktopApp;

/// バックグラウンドジョブの種別。
#[derive(Debug)]
pub(crate) enum JobKind {
    /// プロジェクト保存。
    Save,
    /// 画像 export（`path_display` は表示用パス文字列）。
    Export { path_display: String },
}

/// 汎用バックグラウンドジョブの join handle と種別を保持する。
#[derive(Debug)]
pub(crate) struct BackgroundJob {
    pub(crate) kind: JobKind,
    pub(crate) handle: JoinHandle<Result<(), String>>,
}

impl BackgroundJob {
    fn label(&self) -> String {
        match &self.kind {
            JobKind::Save => "saving".to_string(),
            JobKind::Export { path_display } => format!("exporting → {path_display}"),
        }
    }
}

impl DesktopApp {
    /// プロジェクト保存ジョブをキューへ追加する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(super) fn enqueue_save_project(&mut self, path: PathBuf) -> bool {
        // GPU パスで描画した場合は CPU bitmap が古いため、保存前に読み戻して同期する
        #[cfg(feature = "gpu")]
        self.sync_gpu_bitmaps_to_cpu();
        let document = self.document.clone();
        let workspace_layout = self.panel_presentation.workspace_layout();
        let plugin_configs = self.panel_runtime.persistent_panel_configs();
        let handle = thread::spawn(move || {
            save_project_to_path(&path, &document, &workspace_layout, &plugin_configs)
                .map_err(|error| error.to_string())
        });
        self.io_state.pending_jobs.push(BackgroundJob {
            kind: JobKind::Save,
            handle,
        });
        self.mark_status_dirty();
        true
    }

    /// アクティブパネルを PNG として書き出すジョブをキューへ追加する。
    pub(crate) fn enqueue_export_png(&mut self, path: PathBuf) -> bool {
        let document = self.document.clone();
        let path_display = path.display().to_string();
        let handle = thread::spawn(move || {
            storage::export_active_panel_as_png(&document, &path)
                .map_err(|error| error.to_string())
        });
        self.io_state.pending_jobs.push(BackgroundJob {
            kind: JobKind::Export { path_display },
            handle,
        });
        self.mark_status_dirty();
        true
    }

    /// 完了済みジョブを回収し、エラーがあればダイアログで通知する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(super) fn poll_background_tasks(&mut self) {
        let mut remaining = Vec::new();
        let mut completed_any = false;

        for job in self.io_state.pending_jobs.drain(..) {
            if job.handle.is_finished() {
                completed_any = true;
                let label = job.label();
                match job.handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        eprintln!("background job failed ({label}): {error}");
                        self.io_state
                            .dialogs
                            .show_error(&format!("{label} failed"), &error);
                    }
                    Err(_) => {
                        self.io_state
                            .dialogs
                            .show_error(&format!("{label} failed"), "background task panicked");
                    }
                }
            } else {
                remaining.push(job);
            }
        }

        if completed_any {
            self.mark_status_dirty();
        }
        self.io_state.pending_jobs = remaining;
    }

    /// テスト用: 全ジョブが完了するまで同期的に待機する。
    #[cfg(test)]
    pub(crate) fn wait_for_pending_save_tasks(&mut self) {
        let mut remaining = Vec::new();
        std::mem::swap(&mut remaining, &mut self.io_state.pending_jobs);
        for job in remaining {
            match job.handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("background job failed: {error}"),
                Err(_) => panic!("background task panicked"),
            }
        }
    }

    /// テスト用: 現在の pending ジョブ件数を返す。
    #[cfg(test)]
    pub(crate) fn pending_save_task_count(&self) -> usize {
        self.io_state.pending_jobs.len()
    }
}
