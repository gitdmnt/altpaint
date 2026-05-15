//! キーボード入力の正規化を `DesktopRuntime` へ追加する。
//!
//! ADR 014 以降、テキスト入力 / IME 編集は HTML パネル内部の DOM mutation で完結する。
//! ui-shell 側のテキスト editor state はすべて撤去済み。
//! ここではアプリ全体のグローバルショートカットだけを扱う。

use app_core::Command;
use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use super::DesktopRuntime;

impl DesktopRuntime {
    /// IME イベントは HTML パネル内部で完結するため、winit 経由では消費しない。
    pub(super) fn handle_ime_event(&mut self, _ime: Ime) -> bool {
        false
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_keyboard_input(&mut self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed || event.repeat {
            return false;
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
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
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
