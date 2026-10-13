//! Wasm host ABI の最下層: import 宣言・ptr/len 変換・request 発行 (BL-147)。
//!
//! `host` import module の `extern "C"` 宣言と、それを呼ぶための ptr/len 変換
//! (`with_bytes` / `read_string`) を保持する。上位の state / events / diagnostics
//! モジュールはこの層のヘルパだけを使い、生 FFI を直接触らない。

use panel_protocol::RequestDescriptor;
#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "host")]
unsafe extern "C" {
    pub(super) fn state_toggle(ptr: i32, len: i32);
    pub(super) fn state_set_bool(ptr: i32, len: i32, value: i32);
    pub(super) fn state_set_i32(ptr: i32, len: i32, value: i32);
    pub(super) fn state_set_string(path_ptr: i32, path_len: i32, value_ptr: i32, value_len: i32);
    pub(super) fn state_apply_json(ptr: i32, len: i32);
    pub(super) fn state_get_bool(ptr: i32, len: i32) -> i32;
    pub(super) fn state_get_i32(ptr: i32, len: i32) -> i32;
    pub(super) fn state_get_string_len(ptr: i32, len: i32) -> i32;
    pub(super) fn state_get_string_copy(
        path_ptr: i32,
        path_len: i32,
        buffer_ptr: i32,
        buffer_len: i32,
    );
    pub(super) fn event_get_string_len(ptr: i32, len: i32) -> i32;
    pub(super) fn event_get_string_copy(
        path_ptr: i32,
        path_len: i32,
        buffer_ptr: i32,
        buffer_len: i32,
    );
    pub(super) fn event_get_payload_json_len() -> i32;
    pub(super) fn event_get_payload_json_copy(buffer_ptr: i32, buffer_len: i32);
    pub(super) fn host_get_bool(ptr: i32, len: i32) -> i32;
    pub(super) fn host_get_i32(ptr: i32, len: i32) -> i32;
    pub(super) fn host_get_string_len(ptr: i32, len: i32) -> i32;
    pub(super) fn host_get_string_copy(
        path_ptr: i32,
        path_len: i32,
        buffer_ptr: i32,
        buffer_len: i32,
    );
    pub(super) fn host_get_section_json_len(ptr: i32, len: i32) -> i32;
    pub(super) fn host_get_section_json_copy(
        key_ptr: i32,
        key_len: i32,
        buffer_ptr: i32,
        buffer_len: i32,
    );
    fn command(ptr: i32, len: i32);
    fn command_string(
        name_ptr: i32,
        name_len: i32,
        key_ptr: i32,
        key_len: i32,
        value_ptr: i32,
        value_len: i32,
    );
    fn command_json(name_ptr: i32, name_len: i32, json_ptr: i32, json_len: i32);
    pub(super) fn diagnostic(level: i32, ptr: i32, len: i32);
}

/// wasm / native 対の関数を 1 つの宣言から生成する (BL-147)。
///
/// パネルランタイムの host ABI ラッパは「wasm では FFI を呼び、native では既定値を
/// 返す no-op」という対で常に現れる。この対を `#[cfg]` 2 連で手書きする重複を畳む。
///
/// 形式:
/// ```ignore
/// wasm_or_native! {
///     /// doc
///     pub fn set_state_bool(path: impl AsRef<str>, value: bool) -> () {
///         wasm: { with_bytes(path.as_ref(), |ptr, len| unsafe { state_set_bool(ptr, len, value.into()) }); }
///         native: {}
///     }
/// }
/// ```
/// `native` ブロックは引数を `_` で受けるための束縛を自前で行う (未使用警告回避)。
macro_rules! wasm_or_native {
    (
        $(#[$meta:meta])*
        pub fn $name:ident ( $($arg:ident : $arg_ty:ty),* $(,)? ) -> $ret:ty {
            wasm: $wasm:block
            native: $native:block
        }
    ) => {
        $(#[$meta])*
        #[cfg(target_arch = "wasm32")]
        pub fn $name ( $($arg : $arg_ty),* ) -> $ret $wasm

        $(#[$meta])*
        #[cfg(not(target_arch = "wasm32"))]
        pub fn $name ( $($arg : $arg_ty),* ) -> $ret {
            $( let _ = &$arg; )*
            $native
        }
    };
}

pub(super) use wasm_or_native;

/// 文字列スライスの ptr/len を取り出してクロージャへ渡す (wasm 専用)。
#[cfg(target_arch = "wasm32")]
pub(super) fn with_bytes<T>(value: &str, f: impl FnOnce(i32, i32) -> T) -> T {
    f(value.as_ptr() as i32, value.len() as i32)
}

/// `(len_fn, copy_fn)` 形式の host getter から UTF-8 文字列を読む (wasm 専用)。
#[cfg(target_arch = "wasm32")]
pub(super) fn read_string(
    path: &str,
    length_fn: unsafe extern "C" fn(i32, i32) -> i32,
    copy_fn: unsafe extern "C" fn(i32, i32, i32, i32),
) -> String {
    let length = with_bytes(path, |ptr, len| unsafe { length_fn(ptr, len) });
    if length <= 0 {
        return String::new();
    }

    let mut buffer = vec![0u8; length as usize];
    with_bytes(path, |path_ptr, path_len| unsafe {
        copy_fn(
            path_ptr,
            path_len,
            buffer.as_mut_ptr() as i32,
            buffer.len() as i32,
        )
    });
    String::from_utf8(buffer).unwrap_or_default()
}

/// `RequestDescriptor` を host へ発行する (BL-140 / P27)。
///
/// command / service の区別は提示しない。host 側の translator registry が
/// 名前空間 prefix で静的に振り分ける。payload の形状に応じて、引数なし
/// (`command`) / 単一文字列 (`command_string`) / JSON (`command_json`) の
/// 最適な ABI を選ぶ。
#[cfg(target_arch = "wasm32")]
pub fn emit_request(descriptor: &RequestDescriptor) {
    match descriptor.payload.len() {
        0 => with_bytes(&descriptor.name, |ptr, len| unsafe { command(ptr, len) }),
        1 => {
            let (key, value) = descriptor.payload.iter().next().expect("payload exists");
            match value {
                Value::String(value) => with_bytes(&descriptor.name, |name_ptr, name_len| {
                    with_bytes(key, |key_ptr, key_len| {
                        with_bytes(value, |value_ptr, value_len| unsafe {
                            command_string(
                                name_ptr, name_len, key_ptr, key_len, value_ptr, value_len,
                            )
                        })
                    })
                }),
                Value::Bool(value) => {
                    emit_request_payload_json(descriptor, &serde_json::json!({ key: value }))
                }
                Value::Number(value) => {
                    emit_request_payload_json(descriptor, &serde_json::json!({ key: value }))
                }
                _ => emit_request_payload_json(
                    descriptor,
                    &Value::Object(descriptor.payload.clone()),
                ),
            }
        }
        _ => emit_request_payload_json(descriptor, &Value::Object(descriptor.payload.clone())),
    }
}

#[cfg(target_arch = "wasm32")]
fn emit_request_payload_json(descriptor: &RequestDescriptor, payload: &Value) {
    let Ok(json) = serde_json::to_string(payload) else {
        super::diagnostics::error("failed to serialize request payload in panel-sdk runtime");
        return;
    };

    with_bytes(&descriptor.name, |name_ptr, name_len| {
        with_bytes(&json, |json_ptr, json_len| unsafe {
            command_json(name_ptr, name_len, json_ptr, json_len)
        })
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn emit_request(_descriptor: &RequestDescriptor) {}
