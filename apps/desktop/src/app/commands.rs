//! プロジェクト I/O と `Command` / `HostAction` 適用を `DesktopApp` へ追加する。
//!
//! 永続化やホストアクションのような副作用をここへ寄せ、
//! 入力処理と描画処理から外部依存を分離する。

use std::path::PathBuf;
use std::thread;

use app_core::Command;
use plugin_api::HostAction;
use storage::{load_project_from_path, save_project_to_path};

use super::DesktopApp;
use crate::canvas_bridge::command_for_canvas_gesture;
use crate::dialogs::normalize_project_path;

impl DesktopApp {
    /// キャンバス入力から編集コマンドを組み立てて適用する。
    pub(super) fn execute_canvas_command(
        &mut self,
        x: usize,
        y: usize,
        from: Option<(usize, usize)>,
    ) -> bool {
        let command = command_for_canvas_gesture(self.document.active_tool, (x, y), from);
        self.execute_command(command)
    }

    /// 現在のプロジェクトパスへ保存を行う。
    fn save_project_to_current_path(&mut self) -> bool {
        self.enqueue_save_project(self.project_path.clone())
    }

    /// 保存先を選んでプロジェクトを保存する。
    fn save_project_as(&mut self) -> bool {
        let Some(path) = self.dialogs.pick_save_project_path(&self.project_path) else {
            return false;
        };
        self.save_project_to_path(path)
    }

    /// 指定パスへプロジェクトを保存し、状態上の現在パスも更新する。
    fn save_project_to_path(&mut self, path: PathBuf) -> bool {
        self.project_path = normalize_project_path(path);
        self.mark_status_dirty();
        self.persist_session_state();
        self.save_project_to_current_path()
    }

    fn enqueue_save_project(&mut self, path: PathBuf) -> bool {
        let document = self.document.clone();
        let workspace_layout = self.ui_shell.workspace_layout();
        let plugin_configs = self.ui_shell.persistent_panel_configs();
        let handle = thread::spawn(move || {
            save_project_to_path(&path, &document, &workspace_layout, &plugin_configs)
                .map_err(|error| error.to_string())
        });
        self.pending_save_tasks.push(super::PendingSaveTask { handle });
        self.mark_status_dirty();
        true
    }

    /// 開く対象を選んでプロジェクトを読み込む。
    fn open_project(&mut self) -> bool {
        let Some(path) = self.dialogs.pick_open_project_path(&self.project_path) else {
            return false;
        };
        self.load_project(path)
    }

    /// 指定パスのプロジェクトを読み込み、UI 状態も復元する。
    fn load_project(&mut self, path: PathBuf) -> bool {
        let path = normalize_project_path(path);
        match load_project_from_path(&path) {
            Ok(project) => {
                self.project_path = path;
                self.document = project.document;
                let _ = self.reload_pen_presets();
                self.ui_shell.set_workspace_layout(project.workspace_layout);
                self.ui_shell.set_persistent_panel_configs(project.plugin_configs);
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                self.persist_session_state();
                true
            }
            Err(error) => {
                let message = format!("failed to load project: {error}");
                eprintln!("{message}");
                self.dialogs.show_error("Open failed", &message);
                false
            }
        }
    }

    /// アプリケーション全体で扱うコマンドを解釈して適用する。
    pub(crate) fn execute_command(&mut self, command: Command) -> bool {
        self.poll_background_tasks();
        match command {
            Command::NewDocument => self.activate_panel_control("builtin.app-actions", "app.new"),
            Command::SaveProject => self.save_project_to_current_path(),
            Command::SaveProjectAs => self.save_project_as(),
            Command::SaveProjectToPath { path } => self.save_project_to_path(PathBuf::from(path)),
            Command::LoadProject => self.open_project(),
            Command::LoadProjectFromPath { path } => self.load_project(PathBuf::from(path)),
            Command::ReloadPenPresets => self.reload_pen_presets(),
            other => self.execute_document_command(other),
        }
    }

    /// 指定パネルノードを擬似的にアクティブ化する。
    pub(super) fn activate_panel_control(&mut self, panel_id: &str, node_id: &str) -> bool {
        self.dispatch_panel_event(plugin_api::PanelEvent::Activate {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        })
    }

    /// グローバルキーボードショートカットをパネルプラグインへ配送する。
    pub(crate) fn dispatch_keyboard_shortcut(
        &mut self,
        shortcut: &str,
        key: &str,
        repeat: bool,
    ) -> bool {
        let (handled, actions) = self.ui_shell.handle_keyboard_event(shortcut, key, repeat);
        let mut changed = handled;
        for action in actions {
            changed |= self.execute_host_action(action);
        }
        self.refresh_panel_surface_if_changed(changed)
    }

    /// パネルランタイムから返されたホストアクションを実行する。
    pub(crate) fn execute_host_action(&mut self, action: HostAction) -> bool {
        match action {
            HostAction::DispatchCommand(command) => self.execute_command(command),
            HostAction::InvokePanelHandler { .. } => false,
            HostAction::MovePanel {
                panel_id,
                direction,
            } => {
                let changed = self.ui_shell.move_panel(&panel_id, direction);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                    self.persist_session_state();
                }
                changed
            }
            HostAction::SetPanelVisibility { panel_id, visible } => {
                let changed = self.ui_shell.set_panel_visibility(&panel_id, visible);
                if changed {
                    self.mark_panel_surface_dirty();
                    self.mark_status_dirty();
                    self.persist_session_state();
                }
                changed
            }
        }
    }
}
