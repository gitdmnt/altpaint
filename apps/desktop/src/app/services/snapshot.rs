//! スナップショット service request のハンドラ。

use panel_api::{ServiceRequest, services::names};

use super::DesktopApp;

impl DesktopApp {
    /// snapshot service request を処理する。
    pub(super) fn handle_snapshot_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::SNAPSHOT_CREATE => self.snapshot_create(
                request
                    .string("label")
                    .unwrap_or("Snapshot")
                    .to_string(),
            ),
            names::SNAPSHOT_RESTORE => {
                let id = request.string("snapshot_id")?;
                self.snapshot_restore(id.to_string())
            }
            _ => return None,
        };
        Some(changed)
    }

    /// 現在の Document クローンをスナップショットとして保存する。
    fn snapshot_create(&mut self, label: String) -> bool {
        let document = self.document.clone();
        let id = self.snapshots.push(label, document);
        eprintln!("snapshot created: id={id}");
        true
    }

    /// 指定 ID のスナップショットを Document へ復元する。
    fn snapshot_restore(&mut self, snapshot_id: String) -> bool {
        let Some(entry) = self.snapshots.get(&snapshot_id) else {
            eprintln!("snapshot_restore: id={snapshot_id} not found");
            return false;
        };
        self.document = entry.document.clone();
        // 履歴はスナップショット復元後にクリアして整合性を保つ
        self.history.clear();
        self.refresh_canvas_frame();
        true
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::DesktopApp;
    use crate::app::tests::unique_test_path;
    use desktop_support::NativeDesktopDialogs;

    fn make_app() -> DesktopApp {
        DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
            PathBuf::from("/tmp/altpaint-snapshot-test.altp.json"),
            Box::new(NativeDesktopDialogs),
            unique_test_path("snapshot-session"),
            unique_test_path("snapshot-workspace"),
        )
    }

    /// スナップショット作成後に件数が増えることを確認する。
    #[test]
    fn snapshot_create_increments_count() {
        let mut app = make_app();
        assert_eq!(app.snapshots.len(), 0);
        let changed = app.snapshot_create("my snap".to_string());
        assert!(changed);
        assert_eq!(app.snapshots.len(), 1);
    }

    /// 存在するスナップショットへの復元が成功することを確認する。
    #[test]
    fn snapshot_restore_succeeds() {
        let mut app = make_app();
        let id = app.snapshots.push("test", app.document.clone());
        let changed = app.snapshot_restore(id);
        assert!(changed);
    }

    /// 存在しない ID への復元が false を返すことを確認する。
    #[test]
    fn snapshot_restore_unknown_id_returns_false() {
        let mut app = make_app();
        let changed = app.snapshot_restore("999".to_string());
        assert!(!changed);
    }
}
