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
//! - その他 query/mutation は -1 / 0 をエラー値として使う
//!
//! Iterator (`query_selector_all`) handle:
//! - host 側 `Vec<NodeId>` を `RuntimeCollector::query_iters` に保持し、
//!   handle = index + 1 を Wasm に返す
//! - 著者は `iter_next` で先頭を pop、`iter_drop` で破棄する
//! - Wasm panic で Drop が走らない場合は handle leak が発生し得るが、
//!   `call_with_dom` の入退場で再初期化されるので永続蓄積はない

use crate::RuntimeCollector;
use blitz_dom::{LocalName, Namespace, QualName, node::NodeData};
use blitz_html::HtmlDocument;
use panel_schema::{Diagnostic, DiagnosticLevel};
use std::ptr::NonNull;
use wasmtime::{Caller, Extern, Linker, Memory};

/// 1 つの `query_selector_all` 結果を保持する iterator。
#[derive(Debug, Default)]
pub(crate) struct QueryIter {
    pub(crate) ids: Vec<usize>,
    pub(crate) cursor: usize,
}

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
    pub(crate) iters: Vec<QueryIter>,
}

impl DomCtx {
    pub(crate) fn clear(&mut self) {
        self.document = None;
        self.iters.clear();
    }
}

/// Phase 10 で公開される host module 名。
const DOM_HOST_MODULE: &str = "dom";

/// `DocumentMutator` / `BaseDocument` を Wasm に公開する host function 群を linker に登録する。
pub(crate) fn register_dom_host_functions(
    linker: &mut Linker<RuntimeCollector>,
) -> wasmtime::Result<()> {
    linker.func_wrap(DOM_HOST_MODULE, "query_selector", host_query_selector)?;
    linker.func_wrap(DOM_HOST_MODULE, "query_selector_all", host_query_selector_all)?;
    linker.func_wrap(DOM_HOST_MODULE, "iter_next", host_iter_next)?;
    linker.func_wrap(DOM_HOST_MODULE, "iter_drop", host_iter_drop)?;
    linker.func_wrap(DOM_HOST_MODULE, "get_attribute_len", host_get_attribute_len)?;
    linker.func_wrap(DOM_HOST_MODULE, "get_attribute_copy", host_get_attribute_copy)?;
    linker.func_wrap(DOM_HOST_MODULE, "set_attribute", host_set_attribute)?;
    linker.func_wrap(DOM_HOST_MODULE, "clear_attribute", host_clear_attribute)?;
    linker.func_wrap(DOM_HOST_MODULE, "create_text_node", host_create_text_node)?;
    linker.func_wrap(DOM_HOST_MODULE, "append_children", host_append_children)?;
    linker.func_wrap(
        DOM_HOST_MODULE,
        "remove_and_drop_all_children",
        host_remove_and_drop_all_children,
    )?;
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

fn host_query_selector_all(
    mut caller: Caller<'_, RuntimeCollector>,
    ptr: i32,
    len: i32,
) -> i64 {
    let Some(selector) = read_utf8(&mut caller, ptr, len) else {
        push_err(&mut caller, "query_selector_all: invalid selector ptr/len");
        return 0;
    };
    let Some(doc) = current_document(&caller) else {
        push_err(&mut caller, "query_selector_all: no DOM context");
        return 0;
    };
    let doc_ref = unsafe { doc.as_ref() };
    let ids: Vec<usize> = match doc_ref.query_selector_all(&selector) {
        Ok(ids) => ids.into_iter().collect(),
        Err(_) => {
            push_err(&mut caller, "query_selector_all: invalid CSS selector");
            return 0;
        }
    };
    let dom_ctx = &mut caller.data_mut().dom_ctx;
    dom_ctx.iters.push(QueryIter { ids, cursor: 0 });
    dom_ctx.iters.len() as i64
}

fn host_iter_next(mut caller: Caller<'_, RuntimeCollector>, handle: i64) -> i64 {
    let dom_ctx = &mut caller.data_mut().dom_ctx;
    let Some(idx) = handle_to_index(handle, dom_ctx.iters.len()) else {
        return 0;
    };
    let iter = &mut dom_ctx.iters[idx];
    if iter.cursor >= iter.ids.len() {
        return 0;
    }
    let id = iter.ids[iter.cursor];
    iter.cursor += 1;
    (id as i64) + 1
}

fn host_iter_drop(mut caller: Caller<'_, RuntimeCollector>, handle: i64) {
    let dom_ctx = &mut caller.data_mut().dom_ctx;
    let Some(idx) = handle_to_index(handle, dom_ctx.iters.len()) else {
        return;
    };
    // tombstone: 空にする (Vec の index 安定性を保つ)
    dom_ctx.iters[idx].ids.clear();
    dom_ctx.iters[idx].cursor = 0;
}

fn host_get_attribute_len(
    mut caller: Caller<'_, RuntimeCollector>,
    node_id: i64,
    name_ptr: i32,
    name_len: i32,
) -> i32 {
    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
        push_err(&mut caller, "get_attribute_len: invalid name ptr/len");
        return -1;
    };
    let Some(doc) = current_document(&caller) else {
        push_err(&mut caller, "get_attribute_len: no DOM context");
        return -1;
    };
    let doc_ref = unsafe { doc.as_ref() };
    match read_attribute(doc_ref, node_id, &name) {
        Some(value) => value.len() as i32,
        None => -1,
    }
}

