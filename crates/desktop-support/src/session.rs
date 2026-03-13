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
    /// ワークスペース レイアウト を計算して返す。
    pub fn workspace_layout(&self) -> &app_core::WorkspaceLayout {
        &self.ui_state.workspace_layout
    }

    /// プラグイン configs を計算して返す。
    pub fn plugin_configs(&self) -> &workspace_persistence::PluginConfigs {
        &self.ui_state.plugin_configs
    }
}

/// 既定の セッション パス を返す。
pub fn default_session_path() -> PathBuf {
    PathBuf::from("altpaint-session.json")
}

/// 入力を解析して セッション 状態 に変換する。
///
/// 値を生成できない場合は `None` を返します。
pub fn load_session_state(path: impl AsRef<Path>) -> Option<DesktopSessionState> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 現在の値を セッション 状態 へ変換する。
pub fn save_session_state(
    path: impl AsRef<Path>,
    state: &DesktopSessionState,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let serialized = serde_json::to_vec_pretty(state)?;
    std::fs::write(path, serialized)
}

/// 現在の startup プロジェクト パス を返す。
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

    /// 現在の unique test パス を返す。
    fn unique_test_path(name: &str) -> PathBuf {
        let unique = TEST_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "altpaint-{name}-{}-{unique}.json",
            std::process::id()
        ))
    }

    /// セッション roundtrip preserves last プロジェクト and レイアウト が期待どおりに動作することを検証する。
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
