//! キーボード・IME 入力の正規化を `DesktopRuntime` へ追加する。
//!
//! 文字編集とグローバルショートカットの分岐をここへ閉じ込め、
//! `ApplicationHandler` 実装を高水準の分配だけに保つ。

use app_core::Command;
use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use super::DesktopRuntime;

impl DesktopRuntime {
    /// 現在の値を ime イベント へ変換する。
    pub(super) fn handle_ime_event(&mut self, ime: Ime) -> bool {
        match ime {
            Ime::Commit(text) => {
                self.app.set_focused_panel_input_preedit(None);
                self.app.insert_text_into_focused_panel_input(text.as_ref())
            }
            Ime::Preedit(text, _) => self
                .app
                .set_focused_panel_input_preedit(Some(text.to_string())),
            Ime::Enabled | Ime::Disabled => false,
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_keyboard_input(&mut self, event: &KeyEvent) -> bool {
        let editing_repeat =
            self.app.has_focused_panel_input() && supports_editing_repeat(&event.logical_key);
        if event.state != ElementState::Pressed || (event.repeat && !editing_repeat) {
            return false;
        }

        if self.handle_text_edit_key(&event.logical_key) {
            return true;
        }

        if let Some((shortcut, key_name)) = self.normalized_shortcut(&event.logical_key)
            && self
                .app
                .dispatch_keyboard_shortcut(&shortcut, &key_name, event.repeat)
        {
            return true;
        }

        self.handle_builtin_shortcut(&event.logical_key)
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_text_edit_key(&mut self, key: &Key) -> bool {
        if self.modifiers.control_key() || self.modifiers.alt_key() {
            return false;
        }

        match key {
            Key::Named(NamedKey::Backspace) => self.app.backspace_focused_panel_input(),
            Key::Named(NamedKey::Delete) => self.app.delete_focused_panel_input(),
            Key::Named(NamedKey::ArrowLeft) => self.app.move_focused_panel_input_cursor(-1),
            Key::Named(NamedKey::ArrowRight) => self.app.move_focused_panel_input_cursor(1),
            Key::Named(NamedKey::Home) => self.app.move_focused_panel_input_cursor_to_start(),
            Key::Named(NamedKey::End) => self.app.move_focused_panel_input_cursor_to_end(),
            Key::Named(NamedKey::Space) => self.app.insert_text_into_focused_panel_input(" "),
            Key::Character(text) => self.app.insert_text_into_focused_panel_input(text),
            _ => false,
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_builtin_shortcut(&mut self, key: &Key) -> bool {
        match key {
            Key::Character(text)
                if self.modifiers.control_key()
                    && self.modifiers.shift_key()
                    && text.eq_ignore_ascii_case("s") =>
            {
                self.app.execute_command(Command::SaveProjectAs)
            }
            Key::Character(text)
                if self.modifiers.control_key() && text.eq_ignore_ascii_case("s") =>
            {
                self.app.execute_command(Command::SaveProject)
            }
            Key::Character(text)
                if self.modifiers.control_key() && text.eq_ignore_ascii_case("o") =>
            {
                self.app.execute_command(Command::LoadProject)
            }
            Key::Character(text)
                if self.modifiers.control_key() && text.eq_ignore_ascii_case("n") =>
            {
                self.app.execute_command(Command::NewDocument)
            }
            Key::Named(NamedKey::Tab) if self.modifiers.shift_key() => {
                self.app.focus_previous_panel_control()
            }
            Key::Named(NamedKey::Tab) => self.app.focus_next_panel_control(),
            Key::Named(NamedKey::PageUp) if self.modifiers.alt_key() => {
                self.app.execute_command(Command::SelectPreviousPanel)
            }
            Key::Named(NamedKey::PageDown) if self.modifiers.alt_key() => {
                self.app.execute_command(Command::SelectNextPanel)
            }
            Key::Named(NamedKey::Home) if self.modifiers.alt_key() => {
                self.app.execute_command(Command::FocusActivePanel)
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space)
                if !self.app.has_focused_panel_input() =>
            {
                self.app.activate_focused_panel_control().is_some()
            }
            _ => false,
        }
    }

    /// 現在の値を ショートカット へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub(super) fn normalized_shortcut(&self, key: &Key) -> Option<(String, String)> {
        let key_name = normalized_key_name(key)?;
        let mut parts = Vec::new();
        if self.modifiers.control_key() {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.alt_key() {
            parts.push("Alt".to_string());
        }
        if self.modifiers.super_key() {
            parts.push("Meta".to_string());
        }
        if self.modifiers.shift_key() {
            parts.push("Shift".to_string());
        }
        parts.push(key_name.clone());
        Some((parts.join("+"), key_name))
    }
}

/// 現在の値を key 名前 へ変換する。
///
/// 値を生成できない場合は `None` を返します。
pub(super) fn normalized_key_name(key: &Key) -> Option<String> {
    match key {
        Key::Character(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_uppercase())
            }
        }
        Key::Named(named) => match named {
            NamedKey::Space => Some("Space".to_string()),
            NamedKey::Enter => Some("Enter".to_string()),
            NamedKey::Tab => Some("Tab".to_string()),
            NamedKey::Backspace => Some("Backspace".to_string()),
            NamedKey::Delete => Some("Delete".to_string()),
            NamedKey::ArrowLeft => Some("ArrowLeft".to_string()),
            NamedKey::ArrowRight => Some("ArrowRight".to_string()),
            NamedKey::ArrowUp => Some("ArrowUp".to_string()),
            NamedKey::ArrowDown => Some("ArrowDown".to_string()),
            NamedKey::Home => Some("Home".to_string()),
            NamedKey::End => Some("End".to_string()),
            NamedKey::PageUp => Some("PageUp".to_string()),
            NamedKey::PageDown => Some("PageDown".to_string()),
            NamedKey::Escape => Some("Escape".to_string()),
            NamedKey::F1 => Some("F1".to_string()),
            NamedKey::F2 => Some("F2".to_string()),
            NamedKey::F3 => Some("F3".to_string()),
            NamedKey::F4 => Some("F4".to_string()),
            NamedKey::F5 => Some("F5".to_string()),
            NamedKey::F6 => Some("F6".to_string()),
            NamedKey::F7 => Some("F7".to_string()),
            NamedKey::F8 => Some("F8".to_string()),
            NamedKey::F9 => Some("F9".to_string()),
            NamedKey::F10 => Some("F10".to_string()),
            NamedKey::F11 => Some("F11".to_string()),
            NamedKey::F12 => Some("F12".to_string()),
            NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super => None,
            other => Some(format!("{other:?}")),
        },
        _ => None,
    }
}

/// Supports editing repeat かどうかを返す。
pub(super) fn supports_editing_repeat(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Backspace
                | NamedKey::Delete
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::Space
        ) | Key::Character(_)
    )
}
