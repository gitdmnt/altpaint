//! Wasm linear memory との読み書きヘルパ。
//!
//! host function 群が共通で使う「`memory` export を引く」「ptr/len から UTF-8 を読む」
//! 「ホスト文字列を Wasm 側バッファへコピーする」操作と、診断追加のショートカットを集約する。

use crate::HostCallContext;
use panel_protocol::Diagnostic;
use wasmtime::{Caller, Extern, Memory};

/// Wasm インスタンスの `memory` export を取得する。存在しなければ `None`。
pub(crate) fn current_memory(caller: &mut Caller<'_, HostCallContext>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(memory)) => Some(memory),
        _ => None,
    }
}

/// `ptr`/`len` で示される Wasm メモリ範囲を UTF-8 文字列として読む。
///
/// 範囲外・不正な ptr/len・非 UTF-8 はいずれも `None`。
pub(crate) fn read_utf8(
    caller: &mut Caller<'_, HostCallContext>,
    ptr: i32,
    len: i32,
) -> Option<String> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let memory = current_memory(caller)?;
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start.checked_add(len as usize)?;
    let bytes = data.get(start..end)?;
    std::str::from_utf8(bytes).ok().map(ToString::to_string)
}

/// ホスト側の文字列を Wasm 側バッファ (`buffer_ptr`/`buffer_len`) へコピーする。
///
/// バッファに収まらない分は切り詰める。`memory` 不在・不正な範囲のときは
/// 診断を追加して何もしない。
pub(crate) fn write_str_to_buffer(
    caller: &mut Caller<'_, HostCallContext>,
    value: &str,
    buffer_ptr: i32,
    buffer_len: i32,
    context: &str,
) {
    let Some(memory) = current_memory(caller) else {
        push_error(caller, format!("missing wasm memory for {context}"));
        return;
    };
    if buffer_ptr < 0 || buffer_len < 0 {
        push_error(caller, format!("invalid buffer range for {context}"));
        return;
    }
    let data = memory.data_mut(&mut *caller);
    let start = buffer_ptr as usize;
    let end = start.saturating_add(buffer_len as usize).min(data.len());
    let Some(target) = data.get_mut(start..end) else {
        push_error(caller, format!("buffer range out of bounds for {context}"));
        return;
    };
    let bytes = value.as_bytes();
    let count = bytes.len().min(target.len());
    target[..count].copy_from_slice(&bytes[..count]);
}

/// 現在の host call に error 診断を 1 件追加する。
pub(crate) fn push_error(caller: &mut Caller<'_, HostCallContext>, message: impl Into<String>) {
    caller
        .data_mut()
        .result
        .diagnostics
        .push(Diagnostic::error(message));
}
