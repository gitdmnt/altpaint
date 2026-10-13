//! キーボードショートカット文字列の正規化規約 (BL-152)。
//!
//! ホスト (desktop の入力層) はキー押下を `"Ctrl+Shift+S"` のような単一の文字列へ
//! 正規化し、`keyboard` handler の `event_payload["shortcut"]` としてパネルへ配る。
//! パネル (app-actions / tool-palette) は受け取ったショートカット文字列を、自身が
//! 保持する設定値と **大小無視で**比較する。
//!
//! ホスト側の組み立てとパネル側の比較が同じ規約に従うための単一定義点:
//! - 修飾キーの順序 ([`MODIFIER_ORDER`]) と区切り ([`SEPARATOR`])
//! - 文字キーの大文字化規約 ([`normalize_key_name`])
//! - 設定値との一致判定 ([`shortcut_matches`])
//!
//! 規約値のリテラル直書き (ホスト/パネル独立ハードコード) を禁止する。

/// 修飾キーと基底キーを連結する区切り文字。
pub const SEPARATOR: &str = "+";

/// `Ctrl` 修飾キーのトークン。
pub const MODIFIER_CTRL: &str = "Ctrl";
/// `Alt` 修飾キーのトークン。
pub const MODIFIER_ALT: &str = "Alt";
/// `Meta` (Super / Command) 修飾キーのトークン。
pub const MODIFIER_META: &str = "Meta";
/// `Shift` 修飾キーのトークン。
pub const MODIFIER_SHIFT: &str = "Shift";

/// 正規化文字列に並べる修飾キーの順序 (Ctrl → Alt → Meta → Shift)。
///
/// ホストはこの順序で押下中の修飾キーを連結する。順序が一意であることにより、
/// 同じキー組合せが常に同じ文字列へ正規化される。
pub const MODIFIER_ORDER: [&str; 4] = [MODIFIER_CTRL, MODIFIER_ALT, MODIFIER_META, MODIFIER_SHIFT];

/// 基底キー名を正規化する (文字キーは大文字化)。
///
/// 文字キー (`"s"` 等) は大小無視のため大文字へ揃える。名前付きキー (`"Enter"`、
/// `"PageUp"` 等) は呼出側が正準形を渡す前提でそのまま返す。前後の空白は除去する。
pub fn normalize_key_name(key_name: &str) -> String {
    key_name.trim().to_uppercase()
}

/// 修飾キートークン列と基底キー名から正規化ショートカット文字列を組み立てる。
///
/// `modifiers` は [`MODIFIER_ORDER`] の順に整列済みであることを前提とする
/// (ホストは順序通りに push する)。基底キー名は呼出側が正準形 (文字キーなら
/// 大文字化済み) を渡す。
pub fn join_shortcut(modifiers: &[&str], key_name: &str) -> String {
    let mut parts: Vec<&str> = modifiers.to_vec();
    parts.push(key_name);
    parts.join(SEPARATOR)
}

/// 設定済みショートカットと受信ショートカットが一致するか (大小無視)。
///
/// 設定が空文字列なら常に不一致 (未割当の意味)。
pub fn shortcut_matches(configured: &str, incoming: &str) -> bool {
    !configured.is_empty() && configured.eq_ignore_ascii_case(incoming)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_order_is_ctrl_alt_meta_shift() {
        assert_eq!(MODIFIER_ORDER, ["Ctrl", "Alt", "Meta", "Shift"]);
    }

    #[test]
    fn separator_is_plus() {
        assert_eq!(SEPARATOR, "+");
    }

    #[test]
    fn normalize_key_name_uppercases_and_trims() {
        assert_eq!(normalize_key_name("s"), "S");
        assert_eq!(normalize_key_name("  a  "), "A");
        assert_eq!(normalize_key_name("Enter"), "ENTER");
    }

    #[test]
    fn join_shortcut_orders_modifiers_then_key() {
        assert_eq!(join_shortcut(&[MODIFIER_CTRL], "S"), "Ctrl+S");
        assert_eq!(
            join_shortcut(&[MODIFIER_CTRL, MODIFIER_SHIFT], "S"),
            "Ctrl+Shift+S"
        );
        assert_eq!(join_shortcut(&[], "Enter"), "Enter");
    }

    #[test]
    fn shortcut_matches_is_case_insensitive() {
        assert!(shortcut_matches("Ctrl+S", "ctrl+s"));
        assert!(shortcut_matches("Ctrl+Shift+S", "CTRL+SHIFT+S"));
    }

    #[test]
    fn empty_configured_shortcut_never_matches() {
        assert!(!shortcut_matches("", "Ctrl+S"));
        assert!(!shortcut_matches("", ""));
    }
}
