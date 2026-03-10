//! `UiShell` の workspace 管理責務をまとめる。
//!
//! panel 並び順・表示状態・workspace 管理 panel の生成をここへ寄せ、
//! shell 本体から UI 管理ロジックを分離する。

use super::*;

pub(super) const WORKSPACE_PANEL_ID: &str = "builtin.workspace-layout";

pub(super) fn default_panel_state(panel_id: &str, index: usize) -> WorkspacePanelState {
    let (anchor, position) = default_panel_anchor_and_position(panel_id, index);
    WorkspacePanelState {
        id: panel_id.to_string(),
        visible: true,
        anchor,
        position: Some(position),
        size: Some(WorkspacePanelSize::default()),
    }
}

impl UiShell {
    pub(super) fn ensure_workspace_manager_entry(&mut self) {
        if self
            .workspace_layout
            .panels
            .iter()
            .any(|entry| entry.id == WORKSPACE_PANEL_ID)
        {
            return;
        }

        self.workspace_layout.panels.insert(
            0,
            default_panel_state(WORKSPACE_PANEL_ID, 0),
        );
    }

    pub(super) fn workspace_panel_entries(&self) -> Vec<(&WorkspacePanelState, &str)> {
        let panel_titles = self
            .panels
            .iter()
            .map(|panel| (panel.id(), panel.title()))
            .collect::<std::collections::BTreeMap<_, _>>();

        self.workspace_layout
            .panels
            .iter()
            .filter_map(|entry| {
                panel_titles
                    .get(entry.id.as_str())
                    .copied()
                    .map(|title| (entry, title))
            })
            .collect()
    }

    /// パネルの画面上位置を更新する。
    pub fn move_panel_to(
        &mut self,
        panel_id: &str,
        x: usize,
        y: usize,
        viewport_width: usize,
        viewport_height: usize,
    ) -> bool {
        let Some(entry) = self
            .workspace_layout
            .panels
            .iter_mut()
            .find(|entry| entry.id == panel_id)
        else {
            return false;
        };

        let size = self
            .rendered_panel_rects
            .get(panel_id)
            .copied()
            .map(|rect| WorkspacePanelSize {
                width: rect.width,
                height: rect.height,
            })
            .or(entry.size)
            .unwrap_or_default();
        let next_position = WorkspacePanelPosition {
            x: x.min(viewport_width.saturating_sub(size.width.max(1))),
            y: y.min(viewport_height.saturating_sub(size.height.max(1))),
        };
        let current_position = entry.resolved_position(
            viewport_width,
            viewport_height,
            size,
            default_panel_position(panel_id, 0),
        );
        if current_position == next_position {
            return false;
        }

        entry.set_position_from_absolute(
            next_position.x,
            next_position.y,
            viewport_width,
            viewport_height,
            size,
        );
        self.panel_layout_dirty = true;
        true
    }

    /// 現在のパネル矩形を返す。
    pub fn panel_rect(&self, panel_id: &str) -> Option<render::PixelRect> {
        if let Some(rect) = self.rendered_panel_rects.get(panel_id) {
            return Some(*rect);
        }

        let entry = self
            .workspace_layout
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)?;
        let size = entry.size.unwrap_or_default();
        let position = entry.resolved_position(usize::MAX, usize::MAX, size, default_panel_position(panel_id, 0));
        Some(render::PixelRect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
    }

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
        self.mark_all_panel_content_dirty();
        self.panel_layout_dirty = true;
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
        self.mark_all_panel_content_dirty();
        self.panel_layout_dirty = true;
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
        self.mark_all_panel_content_dirty();
        self.panel_layout_dirty = true;
    }

    /// workspace layout に panel entry が存在することを保証する。
    pub(super) fn ensure_workspace_panel_entry(&mut self, panel_id: &str) {
        if self.workspace_layout.panels.iter().any(|entry| entry.id == panel_id) {
            return;
        }

        self.workspace_layout
            .panels
            .push(default_panel_state(panel_id, self.workspace_layout.panels.len()));
    }

    /// 読み込み済み panel 群と workspace layout の整合を取る。
    pub(super) fn reconcile_workspace_layout(&mut self) {
        self.ensure_workspace_manager_entry();

        let panel_ids = self.panels.iter().map(|panel| panel.id()).collect::<Vec<_>>();
        for panel_id in panel_ids {
            self.ensure_workspace_panel_entry(panel_id);
        }

        for (index, entry) in self.workspace_layout.panels.iter_mut().enumerate() {
            if entry.position.is_none() {
                entry.position = Some(default_panel_position(&entry.id, index));
            }
            if entry.size.is_none() {
                entry.size = Some(WorkspacePanelSize::default());
            }
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
        let ordered_entries = self.workspace_panel_entries();

        let rows = ordered_entries
            .iter()
            .map(|(entry, title)| {
                PanelNode::Row {
                    id: format!("workspace.row.{}", entry.id),
                    children: vec![
                        PanelNode::Text {
                            id: format!("workspace.title.{}", entry.id),
                            text: (*title).to_string(),
                        },
                        PanelNode::Button {
                            id: format!("workspace.visibility.{}", entry.id),
                            label: if entry.visible {
                                "👁 非表示".to_string()
                            } else {
                                "👁 表示".to_string()
                            },
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
            title: "パネル管理",
            children: vec![PanelNode::Column {
                id: "workspace.root".to_string(),
                children: vec![PanelNode::Section {
                    id: "workspace.panels".to_string(),
                    title: "表示 / 非表示".to_string(),
                    children: rows,
                }],
            }],
        }
    }
}

fn default_panel_anchor_and_position(
    panel_id: &str,
    index: usize,
) -> (app_core::WorkspacePanelAnchor, WorkspacePanelPosition) {
    match panel_id {
        WORKSPACE_PANEL_ID => (
            app_core::WorkspacePanelAnchor::TopLeft,
            WorkspacePanelPosition { x: 24, y: 72 },
        ),
        "builtin.layers-panel" => (
            app_core::WorkspacePanelAnchor::TopRight,
            WorkspacePanelPosition { x: 24, y: 72 },
        ),
        "builtin.color-palette" => (
            app_core::WorkspacePanelAnchor::BottomLeft,
            WorkspacePanelPosition { x: 24, y: 24 },
        ),
        "builtin.pen-settings" => (
            app_core::WorkspacePanelAnchor::BottomRight,
            WorkspacePanelPosition { x: 24, y: 24 },
        ),
        "builtin.view-controls" => (
            app_core::WorkspacePanelAnchor::BottomRight,
            WorkspacePanelPosition { x: 376, y: 24 },
        ),
        _ => (
            app_core::WorkspacePanelAnchor::TopLeft,
            WorkspacePanelPosition {
                x: 24 + index * 28,
                y: 72 + index * 36,
            },
        ),
    }
}

fn default_panel_position(panel_id: &str, index: usize) -> WorkspacePanelPosition {
    default_panel_anchor_and_position(panel_id, index).1
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
pub(super) fn workspace_panel_actions(ordered_panels: &[(String, bool)], event: &PanelEvent) -> Vec<HostAction> {
    match event {
        PanelEvent::Activate { node_id, .. } => node_id
            .strip_prefix("workspace.visibility.")
            .map(|panel_id| {
                let visible = ordered_panels
                    .iter()
                    .find(|(candidate, _)| candidate.as_str() == panel_id)
                    .map(|(_, visible)| *visible)
                    .unwrap_or(true);
                vec![HostAction::SetPanelVisibility {
                    panel_id: panel_id.to_string(),
                    visible: !visible,
                }]
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}
