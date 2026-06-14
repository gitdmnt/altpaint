//! パネル→ホストの診断ログ ABI (BL-147)。
//!
//! `DiagnosticLevel` を i32 へ落として host の `diagnostic` import を呼ぶ。
//! ホスト側 (panel-runtime) はこれを log/tracing へ流す (BL-102)。

use super::abi::wasm_or_native;
#[cfg(target_arch = "wasm32")]
use super::abi::with_bytes;

wasm_or_native! {
    /// 情報レベルの診断を出力する。
    pub fn info(message: &str) -> () {
        wasm: {
            let level = panel_protocol::DiagnosticLevel::Info.to_abi();
            with_bytes(message, |ptr, len| unsafe { super::abi::diagnostic(level, ptr, len) });
        }
        native: {}
    }
}

wasm_or_native! {
    /// 警告レベルの診断を出力する。
    pub fn warn(message: &str) -> () {
        wasm: {
            let level = panel_protocol::DiagnosticLevel::Warning.to_abi();
            with_bytes(message, |ptr, len| unsafe { super::abi::diagnostic(level, ptr, len) });
        }
        native: {}
    }
}

wasm_or_native! {
    /// エラーレベルの診断を出力する。
    pub fn error(message: &str) -> () {
        wasm: {
            let level = panel_protocol::DiagnosticLevel::Error.to_abi();
            with_bytes(message, |ptr, len| unsafe { super::abi::diagnostic(level, ptr, len) });
        }
        native: {}
    }
}
