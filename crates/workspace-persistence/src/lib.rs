use std::collections::BTreeMap;

use app_core::WorkspaceLayout;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type PluginConfigs = BTreeMap<String, Value>;

/// project / session の双方で共有する panel UI 永続化スナップショット。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceUiState {
    #[serde(default)]
    pub workspace_layout: WorkspaceLayout,
    #[serde(default)]
    pub plugin_configs: PluginConfigs,
}

impl WorkspaceUiState {
    pub fn new(workspace_layout: WorkspaceLayout, plugin_configs: PluginConfigs) -> Self {
        Self {
            workspace_layout,
            plugin_configs,
        }
    }

    pub fn into_parts(self) -> (WorkspaceLayout, PluginConfigs) {
        (self.workspace_layout, self.plugin_configs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_ui_state_roundtrip_preserves_layout_and_configs() {
        let state = WorkspaceUiState {
            workspace_layout: WorkspaceLayout {
                panels: vec![app_core::WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                }],
            },
            plugin_configs: BTreeMap::from([(
                "builtin.pen-settings".to_string(),
                serde_json::json!({"size": 8}),
            )]),
        };

        let json = serde_json::to_string(&state).expect("serialize should succeed");
        let restored: WorkspaceUiState =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(restored, state);
    }
}
