//! `UiShell` の focus と text input 編集状態をまとめる。
//!
//! pure helper と shell state 更新を切り分け、IME/preedit を含む text input 処理を
//! `UiShell` 本体から独立して保守できるようにする。

use super::tree_query::{collect_focus_targets, find_dropdown_node, find_text_input_value};
use super::*;
use panel_runtime::PanelRuntime;
use panel_api::{PanelEvent, TextInputMode};

impl PanelPresentation {
    /// 指定 panel node へ focus を移す。
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

    /// 次の focusable target へ移動する。
    pub fn focus_next(&mut self, runtime: &PanelRuntime) -> bool {
        self.move_focus(runtime, 1)
    }

    /// 前の focusable target へ移動する。
    pub fn focus_previous(&mut self, runtime: &PanelRuntime) -> bool {
        self.move_focus(runtime, -1)
    }

    /// 現在 focus 中の node を activate する。
    pub fn activate_focused(&mut self) -> Option<PanelEvent> {
        let Some(target) = self.focused_target.clone() else {
            return None;
        };

        Some(PanelEvent::Activate {
            panel_id: target.panel_id,
            node_id: target.node_id,
        })
    }

    /// focus 中の node が text input かを返す。
    pub fn has_focused_text_input(&self, runtime: &PanelRuntime) -> bool {
        self.focused_target
            .as_ref()
            .and_then(|target| self.text_input_state_for_target(runtime, target))
            .is_some()
    }

    /// focus 中の text input へ文字列を挿入する。
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

    /// focus 中の text input で backspace を実行する。
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

    /// focus 中の text input で delete を実行する。
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

    /// focus 中の text input の caret を相対移動する。
    pub fn move_focused_input_cursor(&mut self, runtime: &PanelRuntime, delta_chars: isize) -> bool {
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

    /// caret を先頭へ移動する。
    pub fn move_focused_input_cursor_to_start(&mut self, runtime: &PanelRuntime) -> bool {
        self.set_focused_input_cursor(runtime, 0)
    }

    /// caret を末尾へ移動する。
    pub fn move_focused_input_cursor_to_end(&mut self, runtime: &PanelRuntime) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(runtime, &target) else {
            return false;
        };
        self.set_focused_input_cursor(runtime, text_char_len(&current))
    }

    /// IME preedit を設定する。
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

    /// focusable targets を巡回して focus を移動する。
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

    /// 現在表示中の tree から focusable target 一覧を構築する。
    fn focusable_targets(&self, runtime: &PanelRuntime) -> Vec<FocusTarget> {
        let mut targets = Vec::new();
        for tree in self.panel_trees(runtime) {
            collect_focus_targets(tree.id, &tree.children, &mut targets);
        }
        targets
    }

    /// 指定 target が dropdown かを判定する。
    pub(super) fn is_dropdown_target(&self, runtime: &PanelRuntime, panel_id: &str, node_id: &str) -> bool {
        self.panel_trees(runtime)
            .into_iter()
            .find(|tree| tree.id == panel_id)
            .map(|tree| find_dropdown_node(&tree.children, node_id).is_some())
            .unwrap_or(false)
    }

    /// target に対応する text input 現在値を取得する。
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

    /// editor state が未初期化なら作成し、既存なら cursor を補正する。
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

    /// target に対する editor state を取得し、現在値に合わせて補正する。
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

    /// focus 中 input の caret を絶対位置へ移す。
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

/// input mode に応じて挿入可能文字だけを残す。
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

/// editor state map 用のキーを作る。
fn text_input_state_key(target: &FocusTarget) -> (String, String) {
    (target.panel_id.clone(), target.node_id.clone())
}

/// 文字数ベースで UTF-8 文字列長を返す。
pub(super) fn text_char_len(text: &str) -> usize {
    text.chars().count()
}

/// 文字 index を byte index へ変換する。
fn byte_index_for_char_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

/// 文字 index 位置へ文字列を挿入する。
pub(super) fn insert_text_at_char_index(text: &str, char_index: usize, inserted: &str) -> String {
    let split_at = byte_index_for_char_index(text, char_index);
    let mut next = String::with_capacity(text.len() + inserted.len());
    next.push_str(&text[..split_at]);
    next.push_str(inserted);
    next.push_str(&text[split_at..]);
    next
}

/// caret 前方の 1 文字を削除する。
fn remove_char_before_char_index(text: &str, char_index: usize) -> String {
    if char_index == 0 {
        return text.to_string();
    }
    let start = byte_index_for_char_index(text, char_index - 1);
    let end = byte_index_for_char_index(text, char_index);
    remove_byte_range(text, start, end)
}

/// caret 位置の 1 文字を削除する。
fn remove_char_at_char_index(text: &str, char_index: usize) -> String {
    let start = byte_index_for_char_index(text, char_index);
    let end = byte_index_for_char_index(text, char_index + 1);
    if start >= end {
        return text.to_string();
    }
    remove_byte_range(text, start, end)
}

/// byte range を削除した新文字列を返す。
fn remove_byte_range(text: &str, start: usize, end: usize) -> String {
    let mut next = String::with_capacity(text.len().saturating_sub(end.saturating_sub(start)));
    next.push_str(&text[..start]);
    next.push_str(&text[end..]);
    next
}

/// 先頭から char_count 文字だけを取り出す。
#[allow(dead_code)]
pub(super) fn prefix_for_char_count(text: &str, char_count: usize) -> String {
    text.chars().take(char_count).collect()
}
