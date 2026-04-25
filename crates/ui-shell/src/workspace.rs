//! `UiShell` の workspace 管理責務をまとめる。
//!
//! panel 並び順・表示状態・workspace 管理 panel の生成をここへ寄せ、
//! shell 本体から UI 管理ロジックを分離する。

use super::*;
use panel_api::{PanelMoveDirection, PanelNode, PanelTree};
use panel_runtime::PanelRuntime;

pub(super) const WORKSPACE_PANEL_ID: &str = "builtin.workspace-layout";
const HIDDEN_BY_DEFAULT_PANEL_IDS: &[&str] = &["builtin.panel-list"];

/// 既定の パネル 状態 を返す。
pub(super) fn default_panel_state(panel_id: &str, index: usize) -> WorkspacePanelState {
    let (anchor, position) = default_panel_anchor_and_position(panel_id, index);
    WorkspacePanelState {
        id: panel_id.to_string(),
        visible: !HIDDEN_BY_DEFAULT_PANEL_IDS.contains(&panel_id),
        anchor,
        position: Some(position),
        size: Some(WorkspacePanelSize::default()),
    }
}

impl PanelPresentation {
    /// ワークスペース manager entry が満たされるよう整える。
    pub(super) fn ensure_workspace_manager_entry(&mut self) {
        if self
            .workspace_layout
            .panels
            .iter()
            .any(|entry| entry.id == WORKSPACE_PANEL_ID)
        {
            return;
        }

        self.workspace_layout
            .panels
            .insert(0, default_panel_state(WORKSPACE_PANEL_ID, 0));
    }

    /// 現在の値を パネル entries へ変換する。
    pub(super) fn workspace_panel_entries(
        &self,
        runtime: &PanelRuntime,
    ) -> Vec<(&WorkspacePanelState, String)> {
        let panel_titles = runtime
            .panel_trees()
            .into_iter()
            .map(|tree| (tree.id, tree.title.to_string()))
            .collect::<std::collections::BTreeMap<_, _>>();

        self.workspace_layout
            .panels
            .iter()
            .filter_map(|entry| {
                panel_titles
                    .get(entry.id.as_str())
                    .cloned()
                    .map(|title| (entry, title))
            })
            .collect()
    }

    /// 既存データを走査して move パネル to を組み立てる。
    ///
    /// 必要に応じて dirty 状態も更新します。
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

    /// 既存データを走査して パネル 矩形 を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn panel_rect(&self, panel_id: &str) -> Option<render_types::PixelRect> {
        if let Some(rect) = self.rendered_panel_rects.get(panel_id) {
            return Some(*rect);
        }

