use serde::{Deserialize, Serialize};

fn is_visible_by_default() -> bool {
    true
}

fn default_visible() -> bool {
    is_visible_by_default()
}

fn default_panel_width() -> usize {
    300
}

fn default_panel_height() -> usize {
    220
}

/// 浮動パネルの左上座標。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspacePanelPosition {
    pub x: usize,
    pub y: usize,
}

/// 浮動パネルのサイズ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePanelSize {
    #[serde(default = "default_panel_width")]
    pub width: usize,
    #[serde(default = "default_panel_height")]
    pub height: usize,
}

impl Default for WorkspacePanelSize {
    fn default() -> Self {
        Self {
            width: default_panel_width(),
            height: default_panel_height(),
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<WorkspacePanelPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<WorkspacePanelSize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_panel_visibility_defaults_to_true_when_missing() {
        let panel: WorkspacePanelState = serde_json::from_str(r#"{"id":"builtin.tool-palette"}"#)
            .expect("panel should deserialize");

        assert!(panel.visible);
        assert_eq!(panel.position, None);
        assert_eq!(panel.size, None);
    }

    #[test]
    fn workspace_layout_roundtrip_preserves_order_and_visibility() {
        let layout = WorkspaceLayout {
            panels: vec![
                WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                    position: Some(WorkspacePanelPosition { x: 24, y: 72 }),
                    size: Some(WorkspacePanelSize {
                        width: 280,
                        height: 320,
                    }),
                },
                WorkspacePanelState {
                    id: "builtin.layers-panel".to_string(),
                    visible: true,
                    position: Some(WorkspacePanelPosition { x: 340, y: 72 }),
                    size: Some(WorkspacePanelSize {
                        width: 320,
                        height: 360,
                    }),
                },
            ],
        };

        let json = serde_json::to_string(&layout).expect("layout should serialize");
        let restored: WorkspaceLayout =
            serde_json::from_str(&json).expect("layout should deserialize");

        assert_eq!(restored, layout);
    }
}
