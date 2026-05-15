//! `UiShell` の focus 状態を扱う。
//!
//! ADR 014 以降、HTML パネルへの統一でテキスト入力 (IME/preedit) や dropdown 状態は
//! HTML パネル内部の DOM mutation で完結するようになり、ui-shell は
//! `focused_target` (panel_id, node_id) の保持と HTML hit table ベースの巡回のみを担う。

use super::*;
use panel_api::PanelEvent;
use panel_runtime::PanelRuntime;

impl PanelPresentation {
    /// パネル node へフォーカスを移す。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn focus_panel_node(
        &mut self,
        _runtime: &PanelRuntime,
        panel_id: &str,
        node_id: &str,
    ) -> bool {
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

        let previous = self.focused_target.clone();
        self.focused_target = Some(next);
        if let Some(previous) = previous.as_ref() {
            self.mark_panel_content_dirty(&previous.panel_id);
        }
        self.mark_panel_content_dirty(panel_id);
        true
    }

    /// 次 へフォーカスを移す。
    pub fn focus_next(&mut self, _runtime: &PanelRuntime) -> bool {
        self.move_focus(1)
    }

    /// 前 へフォーカスを移す。
    pub fn focus_previous(&mut self, _runtime: &PanelRuntime) -> bool {
        self.move_focus(-1)
    }

    /// Focused をアクティブ化する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn activate_focused(&mut self) -> Option<PanelEvent> {
        let target = self.focused_target.clone()?;
        Some(PanelEvent::Activate {
            panel_id: target.panel_id,
            node_id: target.node_id,
        })
    }

    /// HTML hit table をフラットな FocusTarget 列に変換する。
    fn focusable_targets(&self) -> Vec<FocusTarget> {
        let mut targets = Vec::new();
        for (panel_id, map) in &self.html_panel_hits {
            for hit in &map.hits {
                targets.push(FocusTarget {
                    panel_id: panel_id.clone(),
                    node_id: hit.node_id.clone(),
                });
            }
        }
        targets
    }

    /// 入力や種別に応じて処理を振り分ける。
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

        let previous = self.focused_target.clone();
        self.focused_target = Some(next);
        if let Some(previous) = previous.as_ref() {
            self.mark_panel_content_dirty(&previous.panel_id);
        }
        if let Some(current) = self.focused_target.as_ref() {
            let panel_id = current.panel_id.clone();
            self.mark_panel_content_dirty(&panel_id);
        }
        true
    }
}
