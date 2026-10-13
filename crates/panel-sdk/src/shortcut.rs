//! キーボードショートカットの capture→割当→マッチ状態機械 (BL-144)。
//!
//! tool-palette / app-actions が個別に持っていた「ショートカット設定スロット +
//! capture_target + keyboard handler 分岐」の二重実装を SDK レジストリに集約する。
//!
//! 本モジュールは **純粋なインメモリ状態機械** であり、ABI も state 永続化も持たない。
//! パネルは登録した slot のバインディングをパネルローカル state へ保存し、起動時に
//! [`ShortcutRegistry::set_binding`] で復元する。文字列比較規約は
//! `panel_protocol::keyboard` に従う (ホスト/パネル独立ハードコード禁止)。
//!
//! 状態遷移:
//! ```text
//!   [idle] --begin_capture(slot)--> [capturing(slot)]
//!   [capturing(slot)] --handle_key(k)--> 割当 (slot ← k) して [idle], Outcome::Assigned
//!   [idle] --handle_key(k)--> 一致 slot を探す
//!                              ├ 見つかれば Outcome::Triggered(slot) ([idle] のまま)
//!                              └ なければ   Outcome::Ignored             ([idle] のまま)
//! ```

use panel_protocol::keyboard;

/// 1 つのショートカットスロット (例: `"pen"` / `"save"`) と現在のバインディング。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    id: String,
    binding: String,
}

/// `handle_key` の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// capture 中だった: `slot` に `shortcut` を割り当てた。
    Assigned { slot: String, shortcut: String },
    /// idle 中に一致したスロットが発火した。
    Triggered { slot: String },
    /// 一致なし (何もしない)。
    Ignored,
}

/// capture→割当→マッチを管理するショートカットレジストリ (BL-144)。
///
/// スロットは [`define`](Self::define) した順序を保持する。`handle_key` の一致探索は
/// 定義順に最初に一致した 1 件を発火する (重複バインディングは先勝ち)。
#[derive(Debug, Clone, Default)]
pub struct ShortcutRegistry {
    slots: Vec<Slot>,
    capture_target: Option<String>,
}

impl ShortcutRegistry {
    /// 空のレジストリを作る。
    pub fn new() -> Self {
        Self::default()
    }

