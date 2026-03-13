//! `UiShell` の focus と text input 編集状態をまとめる。
//!
//! pure helper と shell state 更新を切り分け、IME/preedit を含む text input 処理を
//! `UiShell` 本体から独立して保守できるようにする。

use super::tree_query::{collect_focus_targets, find_dropdown_node, find_text_input_value};
use super::*;
use panel_api::{PanelEvent, TextInputMode};
use panel_runtime::PanelRuntime;

impl PanelPresentation {
    /// パネル node へフォーカスを移す。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn focus_panel_node(
        &mut self,
        runtime: &PanelRuntime,
        panel_id: &str,
        node_id: &str,
    ) -> bool {
        let exists = self
            .focusable_targets(runtime)
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
        if let Some(focused) = self.focused_target.clone()
            && let Some((value, _)) = self.text_input_state_for_target(runtime, &focused)
        {
            self.ensure_text_input_editor_state(&focused, &value);
        }
        if let Some(previous) = previous.as_ref() {
            self.mark_panel_content_dirty(&previous.panel_id);
        }
        self.mark_panel_content_dirty(panel_id);
        true
    }

    /// 次 へフォーカスを移す。
    pub fn focus_next(&mut self, runtime: &PanelRuntime) -> bool {
        self.move_focus(runtime, 1)
    }

    /// 前 へフォーカスを移す。
    pub fn focus_previous(&mut self, runtime: &PanelRuntime) -> bool {
        self.move_focus(runtime, -1)
    }

    /// Focused をアクティブ化する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn activate_focused(&mut self) -> Option<PanelEvent> {
        let Some(target) = self.focused_target.clone() else {
            return None;
        };

        Some(PanelEvent::Activate {
            panel_id: target.panel_id,
            node_id: target.node_id,
        })
    }

    /// Has focused テキスト 入力 かどうかを返す。
    pub fn has_focused_text_input(&self, runtime: &PanelRuntime) -> bool {
        self.focused_target
            .as_ref()
            .and_then(|target| self.text_input_state_for_target(runtime, target))
            .is_some()
    }

    /// insert テキスト into focused 入力 に必要な処理を行う。
    pub fn insert_text_into_focused_input(
        &mut self,
        runtime: &PanelRuntime,
        text: &str,
    ) -> Option<PanelEvent> {
        let Some(target) = self.focused_target.clone() else {
            return None;
        };
        let Some((current, input_mode)) = self.text_input_state_for_target(runtime, &target) else {
            return None;
        };
        let filtered = filter_text_input(text, input_mode);
        if filtered.is_empty() {
            return None;
        }
        let mut editor_state = self.editor_state_for_target(&target, &current);
        let next_value = insert_text_at_char_index(&current, editor_state.cursor_chars, &filtered);
        editor_state.cursor_chars += text_char_len(&filtered);
        editor_state.preedit = None;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        Some(PanelEvent::SetText {
            panel_id: target.panel_id,
            node_id: target.node_id,
            value: next_value,
        })
    }

    /// backspace focused 入力 を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn backspace_focused_input(&mut self, runtime: &PanelRuntime) -> Option<PanelEvent> {
        let Some(target) = self.focused_target.clone() else {
            return None;
        };
        let Some((mut current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return None;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        if editor_state.preedit.take().is_some() {
            self.text_input_states
                .insert(text_input_state_key(&target), editor_state);
            self.mark_panel_content_dirty(&target.panel_id);
            return Some(PanelEvent::Activate {
                panel_id: target.panel_id,
                node_id: target.node_id,
            });
        }
        if current.is_empty() || editor_state.cursor_chars == 0 {
            return None;
        }
        current = remove_char_before_char_index(&current, editor_state.cursor_chars);
        editor_state.cursor_chars -= 1;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        Some(PanelEvent::SetText {
            panel_id: target.panel_id,
            node_id: target.node_id,
            value: current,
        })
    }

    /// delete focused 入力 を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn delete_focused_input(&mut self, runtime: &PanelRuntime) -> Option<PanelEvent> {
        let Some(target) = self.focused_target.clone() else {
            return None;
        };
        let Some((current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return None;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        if editor_state.preedit.take().is_some() {
            self.text_input_states
                .insert(text_input_state_key(&target), editor_state);
            self.mark_panel_content_dirty(&target.panel_id);
            return Some(PanelEvent::Activate {
                panel_id: target.panel_id,
                node_id: target.node_id,
            });
        }
        if editor_state.cursor_chars >= text_char_len(&current) {
            return None;
        }
        let next_value = remove_char_at_char_index(&current, editor_state.cursor_chars);
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        Some(PanelEvent::SetText {
            panel_id: target.panel_id,
            node_id: target.node_id,
            value: next_value,
        })
    }

    /// Move focused 入力 cursor を有効範囲へ補正して返す。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn move_focused_input_cursor(
        &mut self,
        runtime: &PanelRuntime,
        delta_chars: isize,
    ) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        editor_state.preedit = None;
        let max_chars = text_char_len(&current) as isize;
        let next_cursor =
            (editor_state.cursor_chars as isize + delta_chars).clamp(0, max_chars) as usize;
        if next_cursor == editor_state.cursor_chars {
            return false;
        }
        editor_state.cursor_chars = next_cursor;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.mark_panel_content_dirty(&target.panel_id);
        true
    }

    /// move focused 入力 cursor to start を計算して返す。
    pub fn move_focused_input_cursor_to_start(&mut self, runtime: &PanelRuntime) -> bool {
        self.set_focused_input_cursor(runtime, 0)
    }

    /// move focused 入力 cursor to end を計算して返す。
    pub fn move_focused_input_cursor_to_end(&mut self, runtime: &PanelRuntime) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return false;
        };
        self.set_focused_input_cursor(runtime, text_char_len(&current))
    }

    /// Focused 入力 preedit を設定する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn set_focused_input_preedit(
        &mut self,
        runtime: &PanelRuntime,
        preedit: Option<String>,
    ) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        if editor_state.preedit == preedit {
            return false;
        }
        editor_state.preedit = preedit.filter(|value| !value.is_empty());
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.mark_panel_content_dirty(&target.panel_id);
        true
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
    fn move_focus(&mut self, runtime: &PanelRuntime, step: isize) -> bool {
        let targets = self.focusable_targets(runtime);
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

    /// focusable targets を計算して返す。
    fn focusable_targets(&self, runtime: &PanelRuntime) -> Vec<FocusTarget> {
        let mut targets = Vec::new();
        for tree in self.panel_trees(runtime) {
            collect_focus_targets(tree.id, &tree.children, &mut targets);
        }
        targets
    }

    /// Is dropdown target かどうかを返す。
    pub(super) fn is_dropdown_target(
        &self,
        runtime: &PanelRuntime,
        panel_id: &str,
        node_id: &str,
    ) -> bool {
        self.panel_trees(runtime)
            .into_iter()
            .find(|tree| tree.id == panel_id)
            .map(|tree| find_dropdown_node(&tree.children, node_id).is_some())
            .unwrap_or(false)
    }

    /// テキスト 入力 状態 for target に必要な処理を行う。
    pub(super) fn text_input_state_for_target(
        &self,
        runtime: &PanelRuntime,
        target: &FocusTarget,
    ) -> Option<(String, TextInputMode)> {
        self.panel_trees(runtime)
            .into_iter()
            .find(|tree| tree.id == target.panel_id)
            .and_then(|tree| find_text_input_value(&tree.children, &target.node_id))
    }

    /// テキスト 入力 editor 状態 が満たされるよう整える。
    fn ensure_text_input_editor_state(&mut self, target: &FocusTarget, current_value: &str) {
        let max_chars = text_char_len(current_value);
        self.text_input_states
            .entry(text_input_state_key(target))
            .and_modify(|state| {
                state.cursor_chars = state.cursor_chars.min(max_chars);
            })
            .or_insert(TextInputEditorState {
                cursor_chars: max_chars,
                preedit: None,
            });
    }

    /// editor 状態 for target に必要な処理を行う。
    fn editor_state_for_target(
        &self,
        target: &FocusTarget,
        current_value: &str,
    ) -> TextInputEditorState {
        let mut state = self
            .text_input_states
            .get(&text_input_state_key(target))
            .cloned()
            .unwrap_or(TextInputEditorState {
                cursor_chars: text_char_len(current_value),
                preedit: None,
            });
        state.cursor_chars = state.cursor_chars.min(text_char_len(current_value));
        state
    }

    /// Focused 入力 cursor を設定する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    fn set_focused_input_cursor(&mut self, runtime: &PanelRuntime, cursor_chars: usize) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        let next_cursor = cursor_chars.min(text_char_len(&current));
        if next_cursor == editor_state.cursor_chars && editor_state.preedit.is_none() {
            return false;
        }
        editor_state.cursor_chars = next_cursor;
        editor_state.preedit = None;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.mark_panel_content_dirty(&target.panel_id);
        true
    }
}

