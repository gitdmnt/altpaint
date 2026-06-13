//! デスクトップ向けの軽量セッション永続化を担当する。
//!
//! プロジェクト本体とは別に、最後に開いたファイルや UI レイアウトを保持する。

use std::path::{Path, PathBuf};

use editor_state::EditorSession;
use serde::{Deserialize, Serialize};
use panel_workspace::WorkspaceUiState;

use crate::json_store::{JsonLoad, load_json};

/// デスクトップのセッション永続化状態。
///
/// 作品コンテンツ (project ファイル) とは別に、最後に開いたプロジェクトパス・
/// パネル UI レイアウト (`ui_state`)、そして BL-079 の保存境界分離により project
/// ファイルから移されたエディタの一過性編集状態 (`EditorSession`: ツール/色/ペン/
/// ビュー) を保持する。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DesktopSessionState {
    #[serde(default)]
    pub last_project_path: Option<PathBuf>,
    #[serde(default)]
    pub ui_state: WorkspaceUiState,
    /// エディタの一過性編集状態 (ツール/色/ペン/ビュー)。
    #[serde(default)]
    pub editor_session: EditorSession,
}

pub fn load_session_state(path: impl AsRef<Path>) -> Option<DesktopSessionState> {
    match load_json::<DesktopSessionState>(path, "session") {
        JsonLoad::Loaded(state) => Some(state),
        // Missing は初回起動。Corrupt は load_json が診断を出力済みで、既定値
        // (None) で動作しつつ元ファイルは温存される (この層は書き込まない)。
        JsonLoad::Missing | JsonLoad::Corrupt => None,
    }
}

pub fn save_session_state(
    path: impl AsRef<Path>,
    state: &DesktopSessionState,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let serialized = serde_json::to_vec_pretty(state)?;
    std::fs::write(path, serialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_state::ColorRgba8;
    use panel_workspace::WorkspacePanelAnchor;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_test_path(name: &str) -> PathBuf {
        let unique = TEST_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "altpaint-{name}-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[test]
    fn session_roundtrip_preserves_last_project_and_layout() {
        let path = unique_test_path("session-roundtrip");
        let mut editor_session = EditorSession::default();
        editor_session.set_active_color(ColorRgba8::new(0x8e, 0x24, 0xaa, 0xff));
        editor_session.set_active_pen_size(17);
        let state = DesktopSessionState {
            last_project_path: Some(PathBuf::from("custom.altp.json")),
            ui_state: WorkspaceUiState {
                workspace_layout: panel_workspace::WorkspaceLayout {
                    panels: vec![panel_workspace::WorkspacePanelState {
                        id: "builtin.tool-palette".to_string(),
                        visible: false,
                        anchor: WorkspacePanelAnchor::TopLeft,
                        position: None,
                        size: None,
                    }],
                },
                panel_configs: BTreeMap::from([(
                    "builtin.app-actions".to_string(),
                    serde_json::json!({"new_shortcut": "Ctrl+Alt+N"}),
                )]),
            },
            editor_session,
        };

        save_session_state(&path, &state).expect("session save should succeed");

        // BL-079: エディタセッション (色/ペンサイズ等) が session 永続化で round-trip する。
        assert_eq!(load_session_state(&path), Some(state));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_session_returns_none() {
        let path = unique_test_path("session-missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_session_state(&path), None);
    }

    #[test]
    fn corrupt_session_returns_none_without_overwriting() {
        let path = unique_test_path("session-corrupt");
        let raw = b"{ partially written session";
        std::fs::write(&path, raw).expect("write corrupt");

        // 破損セッションは None を返し、既定状態で起動できる。
        assert_eq!(load_session_state(&path), None);

        // ローダはファイルを温存する (黙って既定値で上書きしない)。
        let after = std::fs::read(&path).expect("read back");
        assert_eq!(after, raw);
        let _ = std::fs::remove_file(path);
    }
}
