//! `UiShell` の workspace 管理責務をまとめる。
//!
//! panel 並び順・表示状態・workspace 管理 panel の生成をここへ寄せ、
//! shell 本体から UI 管理ロジックを分離する。

use super::*;

pub(super) const WORKSPACE_PANEL_ID: &str = "builtin.workspace-layout";

impl UiShell {
    /// パネル順序を上下へ移動する。
    pub fn move_panel(&mut self, panel_id: &str, direction: PanelMoveDirection) -> bool {
        let Some(index) = self.workspace_layout.panels.iter().position(|entry| entry.id == panel_id) else {
            return false;
        };

        let target_index = match direction {
            PanelMoveDirection::Up if index > 0 => index - 1,
            PanelMoveDirection::Down if index + 1 < self.workspace_layout.panels.len() => index + 1,
            _ => return false,
        };

        self.workspace_layout.panels.swap(index, target_index);
        self.panel_content_dirty = true;
        true
    }

    /// 指定 panel の表示状態を切り替える。
    pub fn set_panel_visibility(&mut self, panel_id: &str, visible: bool) -> bool {
        if panel_id == WORKSPACE_PANEL_ID {
            return false;
        }

        let Some(entry) = self.workspace_layout.panels.iter_mut().find(|entry| entry.id == panel_id) else {
            return false;
        };

        if entry.visible == visible {
            return false;
        }

        entry.visible = visible;
        if !visible && self.focused_target.as_ref().is_some_and(|target| target.panel_id == panel_id) {
            self.focused_target = None;
        }
        self.panel_content_dirty = true;
        true
    }

    /// DSL 由来で読み込んだ panel 群だけをアンロードする。
    pub(super) fn remove_loaded_panels(&mut self) {
        if self.loaded_panel_ids.is_empty() {
            return;
        }

        self.panels.retain(|panel| {
            !self.loaded_panel_ids.iter().any(|loaded_id| loaded_id == panel.id())
        });
        self.loaded_panel_ids.clear();
        self.panel_content_dirty = true;
    }

    /// workspace layout に panel entry が存在することを保証する。
    pub(super) fn ensure_workspace_panel_entry(&mut self, panel_id: &str) {
        if panel_id == WORKSPACE_PANEL_ID
            || self.workspace_layout.panels.iter().any(|entry| entry.id == panel_id)
        {
            return;
        }

        self.workspace_layout.panels.push(WorkspacePanelState {
            id: panel_id.to_string(),
            visible: true,
        });
    }

    /// 読み込み済み panel 群と workspace layout の整合を取る。
    pub(super) fn reconcile_workspace_layout(&mut self) {
        let panel_ids = self.panels.iter().map(|panel| panel.id()).collect::<Vec<_>>();
        for panel_id in panel_ids {
            self.ensure_workspace_panel_entry(panel_id);
        }

        if self.focused_target.as_ref().is_some_and(|target| !self.panel_is_visible(&target.panel_id)) {
            self.focused_target = None;
        }
    }

    /// 現在の可視順に従って panel iterator を返す。
    pub(super) fn visible_panels_in_order(&self) -> impl Iterator<Item = &dyn PanelPlugin> {
        let ordered_ids = self
            .workspace_layout
            .panels
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();

        ordered_ids.into_iter().filter_map(|panel_id| {
            self.panels
                .iter()
                .find(|panel| panel.id() == panel_id && self.panel_is_visible(panel_id))
                .map(|panel| panel.as_ref())
        })
    }

