//! デスクトップ向けの軽量セッション永続化を担当する。
//!
//! プロジェクト本体とは別に、最後に開いたファイルや UI レイアウトを保持する。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use workspace_persistence::WorkspaceUiState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DesktopSessionState {
    #[serde(default)]
    pub last_project_path: Option<PathBuf>,
    #[serde(default)]
    pub ui_state: WorkspaceUiState,
}

impl DesktopSessionState {
    pub fn workspace_layout(&self) -> &app_core::WorkspaceLayout {
        &self.ui_state.workspace_layout
    }

    pub fn plugin_configs(&self) -> &workspace_persistence::PluginConfigs {
        &self.ui_state.plugin_configs
    }
}

pub fn default_session_path() -> PathBuf {
    PathBuf::from("altpaint-session.json")
}

pub fn load_session_state(path: impl AsRef<Path>) -> Option<DesktopSessionState> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
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
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use app_core::WorkspacePanelAnchor;

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
                plugin_configs: BTreeMap::from([(
                    "builtin.app-actions".to_string(),
                    serde_json::json!({"new_shortcut": "Ctrl+Alt+N"}),
                )]),
            },
        };

        save_session_state(&path, &state).expect("session save should succeed");

        assert_eq!(load_session_state(&path), Some(state));

        let _ = std::fs::remove_file(path);
    }
}
