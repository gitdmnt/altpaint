//! パネルローカル state とホスト state の読み書き ABI (BL-147)。
//!
//! - パネルローカル state: パネル自身が持つ key/value。`set_state_*` / `state_*` /
//!   [`StatePatchBuffer`] / [`apply_state_patches`]。
//! - ホスト state: ホスト→パネルへ配る読み取り専用 state。`host_*` getter と
//!   セクション JSON 取得 (`host_section_json` / [`host_section`])。

use panel_protocol::StatePatch;

#[cfg(target_arch = "wasm32")]
use super::abi::{read_string, with_bytes};
use super::abi::wasm_or_native;

// ---- パネルローカル state: 書き込み ----

wasm_or_native! {
    /// パネルローカル真偽値 state をトグルする。
    pub fn toggle_state(path: impl AsRef<str>) -> () {
        wasm: { with_bytes(path.as_ref(), |ptr, len| unsafe { super::abi::state_toggle(ptr, len) }); }
        native: {}
    }
}

wasm_or_native! {
    /// パネルローカル真偽値 state をセットする。
    pub fn set_state_bool(path: impl AsRef<str>, value: bool) -> () {
        wasm: {
            with_bytes(path.as_ref(), |ptr, len| unsafe {
                super::abi::state_set_bool(ptr, len, i32::from(value))
            });
        }
        native: {}
    }
}

wasm_or_native! {
    /// パネルローカル整数 state をセットする。
    pub fn set_state_i32(path: impl AsRef<str>, value: i32) -> () {
        wasm: {
            with_bytes(path.as_ref(), |ptr, len| unsafe {
                super::abi::state_set_i32(ptr, len, value)
            });
        }
        native: {}
    }
}

wasm_or_native! {
    /// パネルローカル文字列 state をセットする。
    pub fn set_state_string(path: impl AsRef<str>, value: impl AsRef<str>) -> () {
        wasm: {
            with_bytes(path.as_ref(), |path_ptr, path_len| {
                with_bytes(value.as_ref(), |value_ptr, value_len| unsafe {
                    super::abi::state_set_string(path_ptr, path_len, value_ptr, value_len)
                })
            });
        }
        native: {}
    }
}

/// state patch のバッチをまとめて適用する。
#[cfg(target_arch = "wasm32")]
pub fn apply_state_patches(patches: &[StatePatch]) {
    let Ok(serialized) = serde_json::to_string(patches) else {
        super::diagnostics::error("failed to serialize state patch batch in panel-sdk runtime");
        return;
    };
    with_bytes(&serialized, |ptr, len| unsafe {
        super::abi::state_apply_json(ptr, len)
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_state_patches(_patches: &[StatePatch]) {}

/// まとめて適用する state patch バッファ。
#[derive(Debug, Default, Clone)]
pub struct StatePatchBuffer {
    patches: Vec<StatePatch>,
}

impl StatePatchBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    pub fn push(&mut self, patch: StatePatch) {
        self.patches.push(patch);
    }

    pub fn set_bool(&mut self, path: impl Into<String>, value: bool) {
        self.push(StatePatch::set(path.into(), value));
    }

    pub fn set_i32(&mut self, path: impl Into<String>, value: i32) {
        self.push(StatePatch::set(path.into(), value));
    }

    pub fn set_string(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.push(StatePatch::set(path.into(), value.into()));
    }

    pub fn set_json(&mut self, path: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.push(StatePatch::set(path.into(), value.into()));
    }

    pub fn toggle(&mut self, path: impl Into<String>) {
        self.push(StatePatch::toggle(path.into()));
    }

    pub fn apply(&self) {
        apply_state_patches(&self.patches);
    }

    pub fn into_vec(self) -> Vec<StatePatch> {
        self.patches
    }
}

// ---- パネルローカル state: 読み取り ----

wasm_or_native! {
    /// パネルローカル真偽値 state を読む。
    pub fn state_bool(path: impl AsRef<str>) -> bool {
        wasm: { with_bytes(path.as_ref(), |ptr, len| unsafe { super::abi::state_get_bool(ptr, len) != 0 }) }
        native: { false }
    }
}

wasm_or_native! {
    /// パネルローカル整数 state を読む。
    pub fn state_i32(path: impl AsRef<str>) -> i32 {
        wasm: { with_bytes(path.as_ref(), |ptr, len| unsafe { super::abi::state_get_i32(ptr, len) }) }
        native: { 0 }
    }
}

wasm_or_native! {
    /// パネルローカル文字列 state を読む。
    pub fn state_string(path: impl AsRef<str>) -> String {
        wasm: { read_string(path.as_ref(), super::abi::state_get_string_len, super::abi::state_get_string_copy) }
        native: { String::new() }
    }
}

// ---- ホスト state: 読み取り ----

wasm_or_native! {
    /// ホスト state の真偽値を読む。
    pub fn host_bool(path: impl AsRef<str>) -> bool {
        wasm: { with_bytes(path.as_ref(), |ptr, len| unsafe { super::abi::host_get_bool(ptr, len) != 0 }) }
        native: { false }
    }
}

wasm_or_native! {
    /// ホスト state の整数を読む。
    pub fn host_i32(path: impl AsRef<str>) -> i32 {
        wasm: { with_bytes(path.as_ref(), |ptr, len| unsafe { super::abi::host_get_i32(ptr, len) }) }
        native: { 0 }
    }
}

wasm_or_native! {
    /// ホスト state の文字列を読む。
    pub fn host_string(path: impl AsRef<str>) -> String {
        wasm: { read_string(path.as_ref(), super::abi::host_get_string_len, super::abi::host_get_string_copy) }
        native: { String::new() }
    }
}

wasm_or_native! {
    /// host state の トップレベルセクション (例 `"document"`) を JSON 文字列で 1 回取得する
    /// (BL-142)。個別 path getter を値の数だけ呼ぶ代わりに、セクションをまとめて読む。
    pub fn host_section_json(section: impl AsRef<str>) -> String {
        wasm: {
            read_string(
                section.as_ref(),
                super::abi::host_get_section_json_len,
                super::abi::host_get_section_json_copy,
            )
        }
        native: { String::new() }
    }
}

/// host state のセクションを取得し、型付き DTO へ serde デシリアライズする (BL-142)。
///
/// `section` は `panel_protocol::host_state::section` の定数を渡す。デシリアライズに
/// 失敗した場合 (native ビルドの空文字列含む) は `None`。
pub fn host_section<T: serde::de::DeserializeOwned>(section: impl AsRef<str>) -> Option<T> {
    let json = host_section_json(section);
    if json.is_empty() {
        return None;
    }
    serde_json::from_str(&json).ok()
}
