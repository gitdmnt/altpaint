use serde::{Deserialize, Serialize};

fn is_visible_by_default() -> bool {
    true
}

fn default_visible() -> bool {
    is_visible_by_default()
}

/// パネル配置と表示状態を保存する最小ワークスペース設定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceLayout {
    #[serde(default)]
    pub panels: Vec<WorkspacePanelState>,
}

/// 個々のパネルの並び順と表示状態。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePanelState {
    pub id: String,
    #[serde(default = "default_visible")]
    pub visible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_panel_visibility_defaults_to_true_when_missing() {
        let panel: WorkspacePanelState = serde_json::from_str(r#"{"id":"builtin.tool-palette"}"#)
            .expect("panel should deserialize");

        assert!(panel.visible);
    }

    #[test]
    fn workspace_layout_roundtrip_preserves_order_and_visibility() {
        let layout = WorkspaceLayout {
            panels: vec![
                WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                },
                WorkspacePanelState {
                    id: "builtin.layers-panel".to_string(),
                    visible: true,
                },
            ],
        };

        let json = serde_json::to_string(&layout).expect("layout should serialize");
        let restored: WorkspaceLayout =
            serde_json::from_str(&json).expect("layout should deserialize");

        assert_eq!(restored, layout);
    }
}