        let entry = self
            .workspace_layout
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)?;
        let size = entry.size.unwrap_or_default();
        let position = entry.resolved_position(
            usize::MAX,
            usize::MAX,
            size,
            default_panel_position(panel_id, 0),
        );
        Some(render_types::PixelRect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
    }

    /// 指定パネルの workspace_layout 上のサイズを `(width, height)` に書き換える。
    /// HTML パネルが measured_size の変化を永続化する経路で使う。
    /// 戻り値: 値が実際に変わった場合 true（永続化 dirty フラグを立てる判断に使う）。
    pub fn set_panel_size(&mut self, panel_id: &str, width: usize, height: usize) -> bool {
        let Some(entry) = self
            .workspace_layout
            .panels
            .iter_mut()
            .find(|entry| entry.id == panel_id)
        else {
            return false;
        };
        let next = WorkspacePanelSize {
            width: width.max(1),
            height: height.max(1),
        };
        if entry.size == Some(next) {
            return false;
        }
        entry.size = Some(next);
        self.panel_layout_dirty = true;
        true
    }

    /// `panel_rect` の viewport 指定版。anchor (TopRight/BottomRight/BottomLeft) で
    /// `usize::MAX` を使うと座標が画面外に飛ぶため、HTML パネルなど描画前に
    /// `rendered_panel_rects` を持たないパネルではこちらを使う。
    pub fn panel_rect_in_viewport(
        &self,
        panel_id: &str,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Option<render_types::PixelRect> {
        if let Some(rect) = self.rendered_panel_rects.get(panel_id) {
            return Some(*rect);
        }

        let entry = self
            .workspace_layout
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)?;
        let size = entry.size.unwrap_or_default();
        let position = entry.resolved_position(
            viewport_width,
            viewport_height,
            size,
            default_panel_position(panel_id, 0),
        );
        Some(render_types::PixelRect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn move_panel(&mut self, panel_id: &str, direction: PanelMoveDirection) -> bool {
        let Some(index) = self
            .workspace_layout
            .panels
            .iter()
            .position(|entry| entry.id == panel_id)
        else {
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

    /// パネル visibility を設定する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn set_panel_visibility(&mut self, panel_id: &str, visible: bool) -> bool {
        if panel_id == WORKSPACE_PANEL_ID {
            return false;
        }

        let Some(entry) = self
            .workspace_layout
            .panels
            .iter_mut()
            .find(|entry| entry.id == panel_id)
        else {
            return false;
        };

        if entry.visible == visible {
            return false;
        }

        entry.visible = visible;
        if !visible
            && self
                .focused_target
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id)
        {
            self.focused_target = None;
        }
        self.mark_all_panel_content_dirty();
        self.panel_layout_dirty = true;
        true
    }

    /// ワークスペース パネル entry が満たされるよう整える。
    pub(super) fn ensure_workspace_panel_entry(&mut self, panel_id: &str) {
        if self
            .workspace_layout
            .panels
            .iter()
            .any(|entry| entry.id == panel_id)
        {
            return;
        }

        self.workspace_layout.panels.push(default_panel_state(
            panel_id,
            self.workspace_layout.panels.len(),
        ));
    }

    /// reconcile ワークスペース レイアウト に必要な処理を行う。
    pub(super) fn reconcile_workspace_layout(&mut self, panel_ids: Vec<&'static str>) {
        self.ensure_workspace_manager_entry();

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

        if self
            .focused_target
            .as_ref()
            .is_some_and(|target| !self.panel_is_visible(&target.panel_id))
        {
            self.focused_target = None;
        }
    }

    /// 既存データを走査して 表示状態 panels in order を組み立てる。
    pub(super) fn visible_panels_in_order(&self, runtime: &PanelRuntime) -> Vec<PanelTree> {
        let ordered_ids = self
            .workspace_layout
            .panels
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();

        let panel_trees = runtime
            .panel_trees()
            .into_iter()
            .map(|tree| (tree.id, tree))
            .collect::<std::collections::BTreeMap<_, _>>();

        ordered_ids
            .into_iter()
            .filter(|panel_id| self.panel_is_visible(panel_id))
            .filter_map(|panel_id| panel_trees.get(panel_id).cloned())
            .collect()
    }

    /// 指定パネルが現在表示状態かを返す (外部 crate 向け公開 API)。
    ///
    /// HTML パネルの GPU 描画スキップ判定など、ui-shell 外部からも参照される。
    pub fn is_panel_visible(&self, panel_id: &str) -> bool {
        self.panel_is_visible(panel_id)
    }

    /// 既存データを走査して パネル is 表示状態 を組み立てる。
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

    /// 現在の値を manager tree へ変換する。
    pub(super) fn workspace_manager_tree(&self, runtime: &PanelRuntime) -> PanelTree {
        let ordered_entries = self.workspace_panel_entries(runtime);

        let rows = ordered_entries
            .iter()
            .map(|(entry, title)| PanelNode::Row {
                id: format!("workspace.row.{}", entry.id),
                children: vec![
                    PanelNode::Text {
                        id: format!("workspace.title.{}", entry.id),
                        text: title.clone(),
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

/// 既定の パネル anchor and position を返す。
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

/// 既定の パネル position を返す。
fn default_panel_position(panel_id: &str, index: usize) -> WorkspacePanelPosition {
    default_panel_anchor_and_position(panel_id, index).1
}

/// イベント パネル ID を計算して返す。
pub(super) fn event_panel_id(event: &PanelEvent) -> &str {
    match event {
        PanelEvent::Activate { panel_id, .. }
        | PanelEvent::SetValue { panel_id, .. }
        | PanelEvent::DragValue { panel_id, .. }
        | PanelEvent::SetText { panel_id, .. }
        | PanelEvent::Keyboard { panel_id, .. } => panel_id,
    }
}

/// 現在の値を パネル actions へ変換する。
pub(super) fn workspace_panel_actions(
    ordered_panels: &[(String, bool)],
    event: &PanelEvent,
) -> Vec<HostAction> {
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
