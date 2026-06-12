//! Wasm から Blitz `HtmlDocument` を mutate するための host function 群。
//!
//! 設計方針 (Phase 10):
//! - 関数名・責務は Blitz `DocumentMutator` / `BaseDocument` と同じにする (合成 API は提供しない)
//! - DOM への借用は呼出単位で完結 (1 host call = 1 `DocumentMutator`)
//! - DOM context は `RuntimeCollector::dom_ctx` に raw pointer で持たせ、
//!   `WasmPanelRuntime::call_with_dom` のスコープ内でのみ有効
//!
//! NodeId エンコーディング:
//! - blitz NodeId (`usize`) を u64 として ABI に渡す
//! - `query_selector` の Option 返却は u64 で表現 (0 = None, それ以外は NodeId + 1)

use crate::RuntimeCollector;
use blitz_dom::{LocalName, Namespace, QualName};
use blitz_html::HtmlDocument;
use panel_schema::{Diagnostic, DiagnosticLevel};
use std::ptr::NonNull;
use wasmtime::{Caller, Extern, Linker, Memory};

/// Wasm 呼出スコープ内のみ有効な DOM コンテキスト。
///
/// `WasmPanelRuntime::call_with_dom` が NonNull を立て、戻り際に None に戻す。
/// DOM API の host function はこれを deref して `HtmlDocument` を mutate する。
///
/// SAFETY 原則:
/// - `dom_ctx.is_some()` のときに限って deref してよい
/// - deref 期間は host function 1 回分の呼出に閉じる (Wasm に再制御を渡さない)
/// - call_with_dom スコープ外では絶対に dereference しない
#[derive(Default)]
pub(crate) struct DomCtx {
    pub(crate) document: Option<NonNull<HtmlDocument>>,
}

impl DomCtx {
    pub(crate) fn clear(&mut self) {
        self.document = None;
    }
}

/// Phase 10 で公開される host module 名。
const DOM_HOST_MODULE: &str = "dom";

/// `DocumentMutator` / `BaseDocument` を Wasm に公開する host function 群を linker に登録する。
pub(crate) fn register_dom_host_functions(
    linker: &mut Linker<RuntimeCollector>,
) -> wasmtime::Result<()> {
    linker.func_wrap(DOM_HOST_MODULE, "query_selector", host_query_selector)?;
    linker.func_wrap(DOM_HOST_MODULE, "set_attribute", host_set_attribute)?;
    linker.func_wrap(DOM_HOST_MODULE, "clear_attribute", host_clear_attribute)?;
    linker.func_wrap(DOM_HOST_MODULE, "set_inner_html", host_set_inner_html)?;
    Ok(())
}

fn host_query_selector(mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32) -> i64 {
    let Some(selector) = read_utf8(&mut caller, ptr, len) else {
        push_err(&mut caller, "query_selector: invalid selector ptr/len");
        return 0;
    };
    let Some(doc) = current_document(&caller) else {
        push_err(&mut caller, "query_selector: no DOM context");
        return 0;
    };
    let doc_ref = unsafe { doc.as_ref() };
    match doc_ref.query_selector(&selector) {
        Ok(Some(id)) => (id as i64) + 1,
        Ok(None) => 0,
        Err(_) => {
            push_err(&mut caller, "query_selector: invalid CSS selector");
            0
        }
    }
}

fn host_set_attribute(
    mut caller: Caller<'_, RuntimeCollector>,
    node_id: i64,
    name_ptr: i32,
    name_len: i32,
    value_ptr: i32,
    value_len: i32,
) {
    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
        push_err(&mut caller, "set_attribute: invalid name ptr/len");
        return;
    };
    let Some(value) = read_utf8(&mut caller, value_ptr, value_len) else {
        push_err(&mut caller, "set_attribute: invalid value ptr/len");
        return;
    };
    let Some(mut doc) = current_document_mut(&mut caller) else {
        push_err(&mut caller, "set_attribute: no DOM context");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_err(&mut caller, "set_attribute: invalid node_id");
            return;
        }
    };
    let doc_mut = unsafe { doc.as_mut() };
    let mut mutator = doc_mut.mutate();
    mutator.set_attribute(id, qual_name(&name), &value);
}

fn host_clear_attribute(
    mut caller: Caller<'_, RuntimeCollector>,
    node_id: i64,
    name_ptr: i32,
    name_len: i32,
) {
    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
        push_err(&mut caller, "clear_attribute: invalid name ptr/len");
        return;
    };
    let Some(mut doc) = current_document_mut(&mut caller) else {
        push_err(&mut caller, "clear_attribute: no DOM context");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_err(&mut caller, "clear_attribute: invalid node_id");
            return;
        }
    };
    let doc_mut = unsafe { doc.as_mut() };
    let mut mutator = doc_mut.mutate();
    mutator.clear_attribute(id, qual_name(&name));
}

fn host_set_inner_html(
    mut caller: Caller<'_, RuntimeCollector>,
    node_id: i64,
    html_ptr: i32,
    html_len: i32,
) {
    let Some(html) = read_utf8(&mut caller, html_ptr, html_len) else {
        push_err(&mut caller, "set_inner_html: invalid html ptr/len");
        return;
    };
    let Some(mut doc) = current_document_mut(&mut caller) else {
        push_err(&mut caller, "set_inner_html: no DOM context");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_err(&mut caller, "set_inner_html: invalid node_id");
            return;
        }
    };
    let doc_mut = unsafe { doc.as_mut() };
    let mut mutator = doc_mut.mutate();
    mutator.set_inner_html(id, &html);
}

// === ヘルパ ===

fn decode_node_id(raw: i64) -> Option<usize> {
    if raw <= 0 {
        return None;
    }
    Some((raw - 1) as usize)
}

fn current_document(caller: &Caller<'_, RuntimeCollector>) -> Option<NonNull<HtmlDocument>> {
    caller.data().dom_ctx.document
}

fn current_document_mut(
    caller: &mut Caller<'_, RuntimeCollector>,
) -> Option<NonNull<HtmlDocument>> {
    caller.data_mut().dom_ctx.document
}

fn read_utf8(caller: &mut Caller<'_, RuntimeCollector>, ptr: i32, len: i32) -> Option<String> {
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

fn current_memory(caller: &mut Caller<'_, RuntimeCollector>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(memory)) => Some(memory),
        _ => None,
    }
}

fn push_err(caller: &mut Caller<'_, RuntimeCollector>, msg: &str) {
    caller.data_mut().result.diagnostics.push(Diagnostic {
        level: DiagnosticLevel::Error,
        message: msg.to_string(),
    });
}

fn qual_name(local: &str) -> QualName {
    QualName::new(None, Namespace::default(), LocalName::from(local))
}
