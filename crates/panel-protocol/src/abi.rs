//! Wasm パネル境界の ABI 定数 (BL-103)。
//!
//! ホスト (`panel-wasm-host`) と SDK (`panel-sdk`) が **同一の契約**を共有するための
//! 単一定義点。従来は host / SDK / マクロが各々ハードコードしていた以下を集約する:
//!
//! - パネルが export する ライフサイクル関数名 (`panel_init` / `panel_sync_host`) と
//!   イベント handler の export 名 prefix (`panel_handle_`)
//! - ホストが提供する import module 名 (`host` / `dom`)
//! - `DiagnosticLevel` ↔ `i32` の対応 ([`DiagnosticLevel::from_abi`] /
//!   [`DiagnosticLevel::to_abi`])
//! - イベント payload のスカラ値を運ぶ予約キー `"value"` ([`PAYLOAD_VALUE_KEY`])
//!
//! 註: 個々の wire 名 (export/import 名) は Wasm ABI の共有契約点であり、改名は
//! B9 (P26) まで温存する。本モジュールは値を変えずに **置き場**を 1 箇所へ集約する。

use crate::DiagnosticLevel;

/// パネルが export する初期化関数名。
pub const PANEL_INIT_EXPORT: &str = "panel_init";

/// パネルが export する host state 同期 (再描画) フック名。
///
/// 「パネルがホストを sync する」と読めるが実態は host 状態変化時の再描画フック。
/// 改名 (`panel_on_host_change`) は B9 (P26)。
pub const PANEL_SYNC_HOST_EXPORT: &str = "panel_sync_host";

/// イベント handler の export 名 prefix。実 export 名は
/// `panel_handle_<sanitized_handler_name>`。
pub const PANEL_HANDLE_EXPORT_PREFIX: &str = "panel_handle_";

/// ホストが提供する一般 host function の import module 名
/// (`state_*` / `host_get_*` / `event_get_*` / `command*` / `diagnostic`)。
pub const HOST_IMPORT_MODULE: &str = "host";

/// DOM mutation host function の import module 名 (`query_selector` / `set_attribute` 等)。
pub const DOM_IMPORT_MODULE: &str = "dom";

/// イベント payload のスカラ値を運ぶ予約キー。
///
/// `i32` 1 引数の handler 呼出では `event_payload["value"]` を引数へ写す。
/// SetValue / DragValue / SetText 等のホスト側イベントも同キーに値を格納する。
pub const PAYLOAD_VALUE_KEY: &str = "value";

/// `event_payload` 全体の JSON バイト長を返す host function 名 (BL-141)。
///
/// **handler payload 規約**: パネルのイベント handler (`panel_handle_<name>`) は
/// 0 個または 1 個の引数を取る:
///
/// - **引数なし** (`fn handler()`): payload を読まない handler。
/// - **typed payload** (`fn handler(payload: T)` で `T: serde::Deserialize + Default`):
///   ホストの `event_payload` 全体を本 ABI ([`EVENT_PAYLOAD_JSON_LEN`] /
///   [`EVENT_PAYLOAD_JSON_COPY`]) で 1 回取得し、`T` へ serde デシリアライズして渡す。
///   `event_string` 等での暗黙 payload 読みを廃止する。
/// - **legacy i32** (`fn handler(value: i32)`): 移行期間のみ許可。`event_payload["value"]`
///   ([`PAYLOAD_VALUE_KEY`]) を `i32` で渡す。typed payload への移行完了時に撤去する。
pub const EVENT_PAYLOAD_JSON_LEN: &str = "event_get_payload_json_len";

/// `event_payload` 全体の JSON を Wasm バッファへコピーする host function 名 (BL-141)。
pub const EVENT_PAYLOAD_JSON_COPY: &str = "event_get_payload_json_copy";

/// handler の export 名を組み立てる (`panel_handle_<sanitized>`)。
///
/// `sanitize_handler_name` は ASCII 英数字以外を `_` に置換する (host / SDK 共通規約)。
pub fn handler_export_name(handler_name: &str) -> String {
    format!(
        "{PANEL_HANDLE_EXPORT_PREFIX}{}",
        sanitize_handler_name(handler_name)
    )
}

/// handler 名を export 名に使える形へ正規化する (ASCII 英数字以外を `_` に置換)。
pub fn sanitize_handler_name(handler_name: &str) -> String {
    handler_name
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => character,
            _ => '_',
        })
        .collect()
}

impl DiagnosticLevel {
    /// ABI 上の `i32` 表現から `DiagnosticLevel` を復元する。
    /// `0 = Info` / `1 = Warning` / それ以外 = `Error`。
    pub fn from_abi(level: i32) -> Self {
        match level {
            0 => DiagnosticLevel::Info,
            1 => DiagnosticLevel::Warning,
            _ => DiagnosticLevel::Error,
        }
    }

    /// `DiagnosticLevel` を ABI 上の `i32` 表現へ変換する。
    pub fn to_abi(self) -> i32 {
        match self {
            DiagnosticLevel::Info => 0,
            DiagnosticLevel::Warning => 1,
            DiagnosticLevel::Error => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_level_abi_roundtrips() {
        for level in [
            DiagnosticLevel::Info,
            DiagnosticLevel::Warning,
            DiagnosticLevel::Error,
        ] {
            assert_eq!(DiagnosticLevel::from_abi(level.to_abi()), level);
        }
    }

    #[test]
    fn diagnostic_level_abi_values_are_stable() {
        assert_eq!(DiagnosticLevel::Info.to_abi(), 0);
        assert_eq!(DiagnosticLevel::Warning.to_abi(), 1);
        assert_eq!(DiagnosticLevel::Error.to_abi(), 2);
    }

    #[test]
    fn unknown_abi_level_maps_to_error() {
        assert_eq!(DiagnosticLevel::from_abi(99), DiagnosticLevel::Error);
        assert_eq!(DiagnosticLevel::from_abi(-1), DiagnosticLevel::Error);
    }

    #[test]
    fn handler_export_name_uses_prefix_and_sanitizes() {
        assert_eq!(handler_export_name("toggle-expanded"), "panel_handle_toggle_expanded");
        assert_eq!(handler_export_name("save_project"), "panel_handle_save_project");
    }
}