fn host_get_attribute_copy(
    mut caller: Caller<'_, RuntimeCollector>,
    node_id: i64,
    name_ptr: i32,
    name_len: i32,
    buf_ptr: i32,
    buf_cap: i32,
) -> i32 {
    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
        push_err(&mut caller, "get_attribute_copy: invalid name ptr/len");
        return -1;
    };
    let Some(doc) = current_document(&caller) else {
        push_err(&mut caller, "get_attribute_copy: no DOM context");
        return -1;
    };
    let value = {
        let doc_ref = unsafe { doc.as_ref() };
        read_attribute(doc_ref, node_id, &name).map(ToString::to_string)
    };
    let Some(value) = value else {
        return -1;
    };
    let Some(memory) = current_memory(&mut caller) else {
        push_err(&mut caller, "get_attribute_copy: missing wasm memory");
        return -1;
    };
    if buf_ptr < 0 || buf_cap < 0 {
        push_err(&mut caller, "get_attribute_copy: invalid buffer");
        return -1;
    }
    let bytes = value.as_bytes();
    let data = memory.data_mut(&mut caller);
    let start = buf_ptr as usize;
    let cap = buf_cap as usize;
    let end = start.saturating_add(cap).min(data.len());
    let Some(target) = data.get_mut(start..end) else {
        push_err(&mut caller, "get_attribute_copy: buffer out of bounds");
        return -1;
    };
    let count = bytes.len().min(target.len());
    target[..count].copy_from_slice(&bytes[..count]);
    bytes.len() as i32
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

fn host_create_text_node(
    mut caller: Caller<'_, RuntimeCollector>,
    text_ptr: i32,
    text_len: i32,
) -> i64 {
    let Some(text) = read_utf8(&mut caller, text_ptr, text_len) else {
        push_err(&mut caller, "create_text_node: invalid text ptr/len");
        return 0;
    };
    let Some(mut doc) = current_document_mut(&mut caller) else {
        push_err(&mut caller, "create_text_node: no DOM context");
        return 0;
    };
    let doc_mut = unsafe { doc.as_mut() };
    let mut mutator = doc_mut.mutate();
    let id = mutator.create_text_node(&text);
    (id as i64) + 1
}

fn host_append_children(
    mut caller: Caller<'_, RuntimeCollector>,
    parent: i64,
    children_ptr: i32,
    count: i32,
) {
    if count < 0 {
        push_err(&mut caller, "append_children: negative count");
        return;
    }
    let Some(memory) = current_memory(&mut caller) else {
        push_err(&mut caller, "append_children: missing wasm memory");
        return;
    };
    let bytes_needed = (count as usize).checked_mul(8);
    let Some(bytes_needed) = bytes_needed else {
        push_err(&mut caller, "append_children: count overflow");
        return;
    };
    let data = memory.data(&caller);
    let start = if children_ptr < 0 {
        push_err(&mut caller, "append_children: invalid ptr");
        return;
    } else {
        children_ptr as usize
    };
    let end = start.saturating_add(bytes_needed);
    let Some(slice) = data.get(start..end) else {
        push_err(&mut caller, "append_children: ptr/count out of bounds");
        return;
    };
    let mut child_ids: Vec<usize> = Vec::with_capacity(count as usize);
    for chunk in slice.chunks_exact(8) {
        let raw = i64::from_le_bytes(chunk.try_into().expect("8 bytes"));
        match decode_node_id(raw) {
            Some(id) => child_ids.push(id),
            None => {
                push_err(&mut caller, "append_children: invalid child node_id");
                return;
            }
        }
    }
    let Some(parent_id) = decode_node_id(parent) else {
        push_err(&mut caller, "append_children: invalid parent_id");
        return;
    };
    let Some(mut doc) = current_document_mut(&mut caller) else {
        push_err(&mut caller, "append_children: no DOM context");
        return;
    };
    let doc_mut = unsafe { doc.as_mut() };
    let mut mutator = doc_mut.mutate();
    mutator.append_children(parent_id, &child_ids);
}

fn host_remove_and_drop_all_children(mut caller: Caller<'_, RuntimeCollector>, node_id: i64) {
    let Some(mut doc) = current_document_mut(&mut caller) else {
        push_err(&mut caller, "remove_and_drop_all_children: no DOM context");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_err(&mut caller, "remove_and_drop_all_children: invalid node_id");
            return;
        }
    };
    let doc_mut = unsafe { doc.as_mut() };
    let mut mutator = doc_mut.mutate();
    mutator.remove_and_drop_all_children(id);
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

fn read_attribute<'a>(
    doc: &'a HtmlDocument,
    node_id: i64,
    name: &str,
) -> Option<&'a str> {
    let id = decode_node_id(node_id)?;
    let node = doc.get_node(id)?;
    let NodeData::Element(element) = &node.data else {
        return None;
    };
    element.attr(LocalName::from(name))
}

fn decode_node_id(raw: i64) -> Option<usize> {
    if raw <= 0 {
        return None;
    }
    Some((raw - 1) as usize)
}

fn handle_to_index(handle: i64, len: usize) -> Option<usize> {
    if handle <= 0 {
        return None;
    }
    let idx = (handle - 1) as usize;
    if idx >= len { None } else { Some(idx) }
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