    /// panel が現在可視かを判定する。
    pub(super) fn panel_is_visible(&self, panel_id: &str) -> bool {
        if panel_id == WORKSPACE_PANEL_ID {
            return true;
        }

        self.workspace_layout
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)
            .map(|entry| entry.visible)
            .unwrap_or(true)
    }

    /// workspace 管理 panel の tree を構築する。
    pub(super) fn workspace_manager_tree(&self) -> PanelTree {
        let panel_titles = self
            .panels
            .iter()
            .map(|panel| (panel.id(), panel.title()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let ordered_entries = self
            .workspace_layout
            .panels
            .iter()
            .filter(|entry| panel_titles.contains_key(entry.id.as_str()))
            .collect::<Vec<_>>();

        let rows = ordered_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let title = panel_titles
                    .get(entry.id.as_str())
                    .copied()
                    .unwrap_or(entry.id.as_str());
                PanelNode::Row {
                    id: format!("workspace.row.{}", entry.id),
                    children: vec![
                        PanelNode::Text {
                            id: format!("workspace.title.{}", entry.id),
                            text: title.to_string(),
                        },
                        PanelNode::Button {
                            id: format!("workspace.move-up.{}", entry.id),
                            label: "Up".to_string(),
                            action: HostAction::MovePanel {
                                panel_id: entry.id.clone(),
                                direction: PanelMoveDirection::Up,
                            },
                            active: index > 0,
                            fill_color: None,
                        },
                        PanelNode::Button {
                            id: format!("workspace.move-down.{}", entry.id),
                            label: "Down".to_string(),
                            action: HostAction::MovePanel {
                                panel_id: entry.id.clone(),
                                direction: PanelMoveDirection::Down,
                            },
                            active: index + 1 < ordered_entries.len(),
                            fill_color: None,
                        },
                        PanelNode::Button {
                            id: format!("workspace.visibility.{}", entry.id),
                            label: if entry.visible { "Hide".to_string() } else { "Show".to_string() },
                            action: HostAction::SetPanelVisibility {
                                panel_id: entry.id.clone(),
                                visible: !entry.visible,
                            },
                            active: !entry.visible,
                            fill_color: None,
                        },
                    ],
                }
            })
            .collect::<Vec<_>>();

        PanelTree {
            id: WORKSPACE_PANEL_ID,
            title: "Workspace",
            children: vec![PanelNode::Column {
                id: "workspace.root".to_string(),
                children: vec![PanelNode::Section {
                    id: "workspace.panels".to_string(),
                    title: "Panels".to_string(),
                    children: rows,
                }],
            }],
        }
    }
}

/// event から対象 panel id を取り出す。
pub(super) fn event_panel_id(event: &PanelEvent) -> &str {
    match event {
        PanelEvent::Activate { panel_id, .. }
        | PanelEvent::SetValue { panel_id, .. }
        | PanelEvent::DragValue { panel_id, .. }
        | PanelEvent::SetText { panel_id, .. }
        | PanelEvent::Keyboard { panel_id, .. } => panel_id,
    }
}

/// workspace 管理 panel の event を `HostAction` へ変換する。
pub(super) fn workspace_panel_actions(nodes: &[PanelNode], event: &PanelEvent) -> Vec<HostAction> {
    let target_id = match event {
        PanelEvent::Activate { node_id, .. }
        | PanelEvent::SetValue { node_id, .. }
        | PanelEvent::DragValue { node_id, .. }
        | PanelEvent::SetText { node_id, .. } => node_id,
        PanelEvent::Keyboard { .. } => return Vec::new(),
    };
    find_actions_in_nodes_local(nodes, target_id)
}

/// ローカル workspace panel node から action 群を探索する。
fn find_actions_in_nodes_local(nodes: &[PanelNode], target_id: &str) -> Vec<HostAction> {
    for node in nodes {
        if let Some(actions) = find_actions_in_node_local(node, target_id) {
            return actions;
        }
    }
    Vec::new()
}

/// 単一 node 配下から action 群を探索する。
fn find_actions_in_node_local(node: &PanelNode, target_id: &str) -> Option<Vec<HostAction>> {
    match node {
        PanelNode::Column { children, .. }
        | PanelNode::Row { children, .. }
        | PanelNode::Section { children, .. } => children
            .iter()
            .find_map(|child| find_actions_in_node_local(child, target_id)),
        PanelNode::Text { .. } | PanelNode::ColorPreview { .. } => None,
        PanelNode::ColorWheel { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::ColorWheel { .. } => None,
        PanelNode::Button { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Button { .. } => None,
        PanelNode::Slider { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Slider { .. } => None,
        PanelNode::TextInput {
            id,
            action: Some(action),
            ..
        } if id == target_id => Some(vec![action.clone()]),
        PanelNode::TextInput { .. } => None,
        PanelNode::Dropdown { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Dropdown { .. } => None,
        PanelNode::LayerList { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::LayerList { .. } => None,
    }
}
