//! デスクトップ向けの軽量セッション永続化を担当する。
//!
//! プロジェクト本体とは別に、最後に開いたファイルや UI レイアウトを保持する。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use app_core::WorkspaceUiState;

use crate::json_store::{JsonLoad, load_json};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DesktopSessionState {
    #[serde(default)]
    pub last_project_path: Option<PathBuf>,
    #[serde(default)]
    pub ui_state: WorkspaceUiState,
}

pub fn default_session_path() -> PathBuf {
    PathBuf::from("altpaint-session.json")
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

pub fn startup_project_path(default_project_path: impl Into<PathBuf>) -> PathBuf {
    load_session_state(default_session_path())
        .and_then(|state| state.last_project_path)
        .unwrap_or_else(|| default_project_path.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::WorkspacePanelAnchor;
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
        let state = DesktopSessionState {
            last_project_path: Some(PathBuf::from("custom.altp.json")),
            ui_state: WorkspaceUiState {
                workspace_layout: app_core::WorkspaceLayout {
                    panels: vec![app_core::WorkspacePanelState {
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
        };

        save_session_state(&path, &state).expect("session save should succeed");

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