    /// スロットを定義する (既定バインディング付き)。
    ///
    /// 同じ `id` を再定義した場合はバインディングを上書きし、順序は維持する。
    pub fn define(&mut self, id: impl Into<String>, default_binding: impl Into<String>) {
        let id = id.into();
        let binding = default_binding.into();
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.id == id) {
            slot.binding = binding;
        } else {
            self.slots.push(Slot { id, binding });
        }
    }

    /// スロットの現在のバインディングを返す (未定義なら `None`)。
    pub fn binding(&self, id: &str) -> Option<&str> {
        self.slots
            .iter()
            .find(|slot| slot.id == id)
            .map(|slot| slot.binding.as_str())
    }

    /// 既存スロットのバインディングを差し替える (永続 config からの復元用)。
    ///
    /// 未定義の `id` は無視する (定義済みスロットのみ復元する)。
    pub fn set_binding(&mut self, id: &str, binding: impl Into<String>) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.id == id) {
            slot.binding = binding.into();
        }
    }

    /// 定義済みスロット ID を定義順に列挙する。
    pub fn slot_ids(&self) -> impl Iterator<Item = &str> {
        self.slots.iter().map(|slot| slot.id.as_str())
    }

    /// 指定スロットのバインディングを capture モードに入る。
    ///
    /// 未定義の `id` でも capture 自体は受け付ける (割当時に [`define`](Self::define)
    /// 済みでなければ無視されるのではなく、`handle_key` 側で存在チェックする)。
    /// 実用上は capture する slot は事前に define する規約。
    pub fn begin_capture(&mut self, id: impl Into<String>) {
        self.capture_target = Some(id.into());
    }

    /// capture を中断する (割当せず idle に戻る)。
    pub fn cancel_capture(&mut self) {
        self.capture_target = None;
    }

    /// 現在 capture 中のスロット ID (capture 中でなければ `None`)。
    pub fn capture_target(&self) -> Option<&str> {
        self.capture_target.as_deref()
    }

    /// capture 中かどうか。
    pub fn is_capturing(&self) -> bool {
        self.capture_target.is_some()
    }

    /// 受信したショートカット文字列を処理し、状態遷移の結果を返す。
    ///
    /// - capture 中: capture 対象スロットに `incoming` を割り当て、idle へ戻り
    ///   [`Outcome::Assigned`] を返す。capture 対象が未定義スロットなら新規に
    ///   定義する (capture したスロットを必ず保持するため)。
    /// - idle 中: 定義順で `incoming` に一致する最初のスロットを
    ///   [`Outcome::Triggered`] として返す。一致なしは [`Outcome::Ignored`]。
    ///
    /// 空文字列の `incoming` は常に [`Outcome::Ignored`] (未割当キーは無視)。
    pub fn handle_key(&mut self, incoming: &str) -> Outcome {
        if incoming.is_empty() {
            return Outcome::Ignored;
        }
        if let Some(target) = self.capture_target.take() {
            self.set_or_define(&target, incoming);
            return Outcome::Assigned {
                slot: target,
                shortcut: incoming.to_string(),
            };
        }
        for slot in &self.slots {
            if keyboard::shortcut_matches(&slot.binding, incoming) {
                return Outcome::Triggered {
                    slot: slot.id.clone(),
                };
            }
        }
        Outcome::Ignored
    }

    fn set_or_define(&mut self, id: &str, binding: &str) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.id == id) {
            slot.binding = binding.to_string();
        } else {
            self.slots.push(Slot {
                id: id.to_string(),
                binding: binding.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ShortcutRegistry {
        let mut registry = ShortcutRegistry::new();
        registry.define("pen", "P");
        registry.define("eraser", "E");
        registry.define("bucket", "G");
        registry
    }

    #[test]
    fn define_sets_default_binding_and_preserves_order() {
        let registry = registry();
        assert_eq!(registry.binding("pen"), Some("P"));
        assert_eq!(registry.binding("eraser"), Some("E"));
        assert_eq!(registry.binding("missing"), None);
        assert_eq!(
            registry.slot_ids().collect::<Vec<_>>(),
            vec!["pen", "eraser", "bucket"]
        );
    }

    #[test]
    fn redefine_overwrites_binding_without_reordering() {
        let mut registry = registry();
        registry.define("pen", "Ctrl+P");
        assert_eq!(registry.binding("pen"), Some("Ctrl+P"));
        assert_eq!(
            registry.slot_ids().collect::<Vec<_>>(),
            vec!["pen", "eraser", "bucket"]
        );
    }

    #[test]
    fn set_binding_restores_only_defined_slots() {
        let mut registry = registry();
        registry.set_binding("pen", "Shift+P");
        registry.set_binding("undefined", "X");
        assert_eq!(registry.binding("pen"), Some("Shift+P"));
        assert_eq!(registry.binding("undefined"), None);
    }

    #[test]
    fn idle_handle_key_triggers_matching_slot_case_insensitively() {
        let mut registry = registry();
        assert_eq!(
            registry.handle_key("p"),
            Outcome::Triggered {
                slot: "pen".to_string()
            }
        );
        // 発火しても capture には入らない (idle のまま)。
        assert!(!registry.is_capturing());
    }

    #[test]
    fn idle_handle_key_ignores_unmatched_and_empty() {
        let mut registry = registry();
        assert_eq!(registry.handle_key("Z"), Outcome::Ignored);
        assert_eq!(registry.handle_key(""), Outcome::Ignored);
    }

    #[test]
    fn capture_then_key_assigns_and_returns_to_idle() {
        let mut registry = registry();
        registry.begin_capture("pen");
        assert_eq!(registry.capture_target(), Some("pen"));
        assert!(registry.is_capturing());

        let outcome = registry.handle_key("Ctrl+Shift+P");
        assert_eq!(
            outcome,
            Outcome::Assigned {
                slot: "pen".to_string(),
                shortcut: "Ctrl+Shift+P".to_string()
            }
        );
        assert_eq!(registry.binding("pen"), Some("Ctrl+Shift+P"));
        assert!(!registry.is_capturing());
    }

    #[test]
    fn capture_does_not_trigger_existing_binding() {
        // capture 中は受信キーが既存バインディングに一致しても割当が優先される。
        let mut registry = registry();
        registry.begin_capture("bucket");
        let outcome = registry.handle_key("P"); // "pen" の既定と一致するが…
        assert_eq!(
            outcome,
            Outcome::Assigned {
                slot: "bucket".to_string(),
                shortcut: "P".to_string()
            }
        );
        // bucket が P を奪う。
        assert_eq!(registry.binding("bucket"), Some("P"));
    }

    #[test]
    fn cancel_capture_returns_to_idle_without_assigning() {
        let mut registry = registry();
        registry.begin_capture("pen");
        registry.cancel_capture();
        assert!(!registry.is_capturing());
        // 元のバインディングのまま。
        assert_eq!(registry.binding("pen"), Some("P"));
    }

    #[test]
    fn capture_empty_key_stays_capturing() {
        // 空キーは無視され、capture 状態は維持される。
        let mut registry = registry();
        registry.begin_capture("pen");
        assert_eq!(registry.handle_key(""), Outcome::Ignored);
        assert!(registry.is_capturing());
    }

    #[test]
    fn first_defined_slot_wins_on_duplicate_binding() {
        let mut registry = ShortcutRegistry::new();
        registry.define("first", "Q");
        registry.define("second", "Q");
        assert_eq!(
            registry.handle_key("Q"),
            Outcome::Triggered {
                slot: "first".to_string()
            }
        );
    }

    #[test]
    fn capture_undefined_slot_defines_it_on_assign() {
        let mut registry = ShortcutRegistry::new();
        registry.begin_capture("brand_new");
        let outcome = registry.handle_key("F1");
        assert_eq!(
            outcome,
            Outcome::Assigned {
                slot: "brand_new".to_string(),
                shortcut: "F1".to_string()
            }
        );
        assert_eq!(registry.binding("brand_new"), Some("F1"));
    }
}
