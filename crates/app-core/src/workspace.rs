use serde::{Deserialize, Serialize};

fn default_visible() -> bool {
    true
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
