//! デスクトップ向けの軽量セッション永続化を担当する。
//!
//! プロジェクト本体とは別に、最後に開いたファイルや UI レイアウトを保持する。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use app_core::WorkspaceLayout;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct DesktopSessionState {
    #[serde(default)]
    pub(crate) last_project_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) workspace_layout: WorkspaceLayout,
    #[serde(default)]
    pub(crate) plugin_configs: BTreeMap<String, Value>,
}

pub(crate) fn default_session_path() -> PathBuf {
    #[cfg(test)]
    {
        let unique = TEST_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "altpaint-test-session-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[cfg(not(test))]
    {
        PathBuf::from("altpaint-session.json")
    }
}

pub(crate) fn load_session_state(path: impl AsRef<Path>) -> Option<DesktopSessionState> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn save_session_state(
    path: impl AsRef<Path>,
    state: &DesktopSessionState,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let serialized = serde_json::to_vec_pretty(state)?;
    std::fs::write(path, serialized)
}

pub(crate) fn startup_project_path(default_project_path: impl Into<PathBuf>) -> PathBuf {
    load_session_state(default_session_path())
        .and_then(|state| state.last_project_path)
        .unwrap_or_else(|| default_project_path.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrip_preserves_last_project_and_layout() {
        let path = std::env::temp_dir().join(format!(
            "altpaint-session-roundtrip-{}-{}.json",
            std::process::id(),
            TEST_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
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
