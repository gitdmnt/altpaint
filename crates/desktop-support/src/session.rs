//! デスクトップ向けの軽量セッション永続化を担当する。
//!
//! プロジェクト本体とは別に、最後に開いたファイルや UI レイアウトを保持する。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use app_core::WorkspaceLayout;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DesktopSessionState {
    #[serde(default)]
    pub last_project_path: Option<PathBuf>,
    #[serde(default)]
    pub workspace_layout: WorkspaceLayout,
    #[serde(default)]
    pub plugin_configs: BTreeMap<String, Value>,
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
            workspace_layout: WorkspaceLayout {
                panels: vec![app_core::WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                }],
            },
            plugin_configs: BTreeMap::from([(
                "builtin.app-actions".to_string(),
                serde_json::json!({"new_shortcut": "Ctrl+Alt+N"}),
            )]),
        };

        save_session_state(&path, &state).expect("session save should succeed");

        assert_eq!(load_session_state(&path), Some(state));

        let _ = std::fs::remove_file(path);
    }
}
