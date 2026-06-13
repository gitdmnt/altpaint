//! `PanelWorkspace` の focus 状態を扱う。
//!
//! ADR 014 以降、HTML パネルへの統一でテキスト入力 (IME/preedit) や dropdown 状態は
//! HTML パネル内部の DOM mutation で完結するようになり、panel-workspace は
//! `focused_target` (panel_id, node_id) の保持と HTML hit table ベースの巡回のみを担う。

use super::*;

/// 現在 focus 中の panel node。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusTarget {
    pub(crate) panel_id: String,
    pub(crate) node_id: String,
}

impl PanelWorkspace {
    pub fn focus_panel_node(&mut self, panel_id: &str, node_id: &str) -> bool {
        let exists = self
            .focusable_targets()
            .iter()
            .any(|target| target.panel_id == panel_id && target.node_id == node_id);
        if !exists {
            return false;
        }

        let next = FocusTarget {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        };
        if self.focused_target.as_ref() == Some(&next) {
            return false;
        }

        self.focused_target = Some(next);
        true
    }

    pub fn focus_next(&mut self) -> bool {
        self.move_focus(1)
    }

    pub fn focus_previous(&mut self) -> bool {
        self.move_focus(-1)
    }

    /// 現在 focus 中の `(panel_id, node_id)` を返す。
    /// 利用側 (desktop) が `PanelEvent::Activate` を組み立てて dispatch する。
    /// パネルイベント protocol 型に依存しないため、戻り値は素の id ペア。
    pub fn activate_focused(&mut self) -> Option<(String, String)> {
        let target = self.focused_target.clone()?;
        Some((target.panel_id, target.node_id))
    }

    /// HTML hit table をフラットな FocusTarget 列に変換する。
    fn focusable_targets(&self) -> Vec<FocusTarget> {
        let mut targets = Vec::new();
        for (panel_id, map) in &self.panel_hits {
            for hit in &map.hits {
                targets.push(FocusTarget {
                    panel_id: panel_id.clone(),
                    node_id: hit.node_id.clone(),
                });
            }
        }
        targets
    }

    fn move_focus(&mut self, step: isize) -> bool {
        let targets = self.focusable_targets();
        if targets.is_empty() {
            return false;
        }

        let current_index = self.focused_target.as_ref().and_then(|current| {
            targets.iter().position(|target| {
                target.panel_id == current.panel_id && target.node_id == current.node_id
            })
        });
        let next_index = match current_index {
            Some(index) => (index as isize + step).rem_euclid(targets.len() as isize) as usize,
            None if step >= 0 => 0,
            None => targets.len() - 1,
        };
        let next = targets[next_index].clone();
        if self.focused_target.as_ref() == Some(&next) {
            return false;
        }

        self.focused_target = Some(next);
        true
    }
}