/// 入力や種別に応じて処理を振り分ける。
fn filter_text_input(text: &str, input_mode: TextInputMode) -> String {
    match input_mode {
        TextInputMode::Text => text
            .chars()
            .filter(|character| !character.is_control())
            .collect(),
        TextInputMode::Numeric => text
            .chars()
            .filter(|character| character.is_ascii_digit())
            .collect(),
    }
}

/// 現在の テキスト 入力 状態 key を返す。
fn text_input_state_key(target: &FocusTarget) -> (String, String) {
    (target.panel_id.clone(), target.node_id.clone())
}

/// テキスト char len を計算して返す。
pub(super) fn text_char_len(text: &str) -> usize {
    text.chars().count()
}

/// 現在の byte インデックス for char インデックス を返す。
fn byte_index_for_char_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

/// 現在の insert テキスト at char インデックス を返す。
pub(super) fn insert_text_at_char_index(text: &str, char_index: usize, inserted: &str) -> String {
    let split_at = byte_index_for_char_index(text, char_index);
    let mut next = String::with_capacity(text.len() + inserted.len());
    next.push_str(&text[..split_at]);
    next.push_str(inserted);
    next.push_str(&text[split_at..]);
    next
}

/// Char before char インデックス を削除する。
fn remove_char_before_char_index(text: &str, char_index: usize) -> String {
    if char_index == 0 {
        return text.to_string();
    }
    let start = byte_index_for_char_index(text, char_index - 1);
    let end = byte_index_for_char_index(text, char_index);
    remove_byte_range(text, start, end)
}

/// Char at char インデックス を削除する。
fn remove_char_at_char_index(text: &str, char_index: usize) -> String {
    let start = byte_index_for_char_index(text, char_index);
    let end = byte_index_for_char_index(text, char_index + 1);
    if start >= end {
        return text.to_string();
    }
    remove_byte_range(text, start, end)
}

/// Byte range を削除する。
fn remove_byte_range(text: &str, start: usize, end: usize) -> String {
    let mut next = String::with_capacity(text.len().saturating_sub(end.saturating_sub(start)));
    next.push_str(&text[..start]);
    next.push_str(&text[end..]);
    next
}

/// 現在の prefix for char 件数 を返す。
#[allow(dead_code)]
pub(super) fn prefix_for_char_count(text: &str, char_count: usize) -> String {
    text.chars().take(char_count).collect()
}
